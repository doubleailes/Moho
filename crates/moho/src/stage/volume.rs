//! UsdVol: a `Volume`'s `volumeFieldBinding` and each field asset's
//! `volumeField`, as UsdImaging's volume and field asset data sources.
//!
//! A binding is named by its `field:` relationship's base name and holds its
//! single forwarded prim target (`UsdVolVolume::GetFieldPaths`); a field
//! asset lists its schema's attributes and answers any attribute by name. A
//! field binding or field attribute edit dirties the whole container, as
//! UsdImaging does.

use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd::usd;

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::prim_type;
use crate::tokens::volume as t;

const FIELD_PREFIX: &str = "field:";

/// The field relationship's single forwarded target, when it is one prim
/// path (`UsdVolVolume::GetFieldPath`).
fn field_path(relationship: &usd::Relationship) -> Option<sdf::Path> {
    let targets = relationship.forwarded_targets().ok()?;
    match targets.as_slice() {
        [target] if target.is_prim() => Some(target.clone()),
        _ => None,
    }
}

/// The adapter of `Volume` prims.
pub struct VolumeAdapter;

impl Adapter for VolumeAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::VOLUME)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::VOLUME_FIELD_BINDING)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::VOLUME_FIELD_BINDING).then(|| DataSource::Container(Rc::new(FieldBindings(ctx.clone()))))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if name.starts_with(FIELD_PREFIX) {
            LocatorSet::from(Locator::new([t::VOLUME_FIELD_BINDING]))
        } else {
            LocatorSet::new()
        }
    }
}

/// `UsdImagingDataSourceVolumeFieldBindings`.
struct FieldBindings(PrimContext);

impl Container for FieldBindings {
    /// The base names of the `field:` relationships with a single prim
    /// target, sorted (`GetFieldPaths`' map).
    fn names(&self) -> Vec<Token> {
        let Some(prim) = self.0.prim() else {
            return Vec::new();
        };
        let mut names: Vec<Token> = prim
            .relationships_in_namespace("field")
            .unwrap_or_default()
            .into_iter()
            .filter(|rel| field_path(rel).is_some())
            .filter_map(|rel| rel.name().rsplit(':').next().map(Token::from))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The relationship `field:<name>`'s target. A binding named by the
    /// base name of a nested `field:a:b` is not found again (UsdImaging's
    /// quirk, reproduced).
    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        let relationship = prim.relationship(format!("{FIELD_PREFIX}{name}").as_str());
        let path = field_path(&relationship)?;
        Some(value(Value::PathVec(vec![path])))
    }
}

/// A field asset's attributes (`UsdVolVolumeFieldAsset`'s own, then its
/// subclass's).
const BASE: &[&str] = &[
    t::FILE_PATH,
    t::FIELD_NAME,
    t::FIELD_INDEX,
    t::FIELD_DATA_TYPE,
    t::VECTOR_DATA_ROLE_HINT,
];

/// A field asset prim's adapter.
pub struct FieldAssetAdapter {
    prim_type: &'static str,
    /// The subclass's own attributes beside `fieldDataType`, which it
    /// redeclares (and UsdImaging lists twice; listed once here).
    own: &'static [&'static str],
}

/// The adapter of `OpenVDBAsset` prims.
pub const OPENVDB_ASSET: FieldAssetAdapter = FieldAssetAdapter {
    prim_type: prim_type::OPENVDB_ASSET,
    own: &["fieldClass"],
};

/// The adapter of `Field3DAsset` prims.
pub const FIELD3D_ASSET: FieldAssetAdapter = FieldAssetAdapter {
    prim_type: prim_type::FIELD3D_ASSET,
    own: &["fieldPurpose"],
};

impl FieldAssetAdapter {
    fn attribute_names(&self) -> impl Iterator<Item = &'static str> {
        BASE.iter().chain(self.own).copied()
    }
}

impl Adapter for FieldAssetAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(self.prim_type)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::VOLUME_FIELD)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::VOLUME_FIELD).then(|| {
            DataSource::Container(Rc::new(FieldAsset {
                ctx: ctx.clone(),
                names: self.attribute_names().map(Token::from).collect(),
            }))
        })
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if self.attribute_names().any(|n| n == name) {
            LocatorSet::from(Locator::new([t::VOLUME_FIELD]))
        } else {
            LocatorSet::new()
        }
    }
}

/// `UsdImagingDataSourceFieldAsset`.
struct FieldAsset {
    ctx: PrimContext,
    names: Vec<Token>,
}

impl Container for FieldAsset {
    fn names(&self) -> Vec<Token> {
        self.names.clone()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let attribute = self.ctx.prim()?.attribute(name.clone());
        attribute.is_defined().unwrap_or(false).then(|| {
            AttributeValue::data_source(
                attribute,
                &self.ctx,
                Locator::new([t::VOLUME_FIELD, name.as_str()]),
            )
        })
    }
}

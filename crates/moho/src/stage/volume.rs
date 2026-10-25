//! UsdVol: a `Volume`'s `volumeFieldBinding` (each `field:<name>`
//! relationship's target), and each field asset's `volumeField`, as
//! UsdImaging's volume and field-asset data sources.

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::table::{Member, TableAdapter};
use crate::tokens::prim_type;
use crate::tokens::volume as t;

const FIELD_PREFIX: &str = "field:";

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
        if name != t::VOLUME_FIELD_BINDING {
            return None;
        }
        let prim = ctx.prim()?;
        let entries: Vec<(Token, DataSource)> = prim
            .relationships_in_namespace("field")
            .unwrap_or_default()
            .into_iter()
            .filter_map(|rel| {
                let field = rel.name().strip_prefix(FIELD_PREFIX)?.to_owned();
                let target = rel.forwarded_targets().ok()?.into_iter().next()?;
                Some((Token::from(field), value(Value::PathVec(vec![target]))))
            })
            .collect();
        Some(container(entries))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        match name.strip_prefix(FIELD_PREFIX) {
            Some(field) => LocatorSet::from(Locator::new([t::VOLUME_FIELD_BINDING, field])),
            None => LocatorSet::new(),
        }
    }
}

const FIELD: &[Member] = &[
    Member::Attr(t::FILE_PATH, "filePath"),
    Member::Attr(t::FIELD_NAME, "fieldName"),
    Member::Attr(t::FIELD_INDEX, "fieldIndex"),
    Member::Attr(t::FIELD_DATA_TYPE, "fieldDataType"),
    Member::Attr(t::VECTOR_DATA_ROLE_HINT, "vectorDataRoleHint"),
];

/// The adapter of `OpenVDBAsset` prims.
pub const OPENVDB_ASSET: TableAdapter = TableAdapter {
    prim_type: prim_type::OPENVDB_ASSET,
    schema: t::VOLUME_FIELD,
    members: FIELD,
};

/// The adapter of `Field3DAsset` prims.
pub const FIELD3D_ASSET: TableAdapter = TableAdapter {
    prim_type: prim_type::FIELD3D_ASSET,
    schema: t::VOLUME_FIELD,
    members: FIELD,
};

//! `usdMaterialBindings`: the binding opinions a prim applying
//! `MaterialBindingAPI` authors, per purpose, as
//! `UsdImagingMaterialBindingsSchema`. One element per prim; the flattening
//! filter concatenates the ancestors' before it, and the resolving filter
//! picks the winner.

use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd_schemas::shade::{BindingStrength, MATERIAL_PURPOSES, MaterialBindingAPI};

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::material_bindings as t;

/// The adapter of prims applying `MaterialBindingAPI`.
pub struct MaterialBindingAdapter;

fn strength(strength: BindingStrength) -> DataSource {
    let token = match strength {
        BindingStrength::StrongerThanDescendants => t::STRONGER_THAN_DESCENDANTS,
        BindingStrength::WeakerThanDescendants => t::WEAKER_THAN_DESCENDANTS,
    };
    value(Value::Token(Token::from(token)))
}

fn path(path: &sdf::Path) -> DataSource {
    value(Value::PathVec(vec![path.clone()]))
}

/// One purpose's opinions on this prim, one level naming both kinds of
/// binding, or `None` when it authors none (`_BuildMaterialBindingVectorDataSource`).
fn bindings(api: &MaterialBindingAPI, purpose: &str) -> Option<DataSource> {
    let direct = api.direct_binding(purpose).ok().flatten();
    let collections = api.collection_bindings(purpose).unwrap_or_default();
    if direct.is_none() && collections.is_empty() {
        return None;
    }
    let direct = direct.map(|direct| {
        container([
            (t::MATERIAL_PATH, path(direct.material_path())),
            (
                t::BINDING_STRENGTH,
                strength(direct.strength().unwrap_or_default()),
            ),
        ])
    });
    let collections = (!collections.is_empty()).then(|| {
        let elements = collections.iter().map(|binding| {
            let collection = binding.collection_path();
            let (prim, name) = collection
                .split_property()
                .map(|(prim, property)| {
                    let name = property.strip_prefix("collection:").unwrap_or(property);
                    (prim, name.to_owned())
                })
                .unwrap_or_else(|| (collection.clone(), String::new()));
            container([
                (t::COLLECTION_PRIM_PATH, path(&prim)),
                (
                    t::COLLECTION_NAME,
                    value(Value::Token(Token::from(name.as_str()))),
                ),
                (t::MATERIAL_PATH, path(binding.material_path())),
                (
                    t::BINDING_STRENGTH,
                    strength(binding.strength().unwrap_or_default()),
                ),
            ])
        });
        RetainedVector::new(elements).into_data_source()
    });
    let level: DataSource = DataSource::Container(Rc::new(Level { direct, collections }));
    Some(RetainedVector::new([level]).into_data_source())
}

/// One level of opinions, naming both kinds of binding whether or not it
/// has them (`_MaterialBindingContainerDataSource`).
struct Level {
    direct: Option<DataSource>,
    collections: Option<DataSource>,
}

impl Container for Level {
    fn names(&self) -> Vec<Token> {
        vec![
            Token::from(t::DIRECT_MATERIAL_BINDING),
            Token::from(t::COLLECTION_MATERIAL_BINDINGS),
        ]
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::DIRECT_MATERIAL_BINDING => self.direct.clone(),
            t::COLLECTION_MATERIAL_BINDINGS => self.collections.clone(),
            _ => None,
        }
    }
}

/// A prim's opinions by purpose, naming every material purpose whether or
/// not it binds one (`_MaterialBindingsContainerDataSource`).
struct Purposes(MaterialBindingAPI);

impl Container for Purposes {
    fn names(&self) -> Vec<Token> {
        MATERIAL_PURPOSES.iter().map(|p| Token::from(*p)).collect()
    }

    fn get(&self, purpose: &Token) -> Option<DataSource> {
        bindings(&self.0, purpose.as_str())
    }
}

fn api(ctx: &PrimContext) -> Option<MaterialBindingAPI> {
    MaterialBindingAPI::get(&ctx.stage, ctx.path.clone())
        .ok()
        .flatten()
}

impl Adapter for MaterialBindingAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::USD_MATERIAL_BINDINGS)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::USD_MATERIAL_BINDINGS {
            return None;
        }
        Some(DataSource::Container(Rc::new(Purposes(api(ctx)?))))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        // A collection's membership decides which collection binding applies.
        if MaterialBindingAPI::owns_property(name) || name.starts_with("collection:") {
            LocatorSet::from(Locator::new([t::USD_MATERIAL_BINDINGS]))
        } else {
            LocatorSet::new()
        }
    }
}

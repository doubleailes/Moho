//! `usdMaterialBindings`: the binding opinions a prim applying
//! `MaterialBindingAPI` authors, per purpose, as
//! `UsdImagingMaterialBindingsSchema`. One element per prim; the flattening
//! filter concatenates the ancestors' before it, and the resolving filter
//! picks the winner.

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd_schemas::shade::{BindingStrength, MATERIAL_PURPOSES, MaterialBindingAPI};

use crate::data_source::DataSource;
use crate::data_source::retained::{RetainedVector, container, value};
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

/// One purpose's opinions on this prim, or `None` when it authors none.
fn bindings(api: &MaterialBindingAPI, purpose: &str) -> Option<DataSource> {
    let direct = api.direct_binding(purpose).ok().flatten();
    let collections = api.collection_bindings(purpose).unwrap_or_default();
    if direct.is_none() && collections.is_empty() {
        return None;
    }
    let mut entries = Vec::new();
    if let Some(direct) = direct {
        entries.push((
            t::DIRECT_MATERIAL_BINDING,
            container([
                (t::MATERIAL_PATH, path(direct.material_path())),
                (
                    t::BINDING_STRENGTH,
                    strength(direct.strength().unwrap_or_default()),
                ),
            ]),
        ));
    }
    if !collections.is_empty() {
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
                (t::COLLECTION_NAME, value(Value::Token(Token::from(name)))),
                (t::MATERIAL_PATH, path(binding.material_path())),
                (
                    t::BINDING_STRENGTH,
                    strength(binding.strength().unwrap_or_default()),
                ),
            ])
        });
        entries.push((
            t::COLLECTION_MATERIAL_BINDINGS,
            RetainedVector::new(elements).into_data_source(),
        ));
    }
    Some(RetainedVector::new([container(entries)]).into_data_source())
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
        let api = api(ctx)?;
        let entries: Vec<(Token, DataSource)> = MATERIAL_PURPOSES
            .iter()
            .filter_map(|purpose| bindings(&api, purpose).map(|b| (Token::from(*purpose), b)))
            .collect();
        Some(container(entries))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if MaterialBindingAPI::owns_property(name) {
            LocatorSet::from(Locator::new([t::USD_MATERIAL_BINDINGS]))
        } else {
            LocatorSet::new()
        }
    }
}

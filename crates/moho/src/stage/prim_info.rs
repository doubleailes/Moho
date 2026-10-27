//! `__usdPrimInfo`, as `UsdImagingDataSourceUsdPrimInfo`: the USD prim's
//! specifier, type name, load state, applied API schemas, kind, variant
//! selections, and whether it is a native instance (`niPrototypePath`) or a
//! prototype's root (`isNiPrototype`). Each field is read when asked for,
//! and an absent field is not named.

use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd::usd;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, DataSource};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::instancing::prototype_identity;
use crate::tokens::usd_prim_info as t;

/// Contributes `__usdPrimInfo` to every prim.
pub struct PrimInfoAdapter;

/// The fields, in `UsdImagingDataSourceUsdPrimInfo::GetNames`' order.
const FIELDS: &[&str] = &[
    t::SPECIFIER,
    t::TYPE_NAME,
    t::IS_LOADED,
    t::API_SCHEMAS,
    t::KIND,
    t::VARIANT_SELECTIONS,
    t::NI_PROTOTYPE_PATH,
    t::IS_NI_PROTOTYPE,
];

/// One prim's `__usdPrimInfo`.
struct PrimInfo {
    prim: usd::Prim,
    ctx: PrimContext,
}

impl PrimInfo {
    fn field(&self, name: &str) -> Option<DataSource> {
        let prim = &self.prim;
        match name {
            t::SPECIFIER => {
                let token = match prim.specifier().ok()?? {
                    sdf::Specifier::Def => t::DEF,
                    sdf::Specifier::Over => t::OVER,
                    sdf::Specifier::Class => t::CLASS,
                };
                Some(value(Value::Token(Token::from(token))))
            }
            t::TYPE_NAME => {
                let type_name = prim.type_name().ok().flatten().unwrap_or_default();
                Some(value(Value::Token(type_name)))
            }
            t::IS_LOADED => Some(value(prim.is_loaded().unwrap_or(false))),
            // Composed, the typed schema's built-in API schemas included, as
            // `UsdPrim::GetAppliedSchemas`; absent when there are none.
            t::API_SCHEMAS => {
                let schemas = prim.api_schemas().unwrap_or_default();
                (!schemas.is_empty()).then(|| value(Value::TokenVec(schemas)))
            }
            t::KIND => prim.kind().ok().flatten().map(|kind| value(Value::Token(kind))),
            t::VARIANT_SELECTIONS => {
                let selections = prim.variant_sets().selections().unwrap_or_default();
                (!selections.is_empty()).then(|| {
                    container(selections.into_iter().map(|(set, selection)| {
                        (
                            Token::from(set.as_str()),
                            value(Value::Token(Token::from(selection.as_str()))),
                        )
                    }))
                })
            }
            t::NI_PROTOTYPE_PATH => {
                if !prim.is_instance().unwrap_or(false) {
                    return None;
                }
                let prototype = prim.prototype().ok()??;
                Some(value(Value::PathVec(vec![prototype])))
            }
            t::IS_NI_PROTOTYPE => {
                (prim.is_prototype().unwrap_or(false) && self.ctx.path.is_root_prim()).then(|| value(true))
            }
            // Not a field of the schema, and not named: the native-instancing
            // filter's grouping key, until `hydra-parity-instancing` groups
            // by Hydra's own key.
            t::NI_PROTOTYPE_IDENTITY => {
                if !prim.is_instance().unwrap_or(false) {
                    return None;
                }
                let identity = prototype_identity(&self.ctx.stage, &self.ctx.path)?;
                Some(value(Value::Token(Token::from(format!("{identity:016x}")))))
            }
            _ => None,
        }
    }
}

impl Container for PrimInfo {
    fn names(&self) -> Vec<Token> {
        FIELDS
            .iter()
            .filter(|name| self.field(name).is_some())
            .map(|name| Token::from(*name))
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.field(name.as_str())
    }
}

impl Adapter for PrimInfoAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() || ctx.prim().is_none() {
            Vec::new()
        } else {
            vec![Token::from(t::USD_PRIM_INFO)]
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::USD_PRIM_INFO || ctx.path.is_abs_root() {
            return None;
        }
        let prim = ctx.prim()?;
        Some(DataSource::Container(Rc::new(PrimInfo {
            prim,
            ctx: ctx.clone(),
        })))
    }
}

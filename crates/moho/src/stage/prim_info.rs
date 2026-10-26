//! `__usdPrimInfo`, as `UsdImagingUsdPrimInfoSchema`: the USD prim's
//! specifier, type name, applied API schemas, kind and load state, and what
//! the native-instancing filter needs to know about instances and
//! prototypes.

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::instancing::prototype_identity;
use crate::tokens::usd_prim_info as t;

/// Contributes `__usdPrimInfo` to every prim.
pub struct PrimInfoAdapter;

fn entries(ctx: &PrimContext) -> Vec<(&'static str, DataSource)> {
    let Some(prim) = ctx.prim() else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    if let Ok(Some(specifier)) = prim.specifier() {
        let token = match specifier {
            sdf::Specifier::Def => t::DEF,
            sdf::Specifier::Over => t::OVER,
            sdf::Specifier::Class => t::CLASS,
        };
        entries.push((t::SPECIFIER, value(Value::Token(Token::from(token)))));
    }
    let type_name = prim.type_name().ok().flatten().unwrap_or_default();
    entries.push((t::TYPE_NAME, value(Value::Token(type_name))));
    entries.push((t::IS_LOADED, value(prim.is_loaded().unwrap_or(false))));
    // Composed, the typed schema's built-in API schemas included, as
    // `UsdPrim::GetAppliedSchemas`.
    entries.push((
        t::API_SCHEMAS,
        value(Value::TokenVec(prim.api_schemas().unwrap_or_default())),
    ));
    if let Ok(Some(kind)) = prim.kind() {
        entries.push((t::KIND, value(Value::Token(kind))));
    }
    if prim.is_instance().unwrap_or(false)
        && let Ok(Some(prototype)) = prim.prototype()
    {
        entries.push((t::NI_PROTOTYPE_PATH, value(Value::PathVec(vec![prototype]))));
        if let Some(identity) = prototype_identity(&ctx.stage, &ctx.path) {
            entries.push((
                t::NI_PROTOTYPE_IDENTITY,
                value(Value::Token(Token::from(format!("{identity:016x}")))),
            ));
        }
    }
    if prim.is_prototype().unwrap_or(false) && ctx.path.is_root_prim() {
        entries.push((t::IS_NI_PROTOTYPE, value(true)));
    }
    entries
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
        let entries = entries(ctx);
        (!entries.is_empty()).then(|| container(entries))
    }
}

//! `__usdPrimInfo`: what the native-instancing filter needs to know about
//! instances and prototypes, as `UsdImagingUsdPrimInfoSchema`.

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::instancing::prototype_identity;
use crate::tokens::usd_prim_info as t;

/// Contributes `__usdPrimInfo` to instances and prototype roots.
pub struct PrimInfoAdapter;

fn entries(ctx: &PrimContext) -> Vec<(&'static str, DataSource)> {
    let Some(prim) = ctx.prim() else {
        return Vec::new();
    };
    let mut entries = Vec::new();
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
        if ctx.path.is_abs_root() || entries(ctx).is_empty() {
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

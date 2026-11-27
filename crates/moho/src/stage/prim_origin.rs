//! `primOrigin`: the USD prim a Hydra prim comes from, as
//! `UsdImagingDataSourcePrimOrigin` builds `HdPrimOriginSchema`.

use openusd::sdf;

use openusd::tf::Token;
use openusd::vt::Value;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::prim_info::skips_prim_data;
use crate::tokens::prim_origin as t;

/// Contributes `primOrigin/scenePath` to every prim UsdImaging gives prim
/// data ([`skips_prim_data`]).
pub struct PrimOriginAdapter;

/// The prim's path, relative to its prototype root when it lies inside a
/// USD prototype (`/__Prototype_1/MyXform/MySphere` gives
/// `MyXform/MySphere`, the root itself `.`), as UsdImaging's
/// `_ComputePrototypeRelativePath` does with `SdfPath::MakeRelativePath`.
pub fn scene_path(ctx: &PrimContext) -> sdf::Path {
    let path = ctx.path.clone();
    let Some(root) = path.ancestors_below_root().last() else {
        return path;
    };
    let is_prototype = ctx
        .stage
        .prim(&root)
        .ok()
        .and_then(|p| p.is_prototype().ok())
        .unwrap_or(false);
    if !is_prototype {
        return path;
    }
    path.make_relative(&root)
}

impl Adapter for PrimOriginAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() || ctx.prim().is_some_and(|p| skips_prim_data(&p)) {
            Vec::new()
        } else {
            vec![Token::from(t::PRIM_ORIGIN)]
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::PRIM_ORIGIN || ctx.path.is_abs_root() || ctx.prim().is_some_and(|p| skips_prim_data(&p))
        {
            return None;
        }
        Some(container([(t::SCENE_PATH, value(Value::Path(scene_path(ctx))))]))
    }
}

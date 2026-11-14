//! `coordSysBinding`: the coordinate systems a prim applying `CoordSysAPI`
//! instances binds, as UsdImaging's `UsdImagingCoordSysAPIAdapter`.
//!
//! Each applied instance `CoordSysAPI:<name>` whose `coordSys:<name>:binding`
//! relationship targets something (C++ `GetLocalBinding`: the first
//! forwarded target) gives `coordSysBinding/<name>`, the targeted path. An
//! instance that targets nothing gives nothing, and a prim none of whose
//! instances binds has no `coordSysBinding` (UsdImaging's per-instance
//! containers are overlaid, and each of those is null). The flattening filter
//! overlays the prim's bindings on its ancestors'.
//!
//! Only the applied form is read, as UsdImaging 2.0 reads it: a bare
//! `coordSys:<name>` relationship on a prim that applies no instance (UsdShade's
//! deprecated non-applied form) gives nothing.
//!
//! A change to any property whose name starts with `coordSys` (C++
//! `UsdShadeCoordSysAPI::CanContainPropertyName`, a plain prefix test) on a
//! prim applying an instance dirties the whole `coordSysBinding`.

use openusd::tf::Token;
use openusd::vt::Value;
use openusd_schemas::shade::CoordSysAPI;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::coord_sys as t;

/// The adapter of prims applying `CoordSysAPI` instances.
pub struct CoordSysAdapter;

/// Each applied instance's name and the path its binding targets, in
/// applied order, skipping an instance that targets nothing.
fn bindings(ctx: &PrimContext) -> Vec<(Token, DataSource)> {
    let Some(prim) = ctx.prim() else {
        return Vec::new();
    };
    CoordSysAPI::all(&prim)
        .unwrap_or_default()
        .iter()
        .filter_map(|instance| {
            let binding = instance.local_binding().ok().flatten()?;
            Some((
                instance.name().clone(),
                value(Value::Path(binding.coord_sys_path().clone())),
            ))
        })
        .collect()
}

impl Adapter for CoordSysAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if bindings(ctx).is_empty() {
            return Vec::new();
        }
        vec![Token::from(t::COORD_SYS_BINDING)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::COORD_SYS_BINDING {
            return None;
        }
        let bindings = bindings(ctx);
        (!bindings.is_empty()).then(|| container(bindings))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if name.starts_with(t::COORD_SYS_PROPERTY_PREFIX) {
            return LocatorSet::from(Locator::new([t::COORD_SYS_BINDING]));
        }
        LocatorSet::new()
    }
}

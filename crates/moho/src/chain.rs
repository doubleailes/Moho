//! The standard chain of scene indices over a stage scene index, and the
//! renderer filters a consumer applies over it, as a Hydra render index
//! applies a renderer's scene index plugins.
//!
//! A Hydra 2.0 renderer consumes Hydra in three steps, and Moho offers each:
//!
//! 1. the application builds UsdImaging's chain ([`standard`]), possibly
//!    merging several ([`MergingSceneIndex`](crate::filter::MergingSceneIndex));
//! 2. the render index applies the renderer's scene index plugins, ordered
//!    by `HdSceneIndexPluginRegistry` ([`renderer_filters`]): hdPrman and
//!    arnold-usd add light linking at phase 4, arnold-usd dependency
//!    forwarding at phase 1000;
//! 3. the render delegate receives the terminal scene index, batches its
//!    notices by prim type and observes the batches with a prim factory, as
//!    hdPrman's `renderDelegate.cpp` does.
//!
//! A renderer that needs a fix over Hydra's behaviour ships it as a renderer
//! filter of its own; the standard chain keeps UsdImaging's, quirks
//! included.

use std::rc::Rc;

use crate::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use crate::filter::{
    DependencyForwardingSceneIndex, InstanceProxyPathTranslationSceneIndex, LightLinkingSceneIndex,
    MaterialBindingResolvingSceneIndex, NativeInstancingSceneIndex, PointInstancingSceneIndex,
    RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::SceneIndexHandle;
use crate::stage::StageSceneIndex;
use crate::tokens::material_bindings;

/// The standard chain over `stage`: exactly the filters
/// `UsdImagingCreateSceneIndices` builds with `addDrawModeSceneIndex =
/// false`, `displayUnloadedPrimsWithBounds = false`, no overrides callback
/// and no UsdImaging scene index plugins, in its order:
///
/// 1. extent resolving (`extentsHint` for the `geometry`, `render` and
///    `proxy` purposes);
/// 2. point instancer prototype propagation;
/// 3. native instancer prototype propagation (which flattens the stage and
///    each prototype);
/// 4. instance proxy path translation (of `usdMaterialBindings`);
/// 5. material binding resolving;
/// 6. render settings flattening.
///
/// Left out, as they change nothing the chain exposes or notifies: the
/// locator caching scene index on `material` (a cache), the notice batching
/// scene index (a pass-through until a client enables batching) and the
/// selection scene index (adds nothing until something is selected).
///
/// Light linking and dependency forwarding are not in it: a renderer adds
/// them as [`renderer_filters`].
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let extents = ExtentResolvingSceneIndex::new(stage, extent_resolving::STANDARD_PURPOSES.iter().copied());
    let point = PointInstancingSceneIndex::new(extents);
    let native = NativeInstancingSceneIndex::new(point);
    // Bindings may name prims inside native instances.
    let proxies =
        InstanceProxyPathTranslationSceneIndex::new(native, [material_bindings::USD_MATERIAL_BINDINGS]);
    let bindings = MaterialBindingResolvingSceneIndex::new(proxies);
    RenderSettingsFlatteningSceneIndex::new(bindings)
}

/// Where a renderer filter goes among those of its phase
/// (`HdSceneIndexPluginRegistry::InsertionOrder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InsertionOrder {
    /// Before the phase's `AtEnd` filters.
    AtStart,
    /// After the phase's `AtStart` filters.
    AtEnd,
}

/// The phase hdPrman and arnold-usd add light linking at.
pub const LIGHT_LINKING_PHASE: i32 = 4;
/// The phase arnold-usd adds dependency forwarding at.
pub const DEPENDENCY_FORWARDING_PHASE: i32 = 1000;

/// One renderer filter: a Hydra scene index plugin, with the phase and
/// order it is inserted at, and how to build it over its input.
pub struct RendererFilter {
    /// The insertion phase (`HdSceneIndexPluginRegistry::InsertionPhase`):
    /// lower phases are applied first, nearer the input.
    pub phase: i32,
    pub order: InsertionOrder,
    pub build: Box<dyn Fn(SceneIndexHandle) -> SceneIndexHandle>,
}

impl RendererFilter {
    /// A renderer filter built by `build`.
    pub fn new(
        phase: i32,
        order: InsertionOrder,
        build: impl Fn(SceneIndexHandle) -> SceneIndexHandle + 'static,
    ) -> Self {
        Self {
            phase,
            order,
            build: Box::new(build),
        }
    }

    /// Light linking ([`LightLinkingSceneIndex`]) at phase 4, `AtStart`, as
    /// hdPrman and arnold-usd register it.
    pub fn light_linking() -> Self {
        Self::new(LIGHT_LINKING_PHASE, InsertionOrder::AtStart, |input| {
            LightLinkingSceneIndex::new(input)
        })
    }

    /// Dependency forwarding ([`DependencyForwardingSceneIndex`]) at phase
    /// 1000, `AtEnd`, as arnold-usd registers it.
    pub fn dependency_forwarding() -> Self {
        Self::new(DEPENDENCY_FORWARDING_PHASE, InsertionOrder::AtEnd, |input| {
            DependencyForwardingSceneIndex::new(input)
        })
    }
}

/// `filters` applied over `input` (the standard chain, or a merge of
/// several), in `HdSceneIndexPluginRegistry`'s order: ascending phase, then
/// `AtStart` before `AtEnd`, then list position (where Hydra sorts by
/// plugin id). Each filter wraps the result of the one before; the last is
/// the terminal scene index.
pub fn renderer_filters(input: SceneIndexHandle, filters: &[RendererFilter]) -> SceneIndexHandle {
    let mut ordered: Vec<&RendererFilter> = filters.iter().collect();
    ordered.sort_by_key(|f| (f.phase, f.order));
    ordered
        .into_iter()
        .fold(input, |index, filter| (filter.build)(index))
}

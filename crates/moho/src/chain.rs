//! The standard chain of scene indices over a stage scene index, and the
//! renderer filters a consumer applies over it, as a Hydra render index
//! applies a renderer's scene index plugins.
//!
//! A Hydra 2.0 renderer consumes Hydra in three steps, and Moho offers each:
//!
//! 1. the application builds UsdImaging's chain ([`standard`]), possibly
//!    merging several ([`MergingSceneIndex`](crate::filter::MergingSceneIndex));
//! 2. the render index applies the renderer's scene index plugins, ordered
//!    by `HdSceneIndexPluginRegistry` ([`renderer_filters`]): usdview adds
//!    the scene globals a host sets at phase 0
//!    ([`RendererFilter::scene_globals`]), hdPrman and arnold-usd add light
//!    linking at phase 4, arnold-usd dependency forwarding at phase 1000;
//! 3. the render delegate receives the terminal scene index, batches its
//!    notices by prim type
//!    ([`PrimTypeNoticeBatchingSceneIndex`](crate::filter::PrimTypeNoticeBatchingSceneIndex))
//!    and observes the batches with a prim factory
//!    ([`PrimManagingObserver`](crate::prim_managing::PrimManagingObserver)),
//!    flushing in its `Update`, as hdPrman's `renderDelegate.cpp` does.
//!
//! A renderer that needs a fix over Hydra's behaviour ships it as a renderer
//! filter of its own; the standard chain keeps UsdImaging's, quirks
//! included.
//!
//! # Example
//!
//! ```
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! use moho::chain::{self, RendererFilter};
//! use moho::filter::{PrimTypeNoticeBatchingSceneIndex, PrimTypePriority};
//! use moho::openusd::sdf;
//! use moho::openusd::tf::Token;
//! use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
//! use moho::scene_index::{Added, Dirtied};
//! use moho::stage::{StageSceneIndex, stage_builder};
//!
//! /// The renderer's priorities: materials before what binds them.
//! struct Priorities;
//!
//! impl PrimTypePriority for Priorities {
//!     fn priority(&self, prim_type: &Token) -> usize {
//!         usize::from(prim_type != "material")
//!     }
//!     fn num_priorities(&self) -> usize {
//!         2
//!     }
//! }
//!
//! /// The renderer's object for a mesh, which logs its updates.
//! struct Mesh(Rc<RefCell<Vec<String>>>);
//!
//! impl ManagedPrim for Mesh {
//!     fn dirty(&self, entry: &Dirtied, _: &PrimManagingObserver) {
//!         for locator in entry.locators.iter() {
//!             self.0.borrow_mut().push(format!("{} {locator}", entry.path));
//!         }
//!     }
//! }
//!
//! struct Factory(Rc<RefCell<Vec<String>>>);
//!
//! impl PrimFactory for Factory {
//!     fn create_prim(&self, entry: &Added, _: &PrimManagingObserver) -> Option<Rc<dyn ManagedPrim>> {
//!         (entry.ty == "mesh").then(|| Rc::new(Mesh(self.0.clone())) as Rc<dyn ManagedPrim>)
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let stage = stage_builder().in_memory("scene.usda")?;
//! stage.define_typed_prim("/world/geo", "Mesh")?;
//!
//! // 1. UsdImaging's chain.
//! let stage_index = StageSceneIndex::new();
//! stage_index.set_stage(stage.clone());
//! let chain = chain::standard(stage_index.clone());
//! // 2. The renderer's scene index plugins.
//! let terminal = chain::renderer_filters(
//!     chain,
//!     &[RendererFilter::light_linking(), RendererFilter::dependency_forwarding()],
//! );
//! // 3. The delegate: batched by prim type, observed with a prim factory.
//! let batching = PrimTypeNoticeBatchingSceneIndex::new(terminal, Some(Rc::new(Priorities)));
//! let log = Rc::new(RefCell::new(Vec::new()));
//! let observer = PrimManagingObserver::new(batching.clone(), Some(Rc::new(Factory(log.clone()))));
//!
//! // The delegate's `Update`.
//! batching.flush();
//! let geo = sdf::path("/world/geo")?;
//! assert!(observer.prim(&geo).is_some());
//!
//! // An edit reaches the mesh at the next update, at Hydra's locators.
//! stage.attribute("/world/geo.doubleSided")?.set(true)?;
//! stage_index.apply_pending_updates();
//! batching.flush();
//! assert!(log.borrow().iter().any(|l| l == "/world/geo mesh/doubleSided"));
//! # Ok(())
//! # }
//! ```

use std::rc::Rc;

use crate::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use crate::filter::{
    DependencyForwardingSceneIndex, InstanceProxyPathTranslationSceneIndex, LightLinkingSceneIndex,
    MaterialBindingResolvingSceneIndex, NativeInstancingSceneIndex, PointInstancingSceneIndex,
    RenderSettingsFlatteningSceneIndex, SceneGlobalsSceneIndex,
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
/// them as [`renderer_filters`]. Nor are the scene globals a host sets
/// (`sceneGlobals/currentFrame`): the host adds them as a renderer filter
/// ([`RendererFilter::scene_globals`]).
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

/// The phase usdview's `UsdImagingGLEngine` adds the scene globals filter
/// at, for every renderer.
pub const SCENE_GLOBALS_PHASE: i32 = 0;
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

    /// The scene globals a host sets ([`SceneGlobalsSceneIndex`]) at phase
    /// 0, `AtStart`, as usdview's `UsdImagingGLEngine` registers it for
    /// every renderer ("insert earlier so downstream scene indices can query
    /// and be notified of changes"). `on_built` receives the built index,
    /// for the host to drive (`set_current_frame` with the stage index's
    /// time, and so on), as the engine keeps it in its app scene indices.
    pub fn scene_globals(on_built: impl Fn(Rc<SceneGlobalsSceneIndex>) + 'static) -> Self {
        Self::new(SCENE_GLOBALS_PHASE, InsertionOrder::AtStart, move |input| {
            let index = SceneGlobalsSceneIndex::new(input);
            on_built(index.clone());
            index
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

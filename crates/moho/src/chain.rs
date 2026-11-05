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
//! 3. the renderer plugin creates its renderer over the terminal scene index
//!    ([`create_renderer`], `HdRendererPlugin::CreateRenderer`). A Hydra 2.0
//!    renderer batches the terminal's notices by prim type and observes the
//!    batches with a prim factory, flushing in its `Update`, as hdPrman's
//!    `renderDelegate.cpp` does
//!    ([`PrimManagingRenderer`](crate::renderer::PrimManagingRenderer)).
//!
//! [`create_renderer`] runs steps 2 and 3 in one call, with the host's
//! filters and the plugin's, as `UsdImagingGLEngine` does.
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
//! use moho::filter::PrimTypePriority;
//! use moho::openusd::sdf;
//! use moho::openusd::tf::Token;
//! use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
//! use moho::renderer::{PrimManagingRenderer, Renderer, RendererPlugin, Unsupported};
//! use moho::scene_index::{Added, Dirtied, SceneIndexHandle};
//! use moho::schema::RendererCreateArgsSchema;
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
//! /// The renderer plugin: its scene index plugins and its renderer.
//! struct Plugin(Rc<RefCell<Vec<String>>>);
//!
//! impl RendererPlugin for Plugin {
//!     type Renderer = PrimManagingRenderer;
//!
//!     fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
//!         Ok(())
//!     }
//!
//!     fn renderer_filters(&self) -> Vec<RendererFilter> {
//!         vec![RendererFilter::light_linking(), RendererFilter::dependency_forwarding()]
//!     }
//!
//!     fn create_renderer(
//!         &self,
//!         terminal: SceneIndexHandle,
//!         _: &RendererCreateArgsSchema,
//!     ) -> Option<PrimManagingRenderer> {
//!         let factory = Rc::new(Factory(self.0.clone()));
//!         Some(PrimManagingRenderer::new(terminal, Some(Rc::new(Priorities)), factory))
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
//! // 2. and 3. The host's filters and the plugin's, then the renderer.
//! let log = Rc::new(RefCell::new(Vec::new()));
//! let created = chain::create_renderer(
//!     chain,
//!     &[RendererFilter::scene_globals(|_| {})],
//!     &Plugin(log.clone()),
//!     &RendererCreateArgsSchema::build(false),
//! )?;
//! let renderer = created.renderer;
//!
//! // The renderer's update, before rendering.
//! renderer.update();
//! let geo = sdf::path("/world/geo")?;
//! assert!(renderer.prim(&geo).is_some());
//!
//! // An edit reaches the mesh at the next update, at Hydra's locators.
//! stage.attribute("/world/geo.doubleSided")?.set(true)?;
//! stage_index.apply_pending_updates();
//! renderer.update();
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
use crate::renderer::{CreateRendererError, CreatedRenderer, RendererPlugin};
use crate::scene_index::SceneIndexHandle;
use crate::schema::RendererCreateArgsSchema;
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
    apply_in_order(input, filters)
}

/// `filters` applied in renderer filter order, list position breaking ties.
fn apply_in_order<'a>(
    input: SceneIndexHandle,
    filters: impl IntoIterator<Item = &'a RendererFilter>,
) -> SceneIndexHandle {
    let mut ordered: Vec<&RendererFilter> = filters.into_iter().collect();
    ordered.sort_by_key(|f| (f.phase, f.order));
    ordered
        .into_iter()
        .fold(input, |index, filter| (filter.build)(index))
}

/// A renderer created by `plugin` over `input`, as
/// `UsdImagingGLEngine::_CreateSceneIndexChainAndRenderer`.
///
/// Support is checked first (`HdRendererPlugin::IsSupported`), before any
/// filter is built. Then the host's filters and the plugin's
/// ([`RendererPlugin::renderer_filters`]) are applied over `input` in one
/// [`renderer_filters`] order, as `HdSceneIndexPluginRegistry` orders the
/// plugins registered for every renderer with the renderer's own; at an
/// equal phase and order, the host's come first. The plugin then creates its
/// renderer over the result, which is returned with it as the terminal scene
/// index.
///
/// `input` is the standard chain or a merge of several: the host keeps its
/// stage index and drives its time and pending updates.
pub fn create_renderer<P: RendererPlugin>(
    input: SceneIndexHandle,
    host_filters: &[RendererFilter],
    plugin: &P,
    args: &RendererCreateArgsSchema,
) -> Result<CreatedRenderer<P::Renderer>, CreateRendererError> {
    plugin.is_supported(args)?;
    let plugin_filters = plugin.renderer_filters();
    let terminal = apply_in_order(input, host_filters.iter().chain(&plugin_filters));
    let renderer = plugin
        .create_renderer(terminal.clone(), args)
        .ok_or(CreateRendererError::Declined)?;
    Ok(CreatedRenderer { terminal, renderer })
}

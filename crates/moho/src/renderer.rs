//! Renderers and renderer plugins: the consumer side of a terminal scene
//! index, in Hydra 2.0's shape.
//!
//! - [`Renderer`] is `HdRenderer`: what a plugin creates over the terminal
//!   scene index, and what the host holds and updates. Its one method,
//!   [`update`](Renderer::update), is the Hydra 2.0 hook
//!   `HdRenderDelegate::Update`, called before rendering to process what
//!   the terminal index notified since the last update (`HdRenderer` itself
//!   is a stub at the reference revision).
//! - [`RendererPlugin`] is `HdRendererPlugin`'s Hydra 2.0 API: whether it is
//!   supported for given create args (`IsSupported`, with the reason why
//!   not), the renderer's scene index plugins (which Hydra registers per
//!   renderer in `HdSceneIndexPluginRegistry`), and the renderer over a
//!   terminal index (`CreateRenderer`).
//!
//! [`chain::create_renderer`](crate::chain::create_renderer) ties them to a
//! chain, as `UsdImagingGLEngine::_CreateSceneIndexChainAndRenderer` does.
//!
//! A plugin names its renderer's concrete type: Moho has no plugin registry
//! and loads nothing at run time, so the host knows its plugin and calls its
//! renderer's own API (rendering, outputs, settings) directly. Hydra's render
//! control API (`HdLegacyRenderControlInterface`, which `HdRenderer` is to
//! replace) is not ported: Hydra has not settled it.
//!
//! Like the scene indices it observes, a renderer stays on the thread that
//! owns them.

use std::fmt;
use std::rc::Rc;

use openusd::sdf;

use crate::chain::RendererFilter;
use crate::filter::{PrimTypeNoticeBatchingSceneIndex, PrimTypePriority};
use crate::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
use crate::scene_index::SceneIndexHandle;
use crate::schema::RendererCreateArgsSchema;

/// A renderer over a terminal scene index (`HdRenderer`).
///
/// Dropping it releases it, as destroying Hydra's renderer handle does.
pub trait Renderer {
    /// Processes what the terminal index notified since the last update
    /// (`HdRenderDelegate::Update`). The host calls it before rendering.
    fn update(&self);
}

/// Why a plugin is not supported (`IsSupported`'s `reasonWhyNot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub reason: String,
}

impl Unsupported {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "renderer not supported: {}", self.reason)
    }
}

impl std::error::Error for Unsupported {}

/// A renderer plugin (`HdRendererPlugin`'s Hydra 2.0 API).
pub trait RendererPlugin {
    /// The renderer it creates.
    type Renderer: Renderer;

    /// Whether the plugin is supported for `args`, and why not
    /// (`IsSupported`).
    fn is_supported(&self, args: &RendererCreateArgsSchema) -> Result<(), Unsupported>;

    /// The renderer's scene index plugins, applied over the host's input
    /// with the host's own. A fresh list on each call: a filter's
    /// constructor is not shared.
    fn renderer_filters(&self) -> Vec<RendererFilter> {
        Vec::new()
    }

    /// The renderer over `terminal`, or `None` when the plugin declines
    /// (`CreateRenderer`). Called only once
    /// [`is_supported`](Self::is_supported) has answered yes.
    fn create_renderer(
        &self,
        terminal: SceneIndexHandle,
        args: &RendererCreateArgsSchema,
    ) -> Option<Self::Renderer>;
}

/// A renderer with the terminal scene index it was created over, as
/// `UsdImagingGLEngine` keeps `_terminalSceneIndex` beside `_renderer`.
pub struct CreatedRenderer<R> {
    /// The scene index the renderer observes: the input with the host's and
    /// the plugin's renderer filters applied.
    pub terminal: SceneIndexHandle,
    pub renderer: R,
}

/// Why [`create_renderer`](crate::chain::create_renderer) gave no renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateRendererError {
    /// The plugin is not supported for the create args; no filter was
    /// built.
    Unsupported(Unsupported),
    /// The plugin is supported but created no renderer (a null
    /// `_CreateRenderer`).
    Declined,
}

impl From<Unsupported> for CreateRendererError {
    fn from(unsupported: Unsupported) -> Self {
        Self::Unsupported(unsupported)
    }
}

impl fmt::Display for CreateRendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(unsupported) => unsupported.fmt(f),
            Self::Declined => f.write_str("the renderer plugin created no renderer"),
        }
    }
}

impl std::error::Error for CreateRendererError {}

/// A renderer managing one prim per prim of its terminal index, as hdPrman's
/// `_RileySceneIndices`: the terminal index wrapped in a
/// [`PrimTypeNoticeBatchingSceneIndex`] under the consumer's priorities,
/// observed by a [`PrimManagingObserver`] with the consumer's factory.
///
/// Every notice is held until [`update`](Renderer::update), which flushes
/// the batching (hdPrman's `Update`). The batching index is empty until its
/// first flush, so the renderer manages no prim before its first update, and
/// that update makes every prim in priority order (say, materials before
/// the geometry bound to them).
///
/// A plugin returns one from
/// [`create_renderer`](RendererPlugin::create_renderer), or wraps one in a
/// renderer of its own that adds its rendering API.
pub struct PrimManagingRenderer {
    batching: Rc<PrimTypeNoticeBatchingSceneIndex>,
    observer: Rc<PrimManagingObserver>,
}

impl PrimManagingRenderer {
    /// A renderer over `terminal`, batching its notices by `priority` (all
    /// prim types alike without one) and making its prims with `factory`.
    pub fn new(
        terminal: SceneIndexHandle,
        priority: Option<Rc<dyn PrimTypePriority>>,
        factory: Rc<dyn PrimFactory>,
    ) -> Self {
        let batching = PrimTypeNoticeBatchingSceneIndex::new(terminal, priority);
        let observer = PrimManagingObserver::new(batching.clone(), Some(factory));
        Self { batching, observer }
    }

    /// The observer holding the managed prims.
    pub fn observer(&self) -> &Rc<PrimManagingObserver> {
        &self.observer
    }

    /// The managed prim at `path`, if any.
    pub fn prim(&self, path: &sdf::Path) -> Option<Rc<dyn ManagedPrim>> {
        self.observer.prim(path)
    }
}

impl Renderer for PrimManagingRenderer {
    fn update(&self) {
        self.batching.flush();
    }
}

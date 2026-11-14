//! Render control over a Moho terminal scene index: what a renderer can do,
//! what it renders, starting, stopping and pausing it, reading its images
//! from any thread, and observing its progress as a scene index.
//!
//! # Ahead of Hydra
//!
//! Hydra 2.0's `HdRenderer` is a stub: its only method returns
//! `HdLegacyRenderControlInterface`, which Hydra plans to replace. `moho`
//! follows Hydra and drops what Hydra lacks, so its
//! [`Renderer`](moho::renderer::Renderer) has one method, `update`. This
//! crate drafts the control side ahead of the reference, outside `moho`, so
//! `moho` stays at Hydra parity. When Hydra settles its own API, this crate
//! is reconciled with it.
//!
//! # Model
//!
//! - **What to render is scene data.** A render reads the active render
//!   settings prim that the scene globals name, with its products and vars
//!   ([`targets`]). A [`RenderRequest`] carries only per-call overrides.
//!   Host overrides of renderer settings are a filter
//!   ([`SettingsOverrideSceneIndex`]), not a settings map.
//! - **What the renderer reports is a scene index.** Its state, progress,
//!   stats and capabilities, and one prim per product and per var, are an
//!   [`OutputSceneIndex`], observed like any input index. Pixels are not in
//!   it: a var's `renderBuffer/generation` is dirtied for each new
//!   snapshot, and the host reads the [`Frame`] through the control handle.
//! - **Two thread worlds.** A [`ControlledRenderer`] stays on the thread
//!   that owns its scene indices, as every Moho index does. Its
//!   [`ControlHandle`] is `Send + Sync`: stop, pause, wait and frame reads
//!   work from any thread. Render threads queue their changes, and the owning
//!   thread sends the output notices when it calls
//!   [`poll`](ControlledRenderer::poll) or `update`.
//! - **The renderer decides what an edit costs.** After updates it reports
//!   an [`Invalidation`]: keep converging, restart, or recreate it.
//!
//! # The host loop
//!
//! Create the renderer over a chain (`moho::chain::create_renderer`), then
//! on the owning thread: call `update` once per tick (it processes the
//! scene's notices and sends the output notices), `render` when
//! [`invalidation`](ControlledRenderer::invalidation) says the image is
//! stale, and read a var's frame from the [`ControlHandle`] when the output
//! index dirties its `renderBuffer/generation`. A batch host waits for
//! convergence with [`ControlHandle::wait`] from any thread.
//!
//! The `testing` feature adds `moho_render::testing`: a renderer meeting
//! every requirement, whose pixels tests can predict. Its module
//! documentation runs this loop as a tested example
//! (`cargo test -p moho-render --features testing --doc`).

#![forbid(unsafe_code)]

mod capabilities;
mod control;
mod output;
mod renderer;
mod settings_override;
pub mod targets;
pub mod tokens;
mod types;

pub use capabilities::{AovDescriptor, Capabilities, CommandDescriptor, SettingDescriptor, SettingType};
pub use control::{Changes, Control, ControlHandle, Shared};
pub use output::OutputSceneIndex;
pub use renderer::{ControlledRenderer, InvalidationTracker, TrackedNotice, restart_on_any_notice};
pub use settings_override::SettingsOverrideSceneIndex;
pub use targets::{ProductTarget, RenderTargets, VarTarget};
pub use types::{
    Convergence, Frame, Invalidation, PixelFormat, PixelRect, ProductWriter, RenderError, RenderRequest,
    RunState,
};

#[cfg(feature = "testing")]
pub mod testing;

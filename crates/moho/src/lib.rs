//! Moho: a renderer-agnostic scene index over openusd stages.
//!
//! Moho reads a composed USD stage for renderers, in the model of Hydra 2.0:
//! scene indices hand out prims as trees of lazily evaluated data sources,
//! observers learn which prims were added, removed or dirtied (down to the
//! data locator), and a renderer pulls only what changed. The index owns the
//! time: sampled data sources resolve relative to it, and changing it dirties
//! exactly what varies over time.
//!
//! Moho depends on openusd alone. It knows nothing of Mantle (the DCC that
//! feeds it edits) or of Crust and Obsidian (the renderers that consume it).
//!
//! See `docs/architecture.md` for the design and `docs/openusd-notes.md` for
//! the `openusd` APIs it rests on.

#![forbid(unsafe_code)]

pub mod chain;
pub mod data_source;
pub mod filter;
pub mod locator;
pub mod scene_index;
pub mod schema;
pub mod stage;
pub mod tokens;

/// The pinned openusd, re-exported so consumers build against exactly the
/// revision Moho does.
pub use openusd;
/// The pinned openusd-schemas, from the same openusd revision.
pub use openusd_schemas;

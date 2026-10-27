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
pub mod collection;
pub mod data_source;
pub mod filter;
pub mod locator;
pub mod scene_index;
pub mod schema;
pub mod stage;
pub mod tokens;

/// The pinned openusd, re-exported so consumers build against exactly the
/// revision Moho does. Moho's consumers (Mantle, crust-render, Obsidian)
/// take openusd only through this re-export, never as a dependency of their
/// own, so bumping Moho bumps openusd for the whole stack. Moho's features
/// forward openusd's (`mmap`, `serde`).
pub use openusd;
/// The pinned openusd-schemas, from the same openusd revision. The families
/// Moho reads (`geom`, `lux`, `render`, `shade`, `vol`) are always on; the
/// others are Moho features of the same name (`media`, `physics`, `proc`,
/// `semantics`, `skel`, `ui`).
pub use openusd_schemas;

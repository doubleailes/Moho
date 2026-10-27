//! Moho: a renderer-agnostic scene index over openusd stages.
//!
//! Moho reads a composed USD stage for renderers, in the model of Hydra 2.0:
//! scene indices hand out prims as trees of lazily evaluated data sources,
//! observers learn which prims were added, removed or dirtied (down to the
//! data locator), and a renderer pulls only what changed. The index owns the
//! time: sampled data sources resolve relative to it, and changing it dirties
//! exactly what varies over time.
//!
//! Moho follows Hydra 2.0 and UsdImaging at their reference revision: the
//! standard chain is `UsdImagingCreateSceneIndices`', and prim paths, schema
//! and token names, data layout and dirty locators are Hydra's, quirks
//! included. Its one addition is the generic attribute data
//! (`__usdAttributes`, `__usdRelationships`). A renderer adds its own scene
//! index plugins as renderer filters (`chain::renderer_filters`) and
//! consumes the terminal scene index as a Hydra 2.0 render delegate does
//! (`chain`'s docs).
//!
//! Moho depends on openusd alone, and knows nothing of what feeds the stage
//! or of the renderers that consume the index.
//!
//! See `docs/architecture.md` for the design and `docs/openusd-notes.md` for
//! the `openusd` APIs it rests on.

#![forbid(unsafe_code)]

pub mod chain;
pub mod collection;
pub mod data_source;
pub mod filter;
pub mod locator;
pub mod prim_managing;
pub mod scene_index;
pub mod schema;
pub mod stage;
mod tf_hash;
pub mod tokens;

/// The pinned openusd, re-exported so consumers build against exactly the
/// revision Moho does. A consumer that takes openusd only through this
/// re-export, never as a dependency of its own, gets one openusd in every
/// build, and bumping Moho bumps it. Moho's features forward openusd's
/// (`mmap`, `serde`). This is a downstream policy: were Moho to move into
/// openusd's workspace (as `openusd-imaging`), the re-export would move to a
/// thin crate of its own.
pub use openusd;
/// The pinned openusd-schemas, from the same openusd revision. The families
/// Moho reads (`geom`, `lux`, `render`, `shade`, `vol`) are always on; the
/// others are Moho features of the same name (`media`, `physics`, `proc`,
/// `semantics`, `skel`, `ui`).
pub use openusd_schemas;

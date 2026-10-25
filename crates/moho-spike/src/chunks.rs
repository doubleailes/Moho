//! Crust's chunk rule (design D1), copied from crust-render `68560ae`,
//! `usd_import/mod.rs` (`subtree_roots`, `stream_roots`, `MIN_STREAM_CHUNKS`),
//! so that strategy (B) reproduces Crust's import and (A) is compared like for
//! like.

use openusd::sdf;
use openusd::usd::Stage;

use crate::Result;

/// Below this many chunks every strategy degenerates to one stage, as Crust's
/// import does: a masked open re-composes the root layer, which only pays off
/// when the payloads it excludes are the bulk of the cost.
pub const MIN_STREAM_CHUNKS: usize = 4;

/// The stage's top-level subtrees: the children of its one root prim, or its
/// root prims when it has several (Crust's `subtree_roots`).
pub fn subtree_roots(stage: &Stage) -> Result<Vec<sdf::Path>> {
    let top = stage.prim(sdf::Path::abs_root())?.children()?;
    let roots = match top.as_slice() {
        [only] => only.children()?.iter().map(|c| c.path().clone()).collect(),
        many => many.iter().map(|p| p.path().clone()).collect(),
    };
    Ok(roots)
}

/// The chunks to stream, or none when there are fewer than
/// [`MIN_STREAM_CHUNKS`] (Crust's `stream_roots`, without its
/// `CRUST_STREAM_IMPORT` escape hatch: strategy (C) is that).
pub fn stream_roots(stage: &Stage) -> Result<Vec<sdf::Path>> {
    let roots = subtree_roots(stage)?;
    Ok(if roots.len() < MIN_STREAM_CHUNKS {
        Vec::new()
    } else {
        roots
    })
}

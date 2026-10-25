//! Stable identities for native-instance prototypes.
//!
//! openusd numbers prototypes per composition (`/__Prototype_N`), so the same
//! prototype gets a different path on each stage and after each load change.
//! Moho names a prototype by a hash of what composes it instead: the sites of
//! the instance's prim stack outside its own local opinions (layer
//! identifier, spec path, layer offset), which are the arcs instances share,
//! and its variant selections. Two stages composing the same content agree on
//! it.
//!
//! Workarounds, both upstream candidates in `ROADMAP.md`:
//! - openusd's own instancing key (`pcp::instancing::InstanceKey`) is
//!   crate-private, and keys layer stacks by a stage-local id, so it can be
//!   neither read nor compared across stages;
//! - the prim stack of a `/__Prototype_N` prim is empty at `68a867a` (C++
//!   returns its specs), so the instance's stack stands in for it.
//!
//! The key's load rules and population mask, re-rooted on the instance, are
//! left out: they agree between a chunk stage and the full stage for any prim
//! inside the chunk.

use std::hash::{DefaultHasher, Hash, Hasher};

use openusd::sdf;
use openusd::usd::Stage;

/// The stable identity of the prototype the instance at `instance` shares,
/// on `stage`.
pub fn prototype_identity(stage: &Stage, instance: &sdf::Path) -> Option<u64> {
    let prim = stage.prim(instance).ok()?;
    if !prim.is_instance().ok()? {
        return None;
    }
    let local_layers = stage.layer_stack();
    // The instance's own opinions, in the stage's root layer stack at its own
    // path, are not shared with the prototype.
    let shared: Vec<_> = prim
        .prim_stack()
        .ok()?
        .into_iter()
        .filter(|site| !(site.path == *instance && local_layers.contains(&site.layer)))
        .collect();
    if shared.is_empty() {
        return None;
    }
    let mut h = DefaultHasher::new();
    for site in &shared {
        site.layer.hash(&mut h);
        site.path.as_str().hash(&mut h);
        site.offset.offset.to_bits().hash(&mut h);
        site.offset.scale.to_bits().hash(&mut h);
    }
    prim.prim_index()
        .variant_selections()
        .unwrap_or_default()
        .hash(&mut h);
    Some(h.finish())
}

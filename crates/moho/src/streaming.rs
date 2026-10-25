//! Helpers for a streamed scene: a [`MergingSceneIndex`] of
//! [`chain::input`](crate::chain::input) chains over a `LoadNone` scene input
//! and masked chunk inputs, with [`chain::merged`](crate::chain::merged) over
//! it (`docs/architecture.md`, "Streaming").
//!
//! A chunk's prims can depend on prims no loaded input holds: a material
//! bound from inside another chunk's payload, or a network node outside the
//! material. Moho never chooses what is loaded. [`unresolved`] lists those
//! paths; the consumer opens a stage masked to them, payloads loaded, and
//! adds a side input over it with `chain::input`, weaker than the chunks and
//! stronger than the scene input, until nothing is unresolved.
//!
//! [`MergingSceneIndex`]: crate::filter::MergingSceneIndex

use std::collections::BTreeSet;

use openusd::sdf::{self, Value};

use crate::data_source::{ContainerHandle, locate};
use crate::locator::Locator;
use crate::scene_index::SceneIndex;
use crate::schema::member_container;
use crate::tokens::{material, material_bindings};

/// The paths `index`'s prims depend on that `index` does not hold: each
/// material a prim's resolved binding names, and each node of a held
/// material's network named by an absolute path (a node outside the
/// material).
pub fn unresolved(index: &dyn SceneIndex) -> BTreeSet<sdf::Path> {
    let held = |path: &sdf::Path| index.prim(path).is_defined();
    let mut missing = BTreeSet::new();
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        if let Some(data) = index.prim(&path).data {
            for bound in bound_materials(&data) {
                if !held(&bound) {
                    missing.insert(bound);
                }
            }
            for node in outside_nodes(&data) {
                if !held(&node) {
                    missing.insert(node);
                }
            }
        }
        stack.extend(index.children(&path));
    }
    missing
}

/// The materials `materialBindings` names, for every purpose.
fn bound_materials(data: &ContainerHandle) -> Vec<sdf::Path> {
    let Some(bindings) = member_container(data, material_bindings::MATERIAL_BINDINGS) else {
        return Vec::new();
    };
    bindings
        .names()
        .iter()
        .filter_map(|purpose| {
            match locate(
                &bindings,
                &Locator::new([purpose.as_str(), material_bindings::PATH]),
            )?
            .value()?
            {
                Value::PathVec(mut v) if v.len() == 1 => v.pop(),
                _ => None,
            }
        })
        .collect()
}

/// The nodes of a material's networks named by an absolute path: the
/// material adapter names a node inside the material by its relative path,
/// and one outside it by its stage path.
fn outside_nodes(data: &ContainerHandle) -> Vec<sdf::Path> {
    let Some(networks) = member_container(data, material::MATERIAL) else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    for context in networks.names() {
        let Some(network) = networks.get(&context).and_then(|d| d.as_container().cloned()) else {
            continue;
        };
        let Some(names) = member_container(&network, material::NODES).map(|n| n.names()) else {
            continue;
        };
        nodes.extend(
            names
                .iter()
                .filter(|n| n.as_str().starts_with('/'))
                .filter_map(|n| sdf::Path::new(n.as_str()).ok()),
        );
    }
    nodes
}

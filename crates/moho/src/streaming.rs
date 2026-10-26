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
//! stronger than the scene input, until a round resolves nothing new.
//!
//! A connection to a prim that exists nowhere looks, on a chunk's masked
//! stage, like one to a prim outside the mask: it is listed too, and a side
//! input masked to it holds nothing. So the consumer stops when a round adds
//! no prim, not when the list is empty. (On a fully loaded stage, a broken
//! connection is dropped, as openusd's producer walk drops it.)
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

/// The nodes of a material's networks named by an absolute path, and the
/// absolute upstream nodes their connections and terminals name: the
/// material adapter names a node inside the material by its relative path,
/// one outside it by its stage path, and keeps a connection to a node its
/// stage does not hold (outside a chunk's mask) by that path.
fn outside_nodes(data: &ContainerHandle) -> Vec<sdf::Path> {
    let Some(networks) = member_container(data, material::MATERIAL) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    let upstream = |connection: &crate::data_source::DataSource| -> Option<String> {
        match locate(
            connection.as_container()?,
            &Locator::new([material::UPSTREAM_NODE_PATH]),
        )?
        .value()?
        {
            Value::Token(t) => Some(t.as_str().to_owned()),
            _ => None,
        }
    };
    for context in networks.names() {
        let Some(network) = networks.get(&context).and_then(|d| d.as_container().cloned()) else {
            continue;
        };
        if let Some(nodes) = member_container(&network, material::NODES) {
            for name in nodes.names() {
                names.push(name.as_str().to_owned());
                let connections = nodes
                    .get(&name)
                    .and_then(|n| n.as_container().cloned())
                    .and_then(|n| member_container(&n, material::INPUT_CONNECTIONS));
                for links in connections.iter().flat_map(|c| {
                    c.names()
                        .into_iter()
                        .filter_map(|i| c.get(&i).and_then(|d| d.as_vector().cloned()))
                        .collect::<Vec<_>>()
                }) {
                    names.extend((0..links.len()).filter_map(|i| links.get(i).and_then(|l| upstream(&l))));
                }
            }
        }
        if let Some(terminals) = member_container(&network, material::TERMINALS) {
            names.extend(
                terminals
                    .names()
                    .iter()
                    .filter_map(|t| terminals.get(t).and_then(|d| upstream(&d))),
            );
        }
    }
    names
        .iter()
        .filter(|n| n.starts_with('/'))
        .filter_map(|n| sdf::Path::new(n).ok())
        .collect()
}

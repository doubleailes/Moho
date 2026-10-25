//! Stable identities for native-instance prototypes.
//!
//! openusd numbers prototypes per composition (`/__Prototype_N`), so the same
//! prototype gets a different path on each stage and after each load change.
//! Moho names a prototype by a hash of openusd's own instancing key instead,
//! with each layer stack named by its layers' identifiers rather than by a
//! stage-local id. Two stages composing the same content agree on it.
//!
//! The key follows `pcp::instancing::instance_key`: the instance's
//! instanceable prim-index nodes (arc type, layer stack, site path, time
//! offset), walked from the root and not below an instanceable node unless
//! the index has a payload, plus the variant selections. A node is
//! instanceable when it is not instance-local, not inert or culled, and
//! authors specs; instance-local nodes are the root and the nodes introduced
//! above the instance whose parent is instance-local
//! (`PrimIndex::instance_local_nodes`, C++
//! `!PcpNodeRef::HasTransitiveDirectDependency`).
//!
//! Workaround, an upstream candidate in `ROADMAP.md`: openusd's
//! `InstanceKey` and `instanceable_nodes` are crate-private, and the key
//! names layer stacks by a stage-local `LayerStackId`, so Moho rebuilds the
//! key from the public prim-index graph. The key's load rules and
//! population mask, re-rooted on the instance, are left out: they agree
//! between a chunk stage and the full stage for any prim inside the chunk.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use openusd::pcp::{ArcType, NodeId, PrimIndex};
use openusd::sdf;
use openusd::usd::Stage;

/// Whether `id` is instance-local, memoised in `memo`.
fn local(index: &PrimIndex, id: NodeId, memo: &mut HashMap<NodeId, bool>) -> bool {
    if let Some(&known) = memo.get(&id) {
        return known;
    }
    let node = index.node(id);
    let result = match node.arc() {
        ArcType::Root => true,
        _ => match node.parent() {
            Some(parent) => {
                // How far below its introduction the node's site sits: an
                // arc introduced above the instance leaves a positive depth.
                let parent_depth = index.node(parent).path().prim_element_count();
                let below_introduction = (parent_depth as u16).saturating_sub(node.namespace_depth());
                local(index, parent, memo) && below_introduction > 0
            }
            None => true,
        },
    };
    memo.insert(id, result);
    result
}

/// The stable identity of the prototype the instance at `instance` shares,
/// on `stage`.
pub fn prototype_identity(stage: &Stage, instance: &sdf::Path) -> Option<u64> {
    let prim = stage.prim(instance).ok()?;
    if !prim.is_instance().ok()? {
        return None;
    }
    let index = prim.prim_index().graph().ok()?;
    let root = index.root()?;
    let has_payload = index.all_nodes().any(|n| n.arc() == ArcType::Payload);
    let mut memo = HashMap::new();
    let mut h = DefaultHasher::new();
    let mut arcs = 0usize;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = index.node(id);
        if node.is_culled() {
            continue;
        }
        let instanceable = !local(&index, id, &mut memo) && !node.is_inert() && node.has_specs();
        if instanceable {
            arcs += 1;
            (node.arc() as u8).hash(&mut h);
            for (layer, offset) in stage.node_layer_stack(node).unwrap_or_default() {
                stage.layer_identifier(layer).hash(&mut h);
                offset.offset.to_bits().hash(&mut h);
                offset.scale.to_bits().hash(&mut h);
            }
            node.path().as_str().hash(&mut h);
            let time = node.map_to_root().time_offset();
            time.offset.to_bits().hash(&mut h);
            time.scale.to_bits().hash(&mut h);
            if !has_payload {
                continue;
            }
        }
        // Children in reverse, so the stack pops them strongest first.
        let children: Vec<NodeId> = index.children(id).collect();
        stack.extend(children.into_iter().rev());
    }
    if arcs == 0 {
        return None;
    }
    prim.prim_index()
        .variant_selections()
        .unwrap_or_default()
        .hash(&mut h);
    Some(h.finish())
}

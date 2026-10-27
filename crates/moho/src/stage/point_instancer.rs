//! The `instancerTopology` of `PointInstancer` prims, as UsdImaging's
//! `UsdImagingDataSourcePointInstancerTopology`:
//!
//! - `prototypes`: the prototypes relationship's forwarded targets; when the
//!   prim authors `inactiveIds`, its (unforwarded) targets without the
//!   prototypes whose every instance is inactive;
//! - `instanceIndices`: `protoIndices` flipped, one array per prototype index
//!   up to the largest one used, without the fully inactive prototypes'
//!   entries; inactive instances stay;
//! - `mask`: USD's `ComputeMaskAtTime`, `false` for inactive and invisible
//!   ids, empty when nothing is masked.
//!
//! The instance positions, rotations and scales reach `primvars` as
//! `hydra:instance*`.

use std::collections::{BTreeMap, BTreeSet};

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd::TimeCode;
use openusd_schemas::geom::{PointInstancer, PrimMetadata};

use crate::data_source::DataSource;
use crate::data_source::retained::value;
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::instancing as t;
use crate::tokens::prim_type;

/// The adapter of `PointInstancer` prims.
pub struct PointInstancerAdapter;

fn ints(value: Option<Value>) -> Vec<i64> {
    match value {
        Some(Value::IntVec(v)) => v.into_iter().map(i64::from).collect(),
        Some(Value::Int64Vec(v)) => v,
        _ => Vec::new(),
    }
}

/// Which of `count` prototypes have every one of their instances inactive
/// (UsdImaging's `_ComputeFullyInactiveProtoMask`), or `None` when none is.
///
/// As UsdImaging, the inactive ids are the explicit items of the composed
/// `inactiveIds` list op. openusd composes metadata to the strongest
/// opinion, where USD composes list ops across layers.
///
/// UsdImaging counts into arrays of `count` entries; Moho counts only the
/// prototype indices used, as `count` follows the largest `protoIndices`
/// value for `instanceIndices`, which a stray index makes huge.
fn fully_inactive(
    prim: &openusd::usd::Prim,
    proto_indices: &[i64],
    ids: &[i64],
    count: usize,
) -> Option<BTreeSet<usize>> {
    let inactive: BTreeSet<i64> = match prim.inactive_ids().ok()?? {
        Value::Int64ListOp(op) => op.explicit_items().iter().copied().collect(),
        _ => return None,
    };
    if inactive.is_empty() {
        return None;
    }
    // Prototype index to (instances, inactive instances).
    let mut counts: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    for (i, &k) in proto_indices.iter().enumerate() {
        let Some(k) = usize::try_from(k).ok().filter(|k| *k < count) else {
            continue;
        };
        let id = if ids.is_empty() {
            i64::try_from(i).unwrap_or(i64::MAX)
        } else {
            ids.get(i).copied().unwrap_or(i64::MAX)
        };
        let entry = counts.entry(k).or_default();
        entry.0 += 1;
        if inactive.contains(&id) {
            entry.1 += 1;
        }
    }
    let pruned: BTreeSet<usize> = counts
        .into_iter()
        .filter(|(_, (total, inactive))| total == inactive)
        .map(|(k, _)| k)
        .collect();
    (!pruned.is_empty()).then_some(pruned)
}

/// `instanceIndices`: one array per prototype index up to the largest one
/// `protoIndices` uses, empty for the indices it skips, without the
/// `pruned` ones.
///
/// UsdImaging allocates every array (`resize(protoIndex + 1)`), so a stray
/// huge index (2 000 000 000) exhausts memory. Moho keeps only the arrays
/// used and answers the skipped ones empty, which reads the same.
struct InstanceIndices {
    /// Prototype index to its instances, for the indices used.
    used: BTreeMap<usize, Vec<i32>>,
    /// The largest index used, plus one.
    count: usize,
    /// The fully inactive prototype indices, left out.
    pruned: BTreeSet<usize>,
}

impl crate::data_source::Vector for InstanceIndices {
    fn len(&self) -> usize {
        self.count - self.pruned.len()
    }

    fn get(&self, index: usize) -> Option<DataSource> {
        if index >= self.len() {
            return None;
        }
        // The `index`-th prototype index that is not pruned.
        let mut k = index;
        for &q in &self.pruned {
            if q > k {
                break;
            }
            k += 1;
        }
        Some(value(Value::IntVec(
            self.used.get(&k).cloned().unwrap_or_default(),
        )))
    }
}

fn topology(ctx: &PrimContext, name: &str) -> Option<DataSource> {
    let prim = ctx.prim()?;
    let time = ctx.globals.time().map(|t| TimeCode::new(t.value()));
    let read = |name: &str| prim.attribute(name).get_at::<Value>(time).ok().flatten();
    let has_inactive = prim.inactive_ids().ok().flatten().is_some();
    match name {
        t::PROTOTYPES => {
            let relationship = prim.relationship("prototypes");
            if !has_inactive {
                let targets = relationship.forwarded_targets().unwrap_or_default();
                return Some(value(Value::PathVec(targets)));
            }
            let targets = relationship.targets().unwrap_or_default();
            let proto_indices = ints(read("protoIndices"));
            let ids = ints(read("ids"));
            let kept = match fully_inactive(&prim, &proto_indices, &ids, targets.len()) {
                None => relationship.forwarded_targets().unwrap_or_default(),
                Some(pruned) => targets
                    .into_iter()
                    .enumerate()
                    .filter(|(k, _)| !pruned.contains(k))
                    .map(|(_, target)| target)
                    .collect(),
            };
            Some(value(Value::PathVec(kept)))
        }
        t::INSTANCE_INDICES => {
            if prim
                .attribute("protoIndices")
                .might_be_time_varying()
                .unwrap_or(false)
            {
                ctx.globals.flag_time_varying(
                    &ctx.path,
                    Locator::new([t::INSTANCER_TOPOLOGY, t::INSTANCE_INDICES]),
                );
            }
            let proto_indices = ints(read("protoIndices"));
            // [0, 1, 0] gives 0: [0, 2], 1: [1].
            let mut used: BTreeMap<usize, Vec<i32>> = BTreeMap::new();
            for (i, &k) in proto_indices.iter().enumerate() {
                let (Ok(k), Ok(i)) = (usize::try_from(k), i32::try_from(i)) else {
                    continue;
                };
                used.entry(k).or_default().push(i);
            }
            let count = used.last_key_value().map_or(0, |(k, _)| k.saturating_add(1));
            let ids = ints(read("ids"));
            let pruned = fully_inactive(&prim, &proto_indices, &ids, count).unwrap_or_default();
            Some(DataSource::Vector(std::rc::Rc::new(InstanceIndices {
                used,
                count,
                pruned,
            })))
        }
        t::MASK => {
            if prim
                .attribute("invisibleIds")
                .might_be_time_varying()
                .unwrap_or(false)
            {
                ctx.globals
                    .flag_time_varying(&ctx.path, Locator::new([t::INSTANCER_TOPOLOGY, t::MASK]));
            }
            let mask = PointInstancer::get(&ctx.stage, ctx.path.clone())
                .ok()
                .flatten()
                .and_then(|pi| pi.mask_at(time, None).ok())
                .unwrap_or_default();
            Some(value(Value::BoolVec(mask)))
        }
        _ => None,
    }
}

/// `instancerTopology`, each member read when asked for.
struct Topology(PrimContext);

impl crate::data_source::Container for Topology {
    fn names(&self) -> Vec<Token> {
        [t::PROTOTYPES, t::INSTANCE_INDICES, t::MASK]
            .map(Token::from)
            .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        topology(&self.0, name.as_str())
    }
}

impl Adapter for PointInstancerAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::INSTANCER)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::INSTANCER_TOPOLOGY)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name == t::INSTANCER_TOPOLOGY {
            Some(DataSource::Container(std::rc::Rc::new(Topology(ctx.clone()))))
        } else {
            None
        }
    }

    /// As `UsdImagingDataSourcePointInstancerPrim::Invalidate`: `prototypes`
    /// dirties the whole topology, `protoIndices` its `instanceIndices`,
    /// `invisibleIds` its `mask`. An `ids` edit dirties nothing, as in
    /// UsdImaging. `inactiveIds` is metadata, whose edit resyncs the prim in
    /// UsdImaging (a plugin field); Moho ignores prim-level info-only
    /// changes until openusd reports their fields (`stage/pending.rs`).
    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        match name {
            "prototypes" => LocatorSet::from(Locator::new([t::INSTANCER_TOPOLOGY])),
            "protoIndices" => LocatorSet::from(Locator::new([t::INSTANCER_TOPOLOGY, t::INSTANCE_INDICES])),
            "invisibleIds" => LocatorSet::from(Locator::new([t::INSTANCER_TOPOLOGY, t::MASK])),
            _ => LocatorSet::new(),
        }
    }
}

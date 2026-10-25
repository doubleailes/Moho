//! The `instancerTopology` of `PointInstancer` prims, as UsdImaging's point
//! instancer data source: the prototypes, each prototype's instance indices
//! (without `inactiveIds`), and the mask (`false` for `invisibleIds`). The
//! instance positions, rotations and scales reach `primvars` as
//! `hydra:instance*`.

use std::collections::HashSet;

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd::TimeCode;
use openusd_schemas::geom::PointInstancer;

use crate::data_source::DataSource;
use crate::data_source::retained::{RetainedVector, container, value};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::instancing as t;
use crate::tokens::prim_type;

const TOPOLOGY_ATTRIBUTES: &[&str] = &["protoIndices", "ids", "invisibleIds", "prototypes"];

/// The adapter of `PointInstancer` prims.
pub struct PointInstancerAdapter;

fn ints(value: Option<Value>) -> Vec<i64> {
    match value {
        Some(Value::IntVec(v)) => v.into_iter().map(i64::from).collect(),
        Some(Value::Int64Vec(v)) => v,
        _ => Vec::new(),
    }
}

fn topology(ctx: &PrimContext) -> Option<DataSource> {
    let prim = ctx.prim()?;
    let time = ctx.globals.time().map(|t| TimeCode::new(t.value()));
    let read = |name: &str| prim.attribute(name).get_at::<Value>(time).ok().flatten();
    let prototypes = prim
        .relationship("prototypes")
        .forwarded_targets()
        .unwrap_or_default();
    let proto_indices = ints(read("protoIndices"));
    let ids: Vec<i64> = match read("ids") {
        Some(v) => ints(Some(v)),
        None => (0..proto_indices.len())
            .filter_map(|i| i64::try_from(i).ok())
            .collect(),
    };
    let invisible: HashSet<i64> = ints(read("invisibleIds")).into_iter().collect();
    // openusd folds inactive and invisible ids into one mask; Hydra keeps
    // inactive instances out of the indices and invisible ones in the mask.
    let survives = PointInstancer::get(&ctx.stage, ctx.path.clone())
        .ok()
        .flatten()
        .and_then(|pi| pi.mask_at(time, Some(&ids)).ok())
        .unwrap_or_default();
    let inactive = |i: usize| {
        let masked = survives.get(i).is_some_and(|keep| !keep);
        masked && ids.get(i).is_none_or(|id| !invisible.contains(id))
    };
    let mut per_prototype: Vec<Vec<i32>> = vec![Vec::new(); prototypes.len()];
    for (i, &k) in proto_indices.iter().enumerate() {
        if inactive(i) {
            continue;
        }
        if let (Ok(k), Ok(i)) = (usize::try_from(k), i32::try_from(i))
            && let Some(slot) = per_prototype.get_mut(k)
        {
            slot.push(i);
        }
    }
    let mask: Vec<bool> = if invisible.is_empty() {
        Vec::new()
    } else {
        ids.iter().map(|id| !invisible.contains(id)).collect()
    };
    if TOPOLOGY_ATTRIBUTES[..3]
        .iter()
        .any(|a| prim.attribute(*a).might_be_time_varying().unwrap_or(false))
    {
        ctx.globals
            .flag_time_varying(&ctx.path, Locator::new([t::INSTANCER_TOPOLOGY]));
    }
    Some(container([
        (t::PROTOTYPES, value(Value::PathVec(prototypes))),
        (
            t::INSTANCE_INDICES,
            RetainedVector::new(per_prototype.into_iter().map(|v| value(Value::IntVec(v))))
                .into_data_source(),
        ),
        (t::MASK, value(Value::BoolVec(mask))),
    ]))
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
            topology(ctx)
        } else {
            None
        }
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if TOPOLOGY_ATTRIBUTES.contains(&name) {
            LocatorSet::from(Locator::new([t::INSTANCER_TOPOLOGY]))
        } else {
            LocatorSet::new()
        }
    }
}

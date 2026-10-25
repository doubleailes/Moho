//! The `xform` data of transformable prims: the local matrix at the index's
//! time and whether the stack resets, as UsdImaging's xform data source.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd::TimeCode;
use openusd_schemas::geom::{XformQuery, Xformable};

use crate::data_source::retained::{container, value};
use crate::data_source::{DataSource, Sampled, bracketing_sample_times};
use crate::locator::{Locator, LocatorSet};
use crate::stage::adapter::Adapter;
use crate::stage::{Globals, PrimContext};
use crate::tokens::xform as t;

/// Contributes `xform` to every transformable prim whose ops have an effect.
pub struct XformAdapter;

fn query(ctx: &PrimContext) -> Option<XformQuery> {
    let query = XformQuery::for_prim(&ctx.prim()?).ok()?;
    query.transform_might_have_effect().then_some(query)
}

impl Adapter for XformAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() || query(ctx).is_none() {
            return Vec::new();
        }
        vec![Token::from(t::XFORM)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::XFORM || ctx.path.is_abs_root() {
            return None;
        }
        let query = query(ctx)?;
        if query.transform_might_be_time_varying().unwrap_or(false) {
            ctx.globals.flag_time_varying(&ctx.path, Locator::new([t::XFORM]));
        }
        let resets = query.resets_xform_stack();
        let matrix = DataSource::Sampled(Rc::new(LocalMatrix {
            query,
            globals: ctx.globals.clone(),
        }));
        Some(container([
            (t::MATRIX, matrix),
            (t::RESET_XFORM_STACK, value(resets)),
        ]))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if Xformable::affects_transform(name) {
            LocatorSet::from(Locator::new([t::XFORM]))
        } else {
            LocatorSet::new()
        }
    }
}

/// The local transform, read at the index's time plus an offset.
struct LocalMatrix {
    query: XformQuery,
    globals: Rc<Globals>,
}

impl Sampled for LocalMatrix {
    fn value(&self, offset: f32) -> Option<Value> {
        let time = self
            .globals
            .time()
            .map(|t| TimeCode::new(t.value() + f64::from(offset)));
        self.query.local_transform(time).ok().map(Value::Matrix4d)
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let now = self.globals.time()?.value();
        let times = self.query.time_samples().ok()?;
        #[allow(clippy::cast_possible_truncation)]
        let offsets: Vec<f32> = times.iter().map(|t| (t - now) as f32).collect();
        bracketing_sample_times(&offsets, start, end)
    }
}

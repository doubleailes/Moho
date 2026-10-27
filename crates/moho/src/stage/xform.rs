//! The `xform` data of transformable prims: the local matrix at the index's
//! time and whether the stack resets, as UsdImaging's xform data source; and
//! UsdImaging's `geomXformVectors` (`UsdImagingGeomXformVectorsSchema`).

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd::TimeCode;
use openusd_schemas::geom::{RotationOrder, XformCommonAPI, XformQuery, Xformable};

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

/// Whether the prim authors its `Xformable` schema attribute
/// (`xformOpOrder`), UsdImaging's `_HasAuthoredXform`.
fn has_authored_xform(ctx: &PrimContext) -> bool {
    ctx.prim()
        .is_some_and(|p| p.attribute("xformOpOrder").has_authored_value().unwrap_or(false))
}

fn rotation_order(order: RotationOrder) -> &'static str {
    // `TfEnum::GetName` of `UsdGeomXformCommonAPI::RotationOrder`.
    match order {
        RotationOrder::XYZ => "RotationOrderXYZ",
        RotationOrder::XZY => "RotationOrderXZY",
        RotationOrder::YXZ => "RotationOrderYXZ",
        RotationOrder::YZX => "RotationOrderYZX",
        RotationOrder::ZXY => "RotationOrderZXY",
        RotationOrder::ZYX => "RotationOrderZYX",
    }
}

/// `geomXformVectors`: the transform read as XformCommonAPI's vectors, by
/// accumulation, at the index's time, unsampled.
fn xform_vectors(ctx: &PrimContext) -> Option<DataSource> {
    if !has_authored_xform(ctx) {
        return None;
    }
    let vectors = XformCommonAPI::from_prim_unchecked(ctx.prim()?)
        .xform_vectors_by_accumulation(ctx.globals.time())
        .ok()?;
    Some(container([
        (t::TRANSLATION, value(Value::Vec3d(vectors.translation))),
        (t::ROTATION, value(Value::Vec3f(vectors.rotation))),
        (t::SCALE, value(Value::Vec3f(vectors.scale))),
        (t::PIVOT, value(Value::Vec3f(vectors.pivot))),
        (
            t::ROTATION_ORDER,
            value(Value::Token(Token::from(rotation_order(vectors.rotation_order)))),
        ),
    ]))
}

impl Adapter for XformAdapter {
    /// `xform` on every `Xformable` (given only when its ops have an
    /// effect), and `geomXformVectors` where `xformOpOrder` is authored.
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        let xformable = ctx.prim().is_some_and(|p| p.is_a("Xformable").unwrap_or(false));
        if ctx.path.is_abs_root() || !xformable {
            return Vec::new();
        }
        let mut names = vec![Token::from(t::XFORM)];
        if has_authored_xform(ctx) {
            names.push(Token::from(t::GEOM_XFORM_VECTORS));
        }
        names
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if ctx.path.is_abs_root() {
            return None;
        }
        // Never flagged time-varying nor invalidated, as UsdImaging's.
        if name == t::GEOM_XFORM_VECTORS {
            return xform_vectors(ctx);
        }
        if name != t::XFORM {
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

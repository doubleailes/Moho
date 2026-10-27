//! The `camera` data of `Camera` prims, as UsdImaging's camera data source:
//! apertures and focal length scaled from tenths of a scene unit to scene
//! units (`GfCamera::APERTURE_UNIT`, `FOCAL_LENGTH_UNIT`), `shutter:*` and
//! `exposure:*` renamed, clipping planes as doubles, and
//! `linearExposureScale` computed as `UsdGeomCamera` does. As UsdImaging's,
//! `camera` lists the USD attribute names and answers both those and the
//! Hydra names.

use std::rc::Rc;

use openusd::gf;
use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd::{Attribute, TimeCode};

use crate::data_source::{Container, DataSource, Sampled, bracketing_sample_times};
use crate::locator::{Locator, LocatorSet};
use crate::stage::adapter::Adapter;
use crate::stage::attribute::{AttributeValue, MappedAttributeValue};
use crate::stage::{Globals, PrimContext};
use crate::tokens::camera as t;
use crate::tokens::prim_type;

/// Hydra member and the USD attribute it reads.
const MEMBERS: &[(&str, &str)] = &[
    ("projection", "projection"),
    ("horizontalAperture", "horizontalAperture"),
    ("verticalAperture", "verticalAperture"),
    ("horizontalApertureOffset", "horizontalApertureOffset"),
    ("verticalApertureOffset", "verticalApertureOffset"),
    ("focalLength", "focalLength"),
    ("clippingRange", "clippingRange"),
    ("clippingPlanes", "clippingPlanes"),
    ("fStop", "fStop"),
    ("focusDistance", "focusDistance"),
    ("stereoRole", "stereoRole"),
    ("shutterOpen", "shutter:open"),
    ("shutterClose", "shutter:close"),
    ("exposure", "exposure"),
    ("exposureTime", "exposure:time"),
    ("exposureIso", "exposure:iso"),
    ("exposureFStop", "exposure:fStop"),
    ("exposureResponsivity", "exposure:responsivity"),
];

/// `UsdGeomCamera::GetSchemaAttributeNames(false)`: the names `camera`
/// lists, USD's own (`shutter:open`, not `shutterOpen`), as UsdImaging's
/// camera data source does.
const USD_NAMES: &[&str] = &[
    "projection",
    "horizontalAperture",
    "verticalAperture",
    "horizontalApertureOffset",
    "verticalApertureOffset",
    "focalLength",
    "clippingRange",
    "clippingPlanes",
    "fStop",
    "focusDistance",
    "stereoRole",
    "shutter:open",
    "shutter:close",
    "exposure",
    "exposure:iso",
    "exposure:time",
    "exposure:fStop",
    "exposure:responsivity",
];

/// The attributes `linearExposureScale` reads.
const EXPOSURE: &[&str] = &[
    "exposure",
    "exposure:time",
    "exposure:iso",
    "exposure:fStop",
    "exposure:responsivity",
];

/// `GfCamera::APERTURE_UNIT` and `FOCAL_LENGTH_UNIT`.
const UNIT: f32 = 0.1;

fn scaled(value: Value) -> Option<Value> {
    match value {
        Value::Float(f) => Some(Value::Float(f * UNIT)),
        _ => None,
    }
}

fn planes_as_doubles(value: Value) -> Option<Value> {
    match value {
        Value::Vec4fVec(v) => Some(Value::Vec4dVec(
            v.iter()
                .map(|p| gf::Vec4d {
                    x: f64::from(p.x),
                    y: f64::from(p.y),
                    z: f64::from(p.z),
                    w: f64::from(p.w),
                })
                .collect(),
        )),
        other => Some(other),
    }
}

/// The adapter of `Camera` prims.
pub struct CameraAdapter;

impl Adapter for CameraAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::CAMERA)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::CAMERA)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::CAMERA).then(|| DataSource::Container(Rc::new(CameraData(ctx.clone()))))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let mut locators = LocatorSet::new();
        if !USD_NAMES.contains(&name) {
            return locators;
        }
        let member = MEMBERS
            .iter()
            .find(|(_, usd)| *usd == name)
            .map_or(name, |(member, _)| member);
        locators.insert(Locator::new([t::CAMERA, member]));
        if EXPOSURE.contains(&name) {
            locators.insert(Locator::new([t::CAMERA, t::LINEAR_EXPOSURE_SCALE]));
        }
        locators
    }
}

struct CameraData(PrimContext);

impl Container for CameraData {
    /// USD's attribute names, as UsdImaging lists them (its quirk: `get`
    /// answers the Hydra names, `shutterOpen` and `linearExposureScale`
    /// among them, and any attribute of the prim by its USD name).
    fn names(&self) -> Vec<Token> {
        USD_NAMES.iter().map(|n| Token::from(*n)).collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        let locator = Locator::new([t::CAMERA, name.as_str()]);
        if name == t::LINEAR_EXPOSURE_SCALE {
            let attributes: Vec<Attribute> = EXPOSURE.iter().map(|a| prim.attribute(*a)).collect();
            if attributes
                .iter()
                .any(|a| a.might_be_time_varying().unwrap_or(false))
            {
                self.0.globals.flag_time_varying(&self.0.path, locator);
            }
            return Some(DataSource::Sampled(Rc::new(LinearExposureScale {
                attributes,
                globals: self.0.globals.clone(),
            })));
        }
        // The Hydra names that differ from USD's map to the attribute; any
        // other name is an attribute of the prim.
        let usd = MEMBERS
            .iter()
            .find(|(m, _)| *m == name.as_str())
            .map_or(name.as_str(), |(_, usd)| usd);
        let attribute = prim.attribute(usd);
        if !attribute.is_defined().unwrap_or(false) {
            return None;
        }
        let source = match name.as_str() {
            "horizontalAperture"
            | "verticalAperture"
            | "horizontalApertureOffset"
            | "verticalApertureOffset"
            | "focalLength" => MappedAttributeValue::data_source(attribute, &self.0, locator, scaled),
            "clippingPlanes" => {
                MappedAttributeValue::data_source(attribute, &self.0, locator, planes_as_doubles)
            }
            _ => AttributeValue::data_source(attribute, &self.0, locator),
        };
        Some(source)
    }
}

/// `UsdGeomCamera::ComputeLinearExposureScale`.
struct LinearExposureScale {
    /// `exposure`, `exposure:time`, `exposure:iso`, `exposure:fStop`,
    /// `exposure:responsivity`.
    attributes: Vec<Attribute>,
    globals: Rc<Globals>,
}

impl Sampled for LinearExposureScale {
    fn value(&self, offset: f32) -> Option<Value> {
        let time = self
            .globals
            .time()
            .map(|t| TimeCode::new(t.value() + f64::from(offset)));
        let read = |i: usize, fallback: f32| -> f32 {
            match self
                .attributes
                .get(i)
                .and_then(|a| a.get_at::<Value>(time).ok().flatten())
            {
                Some(Value::Float(f)) => f,
                _ => fallback,
            }
        };
        let exponent = read(0, 0.0);
        let exposure_time = read(1, 1.0);
        let iso = read(2, 100.0);
        let f_stop = read(3, 1.0);
        let responsivity = read(4, 1.0);
        let scale = (exposure_time * iso * 2.0f32.powf(exponent) * responsivity) / (100.0 * f_stop * f_stop);
        Some(Value::Float(scale))
    }

    /// The union of every input's bracketing samples.
    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let now = self.globals.time()?.value();
        let mut times: Vec<f32> = Vec::new();
        for attribute in &self.attributes {
            let samples = attribute.time_sample_times().unwrap_or_default();
            #[allow(clippy::cast_possible_truncation)]
            let offsets: Vec<f32> = samples.iter().map(|t| (t - now) as f32).collect();
            times.extend(bracketing_sample_times(&offsets, start, end).unwrap_or_default());
        }
        if times.is_empty() {
            return None;
        }
        times.sort_by(f32::total_cmp);
        times.dedup();
        Some(times)
    }
}

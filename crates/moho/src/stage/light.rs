//! The `light` and `material` data of lights, as UsdImaging's light API
//! adapter gives them.
//!
//! `light` names `filters` (the forwarded `light:filters` targets),
//! `isLight` and `materialSyncMode`, and answers any other parameter by name
//! as `UsdImagingPrimAdapter::LookupLightParamAttribute` does: Hydra's light
//! tokens read their `inputs:` attribute (`intensity` reads
//! `inputs:intensity`), any other name the attribute of that name
//! (`poleAxis`). Dome lights also name `portals`, and `DomeLight_1`
//! `domeOffset`, as their prim adapters overlay them.
//!
//! `material` is the light's network: the light prim is its one node, by
//! its absolute path, feeding the terminal `light` (`stage/material.rs`).
//!
//! A light filter (a `LightFilter`, or a prim of a type derived from it) is
//! UsdImaging's light filter adapter: type `lightFilter`, and `material`,
//! its network with the filter prim as the one node feeding the terminal
//! `lightFilter`, identified by `lightFilter:shaderId`. It has no `light`
//! of its own (the light linking filter gives it one), and its collections
//! come from the collection adapter. It represents its descendants, as a
//! material does.

use std::rc::Rc;

use openusd::gf;
use openusd::sdf::Value;
use openusd::tf::Token;
use openusd::usd;

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::stage::material::networks;
use crate::tokens::light as t;
use crate::tokens::{material, prim_type};

/// Hydra's light parameter tokens and the `inputs:` attributes
/// `LookupLightParamAttribute` reads for them.
const PARAMETERS: &[(&str, &str)] = &[
    ("angle", "inputs:angle"),
    ("color", "inputs:color"),
    ("colorTemperature", "inputs:colorTemperature"),
    ("diffuse", "inputs:diffuse"),
    ("enableColorTemperature", "inputs:enableColorTemperature"),
    ("exposure", "inputs:exposure"),
    ("height", "inputs:height"),
    ("intensity", "inputs:intensity"),
    ("length", "inputs:length"),
    ("normalize", "inputs:normalize"),
    ("radius", "inputs:radius"),
    ("specular", "inputs:specular"),
    ("texture:file", "inputs:texture:file"),
    ("texture:format", "inputs:texture:format"),
    ("width", "inputs:width"),
    ("shaping:focus", "inputs:shaping:focus"),
    ("shaping:focusTint", "inputs:shaping:focusTint"),
    ("shaping:cone:angle", "inputs:shaping:cone:angle"),
    ("shaping:cone:softness", "inputs:shaping:cone:softness"),
    ("shaping:ies:file", "inputs:shaping:ies:file"),
    ("shaping:ies:angleScale", "inputs:shaping:ies:angleScale"),
    ("shaping:ies:normalize", "inputs:shaping:ies:normalize"),
    ("shadow:enable", "inputs:shadow:enable"),
    ("shadow:color", "inputs:shadow:color"),
    ("shadow:distance", "inputs:shadow:distance"),
    ("shadow:falloff", "inputs:shadow:falloff"),
    ("shadow:falloffGamma", "inputs:shadow:falloffGamma"),
];

/// `UsdImagingPrimAdapter::LookupLightParamAttribute`: the attribute a light
/// parameter reads, if the prim has it.
fn lookup_light_param_attribute(prim: &usd::Prim, name: &str) -> Option<usd::Attribute> {
    let attribute = PARAMETERS
        .iter()
        .find(|(token, _)| *token == name)
        .map_or(name, |(_, attribute)| attribute);
    let attribute = prim.attribute(attribute);
    attribute.is_defined().unwrap_or(false).then_some(attribute)
}

/// The dome kind of a light prim, which adds data of its own.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dome {
    No,
    /// `DomeLight`: `portals`.
    V0,
    /// `DomeLight_1`: `domeOffset` and `portals`.
    V1,
}

fn dome(prim: &usd::Prim) -> Dome {
    match prim.type_name().ok().flatten().as_ref().map(Token::as_str) {
        Some("DomeLight") => Dome::V0,
        Some("DomeLight_1") => Dome::V1,
        _ => Dome::No,
    }
}

/// Contributes `light` and `material` to prims applying `LightAPI` (every
/// UsdLux light).
pub struct LightAdapter;

impl Adapter for LightAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(material::MATERIAL), Token::from(t::LIGHT)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::LIGHT => Some(DataSource::Container(Rc::new(LightData(ctx.clone())))),
            material::MATERIAL => Some(networks(ctx, Some(material::LIGHT))),
            _ => None,
        }
    }

    /// As `UsdImagingLightAPIAdapter::InvalidateImagingSubprim`: `inputs:*`
    /// dirties `material` and `light`, `light:*` dirties `light`; and as the
    /// dome adapters, `portals` (and `DomeLight_1`'s `poleAxis`) dirty
    /// `light`.
    fn invalidate(&self, ctx: &PrimContext, name: &str) -> LocatorSet {
        let light = Locator::new([t::LIGHT]);
        if name.starts_with("inputs:") {
            return [Locator::new([material::MATERIAL]), light].into_iter().collect();
        }
        if name.starts_with("light:") {
            return light.into();
        }
        let dome = ctx.prim().map_or(Dome::No, |p| dome(&p));
        if (dome != Dome::No && name == t::PORTALS) || (dome == Dome::V1 && name == "poleAxis") {
            return light.into();
        }
        LocatorSet::new()
    }
}

/// A light's `light` data (`_LightDataSource`, overlaid by the dome
/// adapters').
struct LightData(PrimContext);

impl Container for LightData {
    fn names(&self) -> Vec<Token> {
        let mut names = Vec::new();
        match self.0.prim().map_or(Dome::No, |p| dome(&p)) {
            Dome::No => {}
            Dome::V0 => names.push(Token::from(t::PORTALS)),
            Dome::V1 => {
                names.push(Token::from(t::DOME_OFFSET));
                names.push(Token::from(t::PORTALS));
            }
        }
        names.extend([t::FILTERS, t::IS_LIGHT, t::MATERIAL_SYNC_MODE].map(Token::from));
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        let dome = dome(&prim);
        match name.as_str() {
            t::PORTALS if dome != Dome::No => Some(value(Value::PathVec(
                prim.relationship(t::PORTALS)
                    .forwarded_targets()
                    .unwrap_or_default(),
            ))),
            t::DOME_OFFSET if dome == Dome::V1 => Some(value(Value::Matrix4d(dome_offset(&prim)))),
            t::FILTERS => Some(value(Value::PathVec(
                prim.relationship("light:filters")
                    .forwarded_targets()
                    .unwrap_or_default(),
            ))),
            t::IS_LIGHT => Some(value(Value::Bool(true))),
            t::MATERIAL_SYNC_MODE => {
                let attribute = prim.attribute("light:materialSyncMode");
                attribute.is_defined().unwrap_or(false).then(|| {
                    AttributeValue::data_source(
                        attribute,
                        &self.0,
                        Locator::new([t::LIGHT, t::MATERIAL_SYNC_MODE]),
                    )
                })
            }
            _ => lookup_light_param_attribute(&prim, name.as_str()).map(|attribute| {
                AttributeValue::data_source(attribute, &self.0, Locator::new([t::LIGHT, name.as_str()]))
            }),
        }
    }
}

/// `_GetDomeOffset`: the rotation aligning a `DomeLight_1` with its
/// `poleAxis`, a quarter turn about X for `Z` (or for `scene` on a `Z`-up
/// stage), else identity.
fn dome_offset(prim: &usd::Prim) -> gf::Mat4d {
    let pole = prim
        .attribute("poleAxis")
        .get::<Value>()
        .ok()
        .flatten()
        .and_then(Value::try_as_token);
    let z_up = match pole.as_ref().map(Token::as_str) {
        Some("Z") => true,
        Some("scene") => matches!(
            prim.stage().stage_metadata("upAxis"),
            Ok(Some(Value::Token(axis))) if axis == "Z"
        ),
        _ => false,
    };
    if !z_up {
        return gf::Mat4d::IDENTITY;
    }
    // `GfMatrix4d::SetRotate(GfRotation(X, 90))`, through its quaternion so
    // the near-zero terms are C++'s.
    let half = 90f64.to_radians() / 2.0;
    let (i, r) = (half.sin(), half.cos());
    // Row-major, as GfMatrix4d: rows 1 and 2 turn.
    let mut m = gf::Mat4d::IDENTITY;
    m.0[5] = 1.0 - 2.0 * (i * i);
    m.0[6] = 2.0 * (i * r);
    m.0[9] = -2.0 * (i * r);
    m.0[10] = 1.0 - 2.0 * (i * i);
    m
}

/// The adapter of light filters (`UsdImagingLightFilterAdapter`).
pub struct LightFilterAdapter;

impl Adapter for LightFilterAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::LIGHT_FILTER)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(material::MATERIAL)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == material::MATERIAL).then(|| networks(ctx, Some(material::LIGHT_FILTER)))
    }

    /// As `UsdImagingLightFilterAdapter::InvalidateImagingSubprim`: an
    /// `inputs:*` edit dirties the whole `material` (its node is named by
    /// the filter's path, so no single parameter is named).
    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if name.starts_with("inputs:") {
            return Locator::new([material::MATERIAL]).into();
        }
        LocatorSet::new()
    }
}

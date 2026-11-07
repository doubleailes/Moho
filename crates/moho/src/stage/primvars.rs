//! The `primvars` data of a prim, as UsdImaging's primvars data sources
//! (`dataSourcePrimvars.cpp`, `dataSourceGprim.cpp`).
//!
//! - Every authored `primvars:*` but `points`, `velocities` and
//!   `accelerations` is named, and gives data only when its value is
//!   authored. A `primvars:*` relationship is answered (its forwarded
//!   targets, constant), though not named.
//! - A gprim names `primvars` whether or not it has any, and a point-based
//!   or curve prim overlays, under those, the plain attributes Hydra reads
//!   as primvars (`points`, `velocities`, `accelerations`, `normals`,
//!   `widths`; a NURBS curve's `pointWeights`): always named, given when
//!   authored.
//! - Each primvar names `interpolation` and `role` (a custom primvar always
//!   gives a role, empty when the attribute has none), `colorSpace` for a
//!   colour, its value (`primvarValue`, or `indexedPrimvarValue` and
//!   `indices`) and `elementSize` when it is not 1.
//!
//! A point instancer's instance data are primvars too
//! (`hydra:instanceTranslations`, ...), read as before until
//! `hydra-parity-instancing`.

use std::rc::Rc;

use openusd::sdf::Role;

use openusd::tf::Token;
use openusd::usd;
use openusd::vt::Value;
use openusd_schemas::geom::{Primvar, PrimvarsAPI};

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::primvars as t;

const PRIMVARS_PREFIX: &str = "primvars:";

/// The `primvars:*` names UsdImaging never reads as primvars
/// (`_RejectPrimvar`): their plain attributes are.
const REJECTED: &[&str] = &[t::POINTS, t::VELOCITIES, t::ACCELERATIONS];

/// A gprim's custom primvars (`_GetCustomPrimvarMappings`): `(primvar,
/// attribute, family)`, for the prims of that family.
const GPRIM_CUSTOM: &[(&str, &str, &str)] = &[
    (t::POINTS, "points", "PointBased"),
    (t::VELOCITIES, "velocities", "PointBased"),
    (t::ACCELERATIONS, "accelerations", "PointBased"),
    (t::NORMALS, "normals", "PointBased"),
    (t::WIDTHS, "widths", "Curves"),
    (t::WIDTHS, "widths", "Points"),
    (t::POINT_WEIGHTS, "pointWeights", "NurbsCurves"),
];

/// A point instancer's instance data, read as primvars: `(primvar,
/// attribute)`, both rotation attributes listed (either dirties
/// `hydra:instanceRotations`); [`instancer_custom`] picks one.
const INSTANCER_CUSTOM: &[(&str, &str)] = &[
    (INSTANCE_TRANSLATIONS, "positions"),
    (INSTANCE_ROTATIONS, "orientationsf"),
    (INSTANCE_ROTATIONS, "orientations"),
    (INSTANCE_SCALES, "scales"),
    (t::VELOCITIES, "velocities"),
    (t::ACCELERATIONS, "accelerations"),
    ("angularVelocities", "angularVelocities"),
];

const INSTANCE: &str = "instance";
use crate::tokens::instancing::{INSTANCE_ROTATIONS, INSTANCE_SCALES, INSTANCE_TRANSLATIONS};

/// `UsdColorSpaceAPI::ComputeColorSpaceName` for `attribute`: its own
/// `colorSpace`, authored or declared, else the nearest prim, from its own
/// up, applying `ColorSpaceAPI` with a non-empty `colorSpace:name` (a name
/// neither `GfColorSpace` nor a `ColorSpaceDefinitionAPI` knows ends the
/// search with none); `None` where that is empty.
pub(crate) fn compute_color_space_name(attribute: &usd::Attribute) -> Option<Token> {
    usd::ColorSpaceAPI::compute_attribute_name(attribute, None)
        .ok()
        .flatten()
}

fn is_gprim(prim: &usd::Prim) -> bool {
    crate::stage::is_a(prim, "Gprim")
}

/// The custom primvars of `prim`'s family: `(primvar, attribute)`.
fn gprim_custom(prim: &usd::Prim) -> Vec<(&'static str, &'static str)> {
    GPRIM_CUSTOM
        .iter()
        .filter(|(_, _, family)| crate::stage::is_a(prim, family))
        .map(|(primvar, attribute, _)| (*primvar, *attribute))
        .collect()
}

/// The point instancer data read as primvars, by name, as
/// `_GetCustomPrimvarMappings` gives them: every mapping, named whether or
/// not its attribute has a value (`UsdImagingDataSourceCustomPrimvars`
/// answers one only when authored), rotations read from `orientationsf`
/// when it has a non-empty value at the earliest time
/// (`UsdGeomPointInstancer::UsesOrientationsf`), else `orientations`.
fn instancer_custom(prim: &usd::Prim) -> Vec<(Token, usd::Attribute)> {
    if !crate::stage::is_a(prim, "PointInstancer") {
        return Vec::new();
    }
    let uses_orientationsf = prim
        .attribute("orientationsf")
        .get_at::<Value>(usd::TimeCode::EARLIEST)
        .ok()
        .flatten()
        .is_some_and(|v| v.array_len().is_some_and(|n| n > 0));
    let rotations = if uses_orientationsf {
        "orientationsf"
    } else {
        "orientations"
    };
    INSTANCER_CUSTOM
        .iter()
        .filter(|(name, attribute)| *name != INSTANCE_ROTATIONS || *attribute == rotations)
        .map(|(name, attribute)| (Token::from(*name), prim.attribute(*attribute)))
        .collect()
}

/// The authored primvars UsdImaging names (`GetAuthoredPrimvars`, less the
/// rejected names).
fn authored(prim: &usd::Prim) -> Vec<Primvar> {
    PrimvarsAPI::from_prim_unchecked(prim.clone())
        .authored_primvars()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !REJECTED.contains(&p.primvar_name()))
        .collect()
}

/// Contributes `primvars`.
pub struct PrimvarsAdapter;

impl Adapter for PrimvarsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        let Some(prim) = ctx.prim().filter(|_| !ctx.path.is_abs_root()) else {
            return Vec::new();
        };
        let named = is_gprim(&prim)
            || !PrimvarsAPI::from_prim_unchecked(prim.clone())
                .authored_primvars()
                .unwrap_or_default()
                .is_empty()
            || !instancer_custom(&prim).is_empty();
        if named {
            vec![Token::from(t::PRIMVARS)]
        } else {
            Vec::new()
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::PRIMVARS || ctx.path.is_abs_root() {
            return None;
        }
        let prim = ctx.prim()?;
        // UsdImaging answers nothing without authored primvars, unless the
        // prim has custom primvars to overlay. The custom ones are asked
        // first: they are a type check, where listing the authored primvars
        // composes every property of the prim, and every gprim has some.
        let any = !gprim_custom(&prim).is_empty()
            || !instancer_custom(&prim).is_empty()
            || !PrimvarsAPI::from_prim_unchecked(prim.clone())
                .authored_primvars()
                .unwrap_or_default()
                .is_empty();
        any.then(|| DataSource::Container(Rc::new(Primvars(ctx.clone()))))
    }

    fn invalidate(&self, ctx: &PrimContext, name: &str) -> LocatorSet {
        let mut locators = LocatorSet::new();
        // `UsdImagingDataSourcePrim::Invalidate` appends the rest of the
        // name verbatim: `primvars:st:indices` dirties `primvars/st:indices`,
        // which no primvar is named (UsdImaging's quirk, reproduced).
        if let Some(rest) = name.strip_prefix(PRIMVARS_PREFIX) {
            locators.insert(Locator::new([t::PRIMVARS, rest]));
        }
        let Some(prim) = ctx.prim() else {
            return locators;
        };
        for (primvar, attribute) in gprim_custom(&prim) {
            if attribute == name {
                locators.insert(Locator::new([t::PRIMVARS, primvar]));
            }
        }
        if crate::stage::is_a(&prim, "PointInstancer") {
            for (primvar, attribute) in INSTANCER_CUSTOM {
                if *attribute == name {
                    locators.insert(Locator::new([t::PRIMVARS, *primvar]));
                }
            }
        }
        locators
    }

    /// A `primvars:*` property added or removed dirties the whole
    /// `primvars` (UsdImaging's `Resync` invalidation).
    fn invalidate_resynced(&self, ctx: &PrimContext, name: &str) -> LocatorSet {
        if name.starts_with(PRIMVARS_PREFIX) {
            let mut locators = self.invalidate(ctx, name);
            locators.insert(Locator::new([t::PRIMVARS]));
            locators
        } else {
            self.invalidate(ctx, name)
        }
    }
}

/// Where one primvar's data comes from.
enum Source {
    /// A `primvars:<name>` attribute with an authored value.
    Primvar(Primvar),
    /// A `primvars:<name>` relationship.
    Relationship(usd::Relationship),
    /// A gprim's plain attribute read as a primvar.
    Custom(usd::Attribute),
    /// A point instancer's instance data.
    Instance(usd::Attribute),
}

/// A prim's primvars, by name.
struct Primvars(PrimContext);

impl Primvars {
    fn source(&self, prim: &usd::Prim, name: &str) -> Option<Source> {
        if !REJECTED.contains(&name) {
            let full = format!("{PRIMVARS_PREFIX}{name}");
            let attribute = prim.attribute(full.as_str());
            if attribute.is_defined().unwrap_or(false) {
                if attribute.has_authored_value().unwrap_or(false) {
                    return Primvar::from_attribute(attribute).map(Source::Primvar);
                }
            } else {
                let relationship = prim.relationship(full.as_str());
                if relationship.is_defined().unwrap_or(false) {
                    return Some(Source::Relationship(relationship));
                }
            }
        }
        // The custom primvars are weaker than `primvars:*`.
        if let Some((_, attribute)) = gprim_custom(prim).into_iter().find(|(p, _)| *p == name) {
            let attribute = prim.attribute(attribute);
            return attribute
                .has_authored_value()
                .unwrap_or(false)
                .then_some(Source::Custom(attribute));
        }
        instancer_custom(prim)
            .into_iter()
            .find(|(n, _)| n == name)
            .filter(|(_, a)| a.has_authored_value().unwrap_or(false))
            .map(|(_, a)| Source::Instance(a))
    }
}

impl Container for Primvars {
    fn names(&self) -> Vec<Token> {
        let Some(prim) = self.0.prim() else {
            return Vec::new();
        };
        let mut names: Vec<Token> = authored(&prim)
            .iter()
            .map(|p| Token::from(p.primvar_name()))
            .collect();
        let custom = gprim_custom(&prim)
            .into_iter()
            .map(|(n, _)| Token::from(n))
            .chain(instancer_custom(&prim).into_iter().map(|(n, _)| n));
        for name in custom {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        let source = self.source(&prim, name.as_str())?;
        Some(DataSource::Container(Rc::new(PrimvarEntry {
            ctx: self.0.clone(),
            name: name.clone(),
            source,
        })))
    }
}

/// One primvar.
struct PrimvarEntry {
    ctx: PrimContext,
    name: Token,
    source: Source,
}

impl PrimvarEntry {
    fn attribute(&self) -> Option<&usd::Attribute> {
        match &self.source {
            Source::Primvar(p) => Some(p.attribute()),
            Source::Custom(a) | Source::Instance(a) => Some(a),
            Source::Relationship(_) => None,
        }
    }

    /// Indexed when its indices attribute has a value (custom primvars never
    /// are).
    fn indexed(&self) -> bool {
        matches!(&self.source, Source::Primvar(p) if p.indices_attr().has_value().unwrap_or(false))
    }

    fn locator(&self, member: &str) -> Locator {
        Locator::new([t::PRIMVARS, self.name.as_str(), member])
    }

    fn interpolation(&self) -> Token {
        let metadata = |a: &usd::Attribute| {
            a.metadata::<Value>("interpolation")
                .ok()
                .flatten()
                .and_then(|v| match v {
                    Value::Token(t) => Some(t),
                    _ => None,
                })
        };
        let primvar = match &self.source {
            Source::Primvar(p) => p
                .interpolation_token()
                .unwrap_or_else(|_| Token::from(t::CONSTANT)),
            Source::Relationship(_) => Token::from(t::CONSTANT),
            _ => Token::default(),
        };
        // A point instancer's own primvars are instance-rate unless constant
        // or uniform, which read as constant (`dataSourcePointInstancer.cpp`,
        // `_PrimvarDataSource`).
        let instancer = || {
            self.ctx
                .prim()
                .is_some_and(|p| crate::stage::is_a(&p, "PointInstancer"))
        };
        match &self.source {
            Source::Primvar(_) | Source::Relationship(_) if instancer() => {
                if primvar == t::CONSTANT || primvar == "uniform" {
                    Token::from(t::CONSTANT)
                } else {
                    Token::from(INSTANCE)
                }
            }
            Source::Primvar(_) | Source::Relationship(_) => primvar,
            Source::Custom(a) => metadata(a).unwrap_or_else(|| Token::from(t::VERTEX)),
            Source::Instance(_) => Token::from(INSTANCE),
        }
    }

    /// `UsdImagingUsdToHdRole`: point, normal, vector, colour and texture
    /// coordinate only.
    fn hd_role(&self) -> Option<Token> {
        let role = self.attribute()?.role().ok().flatten()?;
        let token = match role {
            Role::Point => "point",
            Role::Normal => "normal",
            Role::Vector => "vector",
            Role::Color => "color",
            Role::TextureCoordinate => "textureCoordinate",
            _ => return None,
        };
        Some(Token::from(token))
    }

    /// The role data: none without a role for a `primvars:*` attribute, the
    /// empty token for a custom primvar (UsdImaging builds it regardless).
    fn role(&self) -> Option<Token> {
        match &self.source {
            Source::Primvar(_) | Source::Relationship(_) => self.hd_role(),
            // `UsdImagingDataSourceCustomPrimvars` always builds a role,
            // empty when the attribute has none.
            Source::Custom(_) | Source::Instance(_) => Some(self.hd_role().unwrap_or_default()),
        }
    }

    /// `UsdColorSpaceAPI::ComputeColorSpaceName` for a colour primvar
    /// ([`compute_color_space_name`]).
    fn color_space(&self) -> Option<Token> {
        if self.hd_role()?.as_str() != "color" {
            return None;
        }
        compute_color_space_name(self.attribute()?)
    }

    fn element_size(&self) -> Option<i32> {
        match &self.source {
            Source::Primvar(p) => p.element_size().ok().filter(|&n| n != 1),
            _ => None,
        }
    }
}

impl Container for PrimvarEntry {
    fn names(&self) -> Vec<Token> {
        let mut names = vec![Token::from(t::INTERPOLATION), Token::from(t::ROLE)];
        if self.color_space().is_some() {
            names.push(Token::from(t::COLOR_SPACE));
        }
        if self.indexed() {
            names.push(Token::from(t::INDEXED_PRIMVAR_VALUE));
            names.push(Token::from(t::INDICES));
        } else {
            names.push(Token::from(t::PRIMVAR_VALUE));
        }
        if self.element_size().is_some() {
            names.push(Token::from(t::ELEMENT_SIZE));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let indexed = self.indexed();
        match name.as_str() {
            t::PRIMVAR_VALUE if !indexed => match &self.source {
                Source::Relationship(r) => {
                    Some(value(Value::PathVec(r.forwarded_targets().unwrap_or_default())))
                }
                _ => Some(AttributeValue::data_source(
                    self.attribute()?.clone(),
                    &self.ctx,
                    self.locator(t::PRIMVAR_VALUE),
                )),
            },
            t::INDEXED_PRIMVAR_VALUE if indexed => Some(AttributeValue::data_source(
                self.attribute()?.clone(),
                &self.ctx,
                self.locator(t::INDEXED_PRIMVAR_VALUE),
            )),
            t::INDICES if indexed => match &self.source {
                Source::Primvar(p) => Some(AttributeValue::data_source(
                    p.indices_attr(),
                    &self.ctx,
                    self.locator(t::INDICES),
                )),
                _ => None,
            },
            t::INTERPOLATION => Some(value(Value::Token(self.interpolation()))),
            t::ROLE => self.role().map(|r| value(Value::Token(r))),
            t::ELEMENT_SIZE => self.element_size().map(value),
            t::COLOR_SPACE => self.color_space().map(|c| value(Value::Token(c))),
            _ => None,
        }
    }
}

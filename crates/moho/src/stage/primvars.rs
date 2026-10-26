//! The `primvars` data of a prim: every authored `primvars:*` with a value,
//! plus the point-based and curve attributes Hydra reads as primvars
//! (`points`, `normals`, `velocities`, `accelerations`, `widths`), as
//! UsdImaging's primvars data sources.

use std::rc::Rc;

use openusd::sdf::{Role, Value};
use openusd::tf::Token;
use openusd::usd;
use openusd::usd::Object as _;
use openusd_schemas::geom::{Primvar, PrimvarsAPI};

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::primvars as t;

const PRIMVARS_PREFIX: &str = "primvars:";

/// Plain attributes Hydra reads as primvars, the schema family they need,
/// and their default interpolation.
/// `(primvar, attribute, family, default interpolation)`: the first entry
/// for a primvar whose attribute holds a non-empty value wins, so
/// `orientationsf` is preferred over `orientations`.
const CUSTOM: &[(&str, &str, &str, &str)] = &[
    (t::POINTS, "points", "PointBased", t::VERTEX),
    (t::NORMALS, "normals", "PointBased", t::VERTEX),
    (t::VELOCITIES, "velocities", "PointBased", t::VERTEX),
    (t::ACCELERATIONS, "accelerations", "PointBased", t::VERTEX),
    (t::WIDTHS, "widths", "Curves", t::VERTEX),
    (INSTANCE_TRANSLATIONS, "positions", "PointInstancer", INSTANCE),
    (INSTANCE_ROTATIONS, "orientationsf", "PointInstancer", INSTANCE),
    (INSTANCE_ROTATIONS, "orientations", "PointInstancer", INSTANCE),
    (INSTANCE_SCALES, "scales", "PointInstancer", INSTANCE),
    (t::VELOCITIES, "velocities", "PointInstancer", INSTANCE),
    (t::ACCELERATIONS, "accelerations", "PointInstancer", INSTANCE),
    (
        "angularVelocities",
        "angularVelocities",
        "PointInstancer",
        INSTANCE,
    ),
];

const INSTANCE: &str = "instance";
use crate::tokens::instancing::{INSTANCE_ROTATIONS, INSTANCE_SCALES, INSTANCE_TRANSLATIONS};

/// Where one primvar's data comes from.
enum Source {
    /// A `primvars:<name>` attribute.
    Primvar(Primvar),
    /// A plain attribute read as a primvar (`points`, ...), with its default
    /// interpolation.
    Custom(usd::Attribute, &'static str),
}

fn sources(prim: &usd::Prim) -> Vec<(Token, Source)> {
    let mut found: Vec<(Token, Source)> = Vec::new();
    if let Ok(primvars) = PrimvarsAPI::from_prim_unchecked(prim.clone()).primvars_with_values() {
        for primvar in primvars {
            let name = Token::from(primvar.primvar_name());
            found.push((name, Source::Primvar(primvar)));
        }
    }
    for (name, attribute, family, interpolation) in CUSTOM {
        // `primvars:normals` takes precedence over `normals`.
        if found.iter().any(|(n, _)| n == *name) || !prim.is_a(*family).unwrap_or(false) {
            continue;
        }
        let attribute = prim.attribute(*attribute);
        // A value that exists, and is not an empty array at the default
        // time (a sampled-only value has none there and still counts).
        let non_empty = attribute.has_value().unwrap_or(false)
            && attribute
                .get::<Value>()
                .ok()
                .flatten()
                .is_none_or(|v| v.array_len().is_none_or(|n| n > 0));
        if non_empty {
            found.push((Token::from(*name), Source::Custom(attribute, interpolation)));
        }
    }
    found
}

/// Contributes `primvars` to every prim that has one.
pub struct PrimvarsAdapter;

impl Adapter for PrimvarsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        let has = !ctx.path.is_abs_root() && ctx.prim().is_some_and(|p| !sources(&p).is_empty());
        if has {
            vec![Token::from(t::PRIMVARS)]
        } else {
            Vec::new()
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::PRIMVARS || ctx.path.is_abs_root() {
            return None;
        }
        Some(DataSource::Container(Rc::new(Primvars(ctx.clone()))))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let primvar = match name.strip_prefix(PRIMVARS_PREFIX) {
            Some(rest) => rest.strip_suffix(":indices").unwrap_or(rest),
            None => match CUSTOM.iter().find(|(_, attribute, _, _)| *attribute == name) {
                Some((primvar, ..)) => primvar,
                None => return LocatorSet::new(),
            },
        };
        LocatorSet::from(Locator::new([t::PRIMVARS, primvar]))
    }
}

/// A prim's primvars, by name.
struct Primvars(PrimContext);

impl Container for Primvars {
    fn names(&self) -> Vec<Token> {
        self.0
            .prim()
            .map(|p| sources(&p).into_iter().map(|(n, _)| n).collect())
            .unwrap_or_default()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        let (_, source) = sources(&prim).into_iter().find(|(n, _)| n == name)?;
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
    fn attribute(&self) -> &usd::Attribute {
        match &self.source {
            Source::Primvar(p) => p.attribute(),
            Source::Custom(a, _) => a,
        }
    }

    fn indexed(&self) -> bool {
        matches!(&self.source, Source::Primvar(p) if p.is_indexed().unwrap_or(false))
    }

    fn locator(&self) -> Locator {
        Locator::new([t::PRIMVARS, self.name.as_str()])
    }

    fn interpolation(&self) -> Token {
        match &self.source {
            Source::Primvar(p) => p
                .interpolation_token()
                .unwrap_or_else(|_| Token::from(t::CONSTANT)),
            Source::Custom(a, default) => a
                .metadata::<Value>("interpolation")
                .ok()
                .flatten()
                .and_then(|v| match v {
                    Value::Token(t) => Some(t),
                    _ => None,
                })
                .unwrap_or_else(|| Token::from(*default)),
        }
    }

    fn role(&self) -> Option<Token> {
        let role = self.attribute().role().ok().flatten()?;
        let token = match role {
            Role::Point => "point",
            Role::Normal => "normal",
            Role::Vector => "vector",
            Role::Color => "color",
            Role::PointIndex => "pointIndex",
            Role::EdgeIndex => "edgeIndex",
            Role::FaceIndex => "faceIndex",
            Role::TextureCoordinate => "textureCoordinate",
            Role::Frame | Role::Transform | Role::Group => return None,
        };
        Some(Token::from(token))
    }

    /// The attribute's own `colorSpace`, else the nearest ancestor's
    /// `colorSpace:name`, for colour primvars.
    fn color_space(&self) -> Option<Value> {
        if self.role()?.as_str() != "color" {
            return None;
        }
        let attribute = self.attribute();
        if attribute.has_authored_metadata("colorSpace").unwrap_or(false) {
            return attribute.metadata::<Value>("colorSpace").ok().flatten();
        }
        let stage = &self.ctx.stage;
        self.ctx.path.ancestors_below_root().find_map(|path| {
            let attr = stage.prim(&path).ok()?.attribute("colorSpace:name");
            attr.has_authored_value()
                .unwrap_or(false)
                .then(|| attr.get::<Value>().ok().flatten())
                .flatten()
        })
    }
}

impl Container for PrimvarEntry {
    fn names(&self) -> Vec<Token> {
        let mut names = if self.indexed() {
            vec![Token::from(t::INDEXED_PRIMVAR_VALUE), Token::from(t::INDICES)]
        } else {
            vec![Token::from(t::PRIMVAR_VALUE)]
        };
        names.push(Token::from(t::INTERPOLATION));
        if self.role().is_some() {
            names.push(Token::from(t::ROLE));
        }
        names.push(Token::from(t::ELEMENT_SIZE));
        if self.color_space().is_some() {
            names.push(Token::from(t::COLOR_SPACE));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let indexed = self.indexed();
        match name.as_str() {
            t::PRIMVAR_VALUE if !indexed => Some(AttributeValue::data_source(
                self.attribute().clone(),
                &self.ctx,
                self.locator(),
            )),
            t::INDEXED_PRIMVAR_VALUE if indexed => Some(AttributeValue::data_source(
                self.attribute().clone(),
                &self.ctx,
                self.locator(),
            )),
            t::INDICES if indexed => match &self.source {
                Source::Primvar(p) => Some(AttributeValue::data_source(
                    p.indices_attr(),
                    &self.ctx,
                    self.locator(),
                )),
                Source::Custom(..) => None,
            },
            t::INTERPOLATION => Some(value(Value::Token(self.interpolation()))),
            t::ROLE => self.role().map(|r| value(Value::Token(r))),
            t::ELEMENT_SIZE => {
                let size = match &self.source {
                    Source::Primvar(p) => p.element_size().unwrap_or(1),
                    Source::Custom(..) => 1,
                };
                Some(value(size))
            }
            t::COLOR_SPACE => self.color_space().map(value),
            _ => None,
        }
    }
}

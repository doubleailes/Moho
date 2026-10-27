//! UsdRender prims as UsdImaging's render prim data sources
//! (`dataSourceRenderPrims.cpp`): settings, products and vars expose
//! `__usdRenderSettings`, `__usdRenderProduct` and `__usdRenderVar`, each
//! naming its schema's attributes (inherited ones included), the
//! relationships UsdImaging reads, and `namespacedSettings`; a pass exposes
//! `renderPass`. The render settings flattening filter combines them into
//! `HdRenderSettingsSchema`.

use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd::usd;

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::prim_type;
use crate::tokens::render as t;

/// Which UsdRender prim an adapter reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Settings,
    Product,
    Var,
    Pass,
}

/// A UsdRender prim's adapter.
pub struct RenderAdapter {
    kind: Kind,
}

/// `UsdRenderSettingsBase::GetSchemaAttributeNames()`, which settings and
/// products inherit.
const BASE: &[&str] = &[
    "resolution",
    "pixelAspectRatio",
    "aspectRatioConformPolicy",
    "dataWindowNDC",
    "instantaneousShutter",
    "disableMotionBlur",
    "disableDepthOfField",
];

/// `UsdRenderSettings`' own attributes.
const SETTINGS_ATTRIBUTES: &[&str] = &[
    "includedPurposes",
    "materialBindingPurposes",
    "renderingColorSpace",
];

/// `UsdRenderProduct`'s own attributes.
const PRODUCT_ATTRIBUTES: &[&str] = &["productType", "productName"];

/// `UsdRenderVar`'s attributes.
const VAR_ATTRIBUTES: &[&str] = &["dataType", "sourceName", "sourceType"];

/// The RenderMan relationships UsdImaging lists among the namespaced
/// settings (its own `XXX`: they belong in a RenderMan adapter).
const RI_RELATIONSHIPS: &[&str] = &[
    "ri:integrator",
    "ri:sampleFilters",
    "ri:displayFilters",
    "ri:energyFilters",
];

pub const SETTINGS: RenderAdapter = RenderAdapter { kind: Kind::Settings };
pub const PRODUCT: RenderAdapter = RenderAdapter { kind: Kind::Product };
pub const VAR: RenderAdapter = RenderAdapter { kind: Kind::Var };
pub const PASS: RenderAdapter = RenderAdapter { kind: Kind::Pass };

impl Kind {
    fn prim_type(self) -> &'static str {
        match self {
            Kind::Settings => prim_type::RENDER_SETTINGS,
            Kind::Product => prim_type::RENDER_PRODUCT,
            Kind::Var => prim_type::RENDER_VAR,
            Kind::Pass => prim_type::RENDER_PASS,
        }
    }

    fn schema(self) -> &'static str {
        match self {
            Kind::Settings => t::USD_RENDER_SETTINGS,
            Kind::Product => t::USD_RENDER_PRODUCT,
            Kind::Var => t::USD_RENDER_VAR,
            Kind::Pass => t::RENDER_PASS,
        }
    }

    /// The names the schema container lists, in UsdImaging's order
    /// (`GetPropertyNames`).
    fn names(self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        match self {
            Kind::Settings => {
                names.extend(BASE);
                names.extend(SETTINGS_ATTRIBUTES);
                names.extend([t::NAMESPACED_SETTINGS, t::CAMERA, t::PRODUCTS]);
            }
            Kind::Product => {
                names.extend(BASE);
                names.extend(PRODUCT_ATTRIBUTES);
                names.extend([t::NAMESPACED_SETTINGS, t::CAMERA, t::ORDERED_VARS]);
            }
            Kind::Var => {
                names.extend(VAR_ATTRIBUTES);
                names.push(t::NAMESPACED_SETTINGS);
            }
            // UsdImaging leaves out the pass attributes meant for batch
            // processing.
            Kind::Pass => names.extend([t::PASS_TYPE, t::RENDER_SOURCE]),
        }
        names
    }
}

impl Adapter for RenderAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(self.kind.prim_type())
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(self.kind.schema())]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == self.kind.schema()).then(|| {
            DataSource::Container(Rc::new(RenderData {
                ctx: ctx.clone(),
                kind: self.kind,
            }))
        })
    }

    /// A listed property dirties its member; any other property dirties
    /// `namespacedSettings` (a pass: nothing), as UsdImaging's render prims'
    /// `Invalidate`.
    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let base = Locator::new([self.kind.schema()]);
        if self.kind.names().contains(&name) && name != t::NAMESPACED_SETTINGS {
            LocatorSet::from(base.append(name))
        } else if self.kind == Kind::Pass {
            LocatorSet::new()
        } else {
            LocatorSet::from(base.append(t::NAMESPACED_SETTINGS))
        }
    }
}

/// The first forwarded target of a relationship, as a single path.
fn first_target(prim: &usd::Prim, name: &str) -> Option<DataSource> {
    let target = prim
        .relationship(name)
        .forwarded_targets()
        .ok()?
        .into_iter()
        .next()?;
    Some(value(Value::PathVec(vec![target])))
}

/// Every forwarded target of a relationship, as a path array.
fn targets(prim: &usd::Prim, name: &str) -> DataSource {
    let targets = prim.relationship(name).forwarded_targets().unwrap_or_default();
    value(Value::PathVec(targets))
}

struct RenderData {
    ctx: PrimContext,
    kind: Kind,
}

impl Container for RenderData {
    fn names(&self) -> Vec<Token> {
        self.kind.names().into_iter().map(Token::from).collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.ctx.prim()?;
        let schema = self.kind.schema();
        let name = name.as_str();
        if name == t::NAMESPACED_SETTINGS && self.kind != Kind::Pass {
            return Some(DataSource::Container(Rc::new(NamespacedSettings {
                ctx: self.ctx.clone(),
                locator: Locator::new([schema, t::NAMESPACED_SETTINGS]),
            })));
        }
        match (self.kind, name) {
            (Kind::Settings | Kind::Product, t::CAMERA) => return first_target(&prim, name),
            (Kind::Settings, t::PRODUCTS) | (Kind::Product, t::ORDERED_VARS) => {
                return Some(targets(&prim, name));
            }
            (Kind::Pass, t::PASS_TYPE) => {
                return prim.attribute(name).get::<Value>().ok().flatten().map(value);
            }
            (Kind::Pass, t::RENDER_SOURCE) => return first_target(&prim, name),
            (Kind::Pass, _) => return None,
            _ => {}
        }
        if !self.kind.names().contains(&name) {
            return None;
        }
        let attribute = prim.attribute(name);
        // A product answers a settings-base attribute only when it authors
        // it, so the flattening takes the settings' opinion otherwise.
        if self.kind == Kind::Product
            && BASE.contains(&name)
            && !attribute.has_authored_value().unwrap_or(false)
        {
            return None;
        }
        Some(AttributeValue::data_source(
            attribute,
            &self.ctx,
            Locator::new([schema, name]),
        ))
    }
}

/// Whether an attribute is a namespaced setting: its name, without an
/// `outputs:` prefix, holds a namespace delimiter (UsdImaging's
/// `_IsNamespacedAttribute`).
fn is_namespaced(name: &str) -> bool {
    let base = match name.strip_prefix("outputs:") {
        Some(rest) if !rest.is_empty() => rest,
        _ => name,
    };
    base.contains(':')
}

/// A render prim's `namespacedSettings`: every authored namespaced attribute
/// by full name, and the RenderMan relationships with authored targets.
struct NamespacedSettings {
    ctx: PrimContext,
    locator: Locator,
}

impl Container for NamespacedSettings {
    fn names(&self) -> Vec<Token> {
        let Some(prim) = self.ctx.prim() else {
            return Vec::new();
        };
        let mut names: Vec<Token> = prim
            .authored_property_names()
            .unwrap_or_default()
            .into_iter()
            .filter(|name| is_namespaced(name.as_str()))
            .filter(|name| {
                // `GetAuthoredAttributes`: relationships are left out.
                prim.attribute(name.clone()).is_defined().unwrap_or(false)
            })
            .collect();
        for relationship in RI_RELATIONSHIPS {
            let targets = prim.relationship(*relationship).targets().unwrap_or_default();
            if !targets.is_empty() {
                names.push(Token::from(*relationship));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.ctx.prim()?;
        if RI_RELATIONSHIPS.contains(&name.as_str()) {
            let targets: Vec<sdf::Path> = prim.relationship(name.clone()).targets().ok()?;
            return Some(value(Value::PathVec(targets)));
        }
        let attribute = prim.attribute(name.clone());
        if !attribute.is_defined().unwrap_or(false) {
            return None;
        }
        Some(AttributeValue::data_source(
            attribute,
            &self.ctx,
            self.locator.append(name.clone()),
        ))
    }
}

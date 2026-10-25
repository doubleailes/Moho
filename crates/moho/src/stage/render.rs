//! UsdRender: settings, products, vars and passes, each exposing its schema
//! members (relationships as target paths) and, under
//! `namespacedSettings`, every other authored property by full name, as
//! UsdImaging's render prim data sources. The render settings flattening
//! filter combines them into `HdRenderSettingsSchema`.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::prim_type;
use crate::tokens::render as t;

/// A UsdRender prim's adapter: its Hydra type, the schema token it exposes,
/// the attributes and relationships it maps, `(member, USD name)`.
pub struct RenderAdapter {
    pub prim_type: &'static str,
    pub schema: &'static str,
    pub attributes: &'static [(&'static str, &'static str)],
    pub relationships: &'static [(&'static str, &'static str)],
}

/// The settings base every settings and product carries
/// (`UsdRenderSettingsBase`).
const BASE: &[(&str, &str)] = &[
    ("resolution", "resolution"),
    ("pixelAspectRatio", "pixelAspectRatio"),
    ("aspectRatioConformPolicy", "aspectRatioConformPolicy"),
    ("dataWindowNDC", "dataWindowNDC"),
    ("instantaneousShutter", "instantaneousShutter"),
    ("disableMotionBlur", "disableMotionBlur"),
    ("disableDepthOfField", "disableDepthOfField"),
];

pub const SETTINGS: RenderAdapter = RenderAdapter {
    prim_type: prim_type::RENDER_SETTINGS,
    schema: t::RENDER_SETTINGS,
    attributes: &[
        BASE[0],
        BASE[1],
        BASE[2],
        BASE[3],
        BASE[4],
        BASE[5],
        BASE[6],
        ("includedPurposes", "includedPurposes"),
        ("materialBindingPurposes", "materialBindingPurposes"),
        ("renderingColorSpace", "renderingColorSpace"),
    ],
    relationships: &[(t::CAMERA, "camera"), (t::PRODUCTS, "products")],
};

pub const PRODUCT: RenderAdapter = RenderAdapter {
    prim_type: prim_type::RENDER_PRODUCT,
    schema: t::RENDER_PRODUCT,
    attributes: &[
        BASE[0],
        BASE[1],
        BASE[2],
        BASE[3],
        BASE[4],
        BASE[5],
        BASE[6],
        ("productType", "productType"),
        ("productName", "productName"),
    ],
    relationships: &[(t::CAMERA, "camera"), (t::ORDERED_VARS, "orderedVars")],
};

pub const VAR: RenderAdapter = RenderAdapter {
    prim_type: prim_type::RENDER_VAR,
    schema: t::RENDER_VAR,
    attributes: &[
        ("dataType", "dataType"),
        ("sourceName", "sourceName"),
        ("sourceType", "sourceType"),
    ],
    relationships: &[],
};

pub const PASS: RenderAdapter = RenderAdapter {
    prim_type: prim_type::RENDER_PASS,
    schema: t::RENDER_PASS,
    attributes: &[("passType", "passType")],
    relationships: &[("renderSource", "renderSource")],
};

/// The members of the settings base, which a product's own opinion
/// overrides.
pub fn base_members() -> impl Iterator<Item = &'static str> {
    BASE.iter().map(|(m, _)| *m)
}

impl Adapter for RenderAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(self.prim_type)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(self.schema)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == self.schema).then(|| {
            DataSource::Container(Rc::new(RenderData {
                ctx: ctx.clone(),
                adapter: RenderAdapter {
                    prim_type: self.prim_type,
                    schema: self.schema,
                    attributes: self.attributes,
                    relationships: self.relationships,
                },
            }))
        })
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let base = Locator::new([self.schema]);
        let member = self
            .attributes
            .iter()
            .chain(self.relationships)
            .find(|(_, usd)| *usd == name)
            .map(|(member, _)| *member);
        LocatorSet::from(match member {
            Some(member) => base.append(member),
            None => base.append(t::NAMESPACED_SETTINGS),
        })
    }
}

struct RenderData {
    ctx: PrimContext,
    adapter: RenderAdapter,
}

impl RenderData {
    fn namespaced(&self) -> DataSource {
        let Some(prim) = self.ctx.prim() else {
            return container(Vec::<(Token, DataSource)>::new());
        };
        let declared = prim.prim_definition().ok();
        let locator = Locator::new([self.adapter.schema, t::NAMESPACED_SETTINGS]);
        let entries: Vec<(Token, DataSource)> = prim
            .authored_property_names()
            .unwrap_or_default()
            .into_iter()
            .filter(|name| !declared.as_ref().is_some_and(|d| d.has_property(name)))
            .filter_map(|name| {
                let attribute = prim.attribute(name.clone());
                if attribute.is_defined().unwrap_or(false) {
                    let source = AttributeValue::data_source(attribute, &self.ctx, locator.clone());
                    return Some((name, source));
                }
                let relationship = prim.relationship(name.clone());
                let targets = relationship.forwarded_targets().ok()?;
                Some((name, value(Value::PathVec(targets))))
            })
            .collect();
        container(entries)
    }
}

impl Container for RenderData {
    fn names(&self) -> Vec<Token> {
        self.adapter
            .attributes
            .iter()
            .chain(self.adapter.relationships)
            .map(|(m, _)| Token::from(*m))
            .chain([Token::from(t::NAMESPACED_SETTINGS)])
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::NAMESPACED_SETTINGS {
            return Some(self.namespaced());
        }
        let prim = self.ctx.prim()?;
        let locator = Locator::new([self.adapter.schema, name.as_str()]);
        if let Some((_, usd)) = self.adapter.attributes.iter().find(|(m, _)| *m == name.as_str()) {
            return Some(AttributeValue::data_source(
                prim.attribute(*usd),
                &self.ctx,
                locator,
            ));
        }
        let (_, usd) = self
            .adapter
            .relationships
            .iter()
            .find(|(m, _)| *m == name.as_str())?;
        let targets = prim.relationship(*usd).forwarded_targets().unwrap_or_default();
        Some(value(Value::PathVec(targets)))
    }
}

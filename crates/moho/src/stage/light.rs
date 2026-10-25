//! The `light` data of lights: an open container answering any light
//! parameter by name, as UsdImaging's light API adapter does. `intensity`
//! reads `inputs:intensity`; a name with no input reads the plain attribute
//! (`poleAxis`, `treatAsPoint`); `filters` gives the `light:filters` targets.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::light as t;

const INPUTS: &str = "inputs:";
/// Plain (non-input) attributes of UsdLux light types that a renderer reads.
const PLAIN: &[&str] = &["poleAxis", "treatAsPoint", "treatAsLine"];

/// Contributes `light` to prims applying `LightAPI` (every UsdLux light).
pub struct LightAdapter;

impl Adapter for LightAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::LIGHT)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::LIGHT).then(|| DataSource::Container(Rc::new(LightParameters(ctx.clone()))))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if name.starts_with(INPUTS) || name.starts_with("light:") || PLAIN.contains(&name) {
            LocatorSet::from(Locator::new([t::LIGHT]))
        } else {
            LocatorSet::new()
        }
    }
}

/// A light's parameters, by name.
struct LightParameters(PrimContext);

impl Container for LightParameters {
    fn names(&self) -> Vec<Token> {
        let Some(prim) = self.0.prim() else {
            return Vec::new();
        };
        let mut names: Vec<Token> = prim
            .attributes_in_namespace("inputs")
            .unwrap_or_default()
            .iter()
            .map(|a| Token::from(a.name().strip_prefix(INPUTS).unwrap_or(a.name())))
            .collect();
        for plain in PLAIN {
            if prim.attribute(*plain).is_defined().unwrap_or(false) {
                names.push(Token::from(*plain));
            }
        }
        names.push(Token::from(t::FILTERS));
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.0.prim()?;
        if name == t::FILTERS {
            let filters = prim
                .relationship("light:filters")
                .forwarded_targets()
                .unwrap_or_default();
            return Some(value(Value::PathVec(filters)));
        }
        let locator = Locator::new([t::LIGHT]);
        let input = prim.attribute(format!("{INPUTS}{name}"));
        if input.is_defined().unwrap_or(false) {
            return Some(AttributeValue::data_source(input, &self.0, locator));
        }
        let plain = prim.attribute(name.clone());
        plain
            .is_defined()
            .unwrap_or(false)
            .then(|| AttributeValue::data_source(plain, &self.0, locator))
    }
}

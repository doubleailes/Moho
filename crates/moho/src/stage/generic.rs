//! Generic attribute data (design D3): every attribute and relationship of a
//! prim, by name, with what Hydra's schemas do not carry (whether a value is
//! authored, and the metadata a renderer reads).

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::retained::value;
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::usd as t;

/// Contributes `__usdAttributes` and `__usdRelationships` to every prim.
pub struct GenericAdapter;

impl Adapter for GenericAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            return Vec::new();
        }
        vec![Token::from(t::ATTRIBUTES), Token::from(t::RELATIONSHIPS)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if ctx.path.is_abs_root() {
            return None;
        }
        match name.as_str() {
            t::ATTRIBUTES => Some(DataSource::Container(Rc::new(Attributes(ctx.clone())))),
            t::RELATIONSHIPS => Some(DataSource::Container(Rc::new(Relationships(ctx.clone())))),
            _ => None,
        }
    }

    fn invalidate(&self, ctx: &PrimContext, name: &str) -> LocatorSet {
        let is_relationship = ctx
            .prim()
            .is_some_and(|p| p.relationship(name).is_defined().unwrap_or(false));
        let container = if is_relationship {
            t::RELATIONSHIPS
        } else {
            t::ATTRIBUTES
        };
        LocatorSet::from(Locator::new([container, name]))
    }
}

/// Every attribute of a prim, the schema's declared ones included.
struct Attributes(PrimContext);

impl Container for Attributes {
    fn names(&self) -> Vec<Token> {
        self.0
            .prim()
            .and_then(|p| p.attributes().ok())
            .map(|attrs| attrs.iter().map(|a| Token::from(a.name())).collect())
            .unwrap_or_default()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let attribute = self.0.prim()?.attribute(name.clone());
        if !attribute.is_defined().unwrap_or(false) {
            return None;
        }
        Some(DataSource::Container(Rc::new(AttributeEntry {
            ctx: self.0.clone(),
            name: name.clone(),
        })))
    }
}

/// One attribute: its value, whether it is authored, and its metadata.
struct AttributeEntry {
    ctx: PrimContext,
    name: Token,
}

impl AttributeEntry {
    fn attribute(&self) -> Option<openusd::usd::Attribute> {
        Some(self.ctx.prim()?.attribute(self.name.clone()))
    }

    fn metadata(&self, key: &str) -> Option<Value> {
        let attribute = self.attribute()?;
        if !attribute.has_authored_metadata(key).unwrap_or(false) {
            return None;
        }
        attribute.metadata::<Value>(key).ok().flatten()
    }

    fn connections(&self) -> Option<Vec<openusd::sdf::Path>> {
        let connections = self.attribute()?.connections().ok()?;
        (!connections.is_empty()).then_some(connections)
    }
}

impl Container for AttributeEntry {
    fn names(&self) -> Vec<Token> {
        let mut names = vec![
            Token::from(t::VALUE),
            Token::from(t::AUTHORED),
            Token::from(t::TYPE_NAME),
        ];
        for key in [t::COLOR_SPACE, t::INTERPOLATION, t::ELEMENT_SIZE] {
            if self.metadata(key).is_some() {
                names.push(Token::from(key));
            }
        }
        if self.connections().is_some() {
            names.push(Token::from(t::CONNECTIONS));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let attribute = self.attribute()?;
        match name.as_str() {
            t::VALUE => Some(AttributeValue::data_source(attribute, self.ctx.globals.clone())),
            t::AUTHORED => Some(value(attribute.has_authored_value().unwrap_or(false))),
            t::TYPE_NAME => attribute
                .type_name()
                .ok()
                .flatten()
                .map(|ty| value(Value::Token(ty.as_token()))),
            t::COLOR_SPACE | t::INTERPOLATION | t::ELEMENT_SIZE => self.metadata(name.as_str()).map(value),
            t::CONNECTIONS => self.connections().map(|c| value(Value::PathVec(c))),
            _ => None,
        }
    }
}

/// Every relationship of a prim.
struct Relationships(PrimContext);

impl Container for Relationships {
    fn names(&self) -> Vec<Token> {
        self.0
            .prim()
            .and_then(|p| p.relationships().ok())
            .map(|rels| rels.iter().map(|r| Token::from(r.name())).collect())
            .unwrap_or_default()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let relationship = self.0.prim()?.relationship(name.clone());
        if !relationship.is_defined().unwrap_or(false) {
            return None;
        }
        let targets = relationship.targets().unwrap_or_default();
        let forwarded = relationship.forwarded_targets().unwrap_or_default();
        Some(crate::data_source::retained::container([
            (t::TARGETS, value(Value::PathVec(targets))),
            (t::FORWARDED_TARGETS, value(Value::PathVec(forwarded))),
        ]))
    }
}

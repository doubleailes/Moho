//! Visibility, purpose and extent, as UsdImaging's prim data source exposes
//! them: visibility only when authored and `false` only when invisible;
//! purpose as an inheritable opinion when authored, else a fallback; extent
//! when authored.

use openusd::gf;
use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::MappedAttributeValue;
use crate::tokens::imageable as t;

/// Contributes `visibility`, `purpose` and `extent`.
pub struct ImageableAdapter;

fn authored(ctx: &PrimContext, name: &str) -> bool {
    ctx.prim()
        .is_some_and(|p| p.attribute(name).has_authored_value().unwrap_or(false))
}

/// The purpose a prim exposes: `(purpose, authored)`, `default` read as
/// `geometry`, or `None` for an unauthored `default`.
fn purpose(ctx: &PrimContext) -> Option<(Token, bool)> {
    let attribute = ctx.prim()?.attribute(t::PURPOSE);
    let token = match attribute.get::<Value>().ok().flatten()? {
        Value::Token(token) => token,
        _ => return None,
    };
    let authored = attribute.has_authored_value().unwrap_or(false);
    if !authored && token == t::DEFAULT {
        return None;
    }
    let token = if token == t::DEFAULT {
        Token::from(t::GEOMETRY)
    } else {
        token
    };
    Some((token, authored))
}

fn invisible(value: Value) -> Option<Value> {
    match value {
        Value::Token(t) if t == t::INVISIBLE => Some(Value::Bool(false)),
        _ => None,
    }
}

fn extent_corner(value: Value, corner: usize) -> Option<Value> {
    match value {
        Value::Vec3fVec(v) => v.get(corner).map(|c| {
            Value::Vec3d(gf::Vec3d {
                x: f64::from(c.x),
                y: f64::from(c.y),
                z: f64::from(c.z),
            })
        }),
        Value::Vec3dVec(v) => v.get(corner).copied().map(Value::Vec3d),
        _ => None,
    }
}

impl Adapter for ImageableAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            return Vec::new();
        }
        let mut names = Vec::new();
        if authored(ctx, t::VISIBILITY) {
            names.push(Token::from(t::VISIBILITY));
        }
        if purpose(ctx).is_some() {
            names.push(Token::from(t::PURPOSE));
        }
        if authored(ctx, t::EXTENT) {
            names.push(Token::from(t::EXTENT));
        }
        names
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if ctx.path.is_abs_root() {
            return None;
        }
        match name.as_str() {
            t::VISIBILITY if authored(ctx, t::VISIBILITY) => {
                let attribute = ctx.prim()?.attribute(t::VISIBILITY);
                let locator = Locator::new([t::VISIBILITY]);
                let visibility = MappedAttributeValue::data_source(attribute, ctx, locator, invisible);
                Some(container([(t::VISIBILITY, visibility)]))
            }
            t::PURPOSE => {
                let (token, authored) = purpose(ctx)?;
                let entries = if authored {
                    vec![
                        (t::PURPOSE, value(Value::Token(token))),
                        (t::INHERITABLE, value(true)),
                    ]
                } else {
                    vec![(t::FALLBACK, value(Value::Token(token)))]
                };
                Some(container(entries))
            }
            t::EXTENT if authored(ctx, t::EXTENT) => {
                let attribute = ctx.prim()?.attribute(t::EXTENT);
                let locator = Locator::new([t::EXTENT]);
                let min = MappedAttributeValue::data_source(attribute.clone(), ctx, locator.clone(), |v| {
                    extent_corner(v, 0)
                });
                let max = MappedAttributeValue::data_source(attribute, ctx, locator, |v| extent_corner(v, 1));
                Some(container([(t::MIN, min), (t::MAX, max)]))
            }
            _ => None,
        }
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        match name {
            t::VISIBILITY | t::PURPOSE | t::EXTENT => LocatorSet::from(Locator::new([name])),
            _ => LocatorSet::new(),
        }
    }
}

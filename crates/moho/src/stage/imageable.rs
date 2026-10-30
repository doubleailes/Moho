//! Visibility, purpose and extent, as UsdImaging's prim data source exposes
//! them: `visibility` named on every Imageable, given when authored, `false`
//! only when invisible at the index's time (unsampled); purpose, on an
//! Imageable whose purpose is authored or not `default`, as an inheritable
//! opinion when authored, else a fallback; `extent` named on every Boundable,
//! given when authored.

use openusd::gf;
use openusd::sdf::Value;
use openusd::tf::Token;

use std::rc::Rc;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, DataSource};
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

/// `visibility`: `false` when the attribute reads `invisible` at the
/// index's time, else nothing (inherited), as `UsdImagingDataSourceVisibility`
/// (which does not vary across the shutter).
struct Visibility {
    attribute: openusd::usd::Attribute,
    ctx: PrimContext,
}

impl Container for Visibility {
    fn names(&self) -> Vec<Token> {
        vec![Token::from(t::VISIBILITY)]
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name != t::VISIBILITY {
            return None;
        }
        let read = self
            .attribute
            .get_at::<Value>(self.ctx.globals.time())
            .ok()
            .flatten();
        matches!(read, Some(Value::Token(t)) if t == t::INVISIBLE).then(|| value(false))
    }
}

/// `purpose`: `purpose` and `inheritable` when authored, else `fallback`
/// (`UsdImagingDataSourcePurpose`, which names all three).
struct Purpose {
    token: Token,
    authored: bool,
}

impl Container for Purpose {
    fn names(&self) -> Vec<Token> {
        [t::PURPOSE, t::INHERITABLE, t::FALLBACK]
            .map(Token::from)
            .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::PURPOSE if self.authored => Some(value(Value::Token(self.token.clone()))),
            t::INHERITABLE if self.authored => Some(value(true)),
            t::FALLBACK if !self.authored => Some(value(Value::Token(self.token.clone()))),
            _ => None,
        }
    }
}

/// One corner of an extent, as a double vector; a missing corner reads
/// (0, 0, 0), as `UsdImagingDataSourceExtentCoordinate` answers.
pub(crate) fn extent_corner(value: Value, corner: usize) -> Option<Value> {
    let zero = gf::Vec3d {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    match value {
        Value::Vec3fVec(v) => Some(Value::Vec3d(v.get(corner).map_or(zero, |c| gf::Vec3d {
            x: f64::from(c.x),
            y: f64::from(c.y),
            z: f64::from(c.z),
        }))),
        Value::Vec3dVec(v) => Some(Value::Vec3d(v.get(corner).copied().unwrap_or(zero))),
        _ => None,
    }
}

fn is_a(ctx: &PrimContext, schema: &'static str) -> bool {
    ctx.prim().is_some_and(|p| crate::stage::is_a(&p, schema))
}

impl Adapter for ImageableAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            return Vec::new();
        }
        let mut names = Vec::new();
        if is_a(ctx, "Imageable") {
            names.push(Token::from(t::VISIBILITY));
            if purpose(ctx).is_some() {
                names.push(Token::from(t::PURPOSE));
            }
        }
        if is_a(ctx, "Boundable") {
            names.push(Token::from(t::EXTENT));
        }
        names
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if ctx.path.is_abs_root() {
            return None;
        }
        match name.as_str() {
            t::VISIBILITY if is_a(ctx, "Imageable") && authored(ctx, t::VISIBILITY) => {
                let attribute = ctx.prim()?.attribute(t::VISIBILITY);
                if attribute.might_be_time_varying().unwrap_or(false) {
                    ctx.globals
                        .flag_time_varying(&ctx.path, Locator::new([t::VISIBILITY]));
                }
                Some(DataSource::Container(Rc::new(Visibility {
                    attribute,
                    ctx: ctx.clone(),
                })))
            }
            t::PURPOSE if is_a(ctx, "Imageable") => {
                let (token, authored) = purpose(ctx)?;
                Some(DataSource::Container(Rc::new(Purpose { token, authored })))
            }
            t::EXTENT if is_a(ctx, "Boundable") && authored(ctx, t::EXTENT) => {
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

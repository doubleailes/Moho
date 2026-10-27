//! UsdImaging's model data: `model` (`UsdImagingModelSchema`, a model's
//! asset info), `extentsHint` (`UsdImagingExtentsHintSchema`, one range per
//! purpose) and `geomModel` (`UsdImagingGeomModelSchema`, `GeomModelAPI`'s
//! draw mode attributes).

use std::rc::Rc;

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd::usd::{self, Object as _};

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, DataSource, Overlay};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::MappedAttributeValue;
use crate::stage::table::{Member, TableContainer};
use crate::tokens::{geom_model, imageable, model as t};

/// The purposes of `extentsHint`'s ranges, in their authored order
/// (`UsdGeomImageable::GetOrderedPurposeTokens`, `default` read as Hydra's
/// `geometry`).
const ORDERED_PURPOSES: &[&str] = &[imageable::GEOMETRY, "render", "proxy", "guide"];

/// Contributes `model` and `extentsHint` to every prim, as
/// `UsdImagingDataSourcePrim` does.
pub struct ModelAdapter;

/// Whether the prim authors asset info (`UsdPrim::HasAssetInfo`).
fn has_asset_info(prim: &usd::Prim) -> bool {
    prim.has_authored_metadata("assetInfo").unwrap_or(false)
}

fn extents_hint(ctx: &PrimContext) -> Option<usd::Attribute> {
    let attribute = ctx.prim()?.attribute(t::EXTENTS_HINT);
    attribute
        .has_authored_value()
        .unwrap_or(false)
        .then_some(attribute)
}

fn corner(value: Value, index: usize) -> Option<Value> {
    match value {
        Value::Vec3fVec(v) => v.get(index).map(|c| {
            Value::Vec3d(gf::Vec3d {
                x: f64::from(c.x),
                y: f64::from(c.y),
                z: f64::from(c.z),
            })
        }),
        Value::Vec3dVec(v) => v.get(index).copied().map(Value::Vec3d),
        _ => None,
    }
}

/// `extentsHint`: a range per purpose, as many as the attribute holds pairs
/// (`UsdImagingDataSourceExtentsHint`).
struct ExtentsHint {
    ctx: PrimContext,
    attribute: usd::Attribute,
}

impl ExtentsHint {
    fn count(&self) -> usize {
        let pairs = self
            .attribute
            .get::<Value>()
            .ok()
            .flatten()
            .and_then(|v| v.array_len())
            .unwrap_or(0)
            / 2;
        pairs.min(ORDERED_PURPOSES.len())
    }
}

impl Container for ExtentsHint {
    fn names(&self) -> Vec<Token> {
        ORDERED_PURPOSES[..self.count()]
            .iter()
            .map(|p| Token::from(*p))
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let i = ORDERED_PURPOSES[..self.count()].iter().position(|p| name == *p)?;
        // The time variance is flagged for the whole `extentsHint`.
        let locator = Locator::new([t::EXTENTS_HINT]);
        let min =
            MappedAttributeValue::data_source(self.attribute.clone(), &self.ctx, locator.clone(), move |v| {
                corner(v, 2 * i)
            });
        let max = MappedAttributeValue::data_source(self.attribute.clone(), &self.ctx, locator, move |v| {
            corner(v, 2 * i + 1)
        });
        Some(container([(imageable::MIN, min), (imageable::MAX, max)]))
    }
}

/// `model`: the model's path and its asset info
/// (`UsdImagingDataSourcePrim_ModelAPI`).
struct Model {
    prim: usd::Prim,
    path: sdf::Path,
}

impl Model {
    fn asset_info(&self, key: &str) -> Option<Value> {
        match self.prim.metadata::<Value>("assetInfo").ok()?? {
            Value::Dictionary(info) => info.get_at(key).cloned(),
            _ => None,
        }
    }
}

impl Container for Model {
    fn names(&self) -> Vec<Token> {
        [
            t::MODEL_PATH,
            t::ASSET_IDENTIFIER,
            t::ASSET_NAME,
            t::ASSET_VERSION,
        ]
        .map(Token::from)
        .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::MODEL_PATH => Some(value(Value::PathVec(vec![self.path.clone()]))),
            t::ASSET_IDENTIFIER => match self.asset_info("identifier")? {
                v @ Value::AssetPath(_) => Some(value(v)),
                _ => None,
            },
            t::ASSET_NAME => match self.asset_info("name")? {
                v @ Value::String(_) => Some(value(v)),
                _ => None,
            },
            t::ASSET_VERSION => match self.asset_info("version")? {
                v @ Value::String(_) => Some(value(v)),
                _ => None,
            },
            _ => None,
        }
    }
}

impl Adapter for ModelAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        let Some(prim) = ctx.prim().filter(|_| !ctx.path.is_abs_root()) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        if prim.is_model().unwrap_or(false) {
            names.push(Token::from(t::MODEL));
        }
        if extents_hint(ctx).is_some() {
            names.push(Token::from(t::EXTENTS_HINT));
        }
        names
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if ctx.path.is_abs_root() {
            return None;
        }
        match name.as_str() {
            // Read for any prim authoring asset info, model or not, as
            // UsdImaging does for texture resolution.
            t::MODEL => {
                let prim = ctx.prim()?;
                has_asset_info(&prim).then(|| {
                    DataSource::Container(Rc::new(Model {
                        prim,
                        path: ctx.path.clone(),
                    }))
                })
            }
            // Read whether or not the prim is a model or applies
            // `GeomModelAPI`, as UsdImaging does.
            t::EXTENTS_HINT => {
                let attribute = extents_hint(ctx)?;
                Some(DataSource::Container(Rc::new(ExtentsHint {
                    ctx: ctx.clone(),
                    attribute,
                })))
            }
            _ => None,
        }
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        if name == t::EXTENTS_HINT {
            LocatorSet::from(Locator::new([t::EXTENTS_HINT]))
        } else {
            LocatorSet::new()
        }
    }
}

/// `GeomModelAPI`'s `model:*` attributes, by their names less `model:`, in
/// the sorted order of `UsdImagingDataSourceMapped`.
const GEOM_MODEL: &[Member] = &[
    Member::Attr(geom_model::APPLY_DRAW_MODE, "model:applyDrawMode"),
    Member::Attr(geom_model::CARD_GEOMETRY, "model:cardGeometry"),
    Member::Attr(geom_model::CARD_TEXTURE_X_NEG, "model:cardTextureXNeg"),
    Member::Attr(geom_model::CARD_TEXTURE_X_POS, "model:cardTextureXPos"),
    Member::Attr(geom_model::CARD_TEXTURE_Y_NEG, "model:cardTextureYNeg"),
    Member::Attr(geom_model::CARD_TEXTURE_Y_POS, "model:cardTextureYPos"),
    Member::Attr(geom_model::CARD_TEXTURE_Z_NEG, "model:cardTextureZNeg"),
    Member::Attr(geom_model::CARD_TEXTURE_Z_POS, "model:cardTextureZPos"),
    Member::Attr(geom_model::CARD_VISIBILITY, "model:cardVisibility"),
    Member::Attr(geom_model::DRAW_MODE, "model:drawMode"),
    Member::Attr(geom_model::DRAW_MODE_COLOR, "model:drawModeColor"),
];

/// Contributes `geomModel` to prims applying `GeomModelAPI`, as
/// `UsdImagingGeomModelAPIAdapter`: a component overlays
/// `applyDrawMode = true`.
pub struct GeomModelAdapter;

impl Adapter for GeomModelAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(geom_model::GEOM_MODEL)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != geom_model::GEOM_MODEL {
            return None;
        }
        let mapped = TableContainer::data_source(ctx, Locator::new([geom_model::GEOM_MODEL]), GEOM_MODEL);
        let component = ctx
            .prim()
            .and_then(|p| {
                usd::ModelAPI::from_prim_unchecked(p)
                    .is_kind("component", Default::default())
                    .ok()
            })
            .unwrap_or(false);
        if !component {
            return Some(mapped);
        }
        let apply = container([(geom_model::APPLY_DRAW_MODE, value(true))]);
        let (Some(apply), Some(mapped)) = (apply.as_container(), mapped.as_container()) else {
            return None;
        };
        Some(DataSource::Container(Overlay::handle([
            apply.clone(),
            mapped.clone(),
        ])))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        crate::stage::table::invalidate(&Locator::new([geom_model::GEOM_MODEL]), GEOM_MODEL, name)
    }
}

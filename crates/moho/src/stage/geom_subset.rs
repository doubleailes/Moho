//! `GeomSubset` prims as UsdImaging's geom subset adapter: prims of type
//! `geomSubset` with `geomSubset/{type, indices}`, beside the prim data
//! every prim has (material bindings among them). The consumer gathers a
//! mesh's subsets from its children; a subset edit does not dirty the mesh.

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::data_source::retained::container;
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::{AttributeValue, MappedAttributeValue};
use crate::tokens::geom_subset as t;
use crate::tokens::prim_type;

/// The adapter of `GeomSubset` prims.
pub struct GeomSubsetAdapter;

/// USD's `elementType` as Hydra's subset type: `face` and `point` only;
/// `edge` and `tetrahedron` give the empty token, as UsdImaging's.
fn hydra_type(value: Value) -> Option<Value> {
    let Value::Token(element) = value else {
        return Some(Value::Token(Token::default()));
    };
    let ty = match element.as_str() {
        "face" => t::TYPE_FACE_SET,
        "point" => t::TYPE_POINT_SET,
        _ => "",
    };
    Some(Value::Token(Token::from(ty)))
}

impl Adapter for GeomSubsetAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::GEOM_SUBSET)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::GEOM_SUBSET)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::GEOM_SUBSET {
            return None;
        }
        let prim = ctx.prim()?;
        // Neither is flagged time-varying, as UsdImaging builds them.
        Some(container([
            (
                t::TYPE,
                MappedAttributeValue::unflagged(prim.attribute("elementType"), ctx, hydra_type),
            ),
            (
                t::INDICES,
                AttributeValue::unflagged(prim.attribute("indices"), ctx),
            ),
        ]))
    }

    /// `indices` dirties the locator `indices` and `elementType` the locator
    /// `type`, at the top of the prim, not under `geomSubset`: UsdImaging's
    /// quirk, reproduced.
    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        match name {
            "indices" => LocatorSet::from(Locator::new([t::INDICES])),
            "elementType" => LocatorSet::from(Locator::new([t::TYPE])),
            _ => LocatorSet::new(),
        }
    }
}

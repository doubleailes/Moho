//! The `mesh` data of `Mesh` prims: topology, subdivision scheme and tags,
//! and sidedness, as UsdImaging's mesh data source.

use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::table::{Member, TableContainer, invalidate};
use crate::tokens::mesh as t;
use crate::tokens::prim_type;

const MESH: &[Member] = &[
    Member::Nested(
        t::TOPOLOGY,
        &[
            Member::Attr(t::FACE_VERTEX_COUNTS, "faceVertexCounts"),
            Member::Attr(t::FACE_VERTEX_INDICES, "faceVertexIndices"),
            Member::Attr(t::HOLE_INDICES, "holeIndices"),
            Member::Attr(t::ORIENTATION, "orientation"),
        ],
    ),
    Member::Attr(t::SUBDIVISION_SCHEME, "subdivisionScheme"),
    Member::Nested(
        t::SUBDIVISION_TAGS,
        &[
            Member::Attr(t::INTERPOLATE_BOUNDARY, "interpolateBoundary"),
            Member::Attr(
                t::FACE_VARYING_LINEAR_INTERPOLATION,
                "faceVaryingLinearInterpolation",
            ),
            Member::Attr(t::TRIANGLE_SUBDIVISION_RULE, "triangleSubdivisionRule"),
            Member::Attr(t::CORNER_INDICES, "cornerIndices"),
            Member::Attr(t::CORNER_SHARPNESSES, "cornerSharpnesses"),
            Member::Attr(t::CREASE_INDICES, "creaseIndices"),
            Member::Attr(t::CREASE_LENGTHS, "creaseLengths"),
            Member::Attr(t::CREASE_SHARPNESSES, "creaseSharpnesses"),
        ],
    ),
    Member::Attr(t::DOUBLE_SIDED, "doubleSided"),
];

/// The adapter of `Mesh` prims.
pub struct MeshAdapter;

impl Adapter for MeshAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::MESH)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::MESH)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::MESH).then(|| TableContainer::data_source(ctx, Locator::new([t::MESH]), MESH))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        invalidate(&Locator::new([t::MESH]), MESH, name)
    }
}

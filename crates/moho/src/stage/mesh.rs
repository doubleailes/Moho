//! The `mesh` data of `Mesh` prims: topology, subdivision scheme and tags,
//! and sidedness, as UsdImaging's mesh data source: only the topology's
//! arrays are flagged time-varying, and a topology or subdivision tag edit
//! dirties its whole container.

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
            Member::Unflagged(t::ORIENTATION, "orientation"),
        ],
    ),
    Member::Unflagged(t::SUBDIVISION_SCHEME, "subdivisionScheme"),
    Member::Nested(
        t::SUBDIVISION_TAGS,
        &[
            Member::Unflagged(t::INTERPOLATE_BOUNDARY, "interpolateBoundary"),
            Member::Unflagged(
                t::FACE_VARYING_LINEAR_INTERPOLATION,
                "faceVaryingLinearInterpolation",
            ),
            Member::Unflagged(t::TRIANGLE_SUBDIVISION_RULE, "triangleSubdivisionRule"),
            Member::Unflagged(t::CORNER_INDICES, "cornerIndices"),
            Member::Unflagged(t::CORNER_SHARPNESSES, "cornerSharpnesses"),
            Member::Unflagged(t::CREASE_INDICES, "creaseIndices"),
            Member::Unflagged(t::CREASE_LENGTHS, "creaseLengths"),
            Member::Unflagged(t::CREASE_SHARPNESSES, "creaseSharpnesses"),
        ],
    ),
    Member::Unflagged(t::DOUBLE_SIDED, "doubleSided"),
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

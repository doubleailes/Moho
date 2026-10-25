//! The `basisCurves` and `nurbsCurves` data of curve prims; `widths` reach
//! them as primvars.

use crate::stage::table::{Member, TableAdapter};
use crate::tokens::curves as t;
use crate::tokens::prim_type;

const BASIS: &[Member] = &[Member::Nested(
    t::TOPOLOGY,
    &[
        Member::Attr(t::CURVE_VERTEX_COUNTS, "curveVertexCounts"),
        Member::Attr(t::TYPE, "type"),
        Member::Attr(t::BASIS, "basis"),
        Member::Attr(t::WRAP, "wrap"),
    ],
)];

const NURBS: &[Member] = &[
    Member::Attr(t::CURVE_VERTEX_COUNTS, "curveVertexCounts"),
    Member::Attr(t::ORDER, "order"),
    Member::Attr(t::KNOTS, "knots"),
    Member::Attr(t::RANGES, "ranges"),
    Member::Attr(t::POINT_WEIGHTS, "pointWeights"),
];

/// The adapter of `BasisCurves` prims.
pub const BASIS_CURVES: TableAdapter = TableAdapter {
    prim_type: prim_type::BASIS_CURVES,
    schema: t::BASIS_CURVES,
    members: BASIS,
};

/// The adapter of `NurbsCurves` prims.
pub const NURBS_CURVES: TableAdapter = TableAdapter {
    prim_type: prim_type::NURBS_CURVES,
    schema: t::NURBS_CURVES,
    members: NURBS,
};

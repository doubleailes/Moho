//! The `basisCurves` and `nurbsCurves` data of curve prims; `widths`, and a
//! NURBS curve's `pointWeights`, reach them as primvars, as UsdImaging's
//! curve data sources give them.

use crate::stage::table::{Member, TableAdapter};
use crate::tokens::curves as t;
use crate::tokens::prim_type;

/// As UsdImaging's basis curves data source: only `curveVertexCounts` is
/// flagged time-varying, and each topology member is dirtied on its own.
const BASIS: &[Member] = &[Member::LeafNested(
    t::TOPOLOGY,
    &[
        Member::Attr(t::CURVE_VERTEX_COUNTS, "curveVertexCounts"),
        Member::Unflagged(t::TYPE, "type"),
        Member::Unflagged(t::BASIS, "basis"),
        Member::Unflagged(t::WRAP, "wrap"),
    ],
)];

/// In the sorted order of UsdImaging's mapped data source.
const NURBS: &[Member] = &[
    Member::Attr(t::CURVE_VERTEX_COUNTS, "curveVertexCounts"),
    Member::Attr(t::KNOTS, "knots"),
    Member::Attr(t::ORDER, "order"),
    Member::Attr(t::RANGES, "ranges"),
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

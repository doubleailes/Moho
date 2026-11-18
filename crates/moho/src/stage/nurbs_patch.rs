//! The `nurbsPatch` data of `NurbsPatch` prims, as UsdImaging's NURBS patch
//! data source (`dataSourceNurbsPatch.cpp`) gives it: the patch's own
//! attributes, the `trimCurve:*` ones nested under `trimCurve`, and the
//! gprim's `orientation` and `doubleSided`. `pointWeights` reaches the prim
//! as a primvar, as a NURBS curve's does; `points` and the other point-based
//! attributes too.
//!
//! Turning a patch into something drawable (Storm's
//! `HdsiNurbsApproximatingSceneIndex`) is the renderer's business.

use crate::stage::table::{Member, TableAdapter};
use crate::tokens::nurbs_patch as t;
use crate::tokens::prim_type;

/// `HdNurbsPatchTrimCurveSchema`, in the sorted order of UsdImaging's mapped
/// data source; each member is dirtied on its own.
const TRIM_CURVE: &[Member] = &[
    Member::Attr(t::COUNTS, "trimCurve:counts"),
    Member::Attr(t::KNOTS, "trimCurve:knots"),
    Member::Attr(t::ORDERS, "trimCurve:orders"),
    Member::Attr(t::POINTS, "trimCurve:points"),
    Member::Attr(t::RANGES, "trimCurve:ranges"),
    Member::Attr(t::VERTEX_COUNTS, "trimCurve:vertexCounts"),
];

/// `HdNurbsPatchSchema`, in the sorted order of UsdImaging's mapped data
/// source.
const NURBS: &[Member] = &[
    Member::Attr(t::DOUBLE_SIDED, "doubleSided"),
    Member::Attr(t::ORIENTATION, "orientation"),
    Member::LeafNested(t::TRIM_CURVE, TRIM_CURVE),
    Member::Attr(t::U_FORM, "uForm"),
    Member::Attr(t::U_KNOTS, "uKnots"),
    Member::Attr(t::U_ORDER, "uOrder"),
    Member::Attr(t::U_RANGE, "uRange"),
    Member::Attr(t::U_VERTEX_COUNT, "uVertexCount"),
    Member::Attr(t::V_FORM, "vForm"),
    Member::Attr(t::V_KNOTS, "vKnots"),
    Member::Attr(t::V_ORDER, "vOrder"),
    Member::Attr(t::V_RANGE, "vRange"),
    Member::Attr(t::V_VERTEX_COUNT, "vVertexCount"),
];

/// The adapter of `NurbsPatch` prims.
pub const NURBS_PATCH: TableAdapter = TableAdapter {
    prim_type: prim_type::NURBS_PATCH,
    schema: t::NURBS_PATCH,
    members: NURBS,
};

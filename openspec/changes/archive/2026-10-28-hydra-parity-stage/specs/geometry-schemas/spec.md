## MODIFIED Requirements

### Requirement: Basis and NURBS curves
A `BasisCurves` SHALL have type `basisCurves` and SHALL expose
`basisCurves/topology` with `curveVertexCounts`, `type`, `basis` and `wrap`,
and `widths` as a primvar. A `NurbsCurves` SHALL have type `nurbsCurves` and
SHALL expose `nurbsCurves` with `curveVertexCounts`, `order`, `knots` and
`ranges`, with `widths` and `pointWeights` as primvars, as UsdImaging's
NURBS curves data source does.

#### Scenario: Cubic B-spline hair
- **WHEN** a basis curves prim authors type `cubic`, basis `bspline` and two curves of 4 vertices
- **THEN** its topology gives those, and `primvars/widths` holds its widths

#### Scenario: Rational NURBS curves
- **WHEN** a NURBS curves prim authors `pointWeights`
- **THEN** `primvars/pointWeights/primvarValue` gives them, and `nurbsCurves` has no `pointWeights`

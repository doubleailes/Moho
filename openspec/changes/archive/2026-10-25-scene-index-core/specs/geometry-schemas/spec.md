## Purpose

The typed data a scene index exposes for geometry, with Hydra 2.0's schema
names: transforms, meshes and subdivision, primvars, visibility, purpose,
extent, basis curves, NURBS curves and implicit shapes.

## ADDED Requirements

### Requirement: Transform
A transformable prim SHALL expose `xform` with `matrix`, its local transform
from its `xformOpOrder` at the index's time, and `resetXformStack`. A prim
whose transform ops have no effect SHALL expose no `xform`.

#### Scenario: A translated prim
- **WHEN** a prim authors `xformOp:translate = (1, 2, 3)` in its `xformOpOrder`
- **THEN** its `xform/matrix` translates by (1, 2, 3), and `xform/resetXformStack` is false

#### Scenario: Sampled ops
- **WHEN** a prim's translate is sampled at 1 and 10
- **THEN** `xform/matrix` reports both samples as contributing over an interval spanning them

### Requirement: Mesh and subdivision
A `Mesh` SHALL have type `mesh` and SHALL expose `mesh` with:
- `topology`: `faceVertexCounts`, `faceVertexIndices`, `holeIndices` and `orientation`;
- `subdivisionScheme`;
- `subdivisionTags`: `interpolateBoundary`, `faceVaryingLinearInterpolation`, `triangleSubdivisionRule`, `cornerIndices`, `cornerSharpnesses`, `creaseIndices`, `creaseLengths` and `creaseSharpnesses`;
- `doubleSided`.

#### Scenario: A quad
- **WHEN** a mesh authors one face of four vertices
- **THEN** `mesh/topology/faceVertexCounts` is [4] and `mesh/topology/faceVertexIndices` holds its four indices

#### Scenario: Creases
- **WHEN** a Catmull-Clark mesh authors crease indices, lengths and sharpnesses
- **THEN** `mesh/subdivisionScheme` is `catmullClark` and `mesh/subdivisionTags` holds those three arrays

### Requirement: Primvars
A geometry prim SHALL expose `primvars`, keyed by name, holding:
- every authored `primvars:<name>`;
- `points`, `normals`, `velocities` and `accelerations` on point-based prims;
- `widths` on curves.

Each primvar SHALL give:
- `primvarValue`, or `indexedPrimvarValue` with `indices` when indexed;
- `interpolation`, defaulting to constant;
- `role`, from the value type's role;
- `elementSize`;
- `colorSpace` for colour primvars.

A consumer SHALL be able to read the flattened (de-indexed) value. A
`primvars:normals` SHALL take precedence over `normals`.

#### Scenario: Indexed UVs
- **WHEN** a mesh authors `primvars:st` with `primvars:st:indices` and interpolation `faceVarying`
- **THEN** `primvars/st` gives `indexedPrimvarValue`, `indices` and interpolation `faceVarying`, and its flattened value equals the values gathered through the indices

#### Scenario: Points
- **WHEN** a mesh authors `points`
- **THEN** `primvars/points/primvarValue` gives them, with interpolation `vertex` and role `point`

### Requirement: Visibility, purpose and extent
A prim that authors `visibility` SHALL expose `visibility/visibility`: false
when invisible, nothing otherwise. A prim SHALL expose `purpose` with
`purpose` and `inheritable` when authored, and `fallback` otherwise. USD's
`default` SHALL map to `geometry`. A boundable prim that authors `extent`
SHALL expose `extent` with `min` and `max`.

#### Scenario: Invisible
- **WHEN** a prim authors `visibility = invisible`
- **THEN** its `visibility/visibility` is false

#### Scenario: Proxy purpose
- **WHEN** a prim authors `purpose = proxy`
- **THEN** its `purpose/purpose` is `proxy` and `purpose/inheritable` is true

### Requirement: Basis and NURBS curves
A `BasisCurves` SHALL have type `basisCurves` and SHALL expose
`basisCurves/topology` with `curveVertexCounts`, `type`, `basis` and `wrap`,
and `widths` as a primvar. A `NurbsCurves` SHALL have type `nurbsCurves` and
SHALL expose `curveVertexCounts`, `order`, `knots`, `ranges` and
`pointWeights`, with `widths` as a primvar.

#### Scenario: Cubic B-spline hair
- **WHEN** a basis curves prim authors type `cubic`, basis `bspline` and two curves of 4 vertices
- **THEN** its topology gives those, and `primvars/widths` holds its widths

### Requirement: Implicit shapes
`Sphere`, `Cube`, `Cone`, `Cylinder`, `Capsule` and `Plane` SHALL have their
own type (`sphere`, `cube`, `cone`, `cylinder`, `capsule`, `plane`) and SHALL
expose a schema of the same name holding their attributes:
- `radius`, `size`, `height` and `axis`;
- `radiusTop` and `radiusBottom` for `Cylinder_1` and `Capsule_1`;
- `width`, `length` and `doubleSided` for planes.

Shapes SHALL NOT be converted to meshes.

#### Scenario: A tapered cylinder
- **WHEN** a `Cylinder_1` authors radiusBottom 2, radiusTop 1, height 4 and axis `X`
- **THEN** its type is `cylinder` and `cylinder` gives those four values

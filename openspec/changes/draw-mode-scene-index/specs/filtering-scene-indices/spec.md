## ADDED Requirements

### Requirement: Draw mode
`DrawModeSceneIndex` SHALL be `UsdImagingDrawModeSceneIndex`. A prim whose
`geomModel/applyDrawMode` is true and whose `geomModel/drawMode` is
`origin`, `bounds` or `cards` SHALL be replaced, with its subtree, by its
standin: the prim itself with no type and no data, and
- `origin`: `<prim>/originCurves`, a `basisCurves` prim of three unit
  segments from the origin along the axes;
- `bounds`: `<prim>/boundsCurves`, a `basisCurves` prim of the twelve edges
  of the box the prim's `extent` spans;
- `cards`: `<prim>/cardsMesh`, a `mesh` of a quad per face drawn (`box`:
  the faces of the extent's box; `cross`: the quads halving it; a face is
  drawn when it or its opposite has a `cardTexture*`, every face when none
  has), and, when any face is textured, a `geomSubset`
  `<prim>/cardsMesh/subset<Axis>` per quad bound (`materialBindings`) to a
  `material` `<prim>/subsetMaterial<Axis>` per textured face.
Each standin prim SHALL take `xform`, `purpose`, `visibility`,
`instancedBy`, `primOrigin` and `extent` from the replaced prim, and SHALL
have `primvars` `displayColor` (`geomModel/drawModeColor`, else 0.18 grey),
`displayOpacity` and `widths`. Any other draw mode SHALL keep the prim's
geometry. Notices SHALL be UsdImaging's: a draw mode change removes the
prim and adds what replaces it; a dirtied `extent` or `drawModeColor`
dirties the standin's points or colour; a dirtied card attribute rebuilds
the cards; a prim under a standin is neither added nor dirtied.

#### Scenario: Bounds
- **WHEN** a component authors `model:drawMode = "bounds"` and the chain adds the draw mode scene index
- **THEN** its only child is `boundsCurves`, whose points are the corners of its extent, and its own geometry is gone

#### Scenario: A mode edit
- **WHEN** the draw mode of a prim changes from `default` to `bounds`
- **THEN** the prim is notified removed, then added with its standin's prims

#### Scenario: Textured cards
- **WHEN** a `cross` card authors only `model:cardTextureXPos`
- **THEN** `cardsMesh` has subsets `subsetXPos` and `subsetXNeg`, both bound to `subsetMaterialXPos`, a `UsdPreviewSurface` reading a `UsdUVTexture` of that texture through `cardsUv`

#### Scenario: Inside a native prototype
- **WHEN** a component with a draw mode lies inside an instanced assembly
- **THEN** its standin is in the propagated prototype under the instancer, which instances every instance

### Requirement: Unloaded prims drawn as bounds
`UnloadedDrawModeSceneIndex` SHALL be `UsdImagingUnloadedDrawModeSceneIndex`:
a prim whose `__usdPrimInfo/isLoaded` is false SHALL have `geomModel`
`applyDrawMode` true and `drawMode` `bounds`, stronger than its own.

#### Scenario: An unloaded payload
- **WHEN** a stage is opened with no payload loaded and the chain displays unloaded prims with bounds
- **THEN** the payload's prim has a `boundsCurves` child over its `extentsHint`

### Requirement: Standard chain options
`chain::standard_with` SHALL build the filters `UsdImagingCreateSceneIndices`
builds with `addDrawModeSceneIndex` and `displayUnloadedPrimsWithBounds` as
given, in its order: the unloaded filter after the stage index, with the
stage index including unloaded prims; the draw mode index as native
instancer propagation's append callback. With both off it SHALL be
`chain::standard`.

#### Scenario: Off by default
- **WHEN** a consumer builds `chain::standard` over a stage with a draw mode
- **THEN** the model keeps its geometry

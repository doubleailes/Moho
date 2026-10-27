## ADDED Requirements

### Requirement: Flattening inside native instancer propagation
Flattening SHALL run inside native instancer propagation, as Hydra's
`HdFlatteningSceneIndex` with `UsdImagingFlattenedDataSourceProviders`
does: once over the stage without its USD prototypes, and once over each
propagated prototype in the prototype's own namespace, its root carrying
the group's instance data. Instance aggregation SHALL read the flattened
data. A flattening index SHALL remain usable on its own, outside the
standard chain.

#### Scenario: Prototype prims are flattened in their prototype
- **WHEN** an instance at `/world/inst` is translated by (5, 0, 0) and its asset's mesh by (0, 1, 0)
- **THEN** the prototype's mesh has the flattened transform (0, 1, 0), and the instancer's `hydra:instanceTransforms` holds (5, 0, 0) for that instance

### Requirement: Flattened model data and up axis
A flattening index SHALL also flatten, as UsdImaging's providers do:
- `__usdUpAxis` and `model`, a prim's own data overlaying its parent's flattened data;
- `geomModel`: a prim without its own takes its parent's flattened `geomModel`; a prim with its own takes `drawMode` and `cardVisibility` from its parent's when it has none (or `drawMode` is `inherited`).

A change to these on a prim SHALL dirty the same data on every descendant
that inherits it.

#### Scenario: The up axis reaches every prim
- **WHEN** the stage authors `upAxis = "Z"`
- **THEN** every prim of the flattened index has `__usdUpAxis/upAxis` `Z`

#### Scenario: An inherited draw mode
- **WHEN** a model authors `model:drawMode = "bounds"` and its child applies `GeomModelAPI` without a draw mode
- **THEN** the child's flattened `geomModel/drawMode` is `bounds`

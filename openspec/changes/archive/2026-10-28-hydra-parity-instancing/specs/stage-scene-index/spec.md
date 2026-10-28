## MODIFIED Requirements

### Requirement: Prim origin
Every prim of a stage index SHALL carry `primOrigin/scenePath`, as
UsdImaging's prim origin data source does: the prim's path, made relative
to its prototype root when the prim lies inside a USD prototype. The
instancing filters SHALL keep it on the prims they re-root, unchanged by
re-rooting.

#### Scenario: A prim
- **WHEN** a consumer reads `/world/geo`'s prim origin
- **THEN** its scene path is `/world/geo`

#### Scenario: A propagated prototype prim
- **WHEN** a native instance's prototype holds `geo`, and native instancer propagation re-roots it under `.../UsdNiInstancer/UsdNiPrototype`
- **THEN** the re-rooted prim's scene path is `geo`, relative to the prototype root

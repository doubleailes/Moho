## ADDED Requirements

### Requirement: Stage metadata as generic data
The absolute root SHALL expose the stage metadata the scene globals read,
generically, under `__usdStageMetadata`: for `startTimeCode`,
`endTimeCode`, `timeCodesPerSecond`, `framesPerSecond`, `metersPerUnit`,
`upAxis` and `renderSettingsPrimPath`, the resolved value and whether a
layer authors it. A change to the stage's metadata SHALL dirty it.

#### Scenario: An unauthored meters per unit
- **WHEN** a stage authors `metersPerUnit` but not `startTimeCode`
- **THEN** `__usdStageMetadata/metersPerUnit/authored` is true and `__usdStageMetadata/startTimeCode/authored` is false, with the fallback as its value

### Requirement: Prim origin
Every prim of a stage index SHALL carry `primOrigin/scenePath`, as
UsdImaging's prim origin data source does: the prim's path, made relative
to its prototype root when the prim lies inside a USD prototype. The
instancing filters SHALL keep it on the prims they re-root.

#### Scenario: A prim
- **WHEN** a consumer reads `/world/geo`'s prim origin
- **THEN** its scene path is `/world/geo`

#### Scenario: A propagated prototype prim
- **WHEN** a native instance's prototype holds `geo`, and the native instancing filter re-roots it under `MohoNiPropagatedPrototypes`
- **THEN** the re-rooted prim's scene path is `geo`, relative to the prototype root

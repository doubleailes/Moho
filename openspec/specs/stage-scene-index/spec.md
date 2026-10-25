# stage-scene-index Specification

## Purpose
Reading one openusd stage as a scene index, in `UsdImagingStageSceneIndex`'s
shape: the stage is set from outside, the index owns the time, and stage
edits become notices only when updates are applied.

## Requirements

### Requirement: Built empty, then given a stage
A stage scene index SHALL start empty. Setting a stage SHALL remove every
prim it held, then add every prim of the new stage that the default
traversal reaches (active, loaded, defined, not abstract), with its type. Built to
include unloaded prims (UsdImaging's `includeUnloadedPrims`), it SHALL also
expose each unloaded payload's root, without what the payload brings. It
SHALL read whatever load set and population mask the stage was opened with,
and SHALL never load or unload anything itself.

#### Scenario: Setting a stage
- **WHEN** a stage with `/world`, `/world/geo` (a mesh) and `/world/cam` (a camera) is set on an empty index
- **THEN** observers are notified of the three prims added with types `""`, `mesh` and `camera`, and the index lists them

#### Scenario: Replacing the stage
- **WHEN** another stage is set
- **THEN** observers are notified of the old prims removed before the new prims are added

#### Scenario: Unloaded prims included
- **WHEN** an index that includes unloaded prims is given a stage opened `LoadNone`
- **THEN** each element carrying a payload is a prim of the index, with none of the payload's prims beneath it

#### Scenario: A masked stage
- **WHEN** the stage was opened with a population mask of `/world/a`
- **THEN** the index holds `/world`, `/world/a` and its descendants, and nothing else under `/world`

### Requirement: Stage edits are queued until applied
Edits to the stage SHALL NOT notify observers when they happen. Applying
pending updates SHALL turn every edit since the last application into
notices:
- a prim resync becomes the prim removed and re-added, or added when it is new;
- a removed prim becomes its removal;
- a property change becomes its prim dirtied, for the locators that property feeds.

Updates nested under a resynced prim SHALL be folded into that resync.

#### Scenario: Nothing until applied
- **WHEN** an attribute is edited on the stage and updates are not yet applied
- **THEN** observers have received nothing

#### Scenario: A value edit
- **WHEN** a mesh's `points` are edited and updates are applied
- **THEN** the mesh is notified dirtied for `primvars/points` and its generic entry `__usdAttributes/points`, and nothing else

#### Scenario: A prim defined and another removed
- **WHEN** a prim is defined, another removed, and updates applied
- **THEN** observers receive the first added with its type and the second removed

#### Scenario: Several edits, one application
- **WHEN** three attributes on two prims are edited before updates are applied
- **THEN** a single application notifies both prims dirtied, each for the union of its locators

### Requirement: Property changes dirty the locators they feed
Each property SHALL map to the locators of the data it feeds:
- `visibility` to `visibility`, `purpose` to `purpose`, `extent` to `extent`;
- any transform op or `xformOpOrder` to `xform`;
- `primvars:<name>` to `primvars/<name>`;
- the family attributes each schema reads to that schema's locators.

Every property change SHALL also dirty its own entry in the generic
attribute data (`__usdAttributes/<name>`), and a property the index exposes
in no schema SHALL dirty only that entry.

#### Scenario: A transform op
- **WHEN** `xformOp:translate` changes on a prim
- **THEN** the prim is dirtied for `xform`

#### Scenario: A primvar
- **WHEN** `primvars:displayColor` changes on a mesh
- **THEN** the mesh is dirtied for `primvars/displayColor`

### Requirement: The index owns the time
The index SHALL hold a current time, default the stage's default time.
Sampled values SHALL be read relative to it. Setting a different time SHALL
dirty exactly the data that may vary over time, on the prims that hold it.
Setting the time it already has SHALL notify nothing.

#### Scenario: One animated mesh
- **WHEN** the time changes on a stage where only one mesh's points are sampled
- **THEN** that mesh alone is dirtied for `primvars/points`, and its points read at offset 0 equal the stage's at the new time

#### Scenario: The same time
- **WHEN** the time is set to its current value
- **THEN** observers receive nothing

### Requirement: Generic attribute data
Every prim SHALL expose its attributes generically, under a container keyed
by attribute name. Each entry SHALL give:
- the resolved value at the index's time;
- whether a layer authors a value, as distinct from a schema fallback;
- the value type, and the `colorSpace`, `interpolation` and `elementSize` metadata when authored;
- the attribute's connections.

Relationships SHALL be exposed likewise, with their targets and forwarded
targets. Renderer-specific attributes (such as `crust:*`) SHALL be readable
this way without a schema of their own.

#### Scenario: A renderer-specific attribute
- **WHEN** a prim authors `crust:rayMask`
- **THEN** a consumer reads its value through the prim's generic attribute data

#### Scenario: A fallback is not authored
- **WHEN** a mesh's `subdivisionScheme` is not authored
- **THEN** its generic entry gives the schema's fallback value and reports it unauthored

### Requirement: Scene globals
The absolute root SHALL carry scene globals:
- the stage's start and end time codes and time codes per second;
- the active render settings prim the stage's metadata names.

#### Scenario: Time range
- **WHEN** a stage authors start 1001 and end 1050
- **THEN** the root's scene globals give start 1001 and end 1050

### Requirement: Values equal the stage's
Every value the index exposes SHALL equal the value openusd resolves for the
same data on the same stage at the same time.

#### Scenario: Parity on a fixture
- **WHEN** each family's fixture is read through the index and directly through openusd at the same time
- **THEN** every exposed value is equal

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

# stage-scene-index Specification

## Purpose
Reading one openusd stage as a scene index, in `UsdImagingStageSceneIndex`'s
shape: the stage is set from outside, the index owns the time, and stage
edits become notices only when updates are applied.
## Requirements
### Requirement: Built empty, then given a stage
A stage scene index SHALL start empty. Setting a stage SHALL remove every
prim it held, then add every prim of the new stage that UsdImaging's
traversal predicate reaches, with its type: active and not abstract, and
loaded unless the index includes unloaded prims. A prim need not be
defined: `over` prims and their descendants SHALL be exposed, as
`UsdImagingStageSceneIndex` exposes them. The traversal SHALL not descend
into instances; each USD prototype (`/__Prototype_N`) SHALL be a child of
the absolute root. Built to include unloaded prims (UsdImaging's
`includeUnloadedPrims`), the index SHALL also expose each unloaded payload's
root, without what the payload brings. It SHALL read whatever load set and
population mask the stage was opened with, and SHALL never load or unload
anything itself.

#### Scenario: Setting a stage
- **WHEN** a stage with `/world`, `/world/geo` (a mesh) and `/world/cam` (a camera) is set on an empty index
- **THEN** observers are notified of the three prims added with types `""`, `mesh` and `camera`, and the index lists them

#### Scenario: Replacing the stage
- **WHEN** another stage is set
- **THEN** observers are notified of the old prims removed before the new prims are added

#### Scenario: Overs are exposed
- **WHEN** a stage holds `over "Prototypes"` with a mesh `def Mesh "pebble"` under it, an inactive prim and an abstract `class` prim
- **THEN** `/Prototypes` (with its specifier `over` in its prim info) and `/Prototypes/pebble` (a mesh) are prims of the index, and neither the inactive nor the abstract prim is

#### Scenario: Unloaded prims included
- **WHEN** an index that includes unloaded prims is given a stage opened `LoadNone`
- **THEN** each element carrying a payload is a prim of the index, with none of the payload's prims beneath it

#### Scenario: A masked stage
- **WHEN** the stage was opened with a population mask of `/world/a`
- **THEN** the index holds `/world`, `/world/a` and its descendants, and nothing else under `/world`

### Requirement: Stage edits are queued until applied
Edits to the stage SHALL NOT notify observers when they happen. Applying
pending updates SHALL turn every edit since the last application into
notices, as `UsdImagingStageSceneIndex::ApplyPendingUpdates` does:
- a prim resync becomes its subtree added; a prim that was held before and is not re-added is notified removed, one entry per such prim, and a prim that is re-added is not notified removed;
- a removed prim becomes its removal;
- a property change becomes its prim dirtied, for the locators that property feeds;
- a prim-level change of metadata only (an info-only change of a prim or of the absolute root, such as `kind` or the stage's time codes) SHALL notify nothing. Hydra resyncs such a prim when a plugin metadata field changed; openusd does not report which fields changed, so this index cannot, and treats every such change as a built-in field's.

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

#### Scenario: A resync re-adds without removing
- **WHEN** a reference is added to `/world/asset`, which keeps its child `geo` and gains a child `extra`, and updates are applied
- **THEN** observers receive `/world/asset`, `/world/asset/geo` and `/world/asset/extra` added, and no removal

#### Scenario: A kind edit
- **WHEN** a prim's `kind` changes from `component` to `group` and updates are applied
- **THEN** observers receive nothing, and the prim's `__usdPrimInfo/kind` reads `group`

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
The index SHALL hold a current time, as `UsdImagingStageSceneIndex` holds
one. A new index's time SHALL be time code 0, the value of a
default-constructed `UsdTimeCode`; it SHALL NOT be the stage's default
time. Replacing a stage the index already holds SHALL reset the time to
`UsdTimeCode::EarliestTime()`, as `_StageGlobals::Clear` does. Sampled
values SHALL be read relative to the current time. Setting a different
time SHALL dirty exactly the data that may vary over time, on the prims
that hold it. Setting the time it already has SHALL notify nothing.

#### Scenario: One animated mesh
- **WHEN** the time changes on a stage where only one mesh's points are sampled
- **THEN** that mesh alone is dirtied for `primvars/points`, and its points read at offset 0 equal the stage's at the new time

#### Scenario: The same time
- **WHEN** the time is set to its current value
- **THEN** observers receive nothing

#### Scenario: A new index reads at time 0
- **WHEN** a point instancer authors `protoIndices` only as time samples, at frames 0 and 10 with different values, and an index whose time was never set reads it
- **THEN** its `instancerTopology/instanceIndices` are those at frame 0, and setting the time to 0 afterwards notifies nothing

#### Scenario: Replacing the stage
- **WHEN** an index holding a stage is given another stage
- **THEN** its time is `UsdTimeCode::EarliestTime()`

### Requirement: Generic attribute data
Every prim SHALL expose its attributes generically, under a container keyed
by attribute name (`__usdAttributes`). Each entry SHALL give:
- the resolved value at the index's time;
- whether a layer authors a value, as distinct from a schema fallback;
- the value type, and the `colorSpace`, `interpolation` and `elementSize` metadata when authored;
- the attribute's connections.

Relationships SHALL be exposed likewise (`__usdRelationships`), with their
targets and forwarded targets. Attributes in a renderer's own namespace
SHALL be readable this way without a schema of their own. This generic data
is the only data Moho adds to UsdImaging's.

#### Scenario: A renderer-specific attribute
- **WHEN** a prim authors `myRenderer:rayMask`
- **THEN** a consumer reads its value through the prim's generic attribute data

#### Scenario: A fallback is not authored
- **WHEN** a mesh's `subdivisionScheme` is not authored
- **THEN** its generic entry gives the schema's fallback value and reports it unauthored

### Requirement: Scene globals
The absolute root SHALL carry exactly the data `UsdImagingDataSourceStage`
gives, except `system` (openusd has no resolver context to give):
- `sceneGlobals` with `startTimeCode`, `endTimeCode` and `timeCodesPerSecond`, and `activeRenderSettingsPrim` only when the stage authors a non-empty `renderSettingsPrimPath`;
- `__usdUpAxis/upAxis`, the stage's up axis (its fallback when unauthored).

It SHALL carry no other scene globals (no meters per unit, no up axis under
`sceneGlobals`) and no stage metadata container.

#### Scenario: Time range
- **WHEN** a stage authors start 1001 and end 1050
- **THEN** the root's scene globals give start 1001 and end 1050

#### Scenario: Up axis
- **WHEN** a stage authors `upAxis = "Z"`
- **THEN** the root's `__usdUpAxis/upAxis` is `Z`, and its `sceneGlobals` has no `upAxis` and no `metersPerUnit`

### Requirement: Values equal the stage's
Every value the index exposes SHALL equal the value openusd resolves for the
same data on the same stage at the same time.

#### Scenario: Parity on a fixture
- **WHEN** each family's fixture is read through the index and directly through openusd at the same time
- **THEN** every exposed value is equal

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

### Requirement: Prim info
Every prim of a stage index SHALL carry `__usdPrimInfo` with exactly the
fields `UsdImagingUsdPrimInfoSchema` defines and the stage index fills:
- `specifier` (`def`, `over` or `class`);
- `typeName`, the USD type name as authored (an empty token when untyped);
- `isLoaded`;
- `apiSchemas`, its composed applied schemas, the typed schema's built-in ones included;
- `kind`, when authored;
- `variantSelections`, a container of variant set name to selection, when the prim has any;
- `niPrototypePath`, on a native instance, naming its USD prototype;
- `isNiPrototype`, true on a USD prototype's root prim.

No other field SHALL appear in it at the stage index (`piPropagatedPrototypes`
is written by point instancer propagation).

#### Scenario: A versioned light type and an applied schema
- **WHEN** a prim is defined as `DomeLight_1` and applies `ShapingAPI`
- **THEN** its `__usdPrimInfo/typeName` is `DomeLight_1` and its `__usdPrimInfo/apiSchemas` include `ShapingAPI`

#### Scenario: Kind
- **WHEN** a prim authors `kind = "group"`
- **THEN** its `__usdPrimInfo/kind` is `group`, and a prim that authors no kind has none

#### Scenario: Variant selections
- **WHEN** a prim selects `lod = "high"` in its `lod` variant set
- **THEN** its `__usdPrimInfo/variantSelections/lod` is `high`

#### Scenario: A native instance
- **WHEN** a consumer reads an instanceable prim's prim info
- **THEN** it gives `niPrototypePath` naming that prim's `/__Prototype_N`, and no field outside the schema

### Requirement: Time variance in the generic attribute data
Each attribute's generic entry SHALL give `mightBeTimeVarying`: whether its
value might vary over time, from its opinions (USD's
`ValueMightBeTimeVarying`), whatever the index's time.

#### Scenario: At the default time
- **WHEN** an index whose time was never set exposes a transform op authored with time samples, and another with a default value only
- **THEN** the first's `mightBeTimeVarying` is true and the second's false

### Requirement: Model and extents hint data
A prim SHALL carry the model data UsdImaging gives it:
- `model` (`UsdImagingModelSchema`): `modelPath`, `assetIdentifier`, `assetName` and `assetVersion`, on a prim that is a model;
- `geomModel` (`UsdImagingGeomModelSchema`): the `GeomModelAPI` draw mode attributes, on a prim that applies it, with `applyDrawMode` set as UsdImaging's GeomModelAPI adapter sets it;
- `extentsHint` (`UsdImagingExtentsHintSchema`): the authored `extentsHint`, one range per purpose in its order, as many ranges as its value at the index's time holds.

#### Scenario: An asset
- **WHEN** a `component` prim authors `assetInfo` with an identifier and a name
- **THEN** its `model` gives its path, the asset identifier and the asset name

#### Scenario: An extents hint
- **WHEN** a model authors `extentsHint` for the purposes default and render
- **THEN** its `extentsHint` gives one range for `geometry` and one for `render`

#### Scenario: An animated extents hint
- **WHEN** a prim authors `extentsHint` as time samples only, one range at one time and two at another
- **THEN** its `extentsHint` names one purpose at the first time and two at the second

### Requirement: Adapter data follows UsdImaging
For each prim type the index exposes, the data names, the data layout under
each locator, and the locators a property change dirties SHALL be those of
UsdImaging's data source and invalidation for that type. Where the two
differ, UsdImaging's SHALL win, except for the generic attribute data.

#### Scenario: A mesh property
- **WHEN** a mesh's `faceVertexIndices` change and updates are applied
- **THEN** the mesh is dirtied for exactly the locators `UsdImagingDataSourceMesh::Invalidate` gives for that property, plus its generic entry


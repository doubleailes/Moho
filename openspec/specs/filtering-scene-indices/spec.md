# filtering-scene-indices Specification

## Purpose
Scene indices that take other scene indices as input, as Hydra's and
UsdImaging's filters: flattening resolves inherited data down the
hierarchy, merging combines several inputs over one namespace, and the
standard chain plus the renderer filters a consumer inserts by phase.
## Requirements
### Requirement: Flattened transforms
A flattening index SHALL give each prim a world transform: its local matrix
times its parent's world transform, or its local matrix alone when it resets
the transform stack. A prim with no transform of its own SHALL take its
parent's. Sample times over an interval SHALL be the union of the
contributing local and ancestor sample times.

#### Scenario: Nested transforms
- **WHEN** `/a` translates by (1, 0, 0) and its child `/a/b` by (0, 2, 0)
- **THEN** `/a/b`'s flattened matrix translates by (1, 2, 0)

#### Scenario: Reset
- **WHEN** `/a/b` resets the transform stack
- **THEN** its flattened matrix is its local matrix alone

#### Scenario: A moving parent
- **WHEN** only `/a`'s transform is sampled
- **THEN** `/a/b`'s flattened transform reports `/a`'s sample times over the shutter

### Requirement: Inherited visibility and purpose
Flattened visibility SHALL be the prim's own when authored, else its
parent's, else visible, so that an invisible prim makes its subtree
invisible. Flattened purpose SHALL be the prim's own when authored, else its
parent's when that is inheritable, else the prim's fallback.

#### Scenario: Invisible subtree
- **WHEN** `/a` is invisible and `/a/b` authors nothing
- **THEN** `/a/b`'s flattened visibility is invisible

#### Scenario: Purpose inherited
- **WHEN** `/a` authors purpose `proxy` and `/a/b` authors none
- **THEN** `/a/b`'s flattened purpose is `proxy`

### Requirement: Inherited constant primvars
A flattened prim SHALL also carry its ancestors' constant-interpolation
primvars that it does not define itself.

#### Scenario: A constant primvar from a parent
- **WHEN** `/a` authors a constant `primvars:tint` and its child mesh does not
- **THEN** the mesh's flattened primvars include `tint` with `/a`'s value

### Requirement: Flattening keeps dirty propagation
When a prim's flattened data is dirtied, every descendant whose flattened
data depends on it SHALL be dirtied for the same data.

#### Scenario: A parent moves
- **WHEN** `/a`'s transform changes
- **THEN** observers of the flattening index receive `/a` and every prim under it dirtied for `xform`

### Requirement: Merging inputs
A merging index, as `HdMergingSceneIndex`, SHALL combine inputs, each added
at a root path, with earlier inputs stronger. With two or more inputs, an
input SHALL contribute only at and under its root. With a single input, the
index SHALL answer every path from that input, its root ignored, as
`HdMergingSceneIndex::GetPrim` returns the sole input's prim. A prim present in several inputs SHALL take the
first non-empty type, and its data SHALL overlay the inputs' containers,
stronger first; a strict ancestor of a root that no input holds SHALL be a
typeless prim with an empty container. Its children SHALL be the union of
the inputs' children, in no specified order.

#### Scenario: Two inputs share a prim
- **WHEN** two inputs both hold `/world/elem`, the first stronger
- **THEN** reading `/world/elem` gives the first input's data where both define it, and the second's elsewhere

#### Scenario: A sole input answers outside its root
- **WHEN** a merging index has a single input, rooted at `/a/b/c`, whose index holds `/z`
- **THEN** reading `/z` gives the input's prim, as Hydra's merging index does with one input

### Requirement: Adding and removing an input
Adding an input SHALL notify added its root and every descendant its input
lists, whether or not the input defines it, each with its merged type; the
strict ancestors of its root are not notified (`HdMergingSceneIndex`'s
`_AddStrictPrefixesOfSceneRoots` skips every valid root). Removing an input SHALL walk the
removed input's prims and notify added those another input still holds and
removed the others.

#### Scenario: An input comes and goes
- **WHEN** an input rooted at `/world/elem` is added and later removed, over another input holding `/world/elem` but not its children
- **THEN** adding notifies `/world/elem` and every descendant the first input lists added; removing notifies those descendants removed and `/world/elem` added from the other input

#### Scenario: An undefined root
- **WHEN** an input whose index does not define its root `/w` is added
- **THEN** `/w` is notified added all the same

### Requirement: Inputs' notices pass through
The merging index SHALL forward its inputs' added, removed and dirtied
notices without filtering them by the inputs' roots, correcting the type of
an added prim to the merged type, and re-adding, with its subtree, a prim
one input removed while another still holds it. Renames SHALL be forwarded
as removals and additions.

#### Scenario: A weaker input removes a shared prim
- **WHEN** the weaker input removes `/world/elem` while the stronger input still holds it
- **THEN** observers receive `/world/elem` removed and then added with the stronger input's type

#### Scenario: A notice outside a root
- **WHEN** an input rooted at `/a` notifies `/b/x` dirtied
- **THEN** the merging index forwards it, as Hydra's does

### Requirement: Extent resolving
An extent resolving index, as `UsdImagingExtentResolvingSceneIndex` with
the purposes `geometry`, `render` and `proxy`, SHALL give a prim that has
no `extent` of its own but has an `extentsHint` the union of its hint's
ranges for those purposes as its `extent` (`min`, `max`). A prim's own
`extent` SHALL be left as it is. A change to `extentsHint` SHALL also dirty
`extent`.

#### Scenario: A model with only a hint
- **WHEN** a model authors `extentsHint` with a geometry range of (0,0,0)–(1,1,1) and a render range of (-1,0,0)–(0,1,1), and no `extent`
- **THEN** its resolved `extent` is (-1,0,0)–(1,1,1)

#### Scenario: An authored extent wins
- **WHEN** a mesh authors both `extent` and `extentsHint`
- **THEN** its resolved `extent` is its authored `extent`

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

### Requirement: The standard chain
`chain::standard` SHALL build, over a stage scene index, exactly the
filters `UsdImagingCreateSceneIndices` builds with
`addDrawModeSceneIndex = false`, `displayUnloadedPrimsWithBounds = false`,
no overrides callback and no UsdImaging scene index plugins, in its order:
extent resolving, point instancer propagation, native instancer
propagation, instance proxy path translation, material binding resolving,
render settings flattening. Its material locator cache, notice batching and
selection scene indices are left out: none changes what the chain exposes
or notifies while batching is disabled and nothing is selected.

#### Scenario: Composition
- **WHEN** a consumer builds `chain::standard` over a stage index
- **THEN** a native instance inside a point instancer prototype is scattered, bindings into instances resolve, flattened render settings are present, and no prim carries light linking `categories`

### Requirement: Renderer filters by phase
A consumer SHALL be able to apply its renderer filters, Hydra's scene index
plugins, over any scene index (the standard chain, or a merge of several),
as an ordered list of entries `(phase, order, constructor)`, `order` being
`AtStart` or `AtEnd`. They SHALL be applied in
`HdSceneIndexPluginRegistry`'s order: ascending phase, then `AtStart`
entries before `AtEnd` entries, then list position; each entry's
constructor wrapping the result of the previous one.

#### Scenario: Light linking and dependency forwarding
- **WHEN** a consumer passes dependency forwarding at phase 1000 `AtEnd`, light linking at phase 4 `AtStart` and its own fix at phase 0 `AtEnd`, in that list order
- **THEN** the terminal index is dependency forwarding over light linking over the fix over its input

### Requirement: Dependency forwarding
A dependency forwarding index, as `HdDependencyForwardingSceneIndex`, SHALL
read every prim's `__dependencies` and, when a prim is dirtied at locators
intersecting an entry's depended-on locator, also notify the declaring prim
dirtied at that entry's affected locator, transitively, each prim and
locator once per notice. Removing or adding a depended-on prim SHALL dirty
its dependents likewise. It SHALL be a filter a consumer adds, not part of
the standard chain.

#### Scenario: A chained dependency
- **WHEN** `/a` declares `x` depends on `/b`'s `y`, and `/b` declares `y` depends on `/c`'s `z`, and `/c` is dirtied for `z`
- **THEN** observers receive `/c` dirtied for `z`, `/b` for `y` and `/a` for `x`

### Requirement: Notice batching by prim type
A prim type notice batching index, as `HdsiPrimTypeNoticeBatchingSceneIndex`,
SHALL hold its input's notices until flushed and SHALL appear empty until
its first flush. It SHALL consolidate them: several dirtied entries for a
path become one with the union of locators; an added entry absorbs later
dirtied entries for its path; a removal discards earlier added and dirtied
entries at or under its path, and a removal under another removal is
dropped. A flush SHALL send all removals, then the added and dirtied
entries grouped by the priority a consumer-given functor assigns their prim
types, lower first.

#### Scenario: Materials before geometry
- **WHEN** a mesh and its material are added, the functor ranks `material` before `mesh`, and the index is flushed
- **THEN** observers receive the material added before the mesh, in one flush, and nothing before it

### Requirement: Scene globals a host sets
A scene globals index, as `HdsiSceneGlobalsSceneIndex`, SHALL pass its
input's prims, children and notices through, renames as removals and
additions, and SHALL give the absolute root a `sceneGlobals` container
overlaid over the input root's data, stronger than it. That container
SHALL name `activeRenderPassPrim`, `activeRenderSettingsPrim`,
`primaryCameraPrim`, `currentFrame`, `timeCodesPerSecond` and
`sceneStateId`, whether set or not. A field the host has set SHALL read the
value set, read live (a container obtained before a later set reads the new
value), a prim path as a one-element path array (an empty path as an empty
array); a field never set SHALL read what the input's `sceneGlobals` gives
there, or nothing. The input root's other data and its other `sceneGlobals`
fields (`startTimeCode`, `endTimeCode`) SHALL be kept. When the input has
no root data, the root's data SHALL be the `sceneGlobals` container alone.
It SHALL not be part of the standard chain.

#### Scenario: Unset fields
- **WHEN** a scene globals index is built over a stage chain whose stage authors 24 time codes per second, and nothing is set
- **THEN** its root's `sceneGlobals/currentFrame` reads nothing, `sceneGlobals/timeCodesPerSecond` reads 24 from the stage, and `sceneGlobals` still names `currentFrame`

#### Scenario: The input's root data is kept
- **WHEN** the host sets the current frame to 1001 over a stage whose root has `__usdUpAxis` and `sceneGlobals/startTimeCode`
- **THEN** the root's `sceneGlobals/currentFrame` reads 1001, and its `__usdUpAxis/upAxis` and `sceneGlobals/startTimeCode` read the stage's

#### Scenario: A set field hides the input's
- **WHEN** the stage's root gives `sceneGlobals/activeRenderSettingsPrim` `/Render/a` and the host sets the active render settings prim to `/Render/b`
- **THEN** the root's `sceneGlobals/activeRenderSettingsPrim` reads `/Render/b`

### Requirement: Scene globals setters dirty their field
Each setter of a scene globals index SHALL, when its value differs from the
one held, store it and notify the absolute root dirtied at exactly that
field's locator: `sceneGlobals/activeRenderPassPrim`,
`sceneGlobals/activeRenderSettingsPrim`, `sceneGlobals/primaryCameraPrim`,
`sceneGlobals/currentFrame`, `sceneGlobals/timeCodesPerSecond` or
`sceneGlobals/sceneStateId`. Setting a held value again SHALL notify
nothing. The first set of a field SHALL notify, even to the value the input
gives there. The current frame SHALL be compared as a time code (NaN equals
NaN); the time codes per second as a number (setting NaN again notifies
again). No setter SHALL dirty any other locator or prim.

#### Scenario: Current frame round trip
- **WHEN** the host sets the current frame to 1001, then 1001 again, then 1002
- **THEN** observers receive `/` dirtied at `sceneGlobals/currentFrame` twice (for 1001 and 1002), and the root's `sceneGlobals/currentFrame` reads 1002

#### Scenario: Each setter its own locator
- **WHEN** the host sets the primary camera prim to `/cam`
- **THEN** observers receive exactly `/` dirtied at `sceneGlobals/primaryCameraPrim`

### Requirement: Scene globals as a renderer filter
A consumer SHALL be able to add the scene globals index as a renderer filter
at phase 0, `AtStart`, where usdview's engine registers it for every
renderer, and SHALL be given the built index to drive it. The stage index's
time and the scene globals' current frame SHALL stay independent: setting
one SHALL not set the other.

#### Scenario: Before light linking
- **WHEN** a consumer passes light linking (phase 4) and the scene globals filter, in that list order
- **THEN** the terminal index is light linking over the scene globals index over the input, and the consumer holds the scene globals index

#### Scenario: Time is not coupled
- **WHEN** the host sets the stage index's time to 1001 without setting the current frame
- **THEN** the root's `sceneGlobals/currentFrame` still reads nothing


# scene-index Specification

## Purpose
The contract every Moho scene index honours, in Hydra 2.0's model: prims as
types plus lazily read data, children, observers of what changed, and
hierarchical locators naming that data.
## Requirements
### Requirement: Prims and children
A scene index SHALL answer, for any absolute path, the prim at that path as
a type token and a container of data, or an undefined prim when nothing is
there. It SHALL list the paths of a prim's children. The absolute root SHALL
always be a valid query.

#### Scenario: A defined prim
- **WHEN** a scene index holding a mesh at `/world/geo` is asked for `/world/geo`
- **THEN** it returns the type `mesh` and a container of that prim's data

#### Scenario: Nothing at a path
- **WHEN** it is asked for a path no prim occupies
- **THEN** it returns an undefined prim, with no type and no data

#### Scenario: Children
- **WHEN** it is asked for the children of `/world`
- **THEN** it returns the paths of `/world`'s child prims, in a stable order

### Requirement: Data sources
A prim's data SHALL be a tree of data sources: containers (named children),
vectors (indexed elements), and sampled values (a value at a time). Values
SHALL be openusd values. Reading a data source SHALL compute only what is
read, and reading the same data source twice with no change in between SHALL
give the same value.

#### Scenario: Lazy reading
- **WHEN** a consumer reads only a mesh's topology
- **THEN** no other data of that mesh is resolved from the stage

#### Scenario: A container's names
- **WHEN** a consumer lists a container's names and reads one of them
- **THEN** every listed name gives a data source, and a name not listed gives none

### Requirement: Sampled values and sample times
A sampled value SHALL be read at an offset from the index's current time.
Asked for the sample times contributing to an interval of offsets, it SHALL
answer either that the value is uniform over the interval, or the times to
sample, including the samples bracketing the interval's edges.

#### Scenario: A uniform value
- **WHEN** a consumer asks a value with no time samples for the times contributing to the shutter interval
- **THEN** it is told the value is uniform, and reading it at offset 0 gives the value

#### Scenario: An animated value over a shutter
- **WHEN** a consumer asks an attribute sampled at times 1, 2 and 3, with the index at time 2, for the times contributing to offsets [-0.25, 0.25]
- **THEN** it receives offsets that include -1 and 1 (the bracketing samples), and reading at each gives the stage's value at that time

### Requirement: Locators and locator sets
Data SHALL be named by locators, sequences of tokens such as
`primvars/points/primvarValue`. A locator SHALL cover every locator it is a
prefix of. A locator set SHALL answer whether it intersects a locator, and
SHALL stay minimal: a locator covered by another in the set is not kept.

#### Scenario: A prefix covers its descendants
- **WHEN** a set holding `primvars` is asked whether it intersects `primvars/points/primvarValue`
- **THEN** it answers yes

#### Scenario: A set stays minimal
- **WHEN** `primvars/points` and then `primvars` are inserted into a set
- **THEN** the set holds `primvars` alone

### Requirement: Observers and notices
A scene index SHALL let observers register, and SHALL notify them in batches
of four kinds:
- added prims, with their types;
- removed prims;
- dirtied prims, each with the locator set of its changed data;
- renamed prims.

A removed path SHALL stand for its whole subtree. A dirtied entry SHALL
affect only the named prim, not its descendants. After a notice, querying
the index SHALL reflect the change.

#### Scenario: A removal covers the subtree
- **WHEN** an index notifies `/world/a` removed
- **THEN** `/world/a` and every prim under it are undefined when queried

#### Scenario: A dirty entry is local
- **WHEN** an index notifies `/world` dirtied for `xform`
- **THEN** no descendant of `/world` is implied dirtied by that entry

#### Scenario: Observers see changes after they register
- **WHEN** an observer registers after prims were added
- **THEN** it receives only the notices sent after it registered

### Requirement: Single-thread confinement
A scene index and its data sources SHALL be used from the thread that owns
the index. Values read from them SHALL be movable to other threads.

#### Scenario: Values cross threads
- **WHEN** a consumer reads a mesh's points and hands them to a worker thread
- **THEN** the worker can use them while the index stays on its own thread

### Requirement: Path values
A data source standing for one of Hydra's single `SdfPath` fields (such as
`materialBindings/<purpose>/path`, `instance/instancer`,
`__usdPrimInfo/niPrototypePath` or `sceneGlobals/activeRenderSettingsPrim`)
SHALL hold a path array of exactly one element. A field Hydra types as a
path array SHALL hold a path array of any length. An empty path SHALL be
represented by the field's absence.

#### Scenario: A resolved binding path
- **WHEN** a consumer reads a prim's resolved `materialBindings` path
- **THEN** it receives a path array holding exactly the bound material's path

#### Scenario: No active render settings
- **WHEN** the stage authors no `renderSettingsPrimPath`
- **THEN** the root's `sceneGlobals` has no `activeRenderSettingsPrim`

### Requirement: Blocked data
An overlay of containers SHALL honour a block, as Hydra's
`HdBlockDataSource`: a block at a name in a stronger container SHALL hide
the data weaker containers give at that name, and the overlay SHALL give no
data source there.

#### Scenario: A blocked prototype path
- **WHEN** a filter overlays a block at `__usdPrimInfo/niPrototypePath` over a prim whose input data gives that path
- **THEN** reading `__usdPrimInfo/niPrototypePath` gives nothing, and `__usdPrimInfo`'s other fields still read through

### Requirement: Collection expressions over a scene index
A path expression SHALL be judgeable against any scene index, as Hydra's
`HdCollectionExpressionEvaluator` judges it: a path matches when the
expression's patterns match it and every predicate in it holds for the
scene index's prim at that path. The predicates SHALL be those of Hydra's
collection predicate library, under both names:
- `hdType(t)` / `type(t)`: the prim's type is `t`;
- `hdVisible(v)` / `visible(v)`: the prim has `visibility/visibility` and it equals `v` (default true); a prim without that data matches neither value;
- `hdPurpose(p)` / `purpose(p)`: the prim's `purpose/purpose` is `p`;
- `hdHasDataSource(l)` / `hasDataSource(l)`: the prim has data at locator `l`;
- `hdHasPrimvar(n)` / `hasPrimvar(n)`: the prim has the primvar `n`;
- `hdHasMaterialBinding(s)` / `hasMaterialBinding(s)`: the text of the prim's resolved all-purpose `materialBindings` path contains `s`.

An expression that uses a predicate outside that library SHALL match
nothing. Instance proxies are not traversed: only prims the scene index
holds can match.

#### Scenario: A type predicate
- **WHEN** the expression `/world//{hdType:mesh}` is judged against a scene index holding `/world/geo` (a mesh) and `/world/cam` (a camera)
- **THEN** `/world/geo` matches and `/world/cam` does not

#### Scenario: A USD-only predicate
- **WHEN** an expression uses a predicate Hydra's library does not define
- **THEN** it matches no path

### Requirement: Dependencies data
A prim MAY declare dependencies in `__dependencies`, as
`HdDependenciesSchema` lays them out: a container of named entries, each
giving `dependedOnPrimPath`, `dependedOnDataSourceLocator` and
`affectedDataSourceLocator`, meaning that when the depended-on prim is
dirtied at a locator intersecting the depended-on locator, this prim's
affected locator is dirty too. An empty depended-on prim path SHALL mean
the declaring prim itself.

#### Scenario: A self dependency
- **WHEN** a prim declares an entry with an empty depended-on path, depended-on locator `__usdRenderSettings/camera` and affected locator `renderSettings/camera`
- **THEN** a consumer reading it knows that a dirty `__usdRenderSettings/camera` on that prim makes `renderSettings/camera` dirty

### Requirement: Managing prims from notices
A consumer SHALL be able to observe a scene index as
`HdsiPrimManagingSceneIndexObserver` does: given a prim factory, the
observer SHALL create one managed prim per prim the index holds when the
observer is made and per added entry (replacing a prim already at that
path), SHALL pass each dirtied entry, with its locator set, to the managed
prim at its path, and SHALL release every managed prim at or under a
removed path. A managed prim SHALL be retrievable by path.

#### Scenario: Added, dirtied, removed
- **WHEN** an observer with a factory recording calls watches an index that adds `/world/geo`, dirties it for `primvars/points`, then removes `/world`
- **THEN** the factory was asked for `/world/geo` once, its managed prim received one dirty with `primvars/points`, and after the removal the observer holds no prim at or under `/world`

### Requirement: Update scope of a camera edit and a light addition
An edit SHALL dirty no more than Hydra dirties for it, through
`chain::standard` followed by the light linking and dependency forwarding
renderer filters. Editing a camera attribute SHALL dirty only that camera
prim, at the locators `UsdImagingDataSourceCameraPrim::Invalidate` gives
(for example `camera/focalLength` for `focalLength`, `xform` for a
transform op). Adding a light SHALL add that light prim. If its light link
and shadow link collections are trivial (they include everything), as
`HdsiLightLinkingSceneIndex` treats them, nothing else SHALL be added or
dirtied. Otherwise, only the prims whose membership changes SHALL be
dirtied, and only at `categories` (or `instanceCategories`).

#### Scenario: A camera's focal length
- **WHEN** a stage holding a camera and 1,000 meshes is edited to change the camera's `focalLength`, and pending updates are applied
- **THEN** the observer receives one dirtied entry, for the camera, at `camera/focalLength`, and no added or removed entries

#### Scenario: A camera move
- **WHEN** the camera's transform op is edited
- **THEN** the observer receives one dirtied entry, for the camera, at `xform`

#### Scenario: Adding a light that lights everything
- **WHEN** a sphere light with default collections is added under `/lights` on a stage holding 1,000 meshes
- **THEN** the observer receives an added entry for the light and no dirtied entry for any mesh

#### Scenario: Adding a light linked to a few prims
- **WHEN** a light whose `collection:lightLink` includes only `/world/a` is added
- **THEN** besides the light's added entry, only the prims whose categories change are dirtied, and only at `categories`


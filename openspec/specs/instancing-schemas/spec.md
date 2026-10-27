# instancing-schemas Specification

## Purpose
How a scene index exposes native instances and point instancers as
instancers over prototypes, with prototype paths that stay the same across
stages, chunks and load changes.
## Requirements
### Requirement: Native instances become instancers
A native instancing index SHALL turn native instances into instancers:
- instances that share a prototype and the same binding data (material bindings, purpose) are grouped under one instancer prim of type `instancer`;
- the instancer's `instancerTopology` lists its prototype;
- its `hydra:instanceTransforms` primvar holds one world transform per instance;
- each instance prim exposes `instance`, naming its instancer and its index within it.

The prototype's prims SHALL appear once, under a path derived from the
prototype's content and bindings, not from openusd's `/__Prototype_N`
numbering.

#### Scenario: Three instances of one asset
- **WHEN** three instanceable prims reference the same asset with no differing bindings
- **THEN** one instancer lists one prototype with three instance transforms, and the asset's prims appear once under the prototype path

#### Scenario: Differing bindings
- **WHEN** two instances of the same asset bind different materials
- **THEN** they fall under two instancers, one per binding

### Requirement: Stable prototype paths
The path of a native prototype SHALL depend only on the prototype's content
and bindings. Reading the same instances on two stages, or in two chunks of
a streamed scene, SHALL give the same prototype paths.

#### Scenario: Same prototype in two chunks
- **WHEN** two chunk stages each compose an instance of the same asset
- **THEN** both chunk indices name that prototype by the same path

### Requirement: Nested instances
Instances inside a prototype SHALL be turned into instancers within it, at
any depth.

#### Scenario: An instance of a building with instanced windows
- **WHEN** an instanced building's prototype holds instanced windows
- **THEN** the building's prototype holds a windows instancer listing the window prototype

### Requirement: Point instancers
A `PointInstancer` SHALL have type `instancer` and SHALL expose:
- `instancerTopology`: `prototypes` (the prototypes relationship's targets), `instanceIndices` (from `protoIndices`, without `inactiveIds`) and `mask` (false for `invisibleIds`);
- primvars `hydra:instanceTranslations` (positions), `hydra:instanceRotations` (`orientationsf`, else `orientations`) and `hydra:instanceScales`;
- `velocities`, `accelerations` and `angularVelocities` as primvars.

Each prototype SHALL be re-rooted under the instancer so that its root
resets the transform stack and names the instancer in `instancedBy`. Prims
under the instancer that no prototype targets SHALL not be drawn.

#### Scenario: Scattered pebbles
- **WHEN** a point instancer targets two pebble prototypes with protoIndices [0, 1, 0] and three positions
- **THEN** its topology gives the two prototypes, instance indices [[0, 2], [1]] and translations for the three instances

#### Scenario: Invisible ids
- **WHEN** a point instancer authors `invisibleIds = [1]` with ids [0, 1, 2]
- **THEN** its mask is [true, false, true]

### Requirement: Path values inside propagated prototypes
The path values a propagated prototype's prims carry (binding targets,
relationship targets, connections) SHALL name the propagated prototype:
every path at or under the prototype's source root (`/__Prototype_N` for a
native instance, the original prototype for a point instancer's copy) SHALL
be re-rooted under the propagated root. Paths outside the source root SHALL
be left alone, and `primOrigin` SHALL keep naming the USD prim.

#### Scenario: A material bound inside a native prototype
- **WHEN** an instanced asset's mesh binds `/asset/Looks/red`, inside the asset
- **THEN** the propagated prototype's mesh resolves its binding to the propagated `.../Looks/red`, a material prim of the index, and no `/__Prototype_N` path

#### Scenario: A material bound inside a point instancer's prototype
- **WHEN** a point instancer's prototype mesh binds a material inside that prototype
- **THEN** the copy's mesh resolves its binding to the material under the copy, and its `primOrigin` still names the original mesh

### Requirement: Nested point instancers
A point instancer inside another point instancer's prototype SHALL be
re-instanced in every copy of that prototype, as Hydra's propagation does:
the copied inner instancer SHALL name copies of its own prototypes, made
inside the enclosing copy and named by its own output path, whose prims it
instances; the inner originals inside the copy SHALL be drawn only through
it, and a nested copy's path values SHALL be re-rooted level by level.

#### Scenario: Leaves on a scattered tree
- **WHEN** a point instancer scatters a tree whose prototype holds a point instancer of leaves
- **THEN** each copy of the tree holds a leaves instancer whose prototypes are leaf copies inside that tree copy, instanced by it, and the leaf copy's `primOrigin` names the original leaf

### Requirement: Point instancer prototypes inside native instances
A point instancer prototype target that lies inside a native instance (an
instance proxy) SHALL be copied from the same prim in that instance's
propagated prototype, under the instancer at
`<instancer>/ForInstancer<hash>/prototype<i>`, with a typeless scope prim
holding the copies.

#### Scenario: Coral prototypes inside an instanced asset
- **WHEN** a point instancer's prototype targets a mesh inside an instanceable asset under its `Prototypes` scope
- **THEN** its topology names a copy under the instancer, the copy is that mesh with its points, instanced by the point instancer, and the copy is reachable through the instancer's `ForInstancer<hash>` scope


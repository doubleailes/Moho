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

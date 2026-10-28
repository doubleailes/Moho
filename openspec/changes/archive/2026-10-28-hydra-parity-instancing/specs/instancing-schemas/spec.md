## MODIFIED Requirements

### Requirement: Native instances become instancers
Native instancer propagation, as `UsdImagingNiPrototypePropagatingSceneIndex`
and its instance aggregation, SHALL turn native instances into instancers.
It SHALL run after point instancer propagation and read flattened data.

Grouping: instances SHALL share an instancer when they share the
enclosing prototype root, the USD prototype, the set of names and roles of
their flattened constant primvars, and equal flattened `usdMaterialBindings`,
`purpose`, `geomModel`, `model` and `__usdUpAxis`, compared after
re-rooting the instance's path to `/<prototype name>`.

Layout, for an instance of `/__Prototype_N` whose enclosing prototype root
is `R` (the root of the point instancer copy or native prototype the
instance lies in, else `/`):
- `R/UsdNiPropagatedPrototypes/<bindingHash>`, a typeless prim carrying a copy of the group's `usdMaterialBindings`, `purpose`, `geomModel`, `model` and `__usdUpAxis`, re-rooted from the instance to the prototype;
- `<bindingHash>` is `NoPrimvars` or `Primvars<hex>`, followed by `_<name><hex>` for each of those data sources the instance has; the hexadecimal digits are a hash whose value is not part of the contract;
- `.../<bindingHash>/__Prototype_N/UsdNiInstancer`, of type `instancer`;
- `.../UsdNiInstancer/UsdNiPrototype`, the prototype's root, holding the prototype's prims.

The instancer SHALL expose:
- `instancerTopology`: `prototypes` (the prototype root), `instanceIndices` (one array, every instance), `instanceLocations` (the instances' paths, in path order) and `mask` (each instance's flattened visibility, in the same order);
- `primvars`: `hydra:instanceTransforms` (each instance's flattened transform) and every constant primvar of the instances, with interpolation `instance`, one value per instance;
- `instancedBy`, when the instancer lies inside a point instancer copy (that copy root's `instancedBy`) or inside a native prototype (its own instancer and prototype root).

The prototype root SHALL have an identity transform that resets the
transform stack, and every prim of the prototype but a nested instance
prim SHALL carry `instancedBy` naming the instancer (`paths`) and the
prototype root (`prototypeRoots`), as UsdImaging's prototype scene index
gives it.
Each instance prim SHALL expose `instance` (`instancer`, `prototypeIndex`,
`instanceIndex`), and its type SHALL be empty unless it is `material`.

#### Scenario: Three instances of one asset
- **WHEN** three instanceable prims `/a`, `/b`, `/c` reference the same asset with no bindings or constant primvars
- **THEN** one instancer `/UsdNiPropagatedPrototypes/<bindingHash>/__Prototype_N/UsdNiInstancer` (`__Prototype_N` the USD prototype's name on the stage), its `<bindingHash>` starting with `NoPrimvars` (then `___usdUpAxis<hex>`, the up axis being flattened onto every prim), lists the prototype `.../UsdNiInstancer/UsdNiPrototype`, instance locations `/a`, `/b`, `/c`, three instance transforms and the mask [true, true, true], and the asset's prims appear once under the prototype root, each instanced by that instancer

#### Scenario: Differing bindings
- **WHEN** two instances of the same asset bind different materials
- **THEN** they fall under two `<bindingHash>` prims, one instancer each

#### Scenario: A binding only on the instance
- **WHEN** an instanceable prim authors `material:binding = </Looks/red>` and its asset's mesh binds nothing
- **THEN** the prototype's mesh resolves its binding to `/Looks/red`

#### Scenario: An invisible instance
- **WHEN** one of three instances authors `visibility = invisible`
- **THEN** the instancer's mask is false for that instance and true for the others, and the instances still share one instancer

#### Scenario: A per-instance colour
- **WHEN** each instance authors a constant `primvars:displayColor` of its own
- **THEN** the instancer's `primvars/displayColor` has interpolation `instance` and one colour per instance, in instance order

#### Scenario: Component instances group
- **WHEN** two instances of the same asset are `component` models with the same asset info
- **THEN** they share one instancer, their `model` data being equal once re-rooted to the prototype

### Requirement: Nested instances
Instances inside a native prototype SHALL be turned into instancers within
that prototype, at any depth, under
`<prototype root>/UsdNiPropagatedPrototypes`. Instances inside a point
instancer's prototype SHALL be aggregated within each copy of it, under
`<copy root>/UsdNiPropagatedPrototypes`, their instancer instanced by the
point instancer, so that they are drawn once per point instance.

#### Scenario: An instance of a building with instanced windows
- **WHEN** an instanced building's prototype holds instanced windows
- **THEN** the building's prototype root holds a windows instancer under its own `UsdNiPropagatedPrototypes`, listing the window prototype and instanced by the building's instancer

#### Scenario: Instanced assets scattered by a point instancer
- **WHEN** a point instancer's prototype holds an instanceable asset
- **THEN** the asset's native instancer lies under the prototype's `ForInstancer<hash>` copy, its `instancedBy` names the point instancer, and the asset is not drawn at its original place

### Requirement: Point instancers
Point instancer propagation, as `UsdImagingPiPrototypePropagatingSceneIndex`,
SHALL run first after extent resolving. A `PointInstancer` SHALL have type
`instancer` and SHALL expose:
- `instancerTopology`: `prototypes` (the prototypes relationship's targets, without a prototype whose every instance is inactive), `instanceIndices` (from `protoIndices`, one array per remaining prototype, inactive instances included) and `mask` (false for inactive and invisible ids);
- primvars `hydra:instanceTranslations` (positions), `hydra:instanceRotations` (`orientationsf`, else `orientations`) and `hydra:instanceScales`;
- `velocities`, `accelerations` and `angularVelocities` as primvars.

Each prototype `P` SHALL be copied to `P/ForInstancer<hash>`, where
`<hash>` is the lowercase hexadecimal, unpadded, of `TfHash::Combine` of
the instancer's path in the prototype's input namespace and the enclosing
copy root (`/` at the top level), and the instancer's `prototypes` SHALL
name the copies. In each copy:
- the root SHALL take `xform/resetXformStack = true` over its own data, and `visibility/visibility = true` under it;
- every prim SHALL carry `instancedBy` (the instancer, the copy root) under its own data;
- prims strictly under a nested instancer or an `over` SHALL be unrenderable, as at the top level;
- path values SHALL be re-rooted from `P` to the copy.

The original prototype prim SHALL record each copy at
`__usdPrimInfo/piPropagatedPrototypes/ForInstancer<hash>`. A prototype
target the input holds no data for (such as an instance proxy) SHALL be
copied as it is: an empty prim.

#### Scenario: Scattered pebbles
- **WHEN** a point instancer targets two pebble prototypes with protoIndices [0, 1, 0] and three positions
- **THEN** its topology gives the two copies, instance indices [[0, 2], [1]] and translations for the three instances

#### Scenario: Invisible and inactive ids
- **WHEN** a point instancer authors ids [0, 1, 2], `invisibleIds = [1]` and `inactiveIds = [2]`
- **THEN** its mask is [true, false, false] and its instance indices still list all three instances

#### Scenario: A fully inactive prototype
- **WHEN** every instance of prototype 1 of 2 is inactive
- **THEN** `prototypes` and `instanceIndices` hold prototype 0 only

#### Scenario: Prototypes under an invisible scope
- **WHEN** the prototypes live under an invisible `Prototypes` scope
- **THEN** each copy root is visible and its prims are drawn through the instancer, while the originals stay invisible

#### Scenario: A target inside a native instance
- **WHEN** a point instancer's prototype targets a mesh inside an instanceable asset (an instance proxy)
- **THEN** its copy `.../ForInstancer<hash>` is an empty, typeless prim with no children, as in UsdImaging

### Requirement: Nested point instancers
A point instancer inside another point instancer's prototype SHALL be
re-instanced in every copy of that prototype, as Hydra's propagation does:
the copied inner instancer SHALL name copies of its own prototypes, made at
each inner prototype's input path (`<inner prototype>/ForInstancer<hash>`)
and named from the inner instancer's path in the enclosing prototype's
namespace and the enclosing copy root, whose prims it instances; the inner
originals inside the copy SHALL be unrenderable.

#### Scenario: Leaves on a scattered tree
- **WHEN** a point instancer scatters a tree whose prototype holds a point instancer of leaves
- **THEN** each copy of the tree holds a leaves instancer whose prototypes are leaf copies made at the leaf prototype's path, one per tree copy, instanced by that copy's leaves instancer, and the leaf copy's `primOrigin` names the original leaf

## ADDED Requirements

### Requirement: Instance proxy paths in bindings
After both propagations, a path inside a prim's `usdMaterialBindings` that
names a prim under a native instance, which the scene index does not hold,
SHALL be translated as `UsdImaging_InstanceProxyPathTranslationSceneIndex`
translates it: the instance's prefix SHALL be replaced by the root of the
prototype its `instance` data names, repeatedly for nested instances. A
path naming a prim the scene index holds SHALL be left alone. No other data
SHALL be translated.

#### Scenario: A material inside an instanced asset
- **WHEN** `/world/mesh` binds `/world/inst/Looks/red`, inside the instanceable `/world/inst`
- **THEN** its binding names `.../UsdNiInstancer/UsdNiPrototype/Looks/red` in the instance's propagated prototype, and resolves to that material

## REMOVED Requirements

### Requirement: Stable prototype paths
**Reason**: Prototype paths stable across stages served chunked streaming,
which is gone, and rested on rebuilding openusd's private instancing key.
Hydra names a propagated prototype by its USD prototype (`__Prototype_N`)
and the binding hash.
**Migration**: None in Moho. Prototype paths are stable for one stage and
load set, as in Hydra.

### Requirement: Point instancer prototypes inside native instances
**Reason**: M12 resolved a prototype target inside a native instance
through the native instancer's propagated prototype, which needs native
instancing to run first. Hydra runs point instancing first and copies such
a target as the empty prim the stage index gives for an instance proxy.
**Migration**: A consumer that must render such scenes (Moana's `isCoral`)
retargets the point instancer's prototypes outside the instance in the
scene, or keeps a workaround of its own; offering the gap to UsdImaging
upstream is noted in `ROADMAP.md`.

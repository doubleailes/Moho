# light-schemas Specification

## Purpose
How a scene index exposes lights, their parameters, shaping and filters,
and light linking resolved from collections into categories.
## Requirements
### Requirement: Light types and parameters
`SphereLight`, `RectLight`, `DiskLight`, `CylinderLight`, `DistantLight`,
`DomeLight` and `DomeLight_1` SHALL have the types `sphereLight`,
`rectLight`, `diskLight`, `cylinderLight`, `distantLight` and `domeLight`.
A prim with `LightAPI` applied and no light type of its own SHALL have type
`light`. Each SHALL expose, as UsdImaging's light API adapter does:
- `light`, listing `filters` (the light's forwarded filter targets), `isLight` (true) and `materialSyncMode` (a dome light also `portals`, its forwarded portal targets, and a `DomeLight_1` first `domeOffset`, the rotation aligning it with its `poleAxis`), and answering any other name as `UsdImagingPrimAdapter::LookupLightParamAttribute` does: Hydra's light tokens (`intensity`, `exposure`, `color`, `normalize`, `diffuse`, `specular`, `enableColorTemperature`, `colorTemperature`, `radius`, `width`, `height`, `length`, `angle`, `texture:file`, `texture:format`, the `shaping:*` and `shadow:*` tokens) read their `inputs:` attribute, and any other name reads the attribute of that name;
- `material`, a network for the contexts `""`, `__all` and each context a `<context>:light:shaderId` attribute names: one node, keyed by the light prim's absolute path, with the light's shader identifier for that context as `nodeIdentifier`, its `renderContextNodeIdentifiers`, and its inputs as `parameters`; and the terminal `light` naming that node.

#### Scenario: A sphere light
- **WHEN** a `SphereLight` authors `inputs:intensity = 10` and `inputs:radius = 0.5`
- **THEN** its type is `sphereLight`; its `light` lists `filters`, `isLight` and `materialSyncMode`, and answers `intensity` 10 and `radius` 0.5; its `material/""` network has the node `/<light path>` with parameter `radius` 0.5 feeding the terminal `light`

#### Scenario: A dome's texture and pole
- **WHEN** a `DomeLight_1` authors `inputs:texture:file` and a pole axis
- **THEN** its `light` answers `texture:file` with the resolved path, and `poleAxis` with its pole axis

### Requirement: Light parameters dirty the light
A change to a light's `inputs:*` SHALL dirty that light's `light` and
`material`; a change to its `light:*` attributes SHALL dirty `light`; a
change to a dome light's `portals`, or a `DomeLight_1`'s `poleAxis`, SHALL
dirty `light`.

#### Scenario: Intensity edit
- **WHEN** a light's `inputs:intensity` changes and updates are applied
- **THEN** the light is dirtied for `light` and `material`

### Requirement: Collections
A prim that applies `CollectionAPI` instances SHALL expose `collections`,
keyed by instance name. Each SHALL give the complete membership expression,
resolved from its includes, excludes, `includeRoot`, expansion rule and
authored membership expression.

#### Scenario: A light-link collection
- **WHEN** a light's `collection:lightLink:includes` targets `/world/a` and excludes `/world/a/b`
- **THEN** its `collections/lightLink` gives an expression that matches `/world/a/c` and does not match `/world/a/b`

### Requirement: Resolved light linking
A light linking index, as `HdsiLightLinkingSceneIndex`, SHALL be a filter a
consumer adds among its renderer filters; `chain::standard` SHALL NOT
contain it. It SHALL read the linking collections of lights (`lightLink`,
`shadowLink`) and light filters (`filterLink`, `shadowLink`) from their
`collections/<name>/membershipExpression`, and judge them over its input
scene index with Hydra's collection predicates. Then:
- collections whose expressions are equal SHALL share a category id `group_<n>`, `n` counting distinct non-trivial expressions in the order they are first seen; a trivial expression (everything, or everything but properties) SHALL have the empty id;
- a light or light filter that has the collection in `collections` SHALL expose its id at `light/lightLink`, `light/shadowLink` and `light/lightFilterLink` (from `filterLink`), the empty id when trivial;
- a prim of a Hydra geometry (rprim) type whose `instancedBy/paths` is empty SHALL expose `categories` naming the ids of the collections that include it, and no `categories` when none does;
- a native instancer (with `instancerTopology/instanceLocations`) SHALL expose `instanceCategories`, one entry per instance location, judged at that location;
- a point instancer SHALL expose `categories`, judged at its own path, for all its instances;
- an instancer SHALL expose `__dependencies` making its `instanceCategories` depend on each instance's `categories`.

When a linking collection changes, the index SHALL dirty `categories` on
the prims its old and new expressions include, and the light's link
locator when its id changes.

#### Scenario: A light linked to one prim
- **WHEN** a light's light-link collection includes only `/world/a`
- **THEN** `/world/a`'s categories include the light's light-link id `group_<n>`, and `/world/b` has no `categories`

#### Scenario: Default linking
- **WHEN** a light authors no light-link includes beyond the default (`includeRoot`)
- **THEN** its `light/lightLink` is the empty id, and no prim gets a category for it

#### Scenario: Equal expressions share an id
- **WHEN** two lights' light-link collections resolve to the same expression
- **THEN** both lights' `light/lightLink` give the same `group_<n>`

#### Scenario: Instanced geometry
- **WHEN** a light links only the prims under a point instancer's prototype and the instancer itself
- **THEN** the prototype copies' meshes (instanced) have no `categories`, and the point instancer's `categories` hold the id

#### Scenario: Native instances
- **WHEN** a light links one of three native instances of an asset
- **THEN** their instancer's `instanceCategories` hold the id for that instance's entry only

#### Scenario: Not in the standard chain
- **WHEN** a consumer reads a light through `chain::standard` alone
- **THEN** its `light` has no `lightLink`, and no geometry has `categories`

### Requirement: Light filters
A `LightFilter` prim (or a prim of a type derived from it) SHALL have type
`lightFilter` and SHALL expose, as UsdImaging's light filter adapter does,
its collections (`filterLink` among them) and `material`, a network with
the terminal `lightFilter` whose node is the filter prim, keyed by its
absolute path, with the identifier its `lightFilter:shaderId` gives (its
parameters are that node's). It SHALL have no `light` of its own: the light
linking filter gives it one. A light filter SHALL represent its
descendants, as a material does. A light's `light/filters` SHALL name the
filters it targets.

#### Scenario: A light with a filter
- **WHEN** a sphere light targets `/world/blocker`, a `LightFilter` whose filter-link collection includes `/world/a`
- **THEN** `/world/blocker` has type `lightFilter`; with light linking added, its `light/lightFilterLink` is a `group_<n>` that `/world/a`'s categories hold


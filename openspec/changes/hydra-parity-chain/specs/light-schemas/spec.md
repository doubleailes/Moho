## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Light filters
A `LightFilter` prim (or a prim of a type derived from it) SHALL have type
`lightFilter` and SHALL expose, as UsdImaging's light filter adapter does,
its collections (`filterLink` among them), `light` answering its parameters
by name, and `material`, a network with the terminal `lightFilter` whose
node is the filter prim, keyed by its absolute path, with the identifier
its `lightFilter:shaderId` gives. A light's `light/filters` SHALL name the
filters it targets.

#### Scenario: A light with a filter
- **WHEN** a sphere light targets `/world/blocker`, a `LightFilter` whose filter-link collection includes `/world/a`
- **THEN** `/world/blocker` has type `lightFilter`; with light linking added, its `light/lightFilterLink` is a `group_<n>` that `/world/a`'s categories hold

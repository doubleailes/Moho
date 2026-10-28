## ADDED Requirements

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

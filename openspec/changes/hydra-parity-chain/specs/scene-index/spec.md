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

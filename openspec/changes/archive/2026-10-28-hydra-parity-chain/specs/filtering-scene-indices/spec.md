## MODIFIED Requirements

### Requirement: Merging inputs
A merging index, as `HdMergingSceneIndex`, SHALL combine inputs, each added
at a root path, with earlier inputs stronger. An input SHALL contribute
only at and under its root. A prim present in several inputs SHALL take the
first non-empty type, and its data SHALL overlay the inputs' containers,
stronger first; a strict ancestor of a root that no input holds SHALL be a
typeless prim with an empty container. Its children SHALL be the union of
the inputs' children, in no specified order.

#### Scenario: Two inputs share a prim
- **WHEN** two inputs both hold `/world/elem`, the first stronger
- **THEN** reading `/world/elem` gives the first input's data where both define it, and the second's elsewhere

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

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Merged children order
**Reason**: `HdMergingSceneIndex` gathers children in a hash set and
promises no order. Moho may keep listing them deterministically, but the
order is not part of the contract.
**Migration**: A consumer that needs an order sorts the children itself.

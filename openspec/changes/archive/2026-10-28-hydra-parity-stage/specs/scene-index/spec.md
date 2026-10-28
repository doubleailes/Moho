## ADDED Requirements

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

# material-schemas Specification

## Purpose
How a scene index exposes materials: the USD binding opinions, the binding
each prim resolves to, and each material's shading network for
UsdPreviewSurface and MaterialX.
## Requirements
### Requirement: Binding opinions
A prim that applies `MaterialBindingAPI` SHALL expose `usdMaterialBindings`,
keyed by binding purpose (the empty token for all purposes). Each purpose
SHALL list:
- the direct binding: its material path and binding strength;
- the collection bindings: each one's collection prim path, collection name, material path and binding strength.

#### Scenario: A direct binding
- **WHEN** a mesh authors `material:binding = </Looks/red>`
- **THEN** its `usdMaterialBindings` for the empty purpose gives a direct binding to `/Looks/red`, weaker than descendants

#### Scenario: A collection binding
- **WHEN** `/world` authors `material:binding:collection:chrome` with collection `/world.collection:metal` and material `/Looks/chrome`
- **THEN** `/world`'s `usdMaterialBindings` lists that collection binding with its strength

### Requirement: Resolved binding
A material binding resolving index, as
`UsdImagingMaterialBindingsResolvingSceneIndex`, SHALL give each prim that
has `usdMaterialBindings` the data `materialBindings`, keyed by purpose,
with the `path` of the material that applies. For each purpose, walking the
prim's flattened binding levels from the root down, at each level:
1. a collection binding whose collection includes the prim and that is `strongerThanDescendants` wins, and the walk stops;
2. else a direct binding that is `strongerThanDescendants` wins, and the walk stops, even when a weaker collection binding at that level includes the prim;
3. else a collection binding that includes the prim wins for now, and the walk goes on;
4. else the direct binding, if any, wins for now, and the walk goes on.

A collection includes the prim when the collection prim's
`collections/<name>/membershipExpression`, judged over the resolving
index's input scene index with Hydra's collection predicates, matches the
prim's path. A dirtied `usdMaterialBindings` locator SHALL be forwarded as
`materialBindings` instead (prefix replaced), and a change to a
collection's membership SHALL NOT dirty the prims it binds.

#### Scenario: Inherited binding
- **WHEN** `/world` binds `/Looks/red` and its descendant mesh binds nothing
- **THEN** the mesh's resolved `materialBindings` path is `/Looks/red`

#### Scenario: Stronger than descendants
- **WHEN** `/world` binds `/Looks/red` as `strongerThanDescendants` and the mesh binds `/Looks/blue`
- **THEN** the mesh resolves to `/Looks/red`

#### Scenario: Collection beats direct
- **WHEN** `/world` authors both a direct binding to `/Looks/red` and a collection binding to `/Looks/chrome` whose collection includes the mesh, and the mesh binds nothing
- **THEN** the mesh resolves to `/Looks/chrome`

#### Scenario: A strong direct binding beats a weak collection on the same prim
- **WHEN** `/world` authors a direct binding to `/Looks/red` as `strongerThanDescendants` and a weaker collection binding to `/Looks/chrome` whose collection includes the mesh, and the mesh binds `/Looks/blue`
- **THEN** the mesh resolves to `/Looks/red`

#### Scenario: A predicate in a collection
- **WHEN** a collection's expression is `/world//{hdType:mesh}` and binds `/Looks/chrome`
- **THEN** meshes under `/world` resolve to `/Looks/chrome`, and a camera there does not

#### Scenario: Dirty locators
- **WHEN** a mesh's `material:binding` changes and updates are applied
- **THEN** the resolving index notifies the mesh dirtied for `materialBindings` and not for `usdMaterialBindings`

### Requirement: Material networks
A `Material` SHALL have type `material` and SHALL expose `material`, keyed by
render context, as `UsdImagingDataSourceMaterial` builds it:
- one context per render context its outputs name (`""` for `outputs:surface`, `mtlx` for `outputs:mtlx:surface`, ...), and always `__all`;
- a named context holds the nodes reachable from its terminals, its terminals named without the context's namespace (`surface`, `displacement`, `volume`);
- `__all` holds every `Shader` under the material, connected or not, and every terminal under its full name (`surface`, `mtlx:surface`, `ri:surface`).

Each network SHALL give:
- `terminals`, naming the node and output that feed each terminal;
- `nodes`, keyed by path relative to the material (`./`-less, as UsdImaging names them), a node outside the material keeping its absolute path;
- `config`, the material's `config:*` attributes without the prefix;
- `interface`: `parameters`, each material input with its `displayName`, `displayGroup` and `mappings` (the node paths and input names its connections feed), and `parameterOrder`.

Each node SHALL give:
- `nodeIdentifier` (its `info:id`, empty when the shader is not in `id` mode and no shader registry resolves it);
- `nodeTypeInfo`, for a shader not in `id` mode: its `info:*` attributes other than `info:id`, and `sdrMetadata`;
- `parameters` (each authored input's value, type name and colour space);
- `inputConnections` (upstream node and output).

Node graphs SHALL be followed through their outputs and interface inputs. A
connection whose source prim the stage does not hold SHALL be dropped.

#### Scenario: UsdPreviewSurface with a texture
- **WHEN** a material's surface is a `UsdPreviewSurface` whose `diffuseColor` connects to a `UsdUVTexture` reading `st` from a `UsdPrimvarReader_float2`
- **THEN** its universal network has three nodes with those identifiers, the surface terminal on the preview surface, and connections from texture to `diffuseColor` and from reader to `st`

#### Scenario: A MaterialX network
- **WHEN** a material authors `outputs:mtlx:surface` connected to an `ND_standard_surface_surfaceshader` inside a node graph
- **THEN** its `mtlx` network names that node as the surface terminal, with its parameters and connections, the node graph's interface resolved

#### Scenario: The all context
- **WHEN** a material has a connected preview surface and a disconnected texture shader, and authors `outputs:surface` and `outputs:mtlx:surface`
- **THEN** its `__all` network holds both shaders and the terminals `surface` and `mtlx:surface`

#### Scenario: Interface and config
- **WHEN** a material authors `inputs:tint` connected to its preview surface's `diffuseColor`, and `config:mtlx:version = "1.38"`
- **THEN** its networks' `interface/parameters/tint/mappings` name that node and `diffuseColor`, `interface/parameterOrder` lists `tint`, and `config` gives `mtlx:version`

#### Scenario: A connection off the stage
- **WHEN** a shader input connects to a prim outside the stage's population mask
- **THEN** the connection is absent from the network

### Requirement: Asset values
Asset-valued parameters SHALL carry their authored path and their resolved
path. A relative path SHALL be resolved against the layer that authors it.

#### Scenario: A relative texture
- **WHEN** a `UsdUVTexture` in `/assets/look.usda` authors `inputs:file = @tex/wood.png@`
- **THEN** the parameter's resolved path is `/assets/tex/wood.png`


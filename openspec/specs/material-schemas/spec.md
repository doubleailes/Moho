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
A material binding resolving index SHALL give each prim `materialBindings`,
keyed by purpose, with the `path` of the material that applies:
- walking from the root down, the first binding authored `strongerThanDescendants` wins;
- otherwise the deepest binding wins;
- on one prim, a collection binding that includes the prim beats its direct binding.

Collection membership SHALL follow the collection's includes, excludes,
expansion rule and membership expression.

#### Scenario: Inherited binding
- **WHEN** `/world` binds `/Looks/red` and its descendant mesh binds nothing
- **THEN** the mesh's resolved `materialBindings` path is `/Looks/red`

#### Scenario: Stronger than descendants
- **WHEN** `/world` binds `/Looks/red` as `strongerThanDescendants` and the mesh binds `/Looks/blue`
- **THEN** the mesh resolves to `/Looks/red`

#### Scenario: Collection beats direct
- **WHEN** `/world` authors both a direct binding to `/Looks/red` and a collection binding to `/Looks/chrome` whose collection includes the mesh, and the mesh binds nothing
- **THEN** the mesh resolves to `/Looks/chrome`

### Requirement: Material networks
A `Material` SHALL have type `material` and SHALL expose `material`, keyed by
render context. The universal context (`""`) SHALL come from `outputs:surface`
(and `displacement`, `volume`). Other contexts SHALL come from their
namespaced outputs, such as `outputs:mtlx:surface` for `mtlx`. Each network
SHALL give:
- `terminals`, naming the node and output that feed `surface`, `displacement` and `volume`;
- `nodes`, keyed by path relative to the material.

Each node SHALL give:
- `nodeIdentifier` (its `info:id`);
- `parameters` (each authored input's value, type name and colour space);
- `inputConnections` (upstream node and output).

Node graphs SHALL be followed through their outputs and interface inputs.

#### Scenario: UsdPreviewSurface with a texture
- **WHEN** a material's surface is a `UsdPreviewSurface` whose `diffuseColor` connects to a `UsdUVTexture` reading `st` from a `UsdPrimvarReader_float2`
- **THEN** its universal network has three nodes with those identifiers, the surface terminal on the preview surface, and connections from texture to `diffuseColor` and from reader to `st`

#### Scenario: A MaterialX network
- **WHEN** a material authors `outputs:mtlx:surface` connected to an `ND_standard_surface_surfaceshader` inside a node graph
- **THEN** its `mtlx` network names that node as the surface terminal, with its parameters and connections, the node graph's interface resolved

### Requirement: Asset values
Asset-valued parameters SHALL carry their authored path and their resolved
path. A relative path SHALL be resolved against the layer that authors it.

#### Scenario: A relative texture
- **WHEN** a `UsdUVTexture` in `/assets/look.usda` authors `inputs:file = @tex/wood.png@`
- **THEN** the parameter's resolved path is `/assets/tex/wood.png`

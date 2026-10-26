## ADDED Requirements

### Requirement: Connections to shaders the stage does not hold
A material network SHALL keep a connection, or a terminal, whose source
prim lies outside the stage's population mask, naming the upstream node by
its absolute path and the output by its base name. Connections SHALL keep
their authored order, and a terminal SHALL read its first authored source,
as on the fully loaded stage. A connection to a prim the mask includes but
the stage does not hold is broken, and SHALL be dropped.

#### Scenario: Mixed sources keep their order
- **WHEN** an input connects first to a shader outside the mask, then to one inside the material
- **THEN** the masked network lists both connections in that order, as the fully loaded stage does

#### Scenario: A broken connection
- **WHEN** an input connects to a prim that exists on no stage, inside the mask
- **THEN** the connection is dropped

#### Scenario: A shader outside the chunk's mask
- **WHEN** a material in chunk `/B` connects `diffuseColor` to `/Shared/Tint.outputs:result`, and the stage is masked to `/B`
- **THEN** the network's `S` node keeps a `diffuseColor` connection whose upstream node is `/Shared/Tint`, with output `result`

### Requirement: Shader nodes
A `Shader` prim SHALL expose `__materialNodes`: its node and its upstream
subgraph as a material network holds them, every node named by its
absolute path. A `NodeGraph` prim SHALL expose the nodes behind its outputs
the same way, and `__materialNodeGraph/outputs`: the connection each output
reads. Time-varying parameters in them SHALL be flagged at
`__materialNodes`.

#### Scenario: A shared shader
- **WHEN** a consumer reads `/Shared/Tint`'s `__materialNodes`
- **THEN** it holds the node `/Shared/Tint` with its identifier and parameters

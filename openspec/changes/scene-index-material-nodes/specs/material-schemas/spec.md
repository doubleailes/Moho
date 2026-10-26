## ADDED Requirements

### Requirement: Connections to shaders the stage does not hold
A material network SHALL keep a connection, or a terminal, whose source
prim is not on the stage (outside its population mask), naming the upstream
node by its absolute path and the output by its base name.

#### Scenario: A shader outside the chunk's mask
- **WHEN** a material in chunk `/B` connects `diffuseColor` to `/Shared/Tint.outputs:result`, and the stage is masked to `/B`
- **THEN** the network's `S` node keeps a `diffuseColor` connection whose upstream node is `/Shared/Tint`, with output `result`

### Requirement: Shader nodes
A `Shader` prim SHALL expose `__materialNodes`: its node and its upstream
subgraph as a material network holds them, every node named by its
absolute path.

#### Scenario: A shared shader
- **WHEN** a consumer reads `/Shared/Tint`'s `__materialNodes`
- **THEN** it holds the node `/Shared/Tint` with its identifier and parameters

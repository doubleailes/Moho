## Why

Moving Crust's material networks onto Moho (crust-render's `moho-import`,
task 9.2) found gap M10. On a chunk's population-masked stage, a material
that connects to a shader outside the mask loses the node and the
connection itself: openusd's producer walk cannot follow a source the stage
does not hold. Because the connection is gone, `streaming::unresolved` cannot
list the node, and the side input M2 introduced can never bring it in. A
streamed material then renders without that part of its network.

## What Changes

- The material adapter keeps a connection (or a terminal) whose source prim
  is not on the stage, naming the node by its absolute path.
- `streaming::unresolved` lists such nodes, so the consumer adds a side
  input over a stage masked to them, as for a bound material.
- `Shader` prims expose `__materialNodes`: the shader's node and its
  upstream subgraph, named by absolute paths.
- `MaterialNodeResolvingSceneIndex`, after the merge (`chain::merged`, and
  last in `chain::standard`), fills every network's dangling nodes from the
  prims that hold them, and dirties the material when such a prim comes,
  goes or changes.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `material-schemas`: connections to shaders the stage does not hold, and
  shader nodes.
- `filtering-scene-indices`: material nodes resolved after the merge.

## Impact

- `crates/moho`: `stage/material.rs`, `streaming.rs`, the new
  `filter/material_nodes.rs`, `chain.rs`, `tokens.rs`.
- crust-render's `moho-import` resumes 9.2 to 9.6 on the Moho rev that
  carries it.

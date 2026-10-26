## Context

M1 put light linking after the merge, and M2 gave a consumer
`streaming::unresolved` to add a side input for a material bound from
outside a chunk. A material's network is built by its own stage, so a node
the stage does not hold has no place to come from, even once a side input
holds it. UsdImaging never meets this: it does not stream one material
across population-masked stages.

## Decisions

### D1. Keep the connection, named by the absolute path

When an input's (or terminal's) authored connection targets a prim the
stage does not hold (`Prim::is_valid` is false outside the mask), the
adapter keeps the connection with the prim's absolute path and the output's
base name. Nodes inside the material keep their relative names, so an
absolute name always means "outside the material".

### D2. A shader's own nodes: `__materialNodes`

A side input over a stage masked to the shader holds the shader, but no
material, so no network. `Shader` prims expose `__materialNodes`: the node
and its upstream subgraph as the material adapter builds them, every node
named by its absolute path. It is a Moho-private container, with Hydra's
leading underscores; Hydra represents shaders only through their material
(`UsdImagingShaderAdapter` is represented by its material).
*Alternative:* the filter rebuilding nodes from generic attribute data.
Rejected: it would duplicate the network walk (producers, interface inputs,
colour spaces) outside the adapter that owns it.

### D3. Resolving after the merge

`MaterialNodeResolvingSceneIndex` wraps each material's networks. Reading a
network collects the absolute upstream paths its nodes and terminals name
that it does not hold, adds the `__materialNodes` of the prim at each, and
repeats for what those nodes name, until nothing new is asked for. It
records each asked path against the material, and dirties the material's
`material` when a prim at or under such a path is added, removed, or
dirtied for `__materialNodes`. It is a no-op for a network that holds
every node, so it also closes `chain::standard`.

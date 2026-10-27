## Context

M1 put light linking after the merge, and M2 gave a consumer
`streaming::unresolved` to add a side input for a material bound from
outside a chunk. A material's network is built by its own stage, so a node
the stage does not hold has no place to come from, even once a side input
holds it. UsdImaging never meets this: it does not stream one material
across population-masked stages.

## Decisions

### D1. Keep the connection, named by the absolute path

When an input's (or terminal's) authored connection targets a prim outside
the stage's population mask (not valid, and not in `Stage::mask()`), the
adapter keeps the connection with the prim's absolute path and the output's
base name. A target the mask includes but the stage does not hold is
broken, and dropped as openusd's producer walk drops it. Nodes inside the
material keep their relative names, so an absolute name always means
"outside the material".

Order follows authoring: off-stage connections stand where they were
authored, and the producers of the on-stage ones where the first of them
stands (a target may resolve through a node graph to several producers). A
terminal reads its first authored source, on the stage or off it.

On a masked stage, a broken target outside the mask cannot be told from a
real one: it is listed by `streaming::unresolved`, and a side input masked
to it holds nothing. The consumer stops adding side inputs when a round
resolves nothing new.

### D2. A shader's own nodes: `__materialNodes`

A side input over a stage masked to the shader holds the shader, but no
material, so no network. `Shader` prims expose `__materialNodes`: the node
and its upstream subgraph as the material adapter builds them, every node
named by its absolute path. `NodeGraph` prims expose the nodes behind their
outputs the same way, and `__materialNodeGraph/outputs`, the connection
each output reads, so that a connection to a graph outside the mask can be
rewritten. Their time-varying parameters are flagged at `__materialNodes`,
so a time change dirties what a merged index listens to. It is a
Moho-private container, with Hydra's
leading underscores; Hydra represents shaders only through their material
(`UsdImagingShaderAdapter` is represented by its material).
*Alternative:* the filter rebuilding nodes from generic attribute data.
Rejected: it would duplicate the network walk (producers, interface inputs,
colour spaces) outside the adapter that owns it.

### D3. Resolving after the merge

`MaterialNodeResolvingSceneIndex` wraps each material's networks. Reading a
network collects the absolute upstream paths its nodes and terminals name
that it does not hold, adds the `__materialNodes` of the prim at each (and
records a node graph's output connections to rewrite through), and repeats
for what those nodes name, until nothing new is asked for. Per network, it
replaces what the last resolution read: the dangling paths, every node a
subgraph brought, and the graphs. It dirties the material's `material` when
a prim at or under such a path is added, removed, or dirtied for
`__materialNodes` or `__materialNodeGraph`. A notice batch is checked in one
pass over the recorded paths. A network nobody has read records nothing:
there is no value to be stale. It is a no-op for a network that holds every
node, so it also closes `chain::standard`.

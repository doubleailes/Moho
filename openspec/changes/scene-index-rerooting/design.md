## Context

`UsdImagingNiPrototypePropagatingSceneIndex` and the point instancer
propagation build each propagated prototype with a rerooting scene index,
whose `UsdImagingRerootingContainerDataSource` maps every path and path
array data source through the source-to-target map, leaving paths outside
the map alone. Moho's filters mapped prim paths but served the data as is.

## Decisions

### D1. A lazy re-rooting wrapper

`filter::rerooting::rerooted(data, from, to)` wraps a prim's container:
containers, vectors and sampled data sources are wrapped in turn, and a
sampled value that is a `PathVec` (Moho's path and path-array type) maps
each path at or under `from` to the same path under `to`. Nothing is
computed until read.

### D2. `primOrigin` is not re-rooted

`primOrigin/scenePath` names the USD prim the data comes from. In Hydra it
is an `OriginPath`, not a path data source, so rerooting leaves it alone;
Moho skips it at the prim's top level for the same result.

### D3. Nested point instancers

The point instancing model is built over the output namespace. After the
top-level instancers' copies, each copy's subtree is walked (through the
input, re-rooted) for point instancers; each one found gets copies of its own
prototypes, named by its output path, and its copies are walked in turn. A
path resolves through its copies innermost first to its input path, its data
is re-rooted outermost first, edits map forward through every copy, and a
prim under an instancer but in none of its copies stays typeless.

### D4. Where it applies

Native instancing wraps every prim served through a remap (output root,
`/__Prototype_N` root). Point instancing wraps every prim of a copy, from
the original prototype root to the copy root.

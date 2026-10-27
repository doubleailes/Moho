## ADDED Requirements

### Requirement: Material nodes resolved after the merge
After the merge, a material network's nodes named by absolute paths that it
does not hold SHALL be filled from the `__materialNodes` of the prims at
those paths, repeatedly, until no new node is asked for; a connection to a
node graph SHALL be rewritten to the connection its output reads. Such a
node SHALL be listed by `streaming::unresolved` until an input holds it. The
material SHALL be notified dirtied for `material` when a prim its last
resolution read (a dangling node, a node its subgraph brought, or a node
graph) is added, removed, or dirtied for `__materialNodes` or
`__materialNodeGraph`, including a time change of an animated parameter;
a prim it no longer reads SHALL no longer dirty it.

#### Scenario: A side input brings a shared shader
- **WHEN** chunk `/B`'s material connects to `/Shared/Tint`, only chunk `/B` is an input, and then a side input over a stage masked to `/Shared/Tint` is added
- **THEN** `/Shared/Tint` is first listed as unresolved, then the material is notified dirtied, its network holds the node with the parameters the fully loaded stage gives, and nothing is listed

#### Scenario: An upstream node of a shared shader changes
- **WHEN** a side input holds `/Shared`, the material reads `/Shared/Tint`, which reads `/Shared/Noise`, and `/Shared/Noise` is edited or its animated parameter changes with the side input's time
- **THEN** the material is notified dirtied for `material`

#### Scenario: A node graph outside the mask
- **WHEN** an input connects to `/Shared/Graph.outputs:out` outside the mask, and a side input holds `/Shared/Graph`
- **THEN** the connection reads `/Shared/Graph/Inner`, as on the fully loaded stage

#### Scenario: The side input leaves
- **WHEN** that side input is removed
- **THEN** the material is notified dirtied and its network no longer holds the node

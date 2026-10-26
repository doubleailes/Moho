## ADDED Requirements

### Requirement: Material nodes resolved after the merge
After the merge, a material network's nodes named by absolute paths that it
does not hold SHALL be filled from the `__materialNodes` of the prims at
those paths, repeatedly, until no new node is asked for. Such a node SHALL
be listed by `streaming::unresolved` until an input holds it. The material
SHALL be notified dirtied for `material` when such a prim is added, removed,
or dirtied for `__materialNodes`.

#### Scenario: A side input brings a shared shader
- **WHEN** chunk `/B`'s material connects to `/Shared/Tint`, only chunk `/B` is an input, and then a side input over a stage masked to `/Shared/Tint` is added
- **THEN** `/Shared/Tint` is first listed as unresolved, then the material is notified dirtied, its network holds the node with the parameters the fully loaded stage gives, and nothing is listed

#### Scenario: The side input leaves
- **WHEN** that side input is removed
- **THEN** the material is notified dirtied and its network no longer holds the node

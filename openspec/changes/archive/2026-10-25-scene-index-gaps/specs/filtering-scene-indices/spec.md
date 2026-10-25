## ADDED Requirements

### Requirement: Prims outside the loaded inputs
A consumer SHALL be able to list the prims a merged index depends on that
no input holds: each material a prim's resolved binding names, and each
node of a held material's network that lies outside the material. Adding
an input that holds them SHALL resolve them, with no other change to what
the index exposes.

#### Scenario: A material bound from another chunk
- **WHEN** a chunk's mesh is bound to a material inside another chunk's payload, and only the first chunk and the scene input are inputs
- **THEN** the material's path is listed as unresolved, and once an input over a stage masked to it is added, the material's network is readable and nothing is listed

### Requirement: Merged children order
A merging index SHALL list a prim's children in input order, strongest
first: each input's children in that input's own order, each path once,
then the intermediate prims implied by deeper input roots, in input order.

#### Scenario: Two inputs with shared and own children
- **WHEN** a stronger input lists `/w/b` then `/w/a` and a weaker input lists `/w/a` then `/w/c`
- **THEN** the merged children of `/w` are `/w/b`, `/w/a`, `/w/c`

## ADDED Requirements

### Requirement: Path values inside propagated prototypes
The path values a propagated prototype's prims carry (binding targets,
relationship targets, connections) SHALL name the propagated prototype:
every path at or under the prototype's source root (`/__Prototype_N` for a
native instance, the original prototype for a point instancer's copy) SHALL
be re-rooted under the propagated root. Paths outside the source root SHALL
be left alone, and `primOrigin` SHALL keep naming the USD prim.

#### Scenario: A material bound inside a native prototype
- **WHEN** an instanced asset's mesh binds `/asset/Looks/red`, inside the asset
- **THEN** the propagated prototype's mesh resolves its binding to the propagated `.../Looks/red`, a material prim of the index, and no `/__Prototype_N` path

#### Scenario: A material bound inside a point instancer's prototype
- **WHEN** a point instancer's prototype mesh binds a material inside that prototype
- **THEN** the copy's mesh resolves its binding to the material under the copy, and its `primOrigin` still names the original mesh

### Requirement: Nested point instancers
A point instancer inside another point instancer's prototype SHALL be
re-instanced in every copy of that prototype, as Hydra's propagation does:
the copied inner instancer SHALL name copies of its own prototypes, made
inside the enclosing copy and named by its own output path, whose prims it
instances; the inner originals inside the copy SHALL be drawn only through
it, and a nested copy's path values SHALL be re-rooted level by level.

#### Scenario: Leaves on a scattered tree
- **WHEN** a point instancer scatters a tree whose prototype holds a point instancer of leaves
- **THEN** each copy of the tree holds a leaves instancer whose prototypes are leaf copies inside that tree copy, instanced by it, and the leaf copy's `primOrigin` names the original leaf

## ADDED Requirements

### Requirement: Point instancer prototypes inside native instances
A point instancer prototype target that lies inside a native instance (an
instance proxy) SHALL be copied from the same prim in that instance's
propagated prototype, under the instancer at
`<instancer>/ForInstancer<hash>/prototype<i>`, with a typeless scope prim
holding the copies.

#### Scenario: Coral prototypes inside an instanced asset
- **WHEN** a point instancer's prototype targets a mesh inside an instanceable asset under its `Prototypes` scope
- **THEN** its topology names a copy under the instancer, the copy is that mesh with its points, instanced by the point instancer, and the copy is reachable through the instancer's `ForInstancer<hash>` scope

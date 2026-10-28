## ADDED Requirements

### Requirement: Prims under overs and instancers are not drawn
As UsdImaging's point instancer prototype scene index does over the whole
stage, every prim strictly under a prim whose specifier is `over`, or
strictly under an instancer, SHALL be made unrenderable: its type SHALL
become empty unless it is `material`, its `__usdPrimInfo/niPrototypePath`
SHALL be blocked, so that it is not aggregated as a native instance, and
its `geomModel/applyDrawMode` SHALL be false. Its other data SHALL stay
readable. The `over` or instancer prim itself SHALL keep its type.

#### Scenario: Prototypes under an over
- **WHEN** a stage holds `over "Prototypes"` with a mesh `pebble` under it, and no point instancer targets it
- **THEN** `/Prototypes/pebble` is a prim of the chain with an empty type, and its points still read

#### Scenario: A material under an over
- **WHEN** a material is defined under an `over`
- **THEN** it keeps the type `material`

#### Scenario: An instance under an over
- **WHEN** an instanceable prim lies under an `over`
- **THEN** it has no `niPrototypePath` in the chain, and native instancing does not aggregate it

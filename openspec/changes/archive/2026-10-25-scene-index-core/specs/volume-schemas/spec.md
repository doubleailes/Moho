## Purpose

How a scene index exposes UsdVol volumes: the volume prim with its field
bindings, and each field asset.

## ADDED Requirements

### Requirement: Volumes and field bindings
A `Volume` SHALL have type `volume` and SHALL expose `volumeFieldBinding`, a
container mapping each `field:<name>` relationship to the field prim's path.

#### Scenario: A density field
- **WHEN** a volume authors `field:density = </world/smoke/density>`
- **THEN** its `volumeFieldBinding/density` is `/world/smoke/density`

### Requirement: Field assets
An `OpenVDBAsset` or `Field3DAsset` SHALL have type `openvdbAsset` or
`field3dAsset` and SHALL expose `volumeField` with:
- `filePath` (authored and resolved);
- `fieldName`, `fieldIndex` and `fieldDataType`;
- `vectorDataRoleHint`.

#### Scenario: An OpenVDB density grid
- **WHEN** an `OpenVDBAsset` authors `filePath = @smoke.vdb@` and `fieldName = "density"`
- **THEN** its type is `openvdbAsset`, and `volumeField` gives the resolved file path and `density`

### Requirement: Renderer-defined volumes
A prim that carries no UsdVol schema SHALL still expose its attributes
through the generic attribute data, so renderer-defined volumes (such as
Crust's `crust:volume:*`) stay readable.

#### Scenario: A procedural volume
- **WHEN** a prim authors `crust:volume:type = "noise"` and its noise parameters
- **THEN** a consumer reads them through the prim's generic attribute data

# volume-schemas Specification

## Purpose
How a scene index exposes UsdVol volumes: the volume prim with its field
bindings, and each field asset.

## Requirements

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
through the generic attribute data, so volumes defined in a renderer's own
namespace stay readable.

#### Scenario: A procedural volume
- **WHEN** a prim authors `myRenderer:volume:type = "noise"` and its noise parameters
- **THEN** a consumer reads them through the prim's generic attribute data

### Requirement: Particle fields
A `ParticleField` prim, and a prim of any type derived from `ParticleField`
that has no adapter of its own (`ParticleField3DGaussianSplat`), SHALL have
type `particleField` and SHALL expose the data of a gprim.

#### Scenario: A splat and a plain particle field
- **WHEN** a stage defines a `ParticleField3DGaussianSplat` and a `ParticleField`
- **THEN** both have type `particleField`, with `primvars`, `xform`, `visibility` and `extent`

### Requirement: Particle field primvars
A particle field's `primvars` SHALL name, after its authored `primvars:*`,
`radiance:sphericalHarmonicsDegree` (interpolation `constant`), then
`positions`, `orientations`, `scales`, `opacities` and
`radiance:sphericalHarmonicsCoefficients` (interpolation `vertex`), and SHALL
give each only when the attribute it reads has an authored value. Each SHALL
name `interpolation` and `role` (the attribute's Hydra role, or the empty
token), `colorSpace` only for a colour role, and no `elementSize`.

#### Scenario: Nothing authored
- **WHEN** a splat authors none of its attributes
- **THEN** its `primvars` names the six and gives none, the degree's schema fallback included

#### Scenario: Coefficient metadata
- **WHEN** the coefficients author `elementSize = 4` and `colorSpace = "lin_rec709_scene"`
- **THEN** `primvars/radiance:sphericalHarmonicsCoefficients` names no `elementSize` and no `colorSpace`, and the generic attribute data of the attribute gives both

### Requirement: Float or half
Each of `positions`, `orientations`, `scales`, `opacities` and
`radiance:sphericalHarmonicsCoefficients` SHALL read the attribute of its
name when that holds a non-empty array at the earliest time, else its `half`
twin (the name with an `h` suffix), and SHALL be named without the suffix
whichever it reads.

#### Scenario: Float attributes
- **WHEN** a splat authors `positions`, `orientations`, `scales`, `opacities` and the coefficients in float
- **THEN** its type is `particleField` and each primvar gives the float array, `positions` with role `point`

#### Scenario: Half attributes only
- **WHEN** a splat authors only `positionsh`, `orientationsh`, `scalesh`, `opacitiesh` and `radiance:sphericalHarmonicsCoefficientsh`
- **THEN** `primvars/positions` and the other four give the half arrays, and no primvar is named `positionsh`

#### Scenario: Both precisions authored
- **WHEN** a splat authors `positions` with elements and `positionsh` with other values, and an empty `opacities` beside a non-empty `opacitiesh`
- **THEN** `positions` gives the float values and `opacities` the half ones

#### Scenario: A binary layer
- **WHEN** the same splats are read from a `.usdc` OpenUSD wrote
- **THEN** the primvars give the same values

### Requirement: Particle field invalidation
An edit to a pair's float or `half` attribute SHALL dirty that pair's
primvar, `primvars/<name without the suffix>`, and no other primvar; an
edit to `radiance:sphericalHarmonicsDegree` SHALL dirty
`primvars/radiance:sphericalHarmonicsDegree`. A time-varying value SHALL be
flagged at `primvars/<name>/primvarValue`, so a time change dirties it.

#### Scenario: The half twin edited
- **WHEN** `positionsh` changes on a half-only splat
- **THEN** the prim is dirtied at `primvars/positions` and at no other primvar

#### Scenario: Animated positions
- **WHEN** a splat's `positions` has time samples and the index's time changes
- **THEN** only `primvars/positions/primvarValue` is dirtied on it

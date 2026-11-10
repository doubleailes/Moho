# Proposal

## Why

OpenUSD 26.03 standardised Gaussian splats as `UsdVolParticleField3DGaussianSplat`,
and UsdImaging gives them to Hydra as `particleField` rprims
(`usdVolImaging/particleFieldAdapter.cpp`, `dataSourceParticleField.cpp`). The
pinned openusd decodes every splat attribute, the `half` twins included, and
ships the typed schema with `attribute_in_use`. Moho has no adapter for it: a
splat prim gets only the adapters every prim gets (transform, visibility,
prim info, user primvars, the generic attribute data) and no type, so a
renderer that reads Hydra's schemas alone cannot see the particles.

## What Changes

- `ParticleField` and every type derived from it (`includeDerivedPrimTypes`,
  so `ParticleField3DGaussianSplat`) become gprims of type `particleField`.
- Their `primvars` gain UsdImaging's custom primvars: `positions`,
  `orientations`, `scales`, `opacities` and
  `radiance:sphericalHarmonicsCoefficients` (vertex), and
  `radiance:sphericalHarmonicsDegree` (constant). Each of the five pairs reads
  the float attribute when it holds elements at the earliest time, else its
  `half` twin, under the float attribute's name.
- An edit to either attribute of a pair dirties that one primvar; a degree
  edit the degree's. Time-varying values are flagged as any primvar's.
- As every custom primvar in UsdImaging: no `elementSize`, and `colorSpace`
  only for a colour role. The attributes' metadata stays in the generic
  attribute data.
- The adapter registry gains the derived-type lookup UsdImaging's
  `includeDerivedPrimTypes` gives, which the light filter special case
  becomes an instance of.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `volume-schemas`: adds "Particle fields" (type, custom primvars, the
  float-or-half choice, invalidation).

## Impact

- `crates/moho/src/stage/particle_field.rs` (new), `stage/primvars.rs`,
  `stage/adapter.rs`, `stage/mod.rs`, `tokens.rs`;
  `tests/particle_field.rs` and its `.usdc` fixture, written by
  `scripts/particle-field-fixture.py` with OpenUSD's own writer.
- `docs/architecture.md`, `docs/openusd-notes.md`, `docs/hydra-parity-check.md`.
- Consumers: additive. A new Hydra prim type reaches a consumer's prim
  factory, which ignores a type it does not handle. No existing prim changes
  type or data.

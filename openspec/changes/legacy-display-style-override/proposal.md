# Proposal

## Why

Moho has no counterpart of the display style scene index
`UsdImagingGLEngine` puts after UsdImaging's chain
(`HdsiLegacyDisplayStyleOverrideSceneIndex`), so a host built on Moho
cannot hand its renderer a refine level fallback: interactively, a renderer
that refines as Storm does (`displayStyle/refineLevel`) always draws refine
level 0, where `usdrecord --complexity medium|high|veryhigh` refines
(issue #54).

## What Changes

- `LegacyDisplayStyleOverrideSceneIndex`, a port of
  `HdsiLegacyDisplayStyleOverrideSceneIndex` (OpenUSD 26.08):
  `set_refine_level_fallback` and `set_cull_style_fallback` underlay
  `displayStyle/refineLevel` and `displayStyle/cullStyle` beneath every
  prim, and a new value dirties every prim as Hydra does.
- `refine_level_for_complexity`, `UsdImagingGLEngine`'s `_GetRefineLevel`,
  for the host that sets the fallback from a complexity.
- Its place is documented (after the standard chain, before the renderer
  filters, as the engine builds it); it is not a `StandardOptions` switch,
  since `UsdImagingCreateSceneIndicesInfo` has none.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filtering-scene-indices`: adds "Display style fallbacks".

## Impact

- `crates/moho/src/filter/legacy_display_style_override.rs` (new),
  `filter/mod.rs`, `tokens.rs`, `chain.rs` (docs);
  `tests/legacy_display_style_override.rs`.
- `docs/architecture.md`.
- Consumers: additive. A host opts in by wrapping its chain.

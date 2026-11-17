# Proposal

## Why

Moho's chain has no equivalent of UsdImaging's draw mode scene index, so
`UsdGeomModelAPI` draw modes (`model:drawMode = origin | bounds | cards`)
never reach a renderer: a model authored with a draw mode is imaged as its
full geometry, unlike Storm and usdview, which build the chain with
`addDrawModeSceneIndex`. Delegates over Moho that target Storm parity cannot
reproduce draw modes without it, and per-renderer reimplementations would
diverge (issue #52). `hydra-parity-instancing` (D7) deferred it, with the
hook it needs: native instancer propagation's append callback, and
`geomModel` carried by the stage and the flattening.

## What Changes

- `DrawModeSceneIndex`, a port of `UsdImagingDrawModeSceneIndex` and
  `UsdImaging_DrawModeStandin` (OpenUSD 26.08): a prim whose `geomModel`
  applies an `origin`, `bounds` or `cards` draw mode is replaced, with its
  subtree, by stand-in prims named, typed and laid out as UsdImaging's, and
  notified and dirtied as UsdImaging does.
- `UnloadedDrawModeSceneIndex`, a port of
  `UsdImagingUnloadedDrawModeSceneIndex`: an unloaded prim is given the
  `bounds` draw mode.
- `chain::standard_with(stage, StandardOptions)`, with UsdImaging's two
  options: `add_draw_mode_scene_index` (the draw mode index as native
  propagation's append callback) and `display_unloaded_prims_with_bounds`
  (the stage index includes unloaded prims, and the unloaded filter runs
  after it). Both off by default; `chain::standard` is unchanged.
- `fromTexture` cards have no face: openusd reads no image, so no texture's
  `worldtoscreen` matrix is found, as UsdImaging does when it cannot open
  the image. Recorded as an upstream candidate.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filtering-scene-indices`: adds "Draw mode", "Unloaded prims drawn as
  bounds" and "Standard chain options".

## Impact

- `crates/moho/src/filter/draw_mode.rs` and `draw_mode/standin.rs`,
  `filter/unloaded_draw_mode.rs` (new), `filter/mod.rs`, `chain.rs`,
  `tokens.rs`; `tests/draw_mode.rs`.
- `docs/architecture.md`, `docs/openusd-notes.md`, `ROADMAP.md`.
- Consumers: additive. `chain::standard` builds what it built; a host opts
  in through `chain::standard_with`.

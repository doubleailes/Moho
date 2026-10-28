## Why

Under Hydra, `sceneGlobals/currentFrame` does not come from UsdImaging's
chain: the application appends `HdsiSceneGlobalsSceneIndex` and drives it
with `SetCurrentFrame` (usdview's `UsdImagingGLEngine`, Solaris). Moho has
no counterpart, so Crust's delegate (crust-render) learns its index's time
through a host-only workaround (`HostFacts::time()`) instead of reading the
scene globals as it would under Hydra. Porting the filter closes that gap
and keeps Moho at Hydra parity.

## What Changes

- A new filter, a port of `HdsiSceneGlobalsSceneIndex` at the reference
  revision (USD 26.08, OpenUSD `70c45e83b`): a pass-through filter whose
  absolute root carries `sceneGlobals` with the fields a host sets,
  overlaid over the input root's data, the input's own `sceneGlobals`
  (the stage's time range and rate, its active render settings) kept
  beneath it.
- Its setters, as Hydra's: active render pass prim, active render settings
  prim, primary camera prim, current frame, time codes per second and scene
  state id. Each one dirties exactly its own `sceneGlobals/<field>` locator
  at `/`, and only when the value changes (`currentFrame` compared as time
  codes, NaN equal to NaN).
- A renderer filter constructor that places it where usdview's engine
  registers it: phase 0, `AtStart`, for every renderer, handing the built
  index to the host so it can drive it.
- Documentation of the host pattern Hydra uses: the host sets the stage
  index's time and the scene globals' current frame together. Moho does not
  couple them, as Hydra does not.
- Not part of `chain::standard`: UsdImaging's chain does not build it.
- No breaking change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filtering-scene-indices`: adds the scene globals filter (its fields,
  setters and notices, its overlay over the input's root) and its placement
  among the renderer filters.

## Impact

- `crates/moho/src/filter/scene_globals.rs` (new),
  `crates/moho/src/filter/mod.rs`, `crates/moho/src/chain.rs`
  (`RendererFilter::scene_globals`, `SCENE_GLOBALS_PHASE`),
  `crates/moho/src/tokens.rs` (the remaining `HdSceneGlobalsSchema` tokens).
- New public API only. Consumers: crust-render's delegate can read
  `sceneGlobals/currentFrame` from its terminal index once its host appends
  the filter and sets the current frame with the stage time.
- Docs: `docs/architecture.md` (renderer filters, the absolute root, the
  host pattern).

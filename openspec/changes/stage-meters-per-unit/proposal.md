# Proposal

## Why

A render delegate that converts lens millimetres into scene units needs the
stage's `metersPerUnit`. Hydra carries no unit (OpenUSD 26.08's
`HdSceneGlobalsSchema` has none, and UsdImaging's camera data source scales
lens values by a fixed `0.1`), so since `hydra-parity-stage` (D7) Moho tells
such a consumer to read the stage it opened. Crust's depth of field does
exactly that, through `HostFacts::meters_per_unit`, and it is one of the
three facts that keep Crust's renders from depending on the index alone
(Crust's `hydra-only-renders`). A delegate under a host with no stage cannot
render a metre-unit scene's depth of field correctly.

## What Changes

- The stage scene index gives the absolute root one more container,
  `__usdMetersPerUnit/metersPerUnit` (`Value::Double`), present only when the
  stage's session or root layer authors `metersPerUnit`.
- It is a **Moho extension**, named and documented as one: UsdImaging gives
  no such data. It sits beside `__usdUpAxis`, the precedent UsdImaging sets
  for stage metrics on the root, not under `sceneGlobals`, which stays
  exactly `UsdImagingDataSourceStage`'s.
- It follows the root's existing notification rule: a stage metadata edit
  notifies nothing (as UsdImaging and `__usdUpAxis`), a structural change
  resyncs the root.
- It is not flattened and is not an instance data source: it is read at the
  root only.
- This partly reverses `hydra-parity-stage` D7 for this one field; the
  architecture's "what the Hydra parity changes moved" table is updated.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stage-scene-index`: the "Scene globals" requirement allows, and requires,
  `__usdMetersPerUnit/metersPerUnit` when authored, as the one extension of
  the root beyond `UsdImagingDataSourceStage`.

## Impact

- `crates/moho/src/stage/globals.rs` (the root adapter), `tokens.rs` (a
  `usd_meters_per_unit` module), `tests/scene_globals.rs`.
- `docs/architecture.md` ("The absolute root", the parity table),
  `openspec/specs/stage-scene-index/spec.md` through this delta.
- Consumers: additive. Crust's `hydra-only-renders` reads it in place of
  the stage; Obsidian and Mantle are unaffected. Bump order as usual: Moho,
  then crust-render, then Mantle.

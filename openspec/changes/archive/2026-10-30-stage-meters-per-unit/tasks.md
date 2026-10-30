# Tasks

## 1. The root's unit

- [x] 1.1 Add `tokens::usd_meters_per_unit` (`__usdMetersPerUnit`, `metersPerUnit`), beside `usd_up_axis`
- [x] 1.2 In `stage/globals.rs`, name `__usdMetersPerUnit` on the absolute root only when the stage authors `metersPerUnit` (`Stage::meters_per_unit`, no fallback), and give a container holding it as `Value::Double`; update the module doc (the one extension beyond `UsdImagingDataSourceStage`)
- [x] 1.3 Confirm the flattening filter and native instancing leave it alone (not in their flattened names nor in `INSTANCE_DATA_SOURCE_NAMES`): a non-root prim of the standard chain has no `__usdMetersPerUnit`

## 2. Tests

- [x] 2.1 In `tests/scene_globals.rs`: authored `metersPerUnit = 1` reads `1.0` at `__usdMetersPerUnit/metersPerUnit` and `sceneGlobals` has no `metersPerUnit`; unauthored, the root has no `__usdMetersPerUnit` (spec "Meters per unit authored" / "unauthored")
- [x] 2.2 An edit of the root layer's `metersPerUnit` followed by `apply_pending_updates` records no notice, and a re-read gives the new value (spec "Meters per unit edited")
- [x] 2.3 Through `chain::standard`, a non-root prim has no `__usdMetersPerUnit` and the root keeps it

## 3. Documentation and gate

- [x] 3.1 `docs/architecture.md`: "The absolute root" names the extension and its notification rule; the parity table's `metersPerUnit` row reads `__usdMetersPerUnit/metersPerUnit` on the absolute root (authored only)
- [x] 3.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

## Workflow follow-up

- Open the Moho PR; after merge, crust-render's `hydra-only-renders` bumps Moho and reads it.

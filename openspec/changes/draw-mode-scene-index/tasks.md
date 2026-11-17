# Tasks

## 1. The filters

- [x] 1.1 Tokens: curve indices and values, `rightHanded`, `displayColor`/`displayOpacity` and roles, `displayStyle`, `builtinMaterial`, `draw_mode`
- [x] 1.2 `filter/draw_mode/standin.rs`: the origin, bounds and cards standins (prims, data sources, the cards cache, dirty locators)
- [x] 1.3 `filter/draw_mode.rs`: `DrawModeSceneIndex` (initial walk, added, removed, dirtied, renamed) and its append callback
- [x] 1.4 `filter/unloaded_draw_mode.rs`: `UnloadedDrawModeSceneIndex`
- [x] 1.5 `chain::standard_with` and `StandardOptions`

## 2. Tests

- [x] 2.1 `tests/draw_mode.rs`: each standin's prims and data, default and unapplied modes, mode changes, forwarded and rebuilding edits, prims under a standin
- [x] 2.2 Through the chain: off by default, bounds from `extentsHint`, an edit, inheritance, an unloaded payload, a model inside a native prototype

## 3. Documentation and gate

- [x] 3.1 `docs/architecture.md`, `docs/openusd-notes.md`, `ROADMAP.md`
- [x] 3.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

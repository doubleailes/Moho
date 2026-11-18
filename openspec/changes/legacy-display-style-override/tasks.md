# Tasks

## 1. The filter

- [x] 1.1 Tokens: `displayStyle/refineLevel`
- [x] 1.2 `filter/legacy_display_style_override.rs`: `LegacyDisplayStyleOverrideSceneIndex` (underlay, setters, dirtying every prim) and `refine_level_for_complexity`

## 2. Tests

- [x] 2.1 `tests/legacy_display_style_override.rs`: unset fallbacks, fallbacks beneath a prim's own, data read before a change, notices of each setter, over the standard chain
- [x] 2.2 The complexity mapping (unit test)

## 3. Documentation and gate

- [x] 3.1 `docs/architecture.md`, `chain` docs
- [x] 3.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

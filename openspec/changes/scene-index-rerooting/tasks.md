## 1. Re-rooting (M11)

- [x] 1.1 Add `filter::rerooting::rerooted`, re-rooting path values lazily and leaving `primOrigin` alone.
- [x] 1.2 Wrap propagated native prototype prims and point instancer copies. Verify that a material bound inside a native prototype and inside a point instancer's prototype resolves to the propagated material, and that `primOrigin` is unchanged.

## 2. Checks

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate scene-index-rerooting --strict`.

## 1. Re-rooting (M11)

- [x] 1.1 Add `filter::rerooting::rerooted`, re-rooting path values lazily and leaving `primOrigin` alone.
- [x] 1.2 Wrap propagated native prototype prims and point instancer copies. Verify that a material bound inside a native prototype and inside a point instancer's prototype resolves to the propagated material, and that `primOrigin` is unchanged.

## 2. Nested point instancers (review)

- [x] 2.1 Re-instance point instancers inside copies, recursively. Verify that each copy of a scattered tree holds a leaves instancer whose prototypes are leaf copies inside that tree copy, instanced by it, with the inner originals typeless.

## 3. Checks

- [x] 3.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate scene-index-rerooting --strict`.

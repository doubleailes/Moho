## 1. Prim info (M8)

- [x] 1.1 Give every prim `__usdPrimInfo` with `specifier`, `typeName`, `isLoaded`, `apiSchemas` (composed) and `kind` (when authored), beside the instancing fields. Verify with a `DomeLight_1` applying `ShapingAPI`, a prim with a kind and one without, and that the existing instancing tests pass.

## 2. Time variance (M9)

- [x] 2.1 Add `mightBeTimeVarying` to each attribute's generic entry. Verify at the default time that a time-sampled transform op reports true and a default-only one false.

## 3. Checks

- [x] 3.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate scene-index-prim-info --strict`.

## 1. Keep the connection (M10)

- [x] 1.1 Keep connections and terminals whose source prim is not on the stage, named by its absolute path, and list them in `streaming::unresolved`. Verify on a material in a masked chunk connecting to a shader outside the mask.
- [x] 1.2 Expose `__materialNodes` on `Shader` prims: the node and its upstream subgraph, named by absolute paths.

## 2. Resolve after the merge

- [x] 2.1 Add `MaterialNodeResolvingSceneIndex` and put it last in `chain::merged` and `chain::standard`. Verify that a side input over the shared shader fills the node with the fully loaded stage's parameters and dirties the material, that removing it dirties the material and drops the node, and that the existing chain and streaming tests pass unchanged.

## 3. Docs and checks

- [x] 3.1 Update `docs/architecture.md` (streaming, the chain) and run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate scene-index-material-nodes --strict`.

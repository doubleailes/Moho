## 1. Light linking after the merge (M1)

- [x] 1.1 Add `chain::input(stage)` (the standard chain without light linking) and `chain::merged(index)` (light linking over a merging index, no oracle); make `chain::standard` `chain::input` plus linking with the stage as oracle. Verify that the existing chain and light-linking tests pass unchanged.
- [x] 1.2 Add a streaming fixture whose light, in one chunk's payload, links receivers in another chunk's payload. Verify that, with linking after the merge, the receivers' categories equal those of a standard chain over the fully loaded stage, and that removing the light's chunk dirties the receivers' categories and drops the light's category.

## 2. Prims outside the loaded inputs (M2)

- [x] 2.1 Add `streaming::unresolved(index)`: materials named by resolved bindings and network nodes outside held materials, that the index does not hold. Verify with a fixture binding a mesh in one chunk to a material in another chunk's payload: the material is listed; after adding an input over a stage masked to it, its network reads and nothing is listed.

## 3. Stage metadata as generic data (M3)

- [x] 3.1 Expose `__usdStageMetadata/<field>/{value, authored}` on the absolute root for the seven fields, and dirty it with `sceneGlobals` on a stage metadata change. Verify authored and unauthored fields, and the dirty notice. Record `Stage::has_authored_stage_metadata` in `docs/openusd-notes.md`.

## 4. Prim origin (M4)

- [x] 4.1 Add `primOrigin/scenePath` to every stage prim, prototype-relative inside a USD prototype. Verify on a plain prim, a prim inside `/__Prototype_N`, a re-rooted native prototype prim, and a point-instancer prototype copy.

## 5. Strategy (D) in the spike (M5)

- [x] 5.1 Add strategy (D) to `moho-spike`: (B) read through a merging index of `chain::input` chains with `chain::merged`, digesting the index's prims, types and values. Verify on the spike fixture that (D)'s digest equals an index read over the fully loaded stage.
- [x] 5.2 Run (B) and (D) on Moana with `scripts/streaming-spike.sh` and record peak memory and time in `docs/streaming-spike.md`.

## 6. Render pass collections (M6)

- [x] 6.1 Verify, with a render pass applying `prune`, `matte` and `cameraVisibility`, that its `collections` hold those and the built-in `renderVisibility`; fix the collections adapter if they do not.

## 7. Merged children order (M7)

- [x] 7.1 Document the order on `MergingSceneIndex::children` and verify it with two inputs sharing and owning children, and an implied intermediate prim.

## 8. Docs and checks

- [x] 8.1 Update `docs/architecture.md` (linking after the merge, side inputs, generic stage metadata, prim origin), `ROADMAP.md` (the gaps closed; linking predicates across inputs as a known limit) and `CLAUDE.md`'s layout if modules were added.
- [x] 8.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `openspec validate scene-index-gaps --strict`.

## 0. Preconditions

- [x] 0.1 Crust S2 has landed; the five pending changes (`scene-index-prim-info`, `scene-index-material-nodes`, `openusd-through-moho`, `scene-index-rerooting`, `scene-index-proxy-prototypes`) are archived unchanged, so this change's deltas apply to main specs holding their requirements
- [x] 0.2 Re-read every openusd API this change starts relying on at the pinned rev (`PredicateLibrary::define`, `PathExpressionEval::build`/`match_path`, `Prim::variant_selections` or its equivalent, `stage_up_axis`, `Prim::specifier`) and record them in `docs/openusd-notes.md`

## 1. Groundwork

- [x] 1.1 Document in `data_source.rs` and `docs/architecture.md` that a single Hydra `SdfPath` field is a one-element `Value::PathVec`, absent when empty; add `Value::Path` to ROADMAP's upstream candidates and to the gaps in `docs/openusd-notes.md`
- [x] 1.2 Add `DataSource::Block` and move the overlay container out of `filter/merging.rs` into `data_source.rs`, honouring blocks; `locate` treats a block as absent; tests: a block hides weaker data, siblings still read through
- [x] 1.3 Add the `collection` module: Hydra's collection expression evaluator over a `&dyn SceneIndex`, with `hdType`/`type`, `hdVisible`/`visible` (default `true`), `hdPurpose`/`purpose`, `hdHasDataSource`/`hasDataSource`, `hdHasPrimvar`/`hasPrimvar`, `hdHasMaterialBinding`/`hasMaterialBinding`; an unlinkable expression matches nothing
- [x] 1.4 Tests for 1.3: each predicate on a retained scene index, both names, the `hdVisible` default and missing-visibility cases, and an unknown predicate matching nothing

## 2. Stage scene index

- [x] 2.1 Switch the traversal predicate to active, not abstract, and loaded unless unloaded prims are included (`stage/mod.rs` `predicate`, `exposes`); overs and their descendants are exposed
- [x] 2.2 Add the whole-stage pass to the point instancing filter: prims strictly under an `over` or an instancer get an empty type unless `material`, a blocked `__usdPrimInfo/niPrototypePath` and `geomModel/applyDrawMode = false`; native instancing ignores prims whose `niPrototypePath` is blocked
- [x] 2.3 Resync notices: keep the populated path set, send `Added` for a resynced subtree and explicit `Removed` only for populated paths not re-added (`stage/mod.rs`, `pending.rs`)
- [x] 2.4 Ignore every prim-level and root info-only change; delete the `kind` dirtying workaround and comment the gap (no changed fields in openusd); move the ROADMAP upstream candidate for changed fields to say Hydra resyncs on plugin fields
- [x] 2.5 `__usdPrimInfo`: add `variantSelections` (absent when empty), drop `apiSchemas` when empty as UsdImaging does, remove `niPrototypeIdentity` from the container (native instancing reads the key through a crate-private accessor until `hydra-parity-instancing` deletes it)
- [x] 2.6 Add `extentsHint`, `model` and `geomModel` to the prims UsdImaging gives them to, with their invalidation (`stage/imageable.rs` or a new adapter)
- [x] 2.7 Root: `sceneGlobals` with four fields (`activeRenderSettingsPrim` only when authored and non-empty) and `__usdUpAxis/upAxis`; delete `__usdStageMetadata` and the `metersPerUnit`/`upAxis` scene globals (`stage/globals.rs`, `tokens.rs`)
- [x] 2.8 Render prims: `__usdRenderSettings`, `__usdRenderProduct`, `__usdRenderVar` with UsdImaging's field names; delete `instantaneousShutter`; the render settings flattening filter reads the new tokens and writes `HdRenderSettingsSchema`'s fields only, settings-level resolution, aspect and data window reaching products
- [x] 2.9 NURBS curves: `pointWeights` as `primvars/pointWeights`, removed from `nurbsCurves` (`stage/curves.rs`, `tokens.rs`)
- [x] 2.10 Stop copying `membershipExpression` into collection bindings (`stage/material_binding.rs`, `tokens.rs`); the binding filter keeps the stage's membership until `hydra-parity-materials`
- [ ] 2.11 Compare the mesh, curves, primvars, xform, camera, volume, implicit and geom subset adapters with UsdImaging field by field (names, layout, per-property invalidation), record the table as `openspec/changes/hydra-parity-stage/adapters.md`, and fix each divergence found, one commit per adapter

## 3. Extent resolving

- [x] 3.1 Add `filter/extent_resolving.rs` (`ExtentResolvingSceneIndex`, purposes `geometry`, `render`, `proxy`): own `extent` wins, else the union of the hint's ranges; an `extentsHint` dirty also dirties `extent`
- [x] 3.2 Put it first after the stage index in `chain::standard`

## 4. Dropping streaming

- [x] 4.1 Delete `streaming.rs` and its `lib.rs` export
- [x] 4.2 Delete `chain::input` and `chain::merged`; `chain::standard` composes extent resolving, native instancing, point instancing, flattening, material binding resolving, render settings flattening, then light linking, the last two filters given the stage's membership
- [x] 4.3 Delete `filter/material_nodes.rs` and its export
- [x] 4.4 moho-spike: strategy (D) reads one `chain::standard` over a `LoadAll` stage, whole; update its doc comments, `crates/moho-spike/tests/strategy.rs` (`strategy_d_equals_an_index_over_the_loaded_stage`, `below_the_threshold_d_equals_the_reference`) and `scripts/streaming-spike.sh` if it describes (D) as streamed
- [x] 4.5 Rewrite the Streaming section of `docs/architecture.md` as a short note: Moho does not stream; chunking is the consumer's; record why (Hydra parity) and keep the S0 numbers' pointer to `streaming-spike.md`

## 5. Tests

- [ ] 5.1 Delete: `streaming.rs` (4: `the_streamed_index_equals_the_full_one`, `removing_a_chunk_keeps_its_root`, `light_links_reach_across_chunks`, `a_side_input_resolves_a_material_from_another_chunk`); `material_nodes.rs` (7, all built on `chain::merged`, `chain::input` or `streaming::unresolved`); `scene_globals.rs::stage_metadata_says_what_is_authored`; `material_resolve.rs::the_expression_fallback_agrees` (13 in all)
- [ ] 5.2 Modify: `stage.rs::inactive_and_abstract_prims_are_not_exposed` (the `over` is now exposed); `stage_updates.rs::a_prim_defined_and_another_removed` (if its fixture resyncs a held prim); `prim_info.rs::a_kind_edit_dirties_the_prim_info` (inverted: nothing notified) and `prim_info_names_the_usd_prim` (`variantSelections`, no `niPrototypeIdentity`); `scene_globals.rs::authored_globals`, `fallbacks` (no `metersPerUnit`/`upAxis`; `__usdUpAxis`) and `a_stage_metadata_edit_dirties_the_globals` (inverted); `render.rs` (3) and `render_flatten.rs` (6) for `__usdRender*` and the settings' fields; `curves.rs::curves_equal_openusd` (`primvars/pointWeights`); `chain.rs` (2) for the new composition; `material_resolve.rs::chain` helper (no oracle-less mode); adapter tests touched by 2.11 (about 25 in all)
- [ ] 5.3 Add: overs exposed and hidden (a mesh under an `over`, a material under an `over`, an instance under an `over`); a resync re-adding without removal; a kind edit notifying nothing; `variantSelections`; `__usdUpAxis`; `model`, `geomModel`, `extentsHint`; extent resolving (hint only, authored extent wins, hint dirty); NURBS `pointWeights` primvar; blocked data; the evaluator's predicates (about 12)
- [ ] 5.4 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

## 6. Docs

- [x] 6.1 `docs/architecture.md`: the root data (`sceneGlobals`, `__usdUpAxis`), the generic data section without `__usdStageMetadata`, render prim tokens, the chain as it stands after this change
- [ ] 6.2 `docs/openusd-notes.md`: APIs from 0.2; gaps: `Value::Path`, changed fields, resolver context, resolved asset path resyncs
- [ ] 6.3 `ROADMAP.md`: upstream candidates `Value::Path`, changed fields (reworded), asset path resyncs; drop the merged light-linking predicate item and the `CollectionEvaluator::build` item's streaming rationale

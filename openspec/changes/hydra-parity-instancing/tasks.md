## 0. Preconditions

- [ ] 0.1 `hydra-parity-stage` is implemented (block data source, exposed overs and their whole-stage pass, `model`/`geomModel`/`extentsHint`, extent resolving)
- [ ] 0.2 Compute golden `TfHash::Combine(std::string, std::string)` and `TfHash::Combine(std::map<std::string, std::string>)` values with a USD build (a small C++ program against `tf`) for a handful of paths, and keep them as test data

## 1. Building blocks

- [ ] 1.1 Add a re-rooting scene index (`UsdImagingRerootingSceneIndex`): prims, children and notices moved from a source root to a destination root, data wrapped by the existing re-rooting container; tests for prims, children, notices and path values
- [ ] 1.2 Add a private `TfHash` port (SpookyHash `ArchHash64`, `Tf_HashState` folding, `Combine` over strings and over a sorted string map); tests against 0.2's golden values

## 2. Point instancer propagation

- [ ] 2.1 Rewrite `filter/point_instancing.rs` as Hydra's propagation (design D1): the whole-stage pass, then per instancer and target a re-rooting `P → P`, a `PiPrototype` pass, a re-rooting to `P/ForInstancer<hash>` and a nested propagation, merged
- [ ] 2.2 `PiPrototype` pass: copy root overlay `xform/resetXformStack = true`, underlay `visibility/visibility = true`; `instancedBy` underlay on every copy prim; prims under nested instancers and overs unrenderable (empty type unless `material`, `niPrototypePath` blocked, `applyDrawMode` false)
- [ ] 2.3 `ForInstancer<hash>` from `TfHash::Combine(instancer path in the prototype's input namespace, enclosing copy root)`, printed `%zx`; the instancer's `instancerTopology/prototypes` overridden to the copies
- [ ] 2.4 `__usdPrimInfo/piPropagatedPrototypes/ForInstancer<hash>` on each original prototype, dirtied as copies come and go
- [ ] 2.5 Point instancer topology (`stage/point_instancer.rs`): prune fully inactive prototypes and their `instanceIndices` entries; keep inactive instances in `instanceIndices`; mask false for inactive and invisible ids
- [ ] 2.6 Delete M12: `through_instance`, `extra_children`, the scope handling and their notices

## 3. Native instancer propagation

- [ ] 3.1 Rewrite `filter/native_instancing.rs` as Hydra's propagation (design D1): pruning of `/__Prototype_N` at the top level, re-rooting `/__Prototype_N → /UsdNiInstancer/UsdNiPrototype` inside, an `NiPrototype` pass, a flattening, an optional per-prototype append callback, the instance aggregation, and nested propagations re-rooted under each instancer
- [ ] 3.2 Instance aggregation: enclosing prototype root from `instancedBy/prototypeRoots` (or the native prototype root, or `/`); key and `<bindingHash>` name per design D2 and D4; `<bindingHash>` prim with the static, re-rooted copy of the instance data; instances in path order
- [ ] 3.3 Instancer data per design D3: `instancerTopology` with `mask`, `primvars` with instance-rate constant primvars, `instancedBy` when nested; dirty locators as `niInstanceAggregationSceneIndex.cpp`
- [ ] 3.4 `NiPrototype` pass: instance prims' renderable types blanked; prototype root overlay (identity `xform` resetting the stack, `instancedBy`); `instancedBy` underlay on the other prototype prims
- [ ] 3.5 Delete `stage/instancing.rs`, the private identity accessor and `niPrototypeIdentity` from `tokens.rs`; rename `PROPAGATED_ROOT` and the prototype tokens to `UsdNiPropagatedPrototypes`, `UsdNiInstancer`, `UsdNiPrototype`

## 4. Flattening

- [ ] 4.1 Add flattening of `__usdUpAxis` and `model` (overlay) and `geomModel` (UsdImaging's provider) to `filter/flattening.rs`, with dirty propagation
- [ ] 4.2 Remove the top-level flattening from `chain::standard`; flattening runs inside native propagation only

## 5. Instance proxy path translation

- [ ] 5.1 Add `filter/instance_proxy_path_translation.rs` (design D8), configured with `usdMaterialBindings`
- [ ] 5.2 `chain::standard`: extent resolving, point instancer propagation, native instancer propagation, instance proxy path translation, material binding resolving, render settings flattening, light linking (until `hydra-parity-chain`)

## 6. Tests

- [ ] 6.1 Delete: `point_instancing.rs::a_prototype_inside_a_native_instance_is_copied_from_its_propagated_prototype`, `a_proxy_prototype_coming_and_going_notifies_its_scope`, `an_authored_child_named_like_the_scope_keeps_its_prim`; `prototype_identity.rs` (2); `instancing_chunks.rs` (1) (6 in all)
- [ ] 6.2 Modify: `native_instancing.rs` (5: `three_instances_of_one_asset`, `differing_bindings_split_groups`, `nested_instances`, `a_prim_that_stops_being_an_instance_is_dirtied`, `prototype_prims_keep_their_origin`) for paths, mask, primvars, `instancedBy` and dirty locators; `rerooting.rs` (2) for the new paths; `point_instancing.rs` (`scattered_pebbles_and_invisible_ids`, `prototypes_are_rerooted`, `prototype_copies_keep_their_origin`, `a_nested_instancer_is_reinstanced_in_every_copy`) for names, mask and copy root data; `flattening.rs::material_bindings_concatenate_down` if its fixture used the chain; `chain.rs` (2) for the new composition (about 14 in all)
- [ ] 6.3 Add: a native instance inside a point instancer prototype is scattered; a binding only on the instance resolves on the prototype's mesh; an invisible instance is masked; per-instance `displayColor` as an instance primvar; component instances group; `instancedBy` on prototype prims and on a nested instancer; an instance prim's type is blanked; a binding into an instance is translated; a proxy target copies as an empty prim; a copy under an invisible scope is visible; prototypes under an `over` render through the instancer; a fully inactive prototype is pruned; inactive ids are masked, not dropped; `piPropagatedPrototypes`; flattened `__usdUpAxis`, `model`, `geomModel`; `ForInstancer` names against golden values (about 15)
- [ ] 6.4 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; run strategy (D) on ALab and Moana and record the cost against the previous rev

## 7. Docs

- [ ] 7.1 `docs/architecture.md`: the instancing part of the chain, names and layout; draw mode deferred and why
- [ ] 7.2 `ROADMAP.md`: delete the "public instancing key" candidate (keep `prim_stack()` only if still used); add the instance-proxy point instancer target gap (UsdImaging 2.0 against 1.0) to raise upstream with OpenUSD; draw mode under "Later"
- [ ] 7.3 `docs/openusd-notes.md`: drop the instancing key notes; add any API newly relied on

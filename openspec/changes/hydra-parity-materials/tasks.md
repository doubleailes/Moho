## 0. Preconditions

- [x] 0.1 `hydra-parity-instancing` is implemented (instance proxy path translation, flattening inside native propagation)
- [ ] 0.2 Read at the pinned rev the openusd(-schemas) APIs for `info:implementationSource`, `info:*` attributes, `sdrMetadata`, `config:*` properties, interface input consumers, and a light's `light:shaderId`; record them in `docs/openusd-notes.md`

## 1. Material binding resolution

- [x] 1.1 Rewrite `filter/material_binding.rs` resolution with the four per-level steps (design D1), over the flattened binding vector
- [x] 1.2 Judge collection bindings with the collection evaluator over the input scene index (`collections/<name>/membershipExpression` of the collection prim), cached per collection and dropped on `collections` dirties and removals; drop the `CollectionMembership` argument
- [x] 1.3 Give `materialBindings` only to prims that have `usdMaterialBindings`
- [x] 1.4 Dirty locators: replace `usdMaterialBindings` with `materialBindings`; delete the collection-dependent re-dirtying

## 2. Material networks

- [x] 2.1 Add the `__all` context (every `Shader` under the material, terminals under full names) to `stage/material.rs`
- [x] 2.2 Add `config` and `interface` (`parameters` with `displayName`, `displayGroup`, `mappings`; `parameterOrder`) to every context
- [x] 2.3 Add `nodeTypeInfo` for shaders not in `id` mode; `nodeIdentifier` empty when no identifier resolves
- [x] 2.4 Delete `__materialNodes`, `__materialNodeGraph` and off-stage connections (`stage/material.rs`, `tokens.rs`); a connection to a prim the stage does not hold is dropped
- [x] 2.5 Add the material invalidation for `config:*` and interface inputs as UsdImaging's material adapter does

## 3. Lights

- [ ] 3.1 `stage/light.rs`: `light` lists `filters`, `isLight`, `materialSyncMode`; other names through a port of `LookupLightParamAttribute`
- [ ] 3.2 Add `material` to lights: fixed terminal `light`, one node keyed by the light's absolute path, `nodeIdentifier` and `renderContextNodeIdentifiers` from `light:shaderId` and `<context>:light:shaderId`, contexts `""`, `__all` and those named
- [ ] 3.3 Invalidation: `inputs:*` → `light`, `material`; `light:*` → `light`

## 4. Tests

- [ ] 4.1 Delete: `material_resolve.rs::a_collection_edit_reresolves` and `editing_a_collection_elsewhere_reresolves` (Hydra re-resolves on no collection edit) (2)
- [ ] 4.2 Modify: `material_resolve.rs::inherited_strong_deep_and_collection`, `equals_openusd_bound_material` (no oracle; a USD-only predicate fixture, if any, moved to a "matches nothing" case) and its `chain` helper; `material.rs` (5: context names now include `__all`; `an_input_keeps_every_connection` checked against UsdShade's walk; `a_shader_edit_dirties_its_material`, `removing_a_shader_dirties_its_material` unchanged in intent); `light.rs` (`every_light_input_equals_openusd` reads through the lookup, `a_sphere_light`, `a_domes_texture_and_pole`, `an_intensity_edit_dirties_the_light` for `material`) (about 10)
- [ ] 4.3 Add: the strong-direct-over-weak-collection edge case; a collection with `hdType`; dirty locators replaced; no re-dirty on a collection edit; the `__all` context with a disconnected shader; `config`; `interface` mappings and order; `nodeTypeInfo` for a `sourceAsset` shader; a connection off the stage dropped; a light's `material` network and its contexts (about 10)
- [ ] 4.4 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

## 5. Docs

- [ ] 5.1 `docs/architecture.md`: binding resolution over the scene index; network contexts; lights' `material`
- [ ] 5.2 `docs/openusd-notes.md`: APIs from 0.2; gap: no shader registry (Sdr)

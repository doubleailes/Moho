## 0. Preconditions

- [x] 0.1 `hydra-parity-materials` is implemented (no filter but light linking uses `CollectionMembership`)

## 1. Dependencies

- [x] 1.1 Add `HdDependenciesSchema`'s layout to `tokens.rs` and `schema.rs`
- [x] 1.2 Add `filter/dependency_forwarding.rs`, a port of `HdDependencyForwardingSceneIndex` (reverse map from `__dependencies`, transitive dirtying, cycle protection, dependents of added and removed prims)
- [x] 1.3 Render settings flattening: declare Hydra's `__dependencies` entries; delete its internal dependents table and self-dirtying

## 2. Light linking

- [x] 2.1 Rewrite `filter/light_linking.rs` as a port of `HdsiLightLinkingSceneIndex` and its cache (design D2): `group_<n>` ids, trivial expressions, the evaluator over the input, `light/{lightLink, shadowLink, lightFilterLink}` only where `collections` holds the collection
- [x] 2.2 `categories` on non-instanced rprims only, absent when empty; `instanceCategories` on native instancers per instance location; `categories` on point instancers; `__dependencies` on instancers
- [x] 2.3 Invalidation: union of dirty expressions evaluated once, `categories` dirtied on its matches, link locators on lights whose id changed
- [x] 2.4 Light filter prims in `stage/light.rs`: type `lightFilter`, collections, `light`, `material` with terminal `lightFilter` and `lightFilter:shaderId`
- [x] 2.5 Remove light linking from `chain::standard`; remove `CollectionMembership` from `scene_index.rs` and its implementation on the stage index

## 3. Chain and renderer filters

- [x] 3.1 `chain::standard` in Hydra's order (spec "The standard chain"); document its configuration and what it leaves out
- [x] 3.2 Add `chain::renderer_filters` with `RendererFilter { phase, order, build }`, ordered as `HdSceneIndexPluginRegistry`; provide light linking (phase 4, `AtStart`) and dependency forwarding (phase 1000, `AtEnd`) constructors

## 4. Hydra 2.0 consumer path

- [x] 4.1 Add `filter/prim_type_notice_batching.rs`, a port of `HdsiPrimTypeNoticeBatchingSceneIndex` (priority trait, consolidation, `flush`)
- [x] 4.2 Add a prim managing observer, a port of `HdsiPrimManagingSceneIndexObserver` (prim factory trait, managed prims receiving dirties, release on removal, lookup by path)
- [x] 4.3 An example or doc test: `chain::standard` → renderer filters → prim type batching → managing observer, as hdPrman's `renderDelegate.cpp` builds it

## 4b. Update latency

- [ ] 4b.1 Tests for the update-scope requirement: a camera's `focalLength` and transform edits, a light added with trivial and with non-trivial collections; each asserts the exact added and dirtied entries, through `chain::standard` plus the light linking and dependency forwarding filters
- [ ] 4b.2 An update benchmark in `crates/moho-spike` (`bin/update`): on a LoadAll stage, measure the time from the stage edit to the managing observer having received every notice, for (a) a camera `focalLength` edit, (b) a camera transform edit, (c) adding a sphere light with default collections, (d) adding a light linked to one prim. Report the median and worst of N runs, and the count of notices and dirtied entries
- [ ] 4b.3 Run it on a small fixture and on Moana; record the results in `docs/` beside the spike results. Reported, not gated. Update latency ranks ahead of first-load time in later profiling

## 4c. The index's time as UsdImaging's

- [ ] 4c.1 `StageSceneIndex`: a new index's time is time code 0 (a default-constructed `UsdTimeCode`), not the stage's default time; replacing a held stage resets it to `EarliestTime` (`UsdImagingStageSceneIndex::SetStage` → `_StageGlobals::Clear`, `stageSceneIndex.cpp:379-396`, `:966-970`). Check what openusd offers for both time codes and record it in `docs/openusd-notes.md`
- [ ] 4c.2 Tests for the scenarios "A new index reads at time 0" and "Replacing the stage"; update tests that relied on the default time (e.g. `Time variance in the generic attribute data`'s "At the default time" stays: `mightBeTimeVarying` does not depend on the time)
- [ ] 4c.3 Check against the USD 26.08 comparison harness: a point instancer with sampled `protoIndices`, read before any `set_time`, then `set_time(0)`

## 5. Merging

- [ ] 5.1 `filter/merging.rs`: adding an input notifies root and every listed descendant, defined or not; notices forwarded without root filtering; rename as remove plus add (unchanged)
- [ ] 5.2 Keep the deterministic child listing as an implementation detail; drop it from docs as a promise

## 6. Tests

- [ ] 6.1 Delete: `merging.rs::renames_outside_a_root_stay_hidden` (a filtering Hydra does not do); `merging.rs::merged_children_are_in_input_order` (no order promised) (2)
- [ ] 6.2 Modify: `light_linking.rs` (4: ids `group_<n>`, absent `categories`, the filter added over `chain::standard`, invalidation targets); `collections.rs::a_light_link_collection` (expression through the evaluator); `point_instancing.rs::instance_categories_are_judged_per_instance` (now `categories` on the point instancer); `render_flatten.rs::a_var_change_dirties_its_settings` and `removing_and_adding_a_product_dirties_its_settings` (through dependency forwarding); `merging.rs::a_chunk_comes_and_goes`, `two_inputs_share_a_prim`, `a_weaker_input_removes_a_shared_prim` (neutral names), `an_input_at_the_root_notifies_the_root`, `prims_under_undefined_ancestors_are_merged` (undefined descendants notified); `chain.rs` (2: composition, no categories) (about 14)
- [ ] 6.3 Add: equal expressions share an id; instanced geometry without categories; native `instanceCategories`; a light filter's `lightFilterLink`; `chain::standard` alone has no categories; renderer filter ordering; dependency forwarding (chained, removed depended-on prim); render settings without forwarding are not dirtied; prim type batching (consolidation, removal first, priority, empty before flush); managing observer (create, dirty, release); merging an undefined root; a notice outside a root forwarded (about 16)
- [ ] 6.4 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

## 7. Docs and boundaries

- [ ] 7.1 `docs/architecture.md`: the chain in Hydra's order (correcting the old claim), renderer filters by phase, the Hydra 2.0 consumer path, the principle (quirks reproduced; renderer fixes are renderer filters), generic data as the only addition
- [ ] 7.2 Remove Crust, Mantle, Moana and gap ids (M1–M12) from crate docs and comments (`lib.rs`, module docs, comments in `filter/` and `stage/`), keeping openusd workaround comments
- [ ] 7.3 `CLAUDE.md` Boundaries: "Hydra parity; the only addition is generic attribute data" (design D7); the openusd section notes the re-export policy as downstream (design D6)
- [ ] 7.4 `ROADMAP.md`: upstream candidates refreshed (drop `CollectionEvaluator::build` and the merged light-link predicates item; keep changed fields, `Value::Path`, `MakeRelativePath`, rule-map expressions); "Later": draw mode, locator caching, selection, coordinate systems; the Moho-as-`openusd-imaging` item with the re-export note
- [ ] 7.5 At archive, edit the Purpose lines of `openspec/specs/filtering-scene-indices` and `instancing-schemas` that mention chunks and streaming

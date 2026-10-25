# Tasks

Every task ends with the three pre-commit checks (`cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
--workspace`). Every openusd or openusd-schemas API first used in a task is
read at `68a867a` and recorded in `docs/openusd-notes.md` in the same task.

## 1. The contract

- [x] 1.1 Add `openusd-schemas` to `crates/moho` from the workspace (same git rev as openusd), and lay out the modules: `locator`, `data_source`, `scene_index`, `schema`, `stage`, `filter`, `chain` and `tokens`. Verify that `cargo build -p moho` passes and that `cargo tree -i openusd` shows a single openusd.
- [x] 1.2 Implement `Locator` and `LocatorSet` (design D1): prefix cover, `intersects`, and a set that stays minimal on insert. Verify with unit tests for the `scene-index` spec's locator scenarios, plus inserting, intersecting and replacing prefixes.
- [x] 1.3 Implement data sources: `DataSource`, the container, vector and sampled traits on `Rc` handles, sampled values with `sample_times` (`None` when uniform), and retained (in-memory) data sources for tests and filters. Verify with unit tests that a container lists only the names it answers, and that a retained sampled value reports its sample times.
- [x] 1.4 Implement the `SceneIndex` and `Observer` traits with batched added, removed, dirtied and renamed entries; observers held weakly; and a `RetainedSceneIndex` (add, remove and dirty prims by hand), as Hydra has. Add a recording observer for tests. Verify with tests that a removal covers its subtree, a dirty entry is local, and an observer registered late sees only later notices.

## 2. `StageSceneIndex`

- [x] 2.1 Implement `new`, `set_stage` and population by the default traversal, with an adapter registry keyed by prim type and applied API schema (design D2). Prims have their type and an empty container. Verify with a test that setting the fixture stage notifies every prim added with its type, that a second `set_stage` removes the old prims first, and that a masked stage populates only the mask.
- [x] 2.2 Implement the generic attribute data (`__usdAttributes`, `__usdRelationships`; design D3), lazy and per property, with `value`, `authored`, `typeName`, `colorSpace`, `interpolation`, `elementSize` and `connections`, and targets and forwarded targets. Verify that a `crust:rayMask` reads back, that an unauthored `subdivisionScheme` reports its fallback and `authored = false`, and the parity of every attribute on a fixture prim.
- [x] 2.3 Queue stage notices and apply them: a `StageSink` records resynced prims, resynced properties and info-only properties, and `apply_pending_updates` coalesces resyncs by prefix, repopulates them, and maps property changes to locators through the adapters (`load_rules_changed` resyncs its paths). Verify the `stage-scene-index` scenarios "nothing until applied", "a prim defined and another removed" and "several edits, one application".
- [x] 2.4 Implement time: sampled data sources record `(path, locator)` in the time-varying set when built, `set_time` dirties exactly that set, and `sample_times` uses `time_samples_in` shifted to offsets. Verify the "one animated mesh" and "the same time" scenarios, and the `scene-index` spec's sample-times scenario (samples 1, 2 and 3 at time 2, offsets including -1 and 1).
- [x] 2.5 Expose scene globals at `/`: start and end time codes, time codes per second, the active render settings prim, `metersPerUnit` and up axis. Verify against a fixture that authors each.

## 3. Geometry schemas

- [x] 3.1 Add the xform adapter and `XformSchema`: the local matrix from `xformOpOrder` at the index's time, `resetXformStack`, nothing when the ops have no effect, and any transform op or `xformOpOrder` change dirtying `xform`. Verify on a fixture with translate, rotate, scale, a reset and sampled ops: matrices equal openusd's at times 1 and 10, sample times are reported, and edits dirty `xform`.
- [x] 3.2 Add the mesh adapter and `MeshSchema`, `MeshTopologySchema` and `SubdivisionTagsSchema` (every member in the `geometry-schemas` spec), with the invalidation table. Verify parity on a fixture with a quad, holes, a left-handed mesh, and a Catmull-Clark mesh with creases and corners. Verify that a `faceVertexIndices` edit dirties `mesh/topology`.
- [x] 3.3 Add primvars (`PrimvarsSchema`, `PrimvarSchema`): authored `primvars:*`; `points`, `normals`, `velocities` and `accelerations` on point-based prims; `widths` on curves. Include indexed values and indices, interpolation, role, element size, colour space, the flattened value, and `primvars:normals` over `normals`. Verify the indexed-UV and points scenarios, parity of every primvar on the fixture, that a `primvars:displayColor` edit dirties `primvars/displayColor`, and that a time change on a mesh with sampled points dirties `primvars/points` alone (the `stage-scene-index` "one animated mesh" scenario, whose locator needs the primvar schema).
- [x] 3.4 Add visibility, purpose and extent as in the spec (`invisible` → false, authored purpose inheritable, `default` → `geometry`, extent when authored). Verify each scenario and its invalidation.
- [x] 3.5 Add basis curves and NURBS curves (topology, `type`, `basis`, `wrap`, `order`, `knots`, `ranges`, `pointWeights`, and `widths` as a primvar). Verify parity on a fixture with cubic B-spline hair and a NURBS curve.
- [x] 3.6 Add the implicit shapes (`sphere`, `cube`, `cone`, `cylinder`, `capsule` and `plane`, including the `_1` variants' top and bottom radii), never converted to meshes. Verify parity on a fixture with one of each, including `Cylinder_1` and `Capsule_1`.

## 4. Filters

- [x] 4.1 Implement `FlatteningSceneIndex` with providers for `xform`, `visibility`, `purpose`, constant `primvars` and `usdMaterialBindings`, and descendant dirtying (design D4). Verify each `filtering-scene-indices` flattening scenario, plus a parent move dirtying its whole subtree.
- [x] 4.2 Implement `MergingSceneIndex`: inputs at roots, the stronger first; overlaid data; the first non-empty type; unioned children; add and remove notices; forwarded and corrected input notices. Verify the merging scenarios on `RetainedSceneIndex` inputs, including removing a weaker input's prim that a stronger one still holds.

## 5. Materials

- [x] 5.1 Add the binding adapter (`usdMaterialBindings` per purpose, with the direct binding, collection bindings and strengths) through openusd-schemas' `MaterialBindingAPI`. Verify the direct and collection binding scenarios, and that a `material:binding*` edit dirties `usdMaterialBindings`.
- [x] 5.2 Implement `MaterialBindingResolvingSceneIndex`. Membership comes from openusd's collection queries (design D6). Verify the inherited, `strongerThanDescendants` and collection-beats-direct scenarios, and that the resolved paths equal openusd-schemas' `bound_material("full")` and `bound_material("")` on the fixture.
- [x] 5.3 Add the material adapter: networks per render context from the `outputs:*` namespaces; terminals; nodes relative to the material, with `nodeIdentifier`, parameters (value, type name, colour space) and input connections; node graphs followed through outputs and interface inputs. Verify the UsdPreviewSurface-with-texture and MaterialX scenarios on fixtures, and that an `inputs:*` edit on a shader dirties its material's `material`.
- [x] 5.4 Resolve asset-valued parameters (authored and resolved path, relative to the authoring layer). Verify the relative-texture scenario with a fixture in a subdirectory.

## 6. Lights

- [x] 6.1 Add the light adapters: the seven types; the open `light` container answering `LightAPI`, type and `ShapingAPI` inputs by name; `filters`; type `light` for a bare `LightAPI`. Invalidation: `inputs:*` and `light:*` dirty `light`. Verify the sphere light and dome scenarios and parity of every input on a fixture with one light of each type.
- [x] 6.2 Add the collection adapter (`collections` per instance name, with the complete membership expression from openusd). Verify the light-link collection scenario.
- [x] 6.3 Implement `LightLinkingSceneIndex`: `lightLink` and `shadowLink` category ids (empty for "everything"), `light` naming the categories, `categories` on geometry and `instanceCategories` on instancers, judged at instance and instancer paths. Verify the linked-to-one-prim and default-linking scenarios. Settle whether the native-instancing binding hash needs categories (design, open question 2, against Crust's `light_links.rs`), and record the answer in the design.

## 7. Camera

- [x] 7.1 Add the camera adapter and `CameraSchema` (every member in the `camera-schema` spec; apertures and focal length scaled by Hydra's units; shutter and exposure renamed; `linearExposureScale` computed). Verify the 50 mm and shutter scenarios, and parity, after the unit scale, on a fixture with an orthographic and a perspective camera.

## 8. Instancing

- [x] 8.1 Settle the stable prototype identity (design D5, open question 1): read openusd's prim index graph API at `68a867a` and choose the identity (asset path, prim path, variant selections). Record the API in `docs/openusd-notes.md`. If something is missing, add it under "Upstream candidates" in `ROADMAP.md` and fall back to `prim_stack()` sites, with a comment. Verify with a test that the identity of the same asset's prototype is equal on two masked stages, and differs for two assets.
- [x] 8.2 Implement the native-instancing filter: the stage index marks instances and hides `/__Prototype_N`; instances are grouped by prototype identity and binding hash; one instancer per group under `/MohoNiPropagatedPrototypes/...`, with `instancerTopology` and `hydra:instanceTransforms`; `instance` on each instance; nested instances recursed. Verify the three-instances, differing-bindings and nested scenarios.
- [x] 8.3 Verify stable prototype paths across chunks: two chunk stages masked from one instanced scene name a shared prototype by the same path (the "same prototype in two chunks" scenario).
- [x] 8.4 Add the point-instancer adapter (`instancerTopology` with prototypes, instance indices without `inactiveIds` and the mask from `invisibleIds`; the `hydra:instance*` primvars; velocities, accelerations and angular velocities) and the point-instancer filter (prototypes re-rooted under `ForInstancer<hash>`, `resetXformStack`, `instancedBy`, untargeted prims typeless). Verify the scattered-pebbles and invisible-ids scenarios, and that the light linking filter's `instanceCategories` on a native and a point instancer judges each instance at its own path (6.3 implements it; instancers first exist here).

## 9. Volumes

- [x] 9.1 Add the volume and field asset adapters (`volumeFieldBinding` from `field:*`; `volumeField` with resolved `filePath`, `fieldName`, `fieldIndex`, `fieldDataType` and `vectorDataRoleHint`; types `volume`, `openvdbAsset` and `field3dAsset`). Verify the density-field and OpenVDB scenarios, and that a `crust:volume:*` prim reads through generic attribute data.

## 10. Render settings

- [x] 10.1 Add the render settings, product, var and pass adapters, with every member in the `render-settings-schemas` spec. Non-schema attributes go to `namespacedSettings`; passes get their `matte` and `cameraVisibility` collections. Verify parity on a fixture with settings, two products, three vars and a pass.
- [x] 10.2 Implement `RenderSettingsFlatteningSceneIndex`: products and vars flattened into `renderSettings`, product overrides, var order, and dependencies so that a product or var edit dirties its settings. Verify the settings-with-a-product, product-overrides-camera, vars-in-order, pass and var-changes scenarios.

## 11. The standard chain and streaming

- [x] 11.1 Add `moho::chain::standard(stage_index)`: native instancing, point instancing, flattening, material binding resolving, light linking and render settings flattening, in that order (design D4). Verify on an end-to-end fixture with every family: each value read through the chain equals openusd's resolved value (the world transform, inherited visibility and the resolved binding included), and an edit to each family dirties exactly its locators through the chain.
- [x] 11.2 Verify streaming as S0 decided, on `crates/moho-spike`'s generated fixture: a merging index over a `LoadNone` scene input and the eight chunk inputs, each a standard chain, exposes the same prims and values as one standard chain over the fully loaded stage. Removing a chunk notifies its payload contents removed, while its root stays from the scene input.

## 12. Docs and wrap-up

- [x] 12.1 Update `docs/architecture.md`:
  - the implemented contract;
  - the standard chain;
  - generic attribute data (D3) as Moho's one addition to Hydra.

  Update `ROADMAP.md`: S1 (Moho side) done, and any upstream candidates found. Update `CLAUDE.md`:
  - the layout (`crates/moho` modules);
  - the openusd rule covering openusd-schemas.

  Verify with `grep` that each document names the standard chain and the `__usdAttributes` container.
- [ ] 12.2 Run the full checks, and run `cargo doc -p moho --no-deps` with no warnings. Confirm that every spec scenario has a test, by listing the scenario names against the test names.

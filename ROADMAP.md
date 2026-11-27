# Roadmap

The program is planned in Mantle's OpenSpec change `scene-index`. These are
the steps that land in this repository or depend on it.

- **S0: streaming spike. Done.** Streamed ALab and Moana through a single
  stage with payload `load_and_unload` per subtree, against Crust's
  population-masked stage per chunk. Masked stages won on memory (Moana peak
  1.97 against 5.05 GiB). `StageSceneIndex` keeps Hydra's shape over one
  stage, and streaming is one stage index per chunk under a
  `MergingSceneIndex` (`docs/streaming-spike.md`).
  `crates/moho-spike` stays as a regression benchmark for S2.
- **S1: core (Moho side). Done** (OpenSpec change `scene-index-core`, archived;
  its requirements are the specs in `openspec/specs/`). Scene
  index traits, observers, data sources, locators, `StageSceneIndex` (queued
  notices, time, the time-varying set, generic attribute data in
  `__usdAttributes`), the
  flattening, merging, native and point instancing, material binding, light
  linking and render settings filters, the standard chain
  (`moho::chain::standard`), and schemas for
  every family Crust imports: mesh, xform, primvars, visibility, purpose,
  extent, curves, implicit shapes, materials (UsdPreviewSurface, MaterialX),
  lights and light linking, cameras, instancers, volumes, render settings,
  products, vars and passes. Mantle's side of S1 follows in Mantle.
- **Scene index gaps. Done** (OpenSpec change `scene-index-gaps`, archived;
  its requirements are in `openspec/specs/`). The seven
  gaps S2's plan found (crust-render, `moho-import`, M1 to M7): light linking
  after the merge (`chain::input`, `chain::merged`), `streaming::unresolved`
  for side inputs, `__usdStageMetadata` with authored-ness, Hydra's
  `primOrigin`, strategy (D) in the spike, every render pass collection, and
  a specified order for merged children.
- **S2 / S3 (crust-render):** Crust's importer, then its live updates, move
  onto Moho, gated on parity with the direct import.
- **S4 (Mantle):** Obsidian, the GPU offscreen render delegate, as a pure
  Moho consumer.

## Later

- The render control side of `moho::renderer::Renderer` (execute, outputs
  and render buffers, render settings, pause, stop and restart, commands,
  stats). `crates/moho-render` drafts it ahead of Hydra, outside the parity
  core (change `renderer-control`). Reconcile the two once Hydra replaces
  `HdLegacyRenderControlInterface` with an `HdRenderer` API of its own.
- Instancing and material-network-resolve filters beyond what S2 needs.
- Profile the standard chain's per-prim cost when data is read. Strategy
  (D), which reads every data source, took 6.6× (B)'s time on Moana
  (`docs/streaming-spike.md`), about 2 ms per prim on its two largest
  chunks. The cost of `prim()` and `children()` alone was profiled and cut
  in issue #27 (`docs/walk.md`).
- Offering Moho upstream as an `openusd-imaging` crate in openusd's
  workspace, which holds a placeholder for it since `26c81d8`; the steps
  left are in `README.md`. The `moho::openusd` and `moho::openusd_schemas`
  re-exports and feature forwarding are a downstream policy (one openusd
  pin for the stack) and would move to a thin downstream crate then.
- `HdsiLocatorCachingSceneIndex` on `material`, which UsdImaging's chain
  holds: a cache, to weigh with the per-prim cost profiling above.
- `UsdImagingSelectionSceneIndex`, for a consumer with a selection.
- Update latency: a linked light's invalidation walks the whole scene from
  the root (Hydra's algorithm: the union of the dirty expressions matched
  once over the input), so it grows with the scene
  (`docs/update-latency.md`); since issue #27 most of it is openusd's
  prototype discovery (below). A trivial light's add still grows with the
  scene, and ALab's first edit after setup takes 16 s; neither is located.

## Upstream candidates (OpenUSD)

- A draw mode standin outlives a resync. `UsdImagingStageSceneIndex`
  re-adds a resynced prim without removing it, and
  `UsdImagingDrawModeSceneIndex::_PrimsAdded` forwards a re-added prim that
  no longer resolves to a standin without `_DeleteSubtree`, so the old
  standin keeps answering and hides the prim's children. Loading a payload
  drawn as bounds (`displayUnloadedPrimsWithBounds`) leaves its bounds in
  place. Moho follows UsdImaging (`crates/moho/tests/draw_mode.rs`,
  `loading_a_payload_keeps_its_bounds_as_in_usd_imaging`).

- Point instancer prototypes inside native instances. A `PointInstancer`
  whose `prototypes` target an instance proxy (Moana's `isCoral`) draws
  nothing under UsdImaging 2.0: the stage scene index gives an instance
  proxy no data, and point instancer propagation copies that empty prim.
  UsdImaging 1.0's point instancer adapter traversed the proxy and drew it.
  Moho follows 2.0 (`hydra-parity-instancing` removed its M12 workaround);
  the regression is worth raising with OpenUSD.

- Coordinate system bindings go stale after an edit. UsdImaging's
  `CoordSysAPI` adapter returns a retained snapshot of a prim's bindings,
  and `HdFlatteningSceneIndex` keeps the prim's input data source across
  dirties, so a retargeted or removed binding is dirtied but read with its
  old value until the prim is added again (`docs/hydra-parity-check.md`,
  issue #46). Moho reads the bindings when asked and answers the new ones;
  a lazy adapter data source would fix it in UsdImaging.

## Upstream candidates (openusd)

- Image metadata (C++ `HioImage`'s `GetMetadata`). A `fromTexture` draw
  mode card reads its textures' `worldtoscreen` matrix; openusd reads no
  image, so such a card draws no face, as UsdImaging when the image does
  not open (`crates/moho/src/filter/draw_mode/standin.rs`).
- An array-size read on `usd::Attribute`. A Gaussian splat's float-or-half
  choice (`ParticleField3DGaussianSplat::attribute_in_use`, C++
  `_UsesFloatAttr`) composes and copies the float array to ask whether it
  has elements: millions of elements on a capture, once per primvar fetch
  (`crates/moho/src/stage/particle_field.rs`).
- Child listing cost. `Prim::filtered_children` costs about 1 µs per child
  (10 ms for 10 000 siblings), and about twice that on the first listing
  after an edit. The stage index now holds each prim's children until a
  resync reaches them (`crates/moho/src/stage/mod.rs`).
- Incremental prototype discovery. After any structural edit, the next
  `Stage::prototypes()` walks the whole populated namespace again (its
  `TODO(perf)` in `usd/stage.rs`); C++ updates `Usd_InstanceCache` from the
  change set. The absolute root's children name the prototypes, as in
  UsdImaging, so every invalidation that walks from the root pays it: about
  20 ms of a linked light's 25 ms on the 10 000 mesh fixture
  (`docs/update-latency.md`).
- Prototypes a load composes, in its notice. An edit's `after_commit` names
  the prototypes it composes (mxpv/openusd#223), but `load_rules_changed`
  names only the loaded subtrees: a `LoadNone` stage loading a payload that
  holds two instances names `/world/a`, not `/__Prototype_1`, where C++
  `UsdStage` resyncs every prototype. After a load, the stage index
  resyncs every prototype it has not populated
  (`crates/moho/src/stage/mod.rs`, `apply_pending_updates`).
- Resolved-asset-path resyncs in `StageSink::after_commit` (C++
  `UsdNotice::ObjectsChanged::GetResolvedAssetPathsResyncedPaths`), so an
  index can re-read asset-path-dependent data when resolution changes, as
  UsdImaging does. openusd has them since `26c81d8`
  (`CommittedChange::asset_paths_resynced`); Moho does not read them yet.
- A stage's resolver context (C++ `UsdStage::GetPathResolverContext`), for
  the absolute root's `system/assetResolution` (`crates/moho/src/stage/globals.rs`).
- The changed fields of an info-only path in `StageSink::after_commit` (C++
  `UsdNotice::ObjectsChanged::GetChangedFields`). UsdImaging resyncs a prim
  whose changed metadata includes a plugin field (such as the stage's
  `renderSettingsPrimPath`) and ignores built-in fields (time codes).
  openusd reports the fields C++ `UsdStage` resyncs for (`typeName`,
  `apiSchemas`, `kind`, ...) as resyncs; Moho ignores every other
  prim-level info-only change, and misses the plugin-field resyncs
  (`crates/moho/src/stage/pending.rs`).
- An `extentsHint` read as another type than authored. C++
  `UsdAttribute::Get<T>` fails when the authored value type is not `T`, so
  an `extentsHint` authored `vector3d[]` (ALab) gives UsdImaging no hint
  and an empty extent; openusd casts it, and Moho answers the values.
- A shader registry (Sdr, C++ `SdrRegistry`), so that a shader outside
  `id` mode resolves a node identifier from its source asset or code as
  UsdImaging's material data source does. Moho leaves it empty meanwhile
  (`crates/moho/src/stage/material.rs`).

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
  stats), once Hydra replaces `HdLegacyRenderControlInterface` with an
  `HdRenderer` API of its own. Until then each renderer offers its own.
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
- Draw mode (`UsdImagingDrawModeSceneIndex`), through native instancer
  propagation's append callback (`hydra-parity-instancing`, design D7).
- `HdsiLocatorCachingSceneIndex` on `material`, which UsdImaging's chain
  holds: a cache, to weigh with the per-prim cost profiling above.
- `UsdImagingSelectionSceneIndex`, for a consumer with a selection.
- Coordinate systems (`UsdShadeCoordSysAPI`, `coordSysBinding`), which the
  stage index does not give yet.
- Update latency: a linked light's invalidation walks the whole scene from
  the root (Hydra's algorithm: the union of the dirty expressions matched
  once over the input), so it grows with the scene
  (`docs/update-latency.md`); since issue #27 most of it is openusd's
  prototype discovery (below). A trivial light's add still grows with the
  scene, and ALab's first edit after setup takes 16 s; neither is located.

## Upstream candidates (OpenUSD)

- Point instancer prototypes inside native instances. A `PointInstancer`
  whose `prototypes` target an instance proxy (Moana's `isCoral`) draws
  nothing under UsdImaging 2.0: the stage scene index gives an instance
  proxy no data, and point instancer propagation copies that empty prim.
  UsdImaging 1.0's point instancer adapter traversed the proxy and drew it.
  Moho follows 2.0 (`hydra-parity-instancing` removed its M12 workaround);
  the regression is worth raising with OpenUSD.

## Upstream candidates (openusd)

- Splines (`Ts`): typed knots, evaluation, authoring, usdc encoding.
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
- Prototypes a change composes, in its notice. `after_commit` (a prim made
  the first instance of its prototype) and `load_rules_changed` (a payload
  loaded whose instances need a new prototype), like the other `Resynced`
  notices, name the prototypes composed before the change, not the new
  ones, where C++ `UsdStage` resyncs every prototype; the stage index
  resyncs the unpopulated prototype of each instance it repopulates, and
  after a load every prototype it has not populated
  (`crates/moho/src/stage/mod.rs`, `apply_pending_updates`).
- Prototypes of instances a load does not touch. Loading one payload on
  ALab moves an unloaded instance elsewhere
  (`decor_sheets_projector01_0001`) to a new prototype and retires two,
  where C++ `Usd_InstanceCache` keeps every other instance's prototype;
  the stage index's stage-wide prototype check after a load is there for
  it.
- Unloading a payload does not free what composing it took (no layer
  eviction). On Moana, RSS under one stage only grew across 22 load/unload
  rounds. This is what rules out single-stage streaming (S0).
  [mxpv/openusd#173](https://github.com/mxpv/openusd/issues/173).
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
- Authoring notices as USD's (found by `docs/hydra-parity-check.md`).
  Setting a value on an attribute without a spec is one resync where USD
  sends a resync then an info change (UsdImaging dirties the prim twice); a
  write of the value already held notifies nothing where USD notifies; and
  the `over` specs created to author through a reference are resynced where
  USD reports them info-only, so the stage index re-adds that subtree.
- openusd-schemas' UsdGeom schema predates `model:cardVisibility` (fallback
  `inherited`), so `geomModel/cardVisibility` has no fallback.
- A value read as another type than authored. C++ `UsdAttribute::Get<T>`
  fails when the authored value type is not `T`, and UsdImaging answers its
  default: `GetXformVectors` reads a `float3` translate or a `double3`
  rotate or scale as the identity (`geomXformVectors`), and an `extentsHint`
  authored `vector3d[]` (ALab) gives no hint and an empty extent. openusd
  lets a schema's declared type win and casts the value
  (`XformCommonAPI::xform_vectors`, `Attribute::type_name`), so Moho
  answers the values.
- `SdfPath::MakeRelativePath`. Moho builds a prototype-relative prim origin
  from the path's text meanwhile (`crates/moho/src/stage/prim_origin.rs`).
- A shader registry (Sdr, C++ `SdrRegistry`), so that a shader outside
  `id` mode resolves a node identifier from its source asset or code as
  UsdImaging's material data source does. Moho leaves it empty meanwhile
  (`crates/moho/src/stage/material.rs`).

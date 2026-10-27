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

- Instancing and material-network-resolve filters beyond what S2 needs.
- Profile the standard chain's per-prim cost. Strategy (D), which reads every
  data source, took 6.6× (B)'s time on Moana (`docs/streaming-spike.md`),
  about 2 ms per prim on its two largest chunks.
- Light-link predicates across merged inputs: after a merge, linking judges
  complete membership expressions, so a collection whose membership needs a
  predicate is not judged (the expression fallback's limit).
- Offering Moho upstream as an `openusd-imaging` crate.

## Upstream candidates (openusd)

- Clip files opened for no reason by `might_be_time_varying` and an
  `AttributeQuery`'s value source (ALab: 26.0 → 11.1 GiB on a raw read):
  [mxpv/openusd#175](https://github.com/mxpv/openusd/pull/175). Moho pins
  doubleailes/openusd at `d7d2a29` until it merges.

- Splines (`Ts`): typed knots, evaluation, authoring, usdc encoding.
- Unloading a payload does not free what composing it took (no layer
  eviction). On Moana, RSS under one stage only grew across 22 load/unload
  rounds. This is what rules out single-stage streaming (S0).
  [mxpv/openusd#173](https://github.com/mxpv/openusd/issues/173).
- `StageSink::load_rules_changed` reports the nearest ancestor whose load
  rule changes resolution (e.g. `/world`) rather than the loaded or unloaded
  path, so taken as a subtree resync it covers every sibling subtree.
  [mxpv/openusd#174](https://github.com/mxpv/openusd/issues/174).
- A rule-based collection's membership as one path expression (C++
  `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap`). Moho ports
  it (`crates/moho/src/stage/collection.rs`) until openusd has it.
- A public instancing key that is stable across stages (today
  `pcp::instancing::InstanceKey` is crate-private and names layer stacks by
  a stage-local id), and a non-empty `prim_stack()` for prototype prims. Moho
  rebuilds the key from the public prim-index graph meanwhile
  (`crates/moho/src/stage/instancing.rs`).
- A public `CollectionEvaluator::build(stage, expression)` (today
  `pub(super)`). With it, light linking after a merge could judge a light's
  complete expression, predicates included, against the receiver's own input
  stage; today it judges path patterns only
  (`crates/moho/src/chain.rs`, `merged`).
- The changed fields of an info-only path in `StageSink::after_commit` (C++
  `UsdNotice::ObjectsChanged::GetChangedFields`). UsdImaging resyncs a prim
  whose changed metadata includes a plugin field (such as the stage's
  `renderSettingsPrimPath`) and ignores built-in fields (`kind`, time
  codes). Without the fields, Moho ignores every prim-level info-only
  change, and misses the plugin-field resyncs
  (`crates/moho/src/stage/pending.rs`).
- A single-path value (`Value::Path`, C++ `VtValue` holding an `SdfPath`).
  Hydra types many fields as one `SdfPath`; Moho holds each as a
  one-element `Value::PathVec` meanwhile (`crates/moho/src/data_source.rs`),
  so a single path and a path array cannot be told apart.
- `SdfPath::MakeRelativePath`. Moho builds a prototype-relative prim origin
  from the path's text meanwhile (`crates/moho/src/stage/prim_origin.rs`).

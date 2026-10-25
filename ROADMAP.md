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
- **S1: core.** Scene index traits, observers, data sources, locators,
  `StageSceneIndex` (change notices, time, the time-varying set) and
  `FlatteningSceneIndex`, with typed schemas for every family Crust imports:
  mesh, xform, primvars, visibility, purpose, materials (UsdPreviewSurface,
  MaterialX), lights and light linking, cameras, curves, implicit shapes,
  instancers, volumes, render settings, products, vars and passes.
- **S2 / S3 (crust-render):** Crust's importer, then its live updates, move
  onto Moho, gated on parity with the direct import.
- **S4 (Mantle):** Obsidian, the GPU offscreen render delegate, as a pure
  Moho consumer.

## Later

- Instancing and material-network-resolve filters beyond what S2 needs.
- Offering Moho upstream as an `openusd-imaging` crate.

## Upstream candidates (openusd)

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

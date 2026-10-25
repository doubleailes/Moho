## Why

Planning S2 (crust-render's `moho-import`) mapped Crust's importer onto
Moho, family by family, and found seven gaps (M1 to M7). Two of them make a
streamed index wrong or incomplete: light links across chunks, and prims
bound from outside the loaded chunks. The others leave a consumer without
data it needs (stage metadata authored-ness, the USD path of prototype
prims, all of a render pass's collections) or without a guarantee (the order
of merged children), or leave Moho's own memory cost on Moana unmeasured.
S2 can only plan around them; it is better to close them before it starts.

## What Changes

- **M1, light linking after the merge.** As Hydra places
  `HdsiLightLinkingSceneIndex` at the end of the chain, after scenes are
  merged, `chain::input(stage)` is the standard chain without light linking,
  and `chain::merged(merging)` adds light linking over a merging index, so
  a receiver in one chunk gets the categories of lights in any input.
  `chain::standard(stage)` is `chain::input` followed by light linking, as
  before.
- **M2, prims outside the loaded inputs.** `streaming::unresolved(index)`
  lists the paths a merged index's prims depend on that no input holds: the
  materials their resolved bindings name, and the nodes of those materials'
  networks outside the material. A consumer adds a side input over a stage
  masked to them, as it adds a chunk. Specified and tested.
- **M3, stage metadata as generic data.** The absolute root exposes
  `__usdStageMetadata`: each stage metadata field the scene globals read,
  with its value and whether a layer authors it. This extends Moho's generic
  data (D3) to the pseudo-root; Hydra's `sceneGlobals` is left as Hydra
  names it.
- **M4, prim origin.** Every prim of a stage index carries Hydra's
  `primOrigin/scenePath`, as `UsdImagingDataSourcePrimOrigin` does: the
  prim's path, relative to its prototype root inside a USD prototype. The
  instancing filters keep it on the prims they re-root, so a consumer names
  a prototype prim by the USD prim it comes from.
- **M5, an index strategy in the spike.** `moho-spike` gains strategy (D):
  (B) read through a `MergingSceneIndex` of standard input chains with
  linking after the merge, measured on Moana against (B).
- **M6, every collection of a render pass.** A render pass exposes every
  collection it applies, its built-in ones included (`renderVisibility`,
  `prune`, `matte`, `cameraVisibility`), not only `matte` and
  `cameraVisibility`.
- **M7, merged children order.** A merging index lists children in input
  order, strongest first, each input's children in its own order, each path
  once, then the ancestors implied by deeper roots. Hydra's
  `HdMergingSceneIndex` gives no order; Moho specifies its own so that
  consumers can rely on it.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `light-schemas`: light linking across the inputs of a merged index.
- `filtering-scene-indices`: prims outside the loaded inputs; the order of
  merged children.
- `stage-scene-index`: stage metadata as generic data; prim origin.
- `render-settings-schemas`: a render pass exposes all its collections.

## Impact

- `crates/moho`: `chain` (new `input` and `merged`), new `streaming`
  module, `stage/globals.rs` and a new prim origin adapter, the native and
  point instancing filters, `stage/render.rs` or `stage/collection.rs`,
  `filter/merging.rs` docs.
- `crates/moho-spike`: strategy (D), which makes it depend on `moho`.
- Docs: `docs/architecture.md` (streaming with linking after the merge, side
  inputs, generic stage metadata, prim origin), `docs/streaming-spike.md`
  (strategy D on Moana), `ROADMAP.md`.
- crust-render's `moho-import` plan drops M1 to M7 as open gaps.

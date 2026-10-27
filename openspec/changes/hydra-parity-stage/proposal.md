## Why

Moho is meant to become an `openusd-imaging` crate for upstream openusd, and
Crust a Hydra render delegate (`hdcrust`) that reads C++ UsdImaging in
Solaris and usdview. Both need Moho's output to be UsdImaging's, not a
superset shaped around Crust's importer. The Hydra parity audit
(`audit.md`, in this change) found the stage scene index diverging in what
it exposes, how it notifies, and what it adds, and found streaming
machinery (masked chunk stages merged after the fact) that Hydra has no
counterpart for. This change is the first of four
(`hydra-parity-stage` → `hydra-parity-instancing` →
`hydra-parity-materials` → `hydra-parity-chain`) and lays the groundwork the
other three build on. It is implemented after Crust S2 lands.

## What Changes

Groundwork:
- A single `SdfPath` value stays a one-element `Value::PathVec`, documented
  as the representation of Hydra's `SdfPath` fields; `Value::Path` becomes
  an openusd upstream candidate.
- Overlays gain a block data source (Hydra's `HdBlockDataSource`): a block
  in a stronger container hides the weaker data at that name.
- A port of Hydra's collection expression evaluator and predicate library
  (`HdCollectionExpressionEvaluator`, `HdGetCollectionPredicateLibrary`):
  path expressions judged over a scene index, with `hdType`/`type`,
  `hdVisible`/`visible`, `hdPurpose`/`purpose`, `hdHasDataSource`,
  `hdHasPrimvar` and `hdHasMaterialBinding`. openusd's `PredicateLibrary`
  takes custom predicates over any domain, so no upstream change is needed.

Stage scene index:
- **BREAKING** Prims are traversed with UsdImaging's predicate (active and
  not abstract, and loaded unless unloaded prims are included): `over`
  prims and their descendants are exposed. As in UsdImaging, every prim
  strictly under an `over` or an instancer is made unrenderable (empty
  type, `niPrototypePath` blocked, `applyDrawMode` false) by the point
  instancing filter's whole-stage pass, so exposing overs draws nothing new.
- **BREAKING** A prim resync is notified as `Added` only; `Removed` is sent
  only for prims that are not re-added.
- **BREAKING** A prim-level info-only change (`kind`, stage metadata) no
  longer dirties anything: Hydra ignores built-in fields and resyncs on
  plugin fields, and openusd does not say which fields changed. Until it
  does (upstream candidate), every prim-level info-only change is ignored.
- `__usdPrimInfo` carries exactly `UsdImagingUsdPrimInfoSchema`'s fields:
  `variantSelections` is added; **BREAKING** `niPrototypeIdentity` leaves
  it (native instancing keeps the key privately until
  `hydra-parity-instancing` drops it).
- New prim data as UsdImaging has it: `extentsHint`, `model`
  (`UsdImagingModelSchema`) and `geomModel`.
- **BREAKING** The absolute root carries Hydra's `sceneGlobals`
  (`activeRenderSettingsPrim` only when authored, start and end time codes,
  time codes per second) and `__usdUpAxis/upAxis`. `sceneGlobals/metersPerUnit`,
  `sceneGlobals/upAxis` and `__usdStageMetadata` are removed.
- **BREAKING** Render prims expose `__usdRenderSettings`,
  `__usdRenderProduct` and `__usdRenderVar` with UsdImaging's fields;
  `instantaneousShutter` is removed; the flattened `renderSettings` holds
  only `HdRenderSettingsSchema`'s fields.
- **BREAKING** `membershipExpression` is no longer copied into collection
  bindings.
- **BREAKING** NURBS curves' `pointWeights` becomes the primvar
  `primvars/pointWeights`, as UsdImaging has it.
- The adapters the audit did not compare (mesh, curves, primvars, xform,
  camera, volume, implicit shapes, geom subsets) are compared field by
  field and brought to UsdImaging's layout and invalidation.

Extent resolving:
- `ExtentResolvingSceneIndex`, a port of `UsdImagingExtentResolvingSceneIndex`
  (purposes `geometry`, `render`, `proxy`), first filter after the stage
  index.

Dropping streaming:
- **BREAKING** `streaming.rs` (`streaming::unresolved`), `chain::input` and
  `chain::merged` are removed. `chain::standard` no longer serves as a
  chunk's input chain, and light linking stays in it, over the stage index's
  membership, until `hydra-parity-chain` moves it out.
- **BREAKING** `MaterialNodeResolvingSceneIndex` (M10's after-merge filter)
  is removed. The stage-side M10 data (`__materialNodes`,
  `__materialNodeGraph`, off-stage connections) goes in
  `hydra-parity-materials`.
- `MergingSceneIndex` stays as a general filter; its notices become
  HdMergingSceneIndex's in `hydra-parity-chain`. Chunking a scene is the
  consumer's business.
- moho-spike's strategy (D) stops streaming through Moho: it reads one
  `chain::standard` over a `LoadAll` stage, which keeps the per-prim cost
  benchmark against (C). Strategies (A), (B) and (C) do not use Moho and
  stay.

Pending changes this change supersedes (they stay unarchived here; they are
implemented, and are archived before this change is):
- `scene-index-prim-info`: "Prim info" is modified (Hydra's exact field
  set). "Time variance in the generic attribute data" stands: it is
  generic attribute data.
- `scene-index-material-nodes`: "Material nodes resolved after the merge"
  is removed here; its two material-schemas requirements are removed by
  `hydra-parity-materials`.
- `scene-index-rerooting`, `scene-index-proxy-prototypes`: untouched here
  (see `hydra-parity-instancing`).
- `openusd-through-moho`: no requirements (`skip_specs`); untouched.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `scene-index`: path values, blocked data, collection expressions judged
  over a scene index with Hydra's predicates.
- `stage-scene-index`: traversal predicate, resync and info-only notices,
  prim info, scene globals and up axis, model and extents hint data, adapter
  layout as UsdImaging; stage metadata as generic data removed.
- `filtering-scene-indices`: extent resolving added; prims outside the loaded
  inputs, and material nodes resolved after the merge, removed.
- `render-settings-schemas`: `__usdRender*` stage data and Hydra's
  flattened fields.
- `geometry-schemas`: NURBS point weights as a primvar.
- `camera-schema`: camera units from the scene globals removed.
- `instancing-schemas`: prims under overs and instancers are not drawn.
- `light-schemas`: light linking across merged inputs removed.

## Impact

- `crates/moho/src`: `data_source.rs` (block), new `collection` evaluator
  module, `stage/{mod,pending,prim_info,globals,render,curves,imageable}.rs`,
  new `filter/extent_resolving.rs`, `filter/{render_settings,material_binding,light_linking}.rs`
  (token reads), `chain.rs`, `tokens.rs`, `lib.rs`; `streaming.rs` and
  `filter/material_nodes.rs` deleted.
- `crates/moho-spike/src/strategy.rs` (strategy D), its tests, and
  `scripts/streaming-spike.sh` if it names D's streaming.
- Tests: 13 deleted, about 25 modified, about 12 added (`tasks.md`
  lists them).
- Consumers: crust-render and Mantle lose `chain::input`, `chain::merged`,
  `streaming::unresolved`, `__usdStageMetadata` and
  `sceneGlobals/{metersPerUnit, upAxis}`; they read stage metadata from the
  stage (through `moho::openusd`) where they hold one.
- `docs/architecture.md` (Streaming section, generic data section),
  `docs/openusd-notes.md` (gaps), `ROADMAP.md` (upstream candidates).

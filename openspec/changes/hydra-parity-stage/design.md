## Context

See `proposal.md` for why. `audit.md` (copied into this change) compares
Moho at `d09cc94` with OpenUSD at `70c45e83b`, by code reading. Every Hydra
claim this change builds a requirement on was re-read in the OpenUSD
checkout while writing it; the corrections that re-reading made to the
audit are listed at the end of this document.

Program, with dependencies:

```
hydra-parity-stage  -->  hydra-parity-instancing  -->  hydra-parity-materials  -->  hydra-parity-chain
(groundwork, stage,      (PI before NI, NI layout,     (binding resolving,          (light linking out of
 extents, no streaming)   proxy path translation)       networks, light material)    standard, merging, docs)
```

`hydra-parity-instancing` needs the block data source and the stage's
overs; `hydra-parity-materials` needs the collection evaluator and the
instance proxy path translation; `hydra-parity-chain` needs all three
(light linking judges instanced geometry by `instancedBy`, and the
`CollectionMembership` trait is removed only once no filter uses it). All
four are implemented after Crust S2, in this order, each landing green.

The five pending changes (`scene-index-prim-info`,
`scene-index-material-nodes`, `openusd-through-moho`,
`scene-index-rerooting`, `scene-index-proxy-prototypes`) are implemented
and are archived, unchanged, before this change is archived; the deltas of
the four parity changes refer to their requirements as main-spec
requirements by then.

Principle, for all four changes: parity means UsdImaging's and Hydra's
behaviour at the reference revision (`70c45e83b`), quirks included. A fix a
renderer needs over it belongs to that renderer, as a renderer filter
(`hydra-parity-chain`), the way arnold-usd ships
`fixPropagatedPrototypesVisibilitySIP` and `fixMaterialPruningSIP`; Moho
does not fold it in. When the reference revision moves, Moho follows in a
change of its own.

## Goals / Non-Goals

**Goals:**
- The stage index's prims, types, data names and notices are
  `UsdImagingStageSceneIndex`'s, with the generic attribute data as the
  only addition.
- The groundwork the other three changes need exists and is tested on its
  own.
- Nothing in Moho streams; nothing in it is named for a consumer.

**Non-Goals:**
- `system/assetResolution` at the root: openusd has no `ArResolverContext`
  to give. It stays a documented gap.
- `GetResolvedAssetPathsResyncedPaths` (asset path resyncs): openusd has no
  counterpart. A documented gap and upstream candidate.
- `forceDirtyingTimeDeps` on `set_time`: minor, left out.
- Draw mode, coordinate systems (`coordSysBinding`) and unloaded draw mode:
  see `hydra-parity-instancing` and `hydra-parity-chain`.

## Decisions

### D1. A single `SdfPath` is a one-element path array

openusd's `sdf::Value` has `PathVec` and no single-path variant (checked at
`d7d2a29`). Hydra's single-path fields stay one-element arrays, absent when
Hydra's path would be empty. Alternative: add `Value::Path` to openusd
first; that blocks the whole program on an upstream API change, so it
becomes an upstream candidate instead. When openusd gains it, the switch is
one change in Moho and a breaking change for consumers.

### D2. Blocks are a data source kind

`DataSource` gains a `Block` variant, and a shared overlay container
(today private to `filter/merging.rs`) moves to `data_source.rs` and honours
it: a block at a name stops the overlay from reading weaker containers
there, and the overlay answers nothing for that name. `locate` treats a
block as absent. Alternative: a sentinel value inside `Sampled`; it would
leak into every reader instead of stopping at the overlay, as Hydra's
`HdBlockDataSource` does.

### D3. The collection evaluator is a port over openusd's predicate library

openusd's `PredicateLibrary<D>` takes custom predicates over any domain
(`define(name, binder)`, binders receiving the call's arguments) and
`PathExpressionEval::<D>::build(&expr, &library)` judges a path through a
`Fn(&Path) -> D`. The port makes `D` a scene index prim (its type and data),
defines Hydra's six predicates under both names, and answers a path by
asking the scene index. openusd's `define` has no default arguments, so
each binder supplies Hydra's defaults (`hdVisible`'s `isVisible = true`).
An expression that fails to link (an unknown predicate) yields an evaluator
that matches nothing, as Hydra's. It lives in a new module
(`collection`), public, because the material binding and light linking
filters of the later changes and consumers' own filters use it.
Alternative: keep the stage's `CollectionMembership` oracle; it judges USD
predicates on the USD namespace, which Hydra does not.

### D4. Overs are exposed, and hidden as UsdImaging hides them

The stage index switches from `PrimPredicate::DEFAULT` (or ACTIVE|DEFINED
with unloaded prims) to UsdImaging's: active and not abstract, plus loaded
unless unloaded prims are included. UsdImaging hides what that exposes in
`UsdImaging_PiPrototypeSceneIndex` run over the whole stage (prototype root
`/`, no instancer): prims strictly under an `over` or an instancer lose a
renderable type and their `niPrototypePath`. Moho's point instancing filter
gains that whole-stage pass in this change, using D2's block, so the chain
draws nothing more than before. `hydra-parity-instancing` then moves the
pass into PI propagation, first in the chain.

### D5. Prim-level info-only changes notify nothing

Hydra resyncs a prim for a changed plugin metadata field and ignores
built-in ones (`kind`, and the stage's time codes and up axis are built-in;
`renderSettingsPrimPath` is a plugin field). openusd's
`CommittedChange::changed_info_only` names paths, not fields. Alternatives:
- resync on every prim-level info-only change: a `kind` edit or a time code
  edit would re-add a subtree (the whole stage at the root), which Hydra
  never does for them;
- keep dirtying `__usdPrimInfo/kind` (today's workaround): a notice Hydra
  never sends.

Ignoring them all matches Hydra for every built-in field and misses the
plugin-field resync (a `renderSettingsPrimPath` edit, chiefly). That miss
is documented in the code, in `docs/openusd-notes.md` and in the upstream
candidate for changed fields, and it closes when openusd reports fields. A
consumer that edits `renderSettingsPrimPath` live sets the stage again
meanwhile.

### D6. Resyncs notify `Added` only

`apply_pending_updates` keeps the set of populated paths, as Hydra does,
and after computing a resync's removed and added entries, replaces each
removed entry by explicit removals of the populated paths under it that
are not re-added.

### D7. The root as `UsdImagingDataSourceStage`

`stage/globals.rs` keeps `sceneGlobals` (four fields, the active settings
only when authored and non-empty) and adds `__usdUpAxis/upAxis`
(`openusd_schemas::geom::stage_up_axis`, fallback included);
`__usdStageMetadata` and the `metersPerUnit`/`upAxis` scene globals go.
Since root info-only changes notify nothing (D5), nothing at the root is
dirtied by a stage metadata edit; a sublayer or other structural change
resyncs the root as before.

### D8. Render prims as UsdImaging's

The stage index exposes `__usdRenderSettings`, `__usdRenderProduct` and
`__usdRenderVar` with UsdImaging's field names (`products`, `productType`,
`productName`, `orderedVars`, `camera`), and the flattening filter reads
them and writes `renderSettings` with `HdRenderSettingsSchema`'s fields
only; settings-level resolution, aspect and data window stay on
`__usdRenderSettings` and reach each product through the flattening.
`instantaneousShutter` goes. The flattening's dependency handling is
unchanged here; `hydra-parity-chain` replaces it with `__dependencies`.

### D9. Extent resolving is a filter, first after the stage index

A port of `UsdImagingExtentResolvingSceneIndex` with the purposes
`UsdImagingCreateSceneIndices` gives it (`geometry`, `render`, `proxy`): a
prim's own `extent` wins, else the union of its `extentsHint` ranges for
those purposes. The stage index gains `extentsHint`, `model` and
`geomModel` (UsdImaging's model data), which the instance grouping key of
`hydra-parity-instancing` also reads.

### D10. Adapters are compared field by field before they change

The audit compared prim info, the root, render prims, materials, lights and
instancing, not the geometry, camera, volume and implicit adapters. A task
writes that comparison (names, layout, invalidation locators per property,
as `UsdImagingDataSource<Type>::Invalidate` gives them) into this change's
directory, beside `audit.md`, as `adapters.md`, and each divergence it finds is fixed in this change under "Adapter data follows
UsdImaging". The NURBS `pointWeights` primvar is the one already known.

### D11. Streaming goes; `MergingSceneIndex` stays

Deleted: `streaming.rs`, `chain::input`, `chain::merged`,
`filter/material_nodes.rs` and their tests, and the `membershipExpression`
copied into collection bindings (only the merged chain, which had no
membership oracle, read it). `chain::standard` becomes the composition
`chain::input` held, with light linking over it, both still given the
stage's membership; `hydra-parity-chain` rewrites it in Hydra's order.
`MergingSceneIndex` stays a public filter: a consumer can merge scene
indices, and `hydra-parity-chain` brings its notices to Hydra's.

### D12. moho-spike's strategy (D) reads one standard chain

Strategy (D) streamed through `chain::input`/`chain::merged` over masked
chunk stages. It becomes one `chain::standard` over a `LoadAll` stage, read
whole, digested like the other strategies, which keeps the measurement
ROADMAP's "Later" needs (the standard chain's per-prim cost against direct
reading, (C)). Its tests compare (D) with an index over the loaded stage, as
now, without chunks. Strategies (A), (B) and (C) read openusd directly and
stay. Alternative: delete (D); it is the only benchmark of Moho's read cost
on real scenes.

## Risks / Trade-offs

- [Consumers lose streamed import through Moho] → crust-render's S2 import
  either renders from one `LoadAll` stage per scene or does its own
  chunking against the stage; the masked-stage memory win of S0 then lives
  in the consumer, if anywhere.
- [D5 misses plugin-metadata resyncs] → documented gap and upstream
  candidate; consumers set the stage again for such edits.
- [Exposed overs cost traversal] → the subtree under an over is now
  traversed and served (typeless); on scenes that park prototypes under
  overs it was being skipped before. Measured with strategy (D) on ALab and
  Moana before and after.
- [Breaking data names for consumers] → crust-render and Mantle bump Moho
  with this change and adapt: `__usdRender*`, `__usdUpAxis`, no stage
  metadata container.

## Migration Plan

Land after Crust S2, on its own Moho rev. Consumers bump Moho and adapt in
the same step. No rollback beyond reverting the rev.

## Corrections to the audit found while writing these changes

- Light property edits (audit #55): Hydra dirties the whole `light` and
  `material` for any `inputs:*` change, and `light` for `light:*`; not
  `light/<name>` (`lightAPIAdapter.cpp`, `InvalidateImagingSubprim`).
- Point instancer topology (audit #29): Hydra keeps inactive instances in
  `instanceIndices` and masks them out (`ComputeMaskAtTime` folds inactive
  and invisible ids); only prototypes whose every instance is inactive are
  pruned, with their `instanceIndices` entry.
- Stage metadata (section 9): a time code or up axis edit notifies nothing
  in Hydra (built-in fields at `/`), and a `renderSettingsPrimPath` edit
  resyncs the whole stage (plugin field).
- `hdVisible` matches only a prim that carries `visibility/visibility`, and
  `hdHasMaterialBinding` reads the all-purpose binding only
  (`collectionPredicateLibrary.cpp`).
- The material context holding every node is `__all`
  (`HdMaterialSchemaTokens->all`), not `all`.
- `HdDependencyForwardingSceneIndex` is renderer-side (an `hdSt` plugin, not
  the render index), so `__dependencies` dirty nothing unless a consumer
  appends it (`hydra-parity-chain`).

## Deviations recorded while implementing

- **D4, native instancing under an `over`.** Native instancing still runs
  before point instancing in this change (`chain::standard`), so it cannot
  see the block the point instancing pass puts on `niPrototypePath`. It
  stops its walk at an `over` itself, which aggregates exactly what the
  block would leave (nothing strictly under an `over`). Instances under a
  point instancer keep today's handling until `hydra-parity-instancing`
  puts point instancing first. A blocked `niPrototypePath` reads as absent,
  so an instance whose path an upstream filter blocks is not aggregated
  either.
- **D4, instancers found by the whole-stage pass.** As UsdImaging's
  `_InstancerObserver::_Populate`, which finds instancers by type on the
  pass's output and skips their descendants, the point instancing filter
  propagates an instancer strictly under another instancer or an `over`
  only through the copies holding it, not at its own path.

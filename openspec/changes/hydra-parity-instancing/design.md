## Context

See `proposal.md` for why, and `../hydra-parity-stage/audit.md` (sections
1, 2, 6, 8 and 10) for the comparison. Hydra references are to OpenUSD
`70c45e83b`, `pxr/usdImaging/usdImaging/`: `sceneIndices.cpp`
(`UsdImagingCreateSceneIndices`), `piPrototypePropagatingSceneIndex.cpp`,
`piPrototypeSceneIndex.cpp`, `niPrototypePropagatingSceneIndex.cpp`,
`niInstanceAggregationSceneIndex.cpp`, `niPrototypeSceneIndex.cpp`,
`niPrototypePruningSceneIndex.cpp`, `instanceProxyPathTranslationSceneIndex.cpp`,
`dataSourcePointInstancer.cpp`, `flattenedDataSourceProviders.cpp`.

After `hydra-parity-stage`, Moho's chain is extent resolving, native
instancing (scan-based, at `/MohoNiPropagatedPrototypes`), point instancing
(with the whole-stage over/instancer pass), a top-level flattening, binding
resolving, render settings flattening and light linking. This change
replaces the two instancing filters and the top-level flattening.

Principle (see `../hydra-parity-stage/design.md`): UsdImaging's behaviour
at the reference revision, quirks included. arnold-usd patches two of this
area's quirks in its own plugins: the visibility of propagated prototypes
(`fixPropagatedPrototypesVisibilitySIP`) and materials type-erased under
point instancer prototypes (`fixMaterialPruningSIP`, fixed upstream: at the
reference revision `IsRenderablePrimType` spares `material`). Moho follows
the reference revision for both and leaves any further fix to the
renderer's own filters.

## Goals / Non-Goals

**Goals:**
- `chain::standard` from extent resolving to binding resolving is
  `UsdImagingCreateSceneIndices`' with `addDrawModeSceneIndex = false`,
  `displayUnloadedPrimsWithBounds = false`, no overrides callback and no
  plugins.
- Paths, names, types, data and dirty locators of instancing as Hydra's,
  except the hash digits named in D4.

**Non-Goals:**
- Draw mode (D7).
- Notice batching (`HdNoticeBatchingSceneIndex` after the propagation): see
  `hydra-parity-chain`.
- Rendering Moana's coral prototypes (instance-proxy targets): Hydra
  renders nothing for them; this change matches it.

## Decisions

### D1. Mirror Hydra's propagation structure

Point instancer propagation becomes, as in Hydra, a merging of: the input
with the whole-stage `PiPrototype` pass (prims under instancers and overs
unrenderable), and, for each instancer and each prototype target `P`, a
re-rooting `P → P`, a `PiPrototype` pass for that instancer, a re-rooting
`P → P/ForInstancer<hash>`, and a nested propagation over the copy (so
nested instancers are re-instanced per copy). Native instancer propagation
becomes a merging of: the prototype scene (pruning of `/__Prototype_N` at
the top, or re-rooting `/__Prototype_N → /UsdNiInstancer/UsdNiPrototype`
inside), an `NiPrototype` pass (instances' renderable types blanked;
inside a prototype, root overlay and `instancedBy` underlay), a flattening,
the optional append callback; the instance aggregation over that; and, for
each aggregated instancer, a nested native propagation re-rooted under it.

Alternative: keep Moho's scan-based filters and patch names and data. The
audit's divergences (scope never a point instancer copy, flattening after
aggregation, no binding copy) all follow from that structure, and the
resulting notices would not match Hydra's.

`RetainedSceneIndex` and `MergingSceneIndex` exist in Moho, and the
re-rooting container of `scene-index-rerooting` (`filter/rerooting.rs`)
already re-roots path values as `UsdImagingRerootingContainerDataSource`
does. A re-rooting scene index (`UsdImagingRerootingSceneIndex`: prims,
children and notices moved from a source root to a destination root, data
wrapped by that container) is added on top of it.

### D2. Aggregation reads flattened data, keyed as Hydra

Each instance's key is computed on its flattened data, re-rooted from the
instance path to `/<prototype name>` (so `model/modelPath` and bindings
inside the asset compare equal across instances): the constant primvars'
names and roles, then each of `usdMaterialBindings`, `purpose`,
`geomModel`, `model`, `__usdUpAxis` the instance has. The `<bindingHash>`
prim carries a static copy of those, re-rooted onto the propagated
prototype path. Instances are kept in path order (Hydra's `SdfPathSet`),
which fixes instance indices, locations, transforms, mask and primvars.

### D3. Instancer data as Hydra

`instancerTopology {prototypes, instanceIndices, instanceLocations, mask}`;
`mask` always present, from each instance's flattened visibility.
`primvars`: `hydra:instanceTransforms` from each instance's flattened
`xform/matrix`, and every constant primvar of the first instance, read per
instance, with interpolation `instance`. `instancedBy`: the enclosing copy
root's when inside a point instancer copy, else the prototype's own when
inside a native prototype. Dirty locators follow
`niInstanceAggregationSceneIndex.cpp`: adding or removing an instance
dirties the instancer's `instancerTopology` and `primvars` and every
instance's `instance`, instead of rebuilding the model.

### D4. Hash functions

- `ForInstancer<hash>`: a private port of `TfHash` (its `Combine` of two
  `std::string`s: SpookyHash `ArchHash64` over the bytes, folded as
  `Tf_HashState` folds) gives Hydra's digits, printed `%zx`. The port is
  checked against golden values computed with a USD build; until those
  exist, the digits are documented as Hydra's algorithm, unverified.
- `<bindingHash>`: the prefix (`NoPrimvars`/`Primvars`) and the
  `_<name>` structure are Hydra's, and `Primvars<hex>` uses the same `TfHash`
  port over the sorted name-to-role map. The per-data-source digits are
  `HdDataSourceHash` in Hydra, a hash of `VtValue`s that Moho's
  `sdf::Value`s (one-element path arrays, among others) cannot reproduce;
  Moho hashes its own values there, and the specs state those digits are not
  part of the contract.

Alternative: declare every name opaque (the audit's recommendation). The
user asked for Hydra's prim paths; the `ForInstancer` names can be exact at
the cost of a small port, so they are.

### D5. Point instancer topology as UsdImaging's

`prototypes` drops a prototype only when every one of its instances is
inactive (`inactiveIds` explicit items), and `instanceIndices` drops that
prototype's entry; inactive instances otherwise stay in `instanceIndices`
and are masked out, with invisible ids, by the mask
(`PointInstancer::mask_at`, which folds both). openusd's inactive-id helper
is private (`docs/openusd-notes.md`); the metadata is read directly, as
Moho reads it today.

### D6. M12 goes; proxy targets copy as empty prims

`through_instance`, `extra_children` and the typeless scope go. A target
that is an instance proxy maps, through the re-rooting index, to a prim the
stage index answers as empty with no children; the instancer still names
its copy. A note in `ROADMAP.md` records the gap against UsdImaging 1.0 as
something to raise upstream with OpenUSD.

### D7. Draw mode is deferred, with a hook

Hydra adds `UsdImagingDrawModeSceneIndex` per prototype through
`addDrawModeSceneIndex` (default true). It is deferred because:
- it is optional in Hydra's own API, so a chain without it is a valid
  UsdImaging configuration, and `chain::standard` states which;
- it replaces geometry with standins (bounds, cards, origin) for viewport
  level of detail; a render delegate under usdview or Solaris receives it
  already applied by UsdImaging, so `hdcrust` never implements it, and an
  offline render of `default` draw modes is unaffected;
- its card generation (textures per axis, `cardGeometry`) is a sizeable
  port of its own.

The native propagation takes Hydra's `SceneIndexAppendCallback`, applied
after flattening in every prototype, and the stage and flattening already
carry `geomModel` and `applyDrawMode`, so adding it later changes no layout.

### D8. Instance proxy path translation

A port of `_NewTranslatePath` (Hydra's default,
`USDIMAGING_CORRECT_PROXY_PATH_TRANSLATION`): walk the path's ancestors
until one is a prim of the index; at an ancestor with `instance` data,
replace its prefix by the prototype `instancerTopology/prototypes[prototypeIndex]`
of its instancer names, and start again; bounded iterations, and no
replacement when the prototype lies under the instance. Applied recursively
to every path value under `usdMaterialBindings`, the only name
`UsdImagingCreateSceneIndices` configures.

### D9. `niPrototypeIdentity` and the instancing key go

`stage/instancing.rs` rebuilt openusd's crate-private instancing key so
that chunks agree on prototype names. Hydra names by `__Prototype_N`, so the
module, its accessor and its upstream candidate in `ROADMAP.md` ("a public
instancing key stable across stages") are deleted; the candidate for a
non-empty `prim_stack()` of prototype prims is kept only if something else
still needs it.

## Risks / Trade-offs

- [Moana's coral renders empty] → as in UsdImaging 2.0. crust-render may
  keep a consumer-side workaround over Moho's output; the gap is raised
  upstream.
- [The structure costs more scene indices per prototype] → Hydra pays the
  same; strategy (D) of moho-spike measures the chain on ALab and Moana
  before and after.
- [`TfHash` port wrong] → golden values from a USD build; until then names
  keep the right shape and stay unique, which is all consumers may rely on.
- [`__Prototype_N` numbering differs from C++ USD] → openusd numbers
  prototypes per composition as USD does, but not necessarily in the same
  order; the number is the USD prototype's name on that stage, which is
  what Hydra uses.

## Migration Plan

Lands after `hydra-parity-stage`, on its own Moho rev. Consumers update
their instancing import: `instancedBy`, `mask`, instance-rate primvars,
new prototype paths.

## Deviations recorded while implementing

- **Merging, brought forward from `hydra-parity-chain`.** Both
  propagations insert inputs into a `MergingSceneIndex` on edits (a new
  prototype target, a new native instancer). Moho's merging notified every
  strict ancestor of a new root added, resyncing `/world` and the like on
  each insertion. It now notifies none of them, as Hydra's (whose
  `_AddStrictPrefixesOfSceneRoots` skips every valid root: its test is
  inverted at the reference revision), skips notice computation while
  unobserved, and finds inputs through a path table once it has five or
  more, as Hydra does. Its other notice
  differences (filtering by root, defined prims only, the root not always
  added) stay for `hydra-parity-chain`.
- **Instance indices are read fresh.** UsdImaging serves an instance's
  `instance` through an `HdLazyContainerDataSource`, which keeps the first
  result: an index read before a sibling instance came or went stays stale,
  although the aggregation dirties `instance` on it. Moho computes it on
  every read; the notices are UsdImaging's (instances dirtied only once some
  index of their instancer was read).
- **The instancing propagations' notices** come through Moho's
  `MergingSceneIndex`, whose remaining differences from Hydra's (see
  above) are `hydra-parity-chain`'s.
- **An instance prim inside a native prototype has no `instancedBy`.**
  `UsdImaging_NiPrototypeSceneIndex::GetPrim` returns an instance before it
  underlays `instancedBy`, so the spec's "every prim of the prototype SHALL
  carry `instancedBy`" holds for every prim but nested instances, as in
  UsdImaging.
- **Nested point instancer copies are made at their prototype's input
  path.** The spec said the inner copies are made "inside the enclosing
  copy". UsdImaging's `_InstancerObserver` makes them at
  `<prototype>/ForInstancer<hash>` with the prototype's input path (the hash
  takes the enclosing copy root, which keeps the copies of each enclosing
  copy apart), so Moho does too, and the spec is corrected.
- **Skinning relocation is not reproduced.** With deferred skinning,
  UsdImaging relocates `skelBinding/animationSource` to a primvar before
  aggregating top-level instances; Moho has no skeleton data.
- **The stage index resyncs re-pointed prototypes** (beyond the tasks).
  Native instancing now follows notices as UsdImaging's does, where the old
  filter rescanned the stage on any change. openusd, unlike USD, retires a
  prototype whose source instance is removed and composes the other
  instances under a new `/__Prototype_N`, reporting neither; the instances
  then named a prototype that no longer existed. When a populated prototype
  is resynced, the stage index now resyncs the prototypes it has not
  populated and the instances composed under them. The prototype pruning
  also judges removals against the prototypes known before the batch, as
  Moho's stage index removes a prototype's descendants by entries of their
  own. Recorded as an openusd upstream candidate.
- **`__Prototype_N` numbers** are openusd's (from 0, in composition order),
  so propagated prototype paths differ from UsdImaging's by that number,
  as the design's risks foresaw.

## Cost measured (task 6.4)

Strategy (D) of moho-spike (one `chain::standard` over a `LoadAll` stage,
every data source read), same machine (Xeon E5-2699 v4, 94 GiB), openusd
`d7d2a29`, before (`ee8167f`) and after this change:

| scene | rev | open (chain built) | read | total | peak RSS | prims |
|---|---|---|---|---|---|---|
| ALab | before | 19 s | 319 s | 340 s | 26.4 GiB | 32 816 |
| ALab | after | 41 s | 365 s | 409 s | 26.5 GiB | 33 391 |
| Moana | before | 62 s | 2 363 s | 2 438 s | 8.13 GiB | 357 239 |
| Moana | after | 247 s | 2 283 s | 2 595 s | 8.50 GiB | 356 601 |

Building the chain costs more (the propagations populate their scene
indices eagerly, as Hydra's do: about 3 minutes more on Moana), reading
about the same (ALab +14 %, Moana -3 %, its coral prototypes now empty).
Memory grows by under 0.4 GiB.

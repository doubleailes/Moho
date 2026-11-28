# Hydra parity check

Moho compared with UsdImaging and Hydra on a broad corpus: every prim, its
type and every data source under every locator, then the notices each edit
of a generated script gives, with nothing read and with everything read.
The harness is `tools/parity/` (its `README.md` has the format and the
commands); `compare.py`'s docstring lists what the comparison normalizes.

## Reference and revisions

- **Hydra:** USD 26.08, conda-forge build `openusd-26.08-py313h6963fe8_0`,
  which has no `usdMtlx`.
- **Moho:** `b40caad`, over mxpv/openusd `9ba0611`.
- **Run:** 2026-11-28. The C++ side builds `UsdImagingCreateSceneIndices`
  (draw mode off) and, as a render index with hdPrman's or arnold-usd's
  filters, an `HdMergingSceneIndex` with `HdsiLightLinkingSceneIndex` and
  `HdDependencyForwardingSceneIndex` over it. The Rust side builds
  `chain::standard` with `RendererFilter::light_linking` and
  `dependency_forwarding`.

## Corpus

168 scenes:

- **Moho's test fixtures:** every usda literal under `crates/moho/tests`
  (116 scenes, `fixtures.py`), but `point_instancing`'s `huge` instancer,
  which runs Hydra out of memory, and `update_scope`'s, which its test
  assembles from pieces.
- **crust-render's samples:** the 46 scenes of `samples/`.
- **Production scenes:** `Kitchen_set.usd`, `Kitchen_set_instanced.usd`,
  MaterialX's `teapot.usda` and `lion.usda`, ALab (`entry.usda`, 17 681
  prims) and Moana's `isCoral` element (27 769 prims, point instancers
  inside native instances).

Each scene gets an edit set from its own data (`edits.py`), applied one
edit at a time on one stage: a transform, visibility, purpose, a
`displayColor` primvar, a material parameter, a binding retargeted, a
light's intensity, a light link and then `includeRoot`, point instancer
positions, a prim defined, an instance made non-instanceable, a prim
removed, and a time change. An attribute the scene does not author is set
twice, so that the second set is a plain value change. 1 791 edits in all.

## Results

| | |
|---|---|
| Prims and types | The same on both sides in all 168 scenes, before and after the edits |
| Static data | No unexplained difference, before or after the edits |
| Notices, nothing read | 1 763 of 1 791 edits identical |
| Notices, everything read | 1 761 of 1 791 edits identical |

Every edit not identical is one of the three cases under "Real
divergences". The scenes where any edit differs:

| Scene | Prims (Hydra / Moho) | Notices identical, nothing read | Notices identical, all read |
|---|---|---|---|
| alab | 17 681 / 17 681 | 17/18 | 16/18 |
| crust_instancing | 26 / 26 | 18/19 | 18/19 |
| crust_nested_instancing | 40 / 40 | 18/19 | 18/19 |
| crust_purpose_bindings | 24 / 24 | 15/16 | 15/16 |
| fx_chain_1 | 29 / 29 | 15/17 | 15/17 |
| fx_draw_mode_2 | 14 / 14 | 10/12 | 10/12 |
| fx_instance_proxy_path_translation_0 | 19 / 19 | 11/13 | 11/13 |
| fx_light_linking_1 | 17 / 17 | 16/17 | 16/17 |
| fx_native_instancing_3 | 12 / 12 | 10/12 | 10/12 |
| fx_native_instancing_4 | 12 / 12 | 11/12 | 11/12 |
| fx_native_instancing_6 | 13 / 13 | 10/12 | 10/12 |
| fx_native_instancing_7 | 13 / 13 | 11/12 | 11/12 |
| fx_particle_field_0 | 8 / 8 | 11/11 | 10/11 |
| fx_point_instancing_5 | 15 / 15 | 12/13 | 12/13 |
| fx_points_1 | 15 / 15 | 11/13 | 11/13 |
| fx_prim_info_1 | 8 / 8 | 3/5 | 3/5 |
| fx_rerooting_0 | 26 / 26 | 12/14 | 12/14 |
| fx_stage_5 | 10 / 10 | 10/12 | 10/12 |
| iscoral | 27 769 / 27 769 | 14/15 | 14/15 |
| kitchen_set_instanced | 2 565 / 2 565 | 3/4 | 3/4 |

The other 148 scenes are identical in every edit, among them
`Kitchen_set` (2 746 prims) and every MaterialX scene.

Against the previous run (Moho `f33edd8`, openusd `26c81d8`, 908 of 1 214
edits identical with nothing read), the notice differences openusd's
authoring caused are gone (mxpv/openusd#224): a value set on an attribute
without a spec is a resync then a value change, as in USD; an `over`
authored through a reference is changed info (ALab's light edits, which
re-added 33 852 entries, are now identical); and an edit under a
prototype's source keeps the prototype standing. The stage index no longer
re-adds the prototypes its own walk composed (`b40caad`).

## Remaining differences

### (a) Accepted, documented deviations

Static differences, each counted by `compare.py` under its class:

- **MaterialX** (`mtlx`, 11 026 entries in 14 scenes). The reference build
  has no `usdMtlx`, so a material whose network is a `.mtlx` document is
  empty in Hydra. A harness artifact.
- **An extent authored with another type than declared** (`extent`, 1 592
  entries, ALab). C++ `Get<T>` fails on ALab's `vector3d[]`
  `extentsHint`s, and UsdImaging answers no hint and an empty extent;
  openusd casts the value, and Moho answers it.
- **Prototype numbers** (`numbering`, 120 entries, isCoral). USD numbers
  the prototypes composed together in the order of a `TfHashMap` keyed by
  instance key (`Usd_InstanceCache::_pendingAddedPrimIndexes`), openusd in
  path order, so a `ForInstancer<hash>` name, which hashes the instancer's
  `/__Prototype_N/...` path, differs where the numbers do.
- **An equivalent rotation** (`rotation`, 9 entries in 6 scenes):
  `geomXformVectors/rotation` read back by accumulation as the other
  equivalent triple (`(180, -20, 180)` for `(-180, -20, -180)`).
- **The text of a path expression** (`expression`, 2 entries): an explicit
  collection's union in hash map order in C++ (`/world/c /world/a`), path
  order in openusd; a predicate argument quoted in C++
  (`{hdType:"mesh"}`), bare in openusd. The same expressions.
- **The flattening's placeholder for a native instancer** (`placeholder`,
  3 entries before the edits, 30 after, in 10 scenes). Hydra gives a
  `.../UsdNiInstancer` an identity `xform`, `visibility` true and
  `resetXformStack`, Moho the aggregation's data only. While a point
  instancer in the prototype is populated,
  `UsdImagingPiPrototypePropagatingSceneIndex` sends notices into the
  child prototype's `HdFlatteningSceneIndex`
  (`piPrototypePropagatingSceneIndex.cpp:334-370`, `:534-560`), which
  consolidates its recent prims (`flatteningSceneIndex.cpp:573-580`) and
  then wraps the data-less instancer (`:355-370`). After an instance is
  made non-instanceable, the re-propagated group's instancer gets the same
  data. The values are the defaults a delegate assumes when they are
  absent (`HdSceneIndexAdapterSceneDelegate::GetTransform`,
  `GetInstancerTransform` and `GetVisible`).
- **A stale instance index** (`stale`, 15 entries after the edits, in 12
  scenes). When an instance stops being one, Hydra's instancer topology
  renumbers the remaining instances, but their own `instance/instanceIndex`
  keeps the old index and is never dirtied (`/world/b`: topology `[0]`,
  instance data `1`). Moho answers the new index in both. Upstream
  candidate.

Normalized before the comparison (`compare.py`), not counted:

- **`skelBinding` is not named.** UsdImaging's keyless API schema adapters
  name it on every prim, null without a binding; Moho has no skeleton
  data.
- **The absolute root's `system/assetResolution`.** openusd has no
  resolver context (C++ `UsdStage::GetPathResolverContext`).
- **Category ids are numbered in another order** (ALab: a light's
  `group_0` against `group_3`): ids are given in the order collections are
  met, which follows children order, path order in Moho's merging and hash
  order in Hydra's. Each is compared by the light links that use it.
- **A propagated group's name** ends in hashes of the values its
  prototypes share, which Moho computes its own way; each group is
  compared by the prototypes it holds.
- **A name listed with a null data source**, which Hydra's instancers list
  and Moho does not: both are no data in Hydra.
- **A path vector against a path array:** Moho holds Hydra's
  `VtArray<SdfPath>` fields as `PathVec`s, written `[ a b ]` where C++
  writes `[a, b]`.

Not exercised by this run's edits: Hydra's stale coordinate system
bindings ("Coordinate systems (#46)" below), and an edit under an abstract
(`class`) prim, which Hydra also dirties at that prim's own path.

### (b) Moho's generic data

`__usdAttributes` and `__usdRelationships` on every prim, and their dirty
locators on every property edit; `__usdMetersPerUnit` on the absolute root
in the scenes that author `metersPerUnit`. Dropped by the comparison.
`__usdUpAxis` is UsdImaging's too, and is compared.

### (c) Real divergences

Notices, in 28 edits with nothing read and 30 with everything read:

- **An instance made non-instanceable** (19 scenes, ALab, isCoral and
  `Kitchen_set_instanced` among them). Hydra removes and adds the instance
  again and dirties the remaining instances' group (`instanceCategories`,
  `instanceIndices`, the prototype's prims). Moho also adds the group, its
  instancer and its prototype again, and the remaining instances (`/world/b`
  where `/world/a` was made non-instanceable). Both keep `/__Prototype_1`.
  Not yet traced to openusd's notice or the stage index.
- **An edit to a prototype's source transform** (9 scenes:
  `/asset.xformOp:translate` where `/asset` is referenced by the
  instances). Moho dirties the prototype's `UsdNiPrototype` at `xform`,
  twice; Hydra sends nothing for it.
- **A time change** (with everything read, 2 scenes). On ALab, Moho also
  dirties the transforms of two camera rigs (`renderCam_buffer`,
  `renderCam_srt`); on `particle_field`, Hydra dirties a moving particle
  field's `primvars/positions/primvarValue`, which Moho does not.

## Earlier runs

The first check ran from Moho `71017cb` against openusd `d7d2a29`, and was
re-run at `f33edd8` (openusd `26c81d8`), from a harness outside the
repository. Its corpus also held hand-built scenes (`hydra/`, `mat/`,
`q19/`, `q20/`, `chain/`), companion layers for three fixtures (`fxn_ni`,
`fxn_pi`, `fxn_assets`), a colour space fixture, load and unload
sequences on `LoadNone` stages, and hand edits (`kind`, `apiSchemas`,
`specifier`, `active`, `typeName`, prototype sources, `instanceable`
toggles between two prims). That harness is gone; this run covers none of
those. The fixes it led to follow, then the targeted checks made since.

### Fixed

Each with a test, one concern per commit:

- An attribute with neither a value nor a fallback reads its type's zero
  value (`GetTypedValue`'s value-initialized `T`): empty arrays, tokens and
  asset paths, zeros (`faceVertexIndices`, `volumeField/*`,
  `renderingColorSpace`, `cardTexture*`). The generic data still answers
  nothing.
- The flattened `purpose` is `HdFlattenedPurposeDataSourceProvider`'s: the
  input container passed through, an inheritable parent's whole, or a
  fallback alone without `inheritable`.
- A point instancer names its six instance primvars, answers one when
  authored (an empty array included), always gives its role (empty when
  none), and its own primvars are `instance` unless constant or uniform.
- `__usdPrimInfo` names its six fields on every prim; its `apiSchemas`
  leaves out names no registered schema has (`GetAppliedSchemas`).
- Render settings, products, vars and passes, shaders and node graphs carry
  neither `__usdPrimInfo` nor `primOrigin` (the chain change's first open
  gap).
- The stage index invalidates in UsdImaging's two passes, the properties
  whose spec came or went, then those whose value changed, one entry per
  prim per pass (the third open gap).
- Flattening dirties only the descendants whose flattened data was read
  since it was last dirtied (`_DirtyHierarchy`, primvars tracked per
  primvar), and a prim added again dirties its read descendants for every
  flattened name (the second open gap).
- `Points` read `widths` as a custom primvar.
- A prototype a load composes is added, and pruned. openusd's load notice
  names the prototypes composed before the load (ALab: `/__Prototype_1` to
  `381`), not those it composes (`382`, `383`); USD names every prototype.
  The stage index lists `stage.prototypes()` among the root's children but
  adds only what is notified, so the prototype pruning, which learns
  prototypes from `ADDED` as `UsdImaging_NiPrototypePruningSceneIndex`
  does, never hid them: after one load ALab's index held 372 prims where
  Hydra holds 362 (10 under `/__Prototype_382` and `383`), and light
  linking dirtied their `categories` on every later load (6 462 extra
  entries on ALab's sequence). PR #39 introduced it on ALab by removing
  the re-pointed prototype resync (`5689ee3`: #194 covers edits, not
  loads); isCoral opened `LoadNone` already had it on `main` (66 prims).
  After a load, unload, layer muting or retried load notice the stage
  index now resyncs each prototype it has not populated (a workaround, an
  upstream candidate): after one load ALab, isCoral and both minimal
  fixtures hold Hydra's prims, and no generated or hand edit changed.
- A prototype an edit composes is added, and pruned. The same openusd
  gap for edits, which no generated edit makes (they make instances
  non-instanceable): a prim made the first instance of its prototype is
  reported alone, where USD's notice also names `/__Prototype_1`, so the
  root showed the prototype and its prims (on `main` too). The stage index
  now also resyncs, after any resync, the unpopulated prototype of each
  instance it repopulates, one lookup per prim (`Stage::prototypes()`
  walks the stage after an edit); the stage-wide check stays for loads,
  where openusd also moves an unloaded instance outside the payload to a
  new prototype (ALab's projector sheets to `/__Prototype_382`; USD keeps
  its prototype). The five toggles now leave Hydra's prims; the edits
  that make an instance non-instanceable still
  differ by known entries (Hydra's `REMOVED /__Prototype_N/...`, and
  openusd's resync of every instance); no generated, hand or load step
  changed.
- A prim recomposed in place is no longer added again. openusd reports
  the ancestor of a loaded or unloaded payload that has no payload of its
  own as recomposed in place (`Resynced::prims`,
  `CommittedChange::recomposed`); USD names no such path, and Hydra sends
  nothing for it, where Moho added the prim again alone, an extra `ADDED`
  on each load, unload or `LoadAndUnload` below it. The stage index now
  reads what it held for the prim again without a notice (openusd says its
  own composition may have changed), and still removes its subtree when
  the stage no longer exposes it. The load steps went from 2 to 4
  identical of 6 on `flat` and from 0 to 2 on isCoral; an unload of the
  payloads' parent now misses Hydra's `ADDED` for it (openusd reports it
  recomposed where USD resyncs it, mxpv/openusd#203); no generated or
  hand edit changed.

The fourth open gap, the stage-level `includeRoot` edit's two entries, is
openusd's authoring (see "A new spec is one resync"): with USD's two
notices Moho sends both entries.

### Particle fields (`particle-field-adapter`)

Not in the earlier runs' corpus: the reference build (USD 26.08 from conda-forge)
has `usdVolImaging`'s particle field adapter, but the check ran before Moho
had one. The adapter is a line-by-line port of `particleFieldAdapter.cpp`
and `dataSourceParticleField.cpp` (OpenUSD `release`), checked by
`tests/particle_field.rs` against the behaviour read in those sources
(names, interpolations, roles, the float-or-half choice and the dirty
locators). The 2026-11-28 run has `particle_field`'s fixtures: their
static data is identical, and the time change differs (under "Real
divergences").

### Points (#45)

Not in the earlier runs' corpus: `Points` prims were typeless when the check ran.
Checked against the sources at OpenUSD `v26.08` instead
(`pointsAdapter.cpp`, `dataSourcePoints.cpp`, `plugInfo.json`):
`UsdImagingPointsAdapter` is registered for `Points` alone (no
`includeDerivedPrimTypes`) and gives one subprim of type `points`;
`UsdImagingDataSourcePointsPrim` is the gprim data source with
`primvars` overlaid by a single custom primvar mapping (`widths` to
`widths`), and invalidates as the gprim plus that mapping; `ids` is not
read. Moho's `Points` is the type mapping alone, the gprim data and
`widths` already being read for the `Points` family, checked by
`tests/points.rs` (values, interpolations, the `widths` dirty locator,
and points under a native instance and a point instancer). The 2026-11-28
run has `points`' fixtures: their static data is identical, and every
edit that differs is one of the instancing cases under "Real divergences".

### Coordinate systems (#46)

Checked against USD 26.08 (the conda-forge build above) with a small
oracle: a C++ dumper builds `UsdImagingCreateSceneIndices` (draw mode off)
with `HdsiCoordSysPrimSceneIndex` and `HdDependencyForwardingSceneIndex`
over it; a Rust dumper builds `chain::standard` with
`CoordSysPrimSceneIndex` and `DependencyForwardingSceneIndex`. Both print
every prim's `coordSysBinding`, and each `coordSys` prim's name, `xform`
and dependencies, then the notices of seven edits, each followed by a new
dump: a binding retargeted, the target's transform edited, `CoordSysAPI`
applied and bound on a prim, a binding relationship removed, a prim's
`apiSchemas` cleared, a binding inside a native instance's prototype
retargeted, and a binding prim removed. The scene binds at the root of a
hierarchy and below it (a nearer binding of the same name), has an
unbound instance, a bare `coordSys:<name>` relationship, a binding to a
property path, and an asset with bindings referenced by two native
instances.

- **Static data: identical** but for the binding hash digits (Moho's own):
  the same 21 prims, the same 13 naming `coordSysBinding`, the same
  bindings (the inherited ones, the property path left alone, the
  prototype's rewritten into `UsdNiPrototype`), and the same `coordSys`
  prims with the same transforms. `__dependencies` reads as a block on
  both sides (dependency forwarding).
- **Notices:** the same prims dirtied at `coordSysBinding` on a retarget
  (in another order), the same `coordSys` prim added and removed with its
  binding, and the same entries when a binding prim is added again. Every
  other difference follows from Hydra's stale bindings (class a above):
  Moho removes `/World/Space/__coordSys_worldSpace` when every binding to
  it is retargeted, and Hydra, still counting the old targets, keeps it,
  then dirties its `xform` when the target moves; the prototype's retarget
  is Hydra's added-then-removed quirk of the same prim, where Moho adds the
  new target's prim and removes the old one. Besides: Hydra also dirties
  `/Asset/geo` under the abstract class (known, class a) and
  `skelBinding` (class a).

### Scene globals filter (`hydra-scene-globals`)

Checked afterwards against USD 26.08 with a small oracle:
`HdsiSceneGlobalsSceneIndex` over `UsdImagingCreateSceneIndices` (draw mode
off) on a stage authoring a time range, a rate, an up axis and a render
settings path, against `SceneGlobalsSceneIndex` over `chain::standard`. Both
print the root's `sceneGlobals` unset, after the stage index's time alone
changes, and after every setter; record the notices of each setter, of
equal values set again, of NaN frames and rates set twice and of an empty
settings path; read a root container held from before the sets; and run
over an input whose root has no data. The transcripts are equal (an empty
path printed as Hydra's empty `SdfPath`) but for the root's names that
come from the chain, not the filter (`coordSysBinding`, `skelBinding`,
`system`), classified above.

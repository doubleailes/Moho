# Hydra parity check

The final check of Moho against Hydra once the four Hydra-parity changes
(`hydra-parity-stage`, `-instancing`, `-materials`, `-chain`) were merged:
Moho compared with UsdImaging and Hydra on a broad corpus, static data and
notices, the divergences fixed, and what remains classified.

## Reference and revisions

- **Hydra:** USD 26.08, conda-forge build `openusd-26.08-py313h6963fe8_0`
  (built without MaterialX, so without `usdMtlx`); sources read at OpenUSD
  `70c45e83b`.
- **Moho:** checked from `71017cb` (the four changes merged); the fixes
  below run to `HEAD` of `fix/hydra-parity-check`; openusd `d7d2a29`.
- **Harness** (outside the repository). A C++ dumper builds
  `UsdImagingCreateSceneIndices` (draw mode off), inserts it into an
  `HdMergingSceneIndex` with `HdsiLightLinkingSceneIndex` and
  `HdDependencyForwardingSceneIndex` over it (as a render index with
  hdPrman's or arnold-usd's filters); a Rust dumper builds `chain::standard`
  with `RendererFilter::light_linking` and `dependency_forwarding`. Both
  print every prim, its type and every data source under every locator,
  then apply the same edits and print the added, removed and dirtied
  entries.
- **What the comparison normalizes.** Container name order; numbers to
  1e-5; a single `SdfPath` against Moho's one-element path array; USD
  prototypes named by their smallest instance location (USD and openusd
  number them differently) and prototype groups by the prototypes they
  hold (the binding hash digits are Moho's own); Moho's `__usdAttributes`
  and `__usdRelationships` (class b) dropped from data and dirty locators;
  the root's `system/assetResolution` (no resolver context in openusd,
  documented). Values over 2000 characters are compared on their first 25
  elements.

## Corpus

- **Moho's test fixtures:** every `#usda` literal under
  `crates/moho/tests` (64 scenes), and the three whose tests write
  companion layers rebuilt with them (`fxn_ni`: native instancing's
  `asset`, `window`, `building`; `fxn_pi`: instance categories;
  `fxn_assets`: texture anchoring).
- **The harness scenes** of the four changes: `hydra/` (`t1`–`t7`,
  `nested`, `pebbles`, `q18a`, `q18b`), `mat/`, `q19/`, `q20/`, `chain/`.
- **crust-render's samples** whose files are all on disk: the 44 scenes of
  `samples/`, `Kitchen_set.usd`, `Kitchen_set_instanced.usd`, MaterialX's
  `teapot.usda` and `lion.usda`.
- **ALab** (`entry.usda`) and **Moana's `isCoral`** element
  (`elements/isCoral/element.usda`: point instancers inside native
  instances). The whole of Moana was not run; `isHibiscus` is not on disk.

## Notices

Each scene gets an edit set generated from its own data, applied one edit
at a time and compared entry by entry (added, removed, dirtied with its
locators, multiplicity included): a transform (`xformOp:translate`),
visibility, purpose, a `displayColor` primvar, a material parameter (found
through the material's network), a binding retargeted, a light's
`inputs:intensity`, a light link (`collection:lightLink:includes`, then
`includeRoot`), point instancer `positions`, a prim defined and one
removed, an instance made non-instanceable, and a time change. An
attribute the scene does not author is set twice, so that the second edit
is a plain value change. Prototype edits (a prim of an instance source
edited, defined and removed), binding edits on instances and transforms
over instances were also run by hand on the native instancing scenes, and
an edit set by hand on ALab. Every generated set runs twice: with nothing
read before the edits, and with every data source read first (flattening
dirties what was read, as Hydra's does).

## Results per scene

"Static: unexplained" counts the differences in prim set, type or data
that none of the classes below accounts for. "Notices identical" is the
number of edits whose entries are identical, out of the edits run; every
other edit differs for a class-(a) reason (see "Notices" below). Two
edits counted as unexplained by the script (a new `displayColor` on
`Kitchen_set` and on isCoral) are the first case there: the resync pass
gives `primvars`, the update pass `primvars/displayColor`, and openusd
reports the resync only.

| Scene | Prims (Hydra / Moho) | Static: unexplained | Notices identical, nothing read | Notices identical, all read |
|---|---|---|---|---|
| chain_lf | 6 / 6 | 0 | 8/12 | 8/12 |
| chain_ll1 | 32 / 32 | 0 | 10/13 | 9/13 |
| chain_upd | 10 / 10 | 0 | 6/8 | 6/8 |
| fx point_instancing_4 (without `huge`) | 7 / 7 | 0 | 4/4 | 3/4 |
| fx_assets_0 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_camera_0 | 3 / 3 | 0 | 2/2 | 2/2 |
| fx_camera_1 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_chain_0 | 18 / 18 | 0 | 10/13 | 10/13 |
| fx_chain_1 | 29 / 29 | 0 | 8/13 | 8/13 |
| fx_collection_0 | 7 / 7 | 0 | 2/2 | 2/2 |
| fx_collections_0 | 6 / 6 | 0 | 4/6 | 4/6 |
| fx_contract_0 | 3 / 3 | 0 | 5/7 | 5/7 |
| fx_curves_0 | 3 / 3 | 0 | 5/7 | 5/7 |
| fx_extent_resolving_0 | 3 / 3 | 0 | 5/7 | 5/7 |
| fx_flattening_0 | 8 / 8 | 0 | 6/8 | 6/8 |
| fx_flattening_1 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_generic_0 | 7 / 7 | 0 | 6/8 | 6/8 |
| fx_geom_subset_0 | 4 / 4 | 0 | 4/6 | 4/6 |
| fx_imageable_0 | 7 / 7 | 0 | 5/6 | 5/6 |
| fx_imageable_1 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_implicit_0 | 9 / 9 | 0 | 5/7 | 5/7 |
| fx_instance_proxy_path_translation_0 | 19 / 19 | 0 | 5/9 | 5/9 |
| fx_light_0 | 11 / 11 | 0 | 4/5 | 4/5 |
| fx_light_filter_0 | 6 / 6 | 0 | 8/12 | 8/12 |
| fx_light_linking_0 | 8 / 8 | 0 | 8/11 | 8/11 |
| fx_light_linking_1 | 17 / 17 | 0 | 6/7 | 5/7 |
| fx_light_linking_2 | 6 / 6 | 0 | 8/12 | 8/12 |
| fx_material_0 | 3 / 3 | 0 | 3/3 | 3/3 |
| fx_material_1 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_material_2 | 2 / 2 | 0 | 3/3 | 3/3 |
| fx_material_3 | 2 / 2 | 0 | 3/3 | 3/3 |
| fx_material_4 | 4 / 4 | 0 | 3/3 | 3/3 |
| fx_material_5 | 2 / 2 | 0 | 3/3 | 3/3 |
| fx_material_binding_0 | 7 / 7 | 0 | 6/8 | 6/8 |
| fx_material_resolve_0 | 16 / 16 | 0 | 6/8 | 6/8 |
| fx_material_resolve_1 | 8 / 8 | 0 | 6/8 | 6/8 |
| fx_material_resolve_2 | 6 / 6 | 0 | 3/3 | 3/3 |
| fx_material_resolve_3 | 7 / 7 | 0 | 6/8 | 6/8 |
| fx_mesh_0 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_model_0 | 5 / 5 | 0 | 5/7 | 5/7 |
| fx_model_1 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_native_instancing_0 | 5 / 5 | 0 | 6/8 | 6/8 |
| fx_native_instancing_1 | 9 / 9 | 0 | 3/3 | 3/3 |
| fx_native_instancing_2 | 5 / 5 | 0 | 5/7 | 5/7 |
| fx_native_instancing_3 | 12 / 12 | 0 | 3/8 | 3/8 |
| fx_overs_0 | 10 / 10 | 0 | 6/9 | 5/9 |
| fx_point_instancing_0 | 11 / 11 | 0 | 5/5 | 4/5 |
| fx_point_instancing_1 | 7 / 7 | 0 | 4/4 | 3/4 |
| fx_point_instancing_2 | 13 / 13 | 0 | 4/4 | 3/4 |
| fx_point_instancing_3 | 13 / 13 | 0 | 4/4 | 3/4 |
| fx_point_instancing_4 | Hydra runs out of memory (`huge`) | - | - | - |
| fx_point_instancing_5 | 15 / 15 | 0 | 5/9 | 5/9 |
| fx_prim_info_0 | 6 / 6 | 0 | 5/7 | 5/7 |
| fx_prim_info_1 | 8 / 8 | 0 | 1/3 | 1/3 |
| fx_primvars_0 | 5 / 5 | 0 | 6/8 | 6/8 |
| fx_primvars_1 | 3 / 3 | 0 | 5/7 | 5/7 |
| fx_render_0 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_rerooting_0 | 26 / 26 | 0 | 7/10 | 6/10 |
| fx_scene_globals_0 | 2 / 2 | 0 | 2/2 | 2/2 |
| fx_stage_0 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_stage_time_0 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_stage_time_1 | 6 / 6 | 0 | 4/4 | 3/4 |
| fx_stage_updates_0 | 4 / 4 | 0 | 5/7 | 5/7 |
| fx_stage_updates_1 | 6 / 6 | 0 | 5/7 | 5/7 |
| fx_volume_0 | 6 / 6 | 0 | 4/6 | 4/6 |
| fx_volume_1 | 4 / 4 | 0 | 4/6 | 4/6 |
| fx_xform_0 | 6 / 6 | 0 | 3/3 | 3/3 |
| fx_xform_1 | 3 / 3 | 0 | 3/3 | 3/3 |
| fxn_assets | 3 / 3 | 0 | 2/2 | 2/2 |
| fxn_ni | 32 / 32 | 0 | 3/4 | 3/4 |
| fxn_pi | 14 / 14 | 0 | 6/7 | 5/7 |
| hydra_nested | 13 / 13 | 0 | 4/4 | 3/4 |
| hydra_pebbles | 11 / 11 | 0 | 5/5 | 4/5 |
| hydra_q18a | 7 / 7 | 0 | 4/4 | 3/4 |
| hydra_q18b | 7 / 7 | 0 | 6/8 | 6/8 |
| hydra_t1 | 35 / 35 | 0 | 6/10 | 6/10 |
| hydra_t2 | 15 / 15 | 0 | 4/4 | 3/4 |
| hydra_t3 | 13 / 13 | 0 | 4/4 | 3/4 |
| hydra_t4 | 15 / 15 | 0 | 5/9 | 5/9 |
| hydra_t5 | 91 / 91 | 0 | 8/12 | 8/12 |
| hydra_t6 | 12 / 12 | 0 | 4/9 | 4/9 |
| hydra_t7 | 12 / 12 | 0 | 3/8 | 3/8 |
| mat_b1 | 8 / 8 | 0 | 6/8 | 6/8 |
| mat_m1 | 4 / 4 | 0 | 5/6 | 5/6 |
| q19_cyc | 2 / 2 | 0 | 3/3 | 3/3 |
| q19_r1 | 3 / 3 | 0 | 2/2 | 2/2 |
| q19_r2 | 3 / 3 | 0 | 5/7 | 5/7 |
| q19_unc | 2 / 2 | 0 | 2/2 | 2/2 |
| q20_lf | 3 / 3 | 0 | 4/6 | 4/6 |
| q20_rs | 5 / 5 | 0 | 2/2 | 2/2 |
| q20_rs2 | 6 / 6 | 0 | 2/2 | 2/2 |
| ALab | 17681 / 17681 | 0 | 7/10 | not run |
| Kitchen_set.usd | 2746 / 2746 | 0 | 6/9 | 6/9 |
| Kitchen_set_instanced.usd | 2565 / 2565 | 0 | 3/4 | 3/4 |
| PointInstancedMedCity.usd | 38 / 38 | 0 | 5/5 | 4/5 |
| animation.usda | 10 / 10 | 0 | 9/13 | 9/13 |
| aovs.usda | 25 / 25 | 0 | 9/13 | 9/13 |
| aovs_lpe.usda | 39 / 39 | 0 | 11/14 | 11/14 |
| cornellbox.usda | 11 / 11 | 0 | 9/13 | 9/13 |
| cornellbox_guided.usda | 21 / 21 | 0 | 11/14 | 11/14 |
| curves.usda | 10 / 10 | 0 | 9/13 | 9/13 |
| displacement.usda | 14 / 14 | 0 | 11/14 | 11/14 |
| dome_backdrop.usda | 14 / 14 | 0 | 11/14 | 11/14 |
| domelight.usda | 14 / 14 | 0 | 11/14 | 11/14 |
| fog.usda | 9 / 9 | 0 | 9/12 | 9/12 |
| hair.usda | 25 / 25 | 0 | 11/14 | 11/14 |
| hair_variation.usda | 13 / 13 | 0 | 11/14 | 11/14 |
| implicit_shapes.usda | 23 / 23 | 0 | 11/14 | 11/14 |
| instancing.usda | 26 / 26 | 0 | 13/17 | 12/17 |
| Moana isCoral | 27769 / 27769 | 0 | 6/12 | not run |
| light_linking.usda | 13 / 13 | 0 | 9/12 | 9/12 |
| light_visibility.usda | 9 / 9 | 0 | 9/12 | 9/12 |
| lion.usda | 11 / 11 | 0 | 6/9 | 6/9 |
| many_lights.usda | 268 / 268 | 0 | 9/12 | 9/12 |
| mass_spike.usda | 8 / 8 | 0 | 10/13 | 10/13 |
| materialx_basic.usda | 15 / 15 | 0 | 10/13 | 10/13 |
| materialx_cutout.usda | 16 / 16 | 0 | 11/14 | 11/14 |
| materialx_emissive.usda | 13 / 13 | 0 | 8/10 | 8/10 |
| materialx_lion.usda | 21 / 21 | 0 | 9/13 | 9/13 |
| materialx_showcase.usda | 37 / 37 | 0 | 10/14 | 10/14 |
| materialx_subsurface.usda | 16 / 16 | 0 | 11/14 | 11/14 |
| materialx_surfaces.usda | 21 / 21 | 0 | 11/14 | 11/14 |
| materialx_teapot.usda | 24 / 24 | 0 | 9/13 | 9/13 |
| materialx_volume.usda | 17 / 17 | 0 | 11/14 | 11/14 |
| motionblur.usda | 11 / 11 | 0 | 9/13 | 9/13 |
| motionvector.usda | 17 / 17 | 0 | 9/13 | 9/13 |
| nested_instancing.usda | 40 / 40 | 0 | 13/17 | 12/17 |
| openpbr_showcase.usda | 25 / 25 | 0 | 11/15 | 11/15 |
| precomp.usda | 34 / 34 | 0 | 9/12 | 9/12 |
| primvar_aovs.usda | 17 / 17 | 0 | 10/14 | 10/14 |
| ptex_quads.usda | 11 / 11 | 0 | 11/14 | 11/14 |
| purpose_bindings.usda | 24 / 24 | 0 | 10/14 | 10/14 |
| pxr_displace.usda | 9 / 9 | 0 | 10/13 | 10/13 |
| rectlight.usda | 8 / 8 | 0 | 9/12 | 9/12 |
| render_passes.usda | 25 / 25 | 0 | 10/13 | 10/13 |
| smoke.usda | 10 / 10 | 0 | 9/12 | 9/12 |
| subdivision.usda | 21 / 21 | 0 | 11/15 | 11/15 |
| subdivision_adaptive.usda | 15 / 15 | 0 | 10/14 | 10/14 |
| teapot.usda | 14 / 14 | 0 | 6/9 | 6/9 |
| thin_window.usda | 13 / 13 | 0 | 11/14 | 11/14 |
| usdlux.usda | 17 / 17 | 0 | 9/12 | 9/12 |
| usdpreview_textured.usda | 16 / 16 | 0 | 11/14 | 11/14 |
| veach_mis.usda | 21 / 21 | 0 | 11/14 | 11/14 |


In total: 142 scenes (the `huge` fixture compared without that instancer),
with the same prims and types on both sides in each, and no unexplained
static difference. Of 1 213 edits run with nothing read
(ALab and isCoral included), 905 give identical entries; with everything
read, 873 of 1 191. Every other edit differs for one of the openusd
reasons under "Notices".

## Remaining differences

### (a) Accepted, documented deviations

Static data:

- **`skelBinding` and `coordSysBinding` are not named.** UsdImaging's
  keyless API schema adapters name both on every prim, null without a
  binding; Moho has no skeleton data, and coordinate systems are on the
  ROADMAP. (Most of the raw difference count of every scene.)
- **`geomModel/cardVisibility` has no fallback.** openusd-schemas' UsdGeom
  schema predates `model:cardVisibility` (fallback `inherited`). Upstream
  candidate.
- **A value authored with another type than UsdImaging reads.** C++
  `Get<T>` fails when the authored type is not `T`, and UsdImaging answers
  its default: `geomXformVectors` of a stack with a `float3` translate or a
  `double3` rotate or scale is the identity, and ALab's `extentsHint`s
  authored `vector3d[]` give no hint and an empty extent. openusd casts.
  Upstream candidate. Also, rotations read back by accumulation can be the
  other equivalent triple (`(180, -20, 180)` for `(-180, -20, -180)`),
  recorded in `hydra-parity-stage`.
- **Category ids are numbered in another order** (ALab: a light's
  `group_0` against `group_3`): ids are given in the order collections are
  met, which follows children order, path order in Moho's merging and hash
  order in Hydra's. The ids stay consistent between lights and prims.
- **Text of path expressions.** An explicit collection's union in hash map
  order in C++ (`/world/c /world/a`), path order in openusd; a predicate
  argument quoted in C++ (`{hdType:"mesh"}`), bare in openusd. The same
  expressions.
- **Prototype numbers.** openusd numbers prototypes from `__Prototype_0`,
  USD from `__Prototype_1`, so the propagated paths, and the
  `ForInstancer<hash>` of a point instancer inside a native prototype (the
  hash takes the instancer's `/__Prototype_N/...` path: checked, isCoral's
  `ForInstancerc1b037937f9934e3` is `/__Prototype_5/geometry/xgAntlers`,
  Moho's `47c96cc13166b34e` is `/__Prototype_4/...`), differ.
- **`apiSchemas` naming `MaterialXConfigAPI`** (the teapot samples): the
  reference build has no `usdMtlx`, so its registry does not know the
  schema; openusd-schemas does. A harness artifact.
- **Hydra runs out of memory** on `point_instancing`'s `huge` instancer
  (`protoIndices` up to 2 000 000 000): it allocates one index array per
  prototype index. Moho answers the same vector lazily; the rest of that
  fixture matches.

Notices (all openusd's authoring, not the index; upstream candidates):

- **A new spec is one resync.** Setting a value on an attribute without a
  spec is one resync in openusd, a resync then an info change in USD, so
  UsdImaging sends a second entry with the same (or finer) locators: 263
  edits, among them the stage-level `includeRoot` edit (Hydra's two
  `collections/lightLink` entries). Given USD's notices (an attribute
  created, then set), Moho now sends UsdImaging's two entries
  (`stage_updates.rs`).
- **Inert `over`s are resynced.** Authoring through a reference creates
  `over` specs that openusd reports resynced, USD info-only, so Moho
  re-adds that subtree (ALab: 42 150 entries for the first intensity edit
  on a light, where Hydra sends 2; identical once the overs exist): 11
  edits.
- **Prototypes retired.** An edit of a prototype's source (a prim added or
  removed under it) or the source instance made non-instanceable makes
  openusd compose a new prototype, which the stage index resyncs: 32 edits
  (47 with everything read).
- **No-op writes** (a relationship set to the targets it has) notify
  nothing in openusd; USD notifies. Seen on hand-run edits only.

Hydra artifacts, exempted (deterministic in Hydra, not reproduced;
decided after a targeted check, `isCoral` and two minimal fixtures):

- **A native instancer whose prototype holds a point instancer carries the
  flattening's data.** Hydra gives that `.../UsdNiInstancer` an identity
  `xform`, `visibility` true and the other flattened names null; Moho gives
  the aggregation's data only. The cause is internal to Hydra. While the
  nested point instancer is populated, `UsdImagingPiPrototypePropagatingSceneIndex`
  sends notices into the child prototype's `HdFlatteningSceneIndex`
  (`piPrototypePropagatingSceneIndex.cpp:334-370`, `:534-560`). Each one
  consolidates its recent prims (`flatteningSceneIndex.cpp:573-580`), so its
  `SdfPathTable` gains a placeholder for the data-less `/UsdNiInstancer`,
  and `GetPrim` then wraps it (`:355-370`). This happens on every run,
  whatever the consumer reads, and not without a point instancer in the
  prototype. The values are the defaults a delegate assumes when they are
  absent (`HdSceneIndexAdapterSceneDelegate::GetTransform`,
  `GetInstancerTransform` and `GetVisible`), and no notice ever dirties
  them: across 13 edits in both read states, the entries at and under the
  instancer are identical on both sides. Consumers treat missing flattened
  data as its default.
- **An edit to a prim under an abstract (`class`) prim is also dirtied at
  that prim's own path.** `UsdImagingStageSceneIndex::_ComputeDirtiedEntries`
  (`stageSceneIndex.cpp:797` and on) does not check `_populatedPaths`, so
  editing `/Asset/geo/m.visibility` under a class gives
  `DIRTIED /Asset/geo/m { visibility }` besides the prototype's entry. Moho
  sends the prototype's entry only. The path holds no prim in either
  index, so a consumer has nothing to update.

### (b) Moho's generic data

`__usdAttributes` and `__usdRelationships` on every prim, and their dirty
locators on every property edit. Nothing else.

### (c) Real divergences

None left unfixed.

## Fixed

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

The fourth open gap, the stage-level `includeRoot` edit's two entries, is
openusd's authoring (see "A new spec is one resync"): with USD's two
notices Moho sends both entries.

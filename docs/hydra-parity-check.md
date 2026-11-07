# Hydra parity check

The final check of Moho against Hydra once the four Hydra-parity changes
(`hydra-parity-stage`, `-instancing`, `-materials`, `-chain`) were merged:
Moho compared with UsdImaging and Hydra on a broad corpus, static data and
notices, the divergences fixed, and what remains classified. Re-run for
the openusd bump to mxpv/openusd `26c81d8` (PR #39), against the same
reference, with load and unload notices added.

## Reference and revisions

- **Hydra:** USD 26.08, conda-forge build `openusd-26.08-py313h6963fe8_0`
  (built without MaterialX, so without `usdMtlx`); sources read at OpenUSD
  `70c45e83b`.
- **Moho:** first checked from `71017cb` (the four changes merged), the
  fixes below to `HEAD` of `fix/hydra-parity-check`, openusd `d7d2a29`.
  Re-checked at `f33edd8` (PR #39: mxpv/openusd `26c81d8`, the #195
  workarounds dropped) against `main` `d6baa0d` (doubleailes/openusd
  `54db9fd`): both Moho revisions dumped over the same corpus, Moho
  against Moho first, then each against Hydra.
- **Harness** (outside the repository). A C++ dumper builds
  `UsdImagingCreateSceneIndices` (draw mode off), inserts it into an
  `HdMergingSceneIndex` with `HdsiLightLinkingSceneIndex` and
  `HdDependencyForwardingSceneIndex` over it (as a render index with
  hdPrman's or arnold-usd's filters); a Rust dumper builds `chain::standard`
  with `RendererFilter::light_linking` and `dependency_forwarding`. Both
  print every prim, its type and every data source under every locator,
  then apply the same edits and print the added, removed and dirtied
  entries. For the re-check both also open a stage `LoadNone`, load,
  unload and `LoadAndUnload`, edit a prim's `specifier`, include unloaded
  prims (`displayUnloadedPrimsWithBounds`, `set_include_unloaded_prims`),
  dump the index again after edits, and print the stage's own notices
  (`UsdNotice::ObjectsChanged`, openusd's `StageSink`).
- **What the comparison normalizes.** Container name order; numbers to
  1e-5; USD prototypes named by their smallest instance location and
  prototype groups by the prototypes they hold (the binding hash digits are
  Moho's own); Moho's `__usdAttributes` and `__usdRelationships` (class b)
  dropped from data and dirty locators; the root's `system/assetResolution`
  (no resolver context in openusd, documented). Values over 2000 characters
  are compared on their first 25 elements. A single `SdfPath` is compared
  with Moho's `Value::Path` and an `SdfPath` array with its `PathVec`; a
  single path against a path array is a difference (before `26c81d8` a
  single `SdfPath` was compared with a one-element `PathVec`).
- **Prototype names.** Both now number from `/__Prototype_1`. Compared raw
  (without the naming by instance location), 15 of the 19 scenes with
  native prototypes match, isCoral included (its 24 `ForInstancer<hash>`
  names, `ForInstancerc1b037937f9934e3` among them, are Hydra's). The
  other 4 (ALab, `Kitchen_set_instanced`, `hydra_t5`, `fxn_ni`) do not:
  USD numbers the prototypes composed together in the iteration order of
  a `TfHashMap` keyed by instance key
  (`Usd_InstanceCache::_pendingAddedPrimIndexes`), openusd in path order
  (`fxn_ni`: USD's window, house, asset are 1, 2, 3, openusd's 3, 2, 1).
  The naming by instance location stays for them.

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
  `teapot.usda` and `lion.usda`. Now Crust's `samples/` and
  `~/Workspace/samples`; `precomp.usda` changed since (27 prims, 13 edits,
  where it had 34 and 12).
- **ALab** (`entry.usda`) and **Moana's `isCoral`** element
  (`elements/isCoral/element.usda`: point instancers inside native
  instances). The whole of Moana was not run; `isHibiscus` is not on disk.
- **Added for the re-check:** two load fixtures, `world` holding two
  payload siblings `a` and `b` (`flat`), and the same with a payload on
  `world` (`nested`); a colour space fixture (a `ColorSpaceDefinitionAPI`
  name, an undefined name, an inherited one, an attribute's declared
  `colorSpace` beside the prim's `ColorSpaceAPI`, a material parameter).

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

Added for the re-check, each on a fresh stage and in both read states:
`kind`, `apiSchemas`, `specifier` (`over`, `class`), `active` and
`typeName` edits on `model_0` and `Kitchen_set`; the source instance of a
prototype removed, made non-instanceable or inactive, another instance
removed, and a prim added and removed under the referenced asset
(`native_instancing_3`, `fxn_ni`, `Kitchen_set_instanced`, whose source
instances are USD's: `/world/b`, `/world/a`, `Hook_3`). And on a stage
opened `LoadNone` (ALab, isCoral, `flat`, `nested`): load one payload,
unload it, `LoadAndUnload` two siblings, load one already loaded while
unloading the other, unload the payloads' parent, load it. Afterwards,
from Rekall's IPR (which rebuilt its index after any `instanceable` edit
to avoid a prototype left at the root): a class referenced by two prims,
each of the five `instanceable` toggles between them, the index compared
after the edit with Hydra's.

## Results per scene

"Static: unexplained" counts the differences in prim set, type or data
that none of the classes below accounts for. "Notices identical" is the
number of edits whose entries are identical, out of the edits run; every
other edit differs for a class-(a) reason (see "Notices" below). Two
edits counted as unexplained by the script (a new `displayColor` on
`Kitchen_set` and on isCoral) are the first case there: the resync pass
gives `primvars`, the update pass `primvars/displayColor`, and openusd
reports the resync only. The load and unload sequences are under
"Loading and unloading".

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
| fx_point_instancing_5 | 15 / 15 | 0 | 6/9 | 5/9 |
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
| hydra_t4 | 15 / 15 | 0 | 6/9 | 5/9 |
| hydra_t5 | 91 / 91 | 0 | 8/12 | 8/12 |
| hydra_t6 | 12 / 12 | 0 | 4/9 | 4/9 |
| hydra_t7 | 12 / 12 | 0 | 3/8 | 3/8 |
| mat_b1 | 8 / 8 | 0 | 6/8 | 6/8 |
| mat_m1 | 4 / 4 | 0 | 5/6 | 5/6 |
| materialx_basic.usda | 15 / 15 | 0 | 10/13 | 10/13 |
| materialx_cutout.usda | 16 / 16 | 0 | 11/14 | 11/14 |
| materialx_emissive.usda | 13 / 13 | 0 | 8/10 | 8/10 |
| materialx_lion.usda | 21 / 21 | 0 | 9/13 | 9/13 |
| materialx_showcase.usda | 37 / 37 | 0 | 10/14 | 10/14 |
| materialx_subsurface.usda | 16 / 16 | 0 | 11/14 | 11/14 |
| materialx_surfaces.usda | 21 / 21 | 0 | 11/14 | 11/14 |
| materialx_teapot.usda | 24 / 24 | 0 | 9/13 | 9/13 |
| materialx_volume.usda | 17 / 17 | 0 | 11/14 | 11/14 |
| q19_cyc | 2 / 2 | 0 | 3/3 | 3/3 |
| q19_r1 | 3 / 3 | 0 | 2/2 | 2/2 |
| q19_r2 | 3 / 3 | 0 | 5/7 | 5/7 |
| q19_unc | 2 / 2 | 0 | 2/2 | 2/2 |
| q20_lf | 3 / 3 | 0 | 4/6 | 4/6 |
| q20_rs | 5 / 5 | 0 | 2/2 | 2/2 |
| q20_rs2 | 6 / 6 | 0 | 2/2 | 2/2 |
| ALab | 17681 / 17681 | 0 | 7/10 | not run |
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
| Kitchen_set.usd | 2746 / 2746 | 0 | 6/9 | 6/9 |
| Kitchen_set_instanced.usd | 2565 / 2565 | 0 | 3/4 | 3/4 |
| light_linking.usda | 13 / 13 | 0 | 9/12 | 9/12 |
| light_visibility.usda | 9 / 9 | 0 | 9/12 | 9/12 |
| lion.usda | 11 / 11 | 0 | 6/9 | 6/9 |
| many_lights.usda | 268 / 268 | 0 | 9/12 | 9/12 |
| mass_spike.usda | 8 / 8 | 0 | 10/13 | 10/13 |
| Moana isCoral | 27769 / 27769 | 0 | 6/12 | not run |
| motionblur.usda | 11 / 11 | 0 | 9/13 | 9/13 |
| motionvector.usda | 17 / 17 | 0 | 9/13 | 9/13 |
| nested_instancing.usda | 40 / 40 | 0 | 13/17 | 12/17 |
| openpbr_showcase.usda | 25 / 25 | 0 | 11/15 | 11/15 |
| PointInstancedMedCity.usd | 38 / 38 | 0 | 5/5 | 4/5 |
| precomp.usda | 27 / 27 | 0 | 10/13 | 10/13 |
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
static difference. Of 1 214 edits run with nothing read
(ALab and isCoral included), 908 give identical entries; with everything
read, 874 of 1 192. Every other edit differs for one of the openusd
reasons under "Notices". `main` gives 906 and 874: the PR makes two
edits identical (`point_instancing_5` and `hydra_t4`: a point instancer's
`Prototypes` made non-instanceable), moves the 26 other prototype source
edits closer, and makes no edit worse. The single-path fields compared
strictly: binding paths, `instance/instancer`, `niPrototypePath`,
`piPropagatedPrototypes/*`, `primOrigin/scenePath`, `model/modelPath`,
volume field bindings, the render settings' camera, products' `cameraPrim`
and vars' `path`, a render pass's `renderSource`, the `sceneGlobals`
prims: each a `Value::Path` where Hydra holds an `SdfPath`, and the path
arrays (`instancedBy`, `instancerTopology/prototypes`, collections) stay
`PathVec`s. `dependedOnPrimPath` is null in both final indices; Moho
writes it as a `Value::Path` (`DependencySchema::build`).

### What the PR changed in Moho's output

Moho `main` against Moho PR #39, over the 142 scenes, before any
comparison with Hydra:

| Change | Moho entries | Against Hydra |
|---|---|---|
| A single path held as a `Value::Path` (`[/a]` to `/a`) | 96 555 | matches |
| Prototype numbers from 1 (`__Prototype_N` to `N+1`) | 19 scenes | matches when one prototype is composed at a time |
| `ForInstancer<hash>` of point instancers inside native prototypes | isCoral: 4 of 24 names | all 24 match (were 4 mismatches) |
| Propagated prototype group names (`NoPrimvars_model<hash>`, ...): Moho's own digits | prim set of 19 786 entries | normalized |
| `primvars` name order | 13 450 containers | Hydra's order in all but 61 (was 13 511) |
| `namespacedSettings` name order | 42 containers | Hydra's order in all (was 42 out of order) |
| Material node `parameters` and `inputConnections` order | none | already Hydra's (155 679 and 129 844 containers) |
| Material `interface/parameters` order | reordered | Hydra's hash order on both revisions (normalized) |
| Colour spaces | no value changed in the corpus | the colour space fixture: a defined name and its inheritance now match (3 differences on `main`, none on the PR) |

### Loading and unloading

Entries, Hydra against Moho (`main`, PR, and the PR with recomposed prims
ignored), prototype names normalized. "=" is an identical edit, `H/M` the
two counts otherwise. The PR columns include the fix for prototypes a load
composes (under "Fixed"); without it ALab's first load sent 738 entries
and `load root` 41 651, light linking dirtying prototype prims the index
showed. isCoral's `load isCoral` now differs only in prototype numbers
(29 `ForInstancer<hash>` names on each side, hashing `/__Prototype_N`
paths numbered in another order after the earlier loads).

| Scene, step | Hydra | main | PR | PR, recomposed ignored |
|---|---|---|---|---|
| `flat`: load `a` | 2 | 3 | 3 | = |
| `flat`: unload `a` | 2 | 3 | 3 | = |
| `flat`: load `a`, `b` | 4 | 5 | 5 | = |
| `flat`: load `a` (loaded), unload `b` | 4 | 5 | 2 | 2 |
| `flat`: unload `world` | 3 | = | = | 2 |
| `flat`: load `world` | 5 | = | = | = |
| `nested`: load `a` | 4 | = | = | = |
| `nested`: unload `a` | 2 | 4 | 4 | 4 |
| `nested`: load `a`, `b` | 4 | 6 | 6 | 6 |
| `nested`: load `a` (loaded), unload `b` | 4 | 6 | 2 | 2 |
| `nested`: unload, load `world` | 4, 6 | =, = | =, = | =, = |
| isCoral: load `isCoral5` | 4 660 | 4 661 | 4 661 | = |
| isCoral: unload `isCoral5` | 5 428 | 5 285 | 5 285 | 5 284 |
| isCoral: load `isCoral5`, `isCoral4` | 9 291 | 9 292 | 9 292 | = |
| isCoral: load `isCoral5` (loaded), unload `isCoral4` | 10 043 | 9 858 | 5 288 | 5 288 |
| isCoral: unload `isCoral` | 5 429 | 5 285 | 5 285 | 5 284 |
| isCoral: load `isCoral` | 27 891 | 27 894 | 27 909 | 27 909 |
| ALab: load a cinder block | 728 | 729 (grouping) | = | = |
| ALab: unload it | 23 | 759 | 370 | 370 |
| ALab: load it and its sibling | 17 | 733 | 731 | 731 |
| ALab: load it (loaded), unload the sibling | 31 | 408 | 29 | 29 |
| ALab: unload `root` | 387 | 759 | 370 | 370 |
| ALab: load `root` | 36 367 | 36 643 | 35 212 | 35 212 |

The stage notices behind them (`flat`; USD's `ObjectsChanged` resynced
paths, openusd's `Resynced` subtrees and recomposed prims):

| Step | USD | openusd |
|---|---|---|
| load `a` | `/world/a` | `/world/a`; prim `/world` |
| unload `a` | `/world/a` | `/world/a`; prim `/world` |
| load `a`, `b` | `/world/a /world/b` | `/world/a /world/b`; prim `/world` |
| load `a` (loaded), unload `b` | `/world/a /world/b` | `/world/b` |
| unload `world` | `/world` | `/world/a`; prim `/world` |
| load `world` | `/world` | `/world` |

`/world` has no payload: it is loaded in USD before, between and after
every step, and its data never changes, in Moho either. UsdImaging resyncs
what USD names, so for a load or an unload below it Hydra sends nothing
for `/world`; Moho adds it again. In `nested` (`/world` with a payload)
and on ALab (`/root` with one) openusd names the ancestor as a subtree and
Moho matches Hydra wherever the stage does. Run again with unloaded prims
included (`flat`, `nested`), the same three steps are identical once the
recomposed prims are ignored, and `unload world` misses `/world` and
`/world/b` (with the re-add, `/world/b` only).

## Remaining differences

### (a) Accepted, documented deviations

Static data:

- **`skelBinding` and `coordSysBinding` are not named.** UsdImaging's
  keyless API schema adapters name both on every prim, null without a
  binding; Moho has no skeleton data, and coordinate systems are on the
  ROADMAP. (Most of the raw difference count of every scene.) Still holds.
- **`geomModel/cardVisibility` has no fallback.** openusd-schemas' UsdGeom
  schema predates `model:cardVisibility` (fallback `inherited`). Upstream
  candidate. Still holds.
- **A value authored with another type than UsdImaging reads.** C++
  `Get<T>` fails when the authored type is not `T`, and UsdImaging answers
  its default: `geomXformVectors` of a stack with a `float3` translate or a
  `double3` rotate or scale is the identity, and ALab's `extentsHint`s
  authored `vector3d[]` give no hint and an empty extent. openusd casts.
  Upstream candidate. Also, rotations read back by accumulation can be the
  other equivalent triple (`(180, -20, 180)` for `(-180, -20, -180)`),
  recorded in `hydra-parity-stage`. Still holds.
- **Category ids are numbered in another order** (ALab: a light's
  `group_0` against `group_3`): ids are given in the order collections are
  met, which follows children order, path order in Moho's merging and hash
  order in Hydra's. The ids stay consistent between lights and prims.
  Still holds.
- **Text of path expressions.** An explicit collection's union in hash map
  order in C++ (`/world/c /world/a`), path order in openusd; a predicate
  argument quoted in C++ (`{hdType:"mesh"}`), bare in openusd. The same
  expressions. Still holds.
- **Prototype numbers.** Both number from `/__Prototype_1` since openusd
  `26c81d8` (mxpv/openusd#194), and the `ForInstancer<hash>` names of
  isCoral, which hash the instancer's `/__Prototype_N/...` path, are now
  Hydra's. Prototypes composed together are still numbered in another
  order (hash order in USD, path order in openusd; see "Prototype names"),
  in 4 scenes.
- **`apiSchemas` naming `MaterialXConfigAPI`** (the teapot samples): the
  reference build has no `usdMtlx`, so its registry does not know the
  schema; openusd-schemas does. A harness artifact. Not seen in this run,
  on `main` either: no dumped prim names the schema.
- **Hydra runs out of memory** on `point_instancing`'s `huge` instancer
  (`protoIndices` up to 2 000 000 000): it allocates one index array per
  prototype index. Moho answers the same vector lazily; the rest of that
  fixture matches. Still holds.

Notices (all openusd's authoring, not the index; upstream candidates):

- **A new spec is one resync.** Setting a value on an attribute without a
  spec is one resync in openusd, a resync then an info change in USD, so
  UsdImaging sends a second entry with the same (or finer) locators: 263
  edits (261 with everything read), among them the stage-level
  `includeRoot` edit (Hydra's two `collections/lightLink` entries). Given
  USD's notices (an attribute created, then set), Moho now sends
  UsdImaging's two entries (`stage_updates.rs`). Still holds.
- **Inert `over`s are resynced.** Authoring through a reference creates
  `over` specs that openusd reports resynced, USD info-only, so Moho
  re-adds that subtree (ALab: 33 852 entries for the first intensity edit
  on a light, where Hydra sends 2, 42 150 before `26c81d8`; identical once
  the overs exist): 14 edits (9 with everything read). A `typeName` edit
  through a reference (`Kitchen_set`) re-adds the inert `over` parent too.
- **A prototype's source edit resyncs the whole prototype and every
  instance.** The prototype is now kept, as in USD (no more `Prototypes
  retired`), but where USD resyncs the path inside the prototype
  (`/__Prototype_1/__parityNew`) and the source, or for an instance made
  non-instanceable that instance alone, openusd resyncs
  `/__Prototype_1` and every instance (`/world/a`, `/world/b`), which
  Moho repopulates: 27 edits (47 with everything read), each with fewer
  entries than before `26c81d8` (`native_instancing_3`, a prim added under
  the asset: 9 entries where Hydra sends 2, 21 before).
- **No-op writes** (a relationship set to the targets it has) notify
  nothing in openusd; USD notifies. Seen on hand-run edits only; not
  re-run.
- **An already loaded payload in a `LoadAndUnload`** is not reported. USD
  skips a load only when one of the two sets is empty
  (`UsdStage::LoadAndUnload`'s no-op check) and resyncs every path of
  both; openusd drops the loaded one, so Moho does not repopulate it
  (isCoral: 5 288 entries where Hydra sends 10 043). New with `26c81d8`:
  the earlier pin reported it (9 858).
- **Unloading a payload leaves its loaded ancestor loaded in USD.**
  `UsdStage::LoadAndUnload` recomposes the requested paths only, so after
  `Load("/world/a")` and `Unload("/world/a")` USD's `/world` (which has a
  payload) is still loaded, though its effective rule is now `NoneRule`;
  openusd recomposes `/world` and unloads it, so Moho removes the subtree
  (ALab: 370 entries for one cinder block, where Hydra sends 23). Also on
  `main`.
- **An unload above the payloads** (`Unload("/world")`, `/world` without a
  payload) is reported as the payloads unloaded and `/world` recomposed
  in place; USD resyncs `/world`, which Hydra adds again. Moho adds it
  again only through the re-add of recomposed prims (see (c)).

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
  data as its default. Still holds (7 entries).
- **An edit to a prim under an abstract (`class`) prim is also dirtied at
  that prim's own path.** `UsdImagingStageSceneIndex::_ComputeDirtiedEntries`
  (`stageSceneIndex.cpp:797` and on) does not check `_populatedPaths`, so
  editing `/Asset/geo/m.visibility` under a class gives
  `DIRTIED /Asset/geo/m { visibility }` besides the prototype's entry. Moho
  sends the prototype's entry only. The path holds no prim in either
  index, so a consumer has nothing to update. Not re-run.
- **A resynced prototype's prims are removed at their stage paths.** When
  USD resyncs a prototype (an instance removed, a payload unloaded), Hydra
  sends `REMOVED /__Prototype_N/...` for prims its index never held
  (`fxn_ni`, the house removed: 4 entries; isCoral, `isCoral5` unloaded:
  62). Moho sends none. Seen on `main` too.

### (b) Moho's generic data

`__usdAttributes` and `__usdRelationships` on every prim, and their dirty
locators on every property edit.

Also outside Hydra, and not generic data: `__usdMetersPerUnit` on the
absolute root, a Moho extension since `4d63d41` (`stage-meters-per-unit`),
in the 19 scenes that author `metersPerUnit`. `CLAUDE.md` names the generic
data as the only addition; listed here pending that decision.

### (c) Real divergences

Introduced by PR #39, not fixed in it (the second one it introduced is
under "Fixed"):

- **A prim recomposed in place is added again.** openusd reports the
  ancestor of a loaded or unloaded payload that has no payload of its own
  as recomposed in place (`Resynced::prims`, `CommittedChange::recomposed`);
  USD names no such path, and Hydra sends nothing for it. Moho adds the
  prim again alone (`StageSceneIndex::apply_pending_updates`), an extra
  `ADDED` on each load, unload or `LoadAndUnload` below it (`flat`: 3 of
  6 steps; isCoral: 3 of 6). With recomposed prims ignored, every
  generated and hand edit is unchanged, and the load steps go from 2 to 4
  identical of 6 on `flat` and from 0 to 2 on isCoral; one step loses an
  entry Hydra sends (`flat` and isCoral, an unload of the payloads'
  parent: the openusd notice above). Fix: in `stage/pending.rs`,
  `Recorder::after_commit` and `Pending::record_resynced` stop recording
  `CommittedChange::recomposed` and `Resynced::prims`; the recomposed
  block of `apply_pending_updates` (`stage/mod.rs`, `Pending::recomposed`,
  `settle`) goes, and `tests/stage.rs`'s
  `loading_one_payload_leaves_its_siblings` expects `/world/a` and
  `/world/a/geo` only.
Left unchanged by the PR (also on `main`):

- **A resynced subtree's descendants are not dirtied when nothing was
  read.** Hydra's `HdDependencyForwardingSceneIndex` over
  `HdsiLightLinkingSceneIndex` (neither alone) reads flattened data of the
  geometry during population, so when a prim is added again (a `kind`,
  `apiSchemas` or `specifier` edit) the flattening dirties its
  descendants for every flattened name; Moho's filters read none, and
  sends the `ADDED`s alone. With everything read the entries are
  identical but for `skelBinding` and `coordSysBinding` (class a). Not
  yet traced to the read.
- **A prototype's point instancers are re-added more often in Hydra**
  when USD resyncs the prototypes on an unload (isCoral, `isCoral5`
  unloaded: each `xg*` instancer 8 times, 6 in Moho). Not yet traced.

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

The fourth open gap, the stage-level `includeRoot` edit's two entries, is
openusd's authoring (see "A new spec is one resync"): with USD's two
notices Moho sends both entries.

## Scene globals filter (`hydra-scene-globals`)

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

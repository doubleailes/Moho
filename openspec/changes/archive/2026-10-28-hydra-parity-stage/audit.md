# Moho vs Hydra 2.0 / UsdImaging: parity audit

This audit only read code. It made no edits and ran nothing.
- Hydra reference: the local checkout `/home/philippe.llerena/Workspace/github/doubleailes/OpenUSD` at `70c45e83b`. Hydra paths below are relative to `pxr/`.
- Moho: `crates/moho/src` at `d09cc94`.
- "Confirmed" means read in code on both sides. "Not confirmed" means inferred from code, with no test run.

---

## 1. Chain order and filters Moho lacks

**Hydra** (`usdImaging/usdImaging/sceneIndices.cpp`, in `UsdImagingCreateSceneIndices`):

| # | Filter | line |
|---|---|---|
| 1 | `UsdImagingStageSceneIndex` | 202 |
| 2 | `HdsiLocatorCachingSceneIndex` (the `material` locator only, a cache) | 218 |
| 3 | the `overridesSceneIndexCallback` (optional) | 220 |
| 4 | `UsdImagingUnloadedDrawModeSceneIndex` (only with `displayUnloadedPrimsWithBounds`) | 229 |
| 5 | `UsdImagingExtentResolvingSceneIndex` (`extentsHint` for purposes geometry, render and proxy) | 233 |
| 6 | **`UsdImagingPiPrototypePropagatingSceneIndex`** | 240 |
| 7 | **`UsdImagingNiPrototypePropagatingSceneIndex`**, with `UsdImagingDrawModeSceneIndex` as the per-prototype callback (`addDrawModeSceneIndex` defaults to true, `sceneIndices.h:46`) | 282 |
| 8 | `HdNoticeBatchingSceneIndex` | 287 |
| 9 | `UsdImaging_InstanceProxyPathTranslationSceneIndex` (`usdMaterialBindings` only) | 295 |
| 10 | `UsdImagingMaterialBindingsResolvingSceneIndex` | 298 |
| 11 | UsdImaging scene index plugins | 302 |
| 12 | `UsdImagingSelectionSceneIndex` | 305 |
| 13 | `UsdImagingRenderSettingsFlatteningSceneIndex` | 308 |

- **Flattening is not a step of its own in Hydra.** `HdFlatteningSceneIndex`, with `UsdImagingFlattenedDataSourceProviders`, runs **inside** NI propagation for the whole stage and for each prototype (`niPrototypePropagatingSceneIndex.cpp:171-204`).
  - The providers flatten `usdMaterialBindings`, `model` (geomModel), `__usdUpAxis`, `coordSysBinding`, `primvars`, `purpose`, `visibility` and `xform` (`flattenedDataSourceProviders.cpp:28-44`, `hd/flattenedDataSourceProviders.cpp:32-41`).
  - Instance aggregation therefore reads flattened data.
- The NI header requires this order: "the native instancing scene index has to be run after the point instancing scene index" (`niPrototypePropagatingSceneIndex.h`, class comment).
- **`HdsiLightLinkingSceneIndex` is not in the UsdImaging chain.** A renderer registers it as a scene index plugin. hdPrman uses insertion phase 4, `InsertionOrderAtStart` (`third_party/renderman/plugin/hdPrman/lightLinkingSceneIndexPlugin.cpp:51-69`).
- `HdsiMaterialBindingResolvingSceneIndex` (choosing a binding purpose) is also renderer-side (`imaging/hdSt/materialBindingResolvingSceneIndexPlugin.cpp`).

**Moho** (`chain.rs:36-43`, `standard` at 24-27, `merged` at 53-55):

```
Stage → NativeInstancing → PointInstancing → Flattening → MaterialBindingResolving
      → RenderSettingsFlattening → LightLinking → MaterialNodeResolving
```

`docs/architecture.md` claims it is "in UsdImagingCreateSceneIndices' order". **It is not.**

**Verdict: divergence.**
1. NI runs before PI, the reverse of Hydra.
2. Flattening is a top-level step, not part of NI propagation.
3. Light linking and MaterialNodeResolving are in Moho's chain. Light linking is renderer-side in Hydra, and MaterialNodeResolving is Moho-only.

Filters Moho lacks:

| Filter | Assessment |
|---|---|
| ExtentResolving | missing, and no `extentsHint` on the stage index either (grep finds none) |
| DrawMode / UnloadedDrawMode | missing; no `model`/geomModel data at all |
| InstanceProxyPathTranslation | missing; bindings that target a prim inside an instance stay proxy paths |
| NoticeBatching | missing; only affects performance and notice batching |
| LocatorCaching | missing; performance only |
| Selection | missing; UI only, can be dropped deliberately |

**What parity would require:**
- `chain::standard` = Stage → ExtentResolving → PI propagation → NI propagation (flattening and the optional draw mode inside) → InstanceProxyPathTranslation → MaterialBindingsResolving → RenderSettingsFlattening.
- Light linking becomes a separate filter that consumers append, as a renderer plugin would.

**The order flip has a consequence beyond M12 (not confirmed by a test).** Take a native instance inside a point instancer's prototype.
- In Hydra:
  - The PI copy `P/ForInstancerX/inst` carries `instancedBy` (the point instancer) and keeps `niPrototypePath`.
  - Aggregation uses that copy root as the enclosing prototype root. It puts the native instancer under `P/ForInstancerX/UsdNiPropagatedPrototypes/...` with the point instancer's `instancedBy` (`niInstanceAggregationSceneIndex.cpp:677-693`, 1360-1375).
  - So the native instance is drawn once per point instance.
- In Moho:
  - NI runs first, so `scan` groups that instance at scope `/` (`native_instancing.rs:215-228`).
  - The native instancer lives at `/MohoNiPropagatedPrototypes/...` with no `instancedBy`, and its `instanceLocations` is the original path.
  - The PI copy only carries a typeless instance prim with `instance` data.
  - Result: the asset is drawn once at its original transform and is not scattered.
- This matters on Moana-like scenes, where point instancers scatter instanceable assets.

## 2. M12: a PI prototype target inside a native instance

**Hydra, confirmed:**
1. **The stage scene index returns an empty prim for an instance proxy.** `GetPrim` returns `{TfToken(), nullptr}` for `prim.IsInstanceProxy()` (`stageSceneIndex.cpp:257-259`). `GetChildPrimPaths` returns `{}` under an instance or an instance proxy (`stageSceneIndex.cpp:306-314`).
2. **The point instancer's targets are raw stage paths.** The adapter reads `instancerTopology/prototypes` from `GetPrototypesRel().GetTargets` (`dataSourcePointInstancer.cpp:307-326`), so they can name instance proxies.
3. **PI propagation builds the copy over the raw path.**
   - For each target `P` it builds `Rerooting(input, P→P)`, then `PiPrototypeSceneIndex`, then `Rerooting(P→P/ForInstancer<hash>)`, mounted in a merging index at the copy root (`piPrototypePropagatingSceneIndex.cpp:344-374`, 486-518).
   - The rerooting index maps the copy path back to `P` (`rerootingSceneIndex.cpp`, `GetPrim`).
   - `P` is a proxy, so the stage index yields a null data source and no children. `PiPrototypeSceneIndex::GetPrim` returns that prim unchanged when the data source is null (`piPrototypeSceneIndex.cpp:225-227`).
4. **The instancer still names the copy.** `instancerTopology/prototypes` is overridden to `P/ForInstancer<hash>` (`piPrototypePropagatingSceneIndex.cpp:515-518`), which is an empty, typeless prim with no children.
5. **Nothing downstream repairs the path.** NI propagation leaves it alone (`niPrototypeSceneIndex.cpp:161-203` only blanks instances and adds `instancedBy` inside prototypes). `InstanceProxyPathTranslationSceneIndex` runs after both propagations and translates only the configured data sources: `usdMaterialBindings` plus plugin names (`sceneIndices.cpp:174-193`), never `instancerTopology`.

**Under UsdImaging 2.0, Moana's `isCoral` point instancers therefore render nothing for those prototypes.** That is the same loss of about 2.9M geometries that Moho had before M12.
- Not confirmed at runtime: no usdview or Hydra run was made.
- The legacy UsdImaging 1.0 delegate does handle it. It takes the prim at the target path with `_GetPrim`, which works for proxies, and traverses it (`pointInstancerAdapter.cpp:243-263`, 286-322). That is why Moana renders in older usdview.

**Moho:** `point_instancing.rs:239-317` (`through_instance`, `add_instancer`).
- A target the index does not hold is resolved through the nearest ancestor's `instance` → instancer topology → propagated prototype.
- Its copies go to `I/ForInstancer<hash>/prototype<i>` under a typeless scope (`Model::extra_children`, `is_scope`).
- This only works because Moho runs NI first.

**Verdict: Moho-only behaviour**, in the same spirit as Hydra's InstanceProxyPathTranslation but applied to PI prototypes.

**Parity requires** removing M12 (`through_instance`, `extra_children`, scope handling, and the 3 tests at `point_instancing.rs:352-470`). Copies of proxy targets then come out empty, as in Hydra.

**Consumers lose** Moana's coral geometry (about 2.9M geometries), unless:
- crust-render keeps a consumer-side workaround, or
- the gap is offered upstream to OpenUSD (UsdImaging 2.0 regresses against 1.0 here).

## 3. M10 and streaming: `__materialNodes`, `__materialNodeGraph`, `MaterialNodeResolvingSceneIndex`, `streaming::unresolved`

**Hydra, confirmed: there is no counterpart.**
- `pxr/usdImaging` never uses a population mask; grep finds `PopulationMask` only in `bin/usdrender/renderRequest.cpp`.
- Nothing resolves material nodes across scenes.
- An upstream connection to a prim the stage cannot reach is just missing from the UsdShade walk: `_WalkGraph` follows `GetValueProducingAttributes(shaderOutputsOnly=true)` (`dataSourceMaterial.cpp:711-753`).

**What Hydra's material produces** (`UsdImagingDataSourceMaterial`, `dataSourceMaterial.cpp`):
- **Render contexts** (`GetNames` 676-701): one per material output namespace (`""`, `ri`, `mtlx`, ...), **plus always `all`**.
- **Each context**, built by `_BuildMaterial` (894-1051), is an `HdMaterialNetworkSchema`:
  - `nodes`, named by the node's path relative to the material via `ReplacePrefix(materialPrefix, ".")`. A node outside the material keeps its absolute path (`_RelativePath` 48-52). **Moho's naming rule matches this.**
  - `terminals`: name → `{upstreamNodePath, upstreamNodeOutputName}`. The `all` context keeps the namespaced names, such as `ri:surface`.
  - `config`: the material's `config:*` attributes (1016-1048).
  - `interface`: `parameters` (interface mappings: nodePath, inputName) and `parameterOrder` (`_BuildMaterialInterface`).
- **Each node** (`_UsdImagingDataSourceShadingNode`, 540-650):
  - `nodeIdentifier`;
  - `renderContextNodeIdentifiers`;
  - `nodeTypeInfo`, when the implementation source is not `id`;
  - `parameters`, with `value`, `colorSpace` and `typeName`;
  - `inputConnections`.
- **In the `all` context**, every Shader under the material is a node, connected or not (957-976).

**Moho:**
- `stage/material.rs` emits:
  - per-context networks with `nodes` and `terminals`, but no `all` context, no `config`, no `interface`, no `nodeTypeInfo` and no `renderContextNodeIdentifiers`;
  - `OffStage` connections (`material.rs:161-186`, 258, 327-349);
  - `__materialNodes` and `__materialNodeGraph` on Shader and NodeGraph prims (`material.rs:405-446`).
- `filter/material_nodes.rs`, in `chain::merged` and `standard`, fills dangling nodes. Its own doc says (line 14): "Hydra has no counterpart".
- `streaming::unresolved` (`streaming.rs:35-55`) lists missing materials and nodes.

**Verdict: Moho-only** (`__materialNodes`, `__materialNodeGraph`, OffStage, the resolving filter, `unresolved`). The network itself is a **partial divergence**: the fields listed above are missing.

**Parity requires:**
- Dropping all of the Moho-only pieces.
- Adding `all`, `config`, `interface`, `nodeTypeInfo` and `renderContextNodeIdentifiers`.
- Dropping a connection to a node the stage cannot reach, as UsdShade does. Moho already does this on a fully loaded stage.

**Consumers lose** streamed materials whose networks reach outside a chunk's mask, and materials bound from another chunk's payload; the S0/S2 side-input loop goes away. A chunked import would have to load every material's dependencies itself, or render from one LoadAll stage.

## 4. Light linking after a merge

**Hydra, confirmed** (`imaging/hdsi/lightLinkingSceneIndex.cpp`):

| Aspect | Hydra | where |
|---|---|---|
| Membership | `HdCollectionExpressionEvaluator` over the **input scene index**, with `HdGetCollectionPredicateLibrary()` (`hdType`/`type`, `hdVisible`/`visible`, `hdPurpose`, `hdHasDataSource`, `hdHasPrimvar`, `hdHasMaterialBinding`). Not USD's predicates, and it judges the Hydra namespace ("doesn't support instance proxy traversal"). | 503-521; `hd/collectionPredicateLibrary.cpp:93-257` |
| Category id | Sequential `group_<n>`, shared by collections with equal expressions; trivial expressions get the empty token. | 57, 432-450, 171-193 |
| `categories` | Only on geometry (all `HdRprimTypeTokens`) that is **not instanced**, i.e. with empty `instancedBy/paths`. Absent (null) when empty. | 1147-1151, 665-684 |
| Instancers | `instanceCategories` only for native instancers (with `instanceLocations`). A **point instancer gets `categories`** for all its instances, not `instanceCategories`. Plus `__dependencies` on each instance's `categories`. | 693-757, 888-947, 759-815 |
| Lights and filters | `light/{lightLink, shadowLink, lightFilterLink}`, from the collections `lightLink`, `shadowLink` and `filterLink`; light filter prims too. Overridden only when the prim has a `collections` data source. | 580-604, 940-1005 |

**Moho** (`filter/light_linking.rs`):

| Aspect | Moho | where |
|---|---|---|
| Membership | The stage `CollectionMembership` oracle (USD semantics) when given, else a `PathExpressionEval` built with an **empty `PredicateLibrary`**. After a merge (`chain::merged`, `membership=None`), any expression with a predicate fails to build, so `eval=None` and nothing is included. | 145-148, 161-170 |
| Category id | `"<collection>:<expression text>"` | 141-144 |
| `categories` | On every geometry prim, prototypes included, from a hard-coded type list with no `points` or `tetMesh`. An empty container rather than absent. | 33-43, 231-242 |
| Instancers | Per-instance `instanceCategories` for point instancers too, each judged at the instancer path. No `__dependencies`. | 244-284 |
| Lights and filters | No `filterLink` or `lightFilterLink`; no light filters. A light is detected by a `light` container, not by type. | 70-74 |

**Verdict: divergence** on every row. Moho's merged mode is weaker than Hydra's: Hydra evaluates with predicates over the merged scene index and needs no stage.

**Parity requires:**
- Evaluating expressions on the input scene index with a port of Hydra's predicate library (whether openusd's `PredicateLibrary` can register custom predicates was not confirmed).
- Dropping the `CollectionMembership` oracle for linking.
- `group_N` ids; skipping instanced geometry; `categories` on point instancers.
- Adding `filterLink`/`lightFilterLink` and light filters.
- Moving it out of `chain::standard`.

**Loss for consumers:** expressions that use USD-only predicates now evaluate to nothing, as they do in Hydra. In exchange, linking stops depending on a stage, which helps a merged scene.

## 5. `kind` edits

**Hydra, confirmed:**
- For a prim path in `ChangedInfoOnlyPaths`, it **resyncs if any changed field is a plugin field, and ignores built-in fields** (`stageSceneIndex.cpp:556-575`).
- `kind` is a built-in field: it is registered by `SdfSchemaBase` (`usd/sdf/schema.cpp:749`), and `_isPlugin` defaults to false (72).
- So a `kind` edit produces **no notice** in Hydra.

**Moho:**
- Every info-only prim change dirties `__usdPrimInfo/kind` (`stage/mod.rs:321-337`, `pending.rs:41-55`), because openusd reports no changed fields. This is a workaround noted in the ROADMAP.

**Verdict: divergence.**

**Parity requires** ignoring prim-level info-only changes, and resyncing on plugin-field changes once openusd exposes `GetChangedFields` (an upstream candidate). The test `prim_info.rs::a_kind_edit_dirties_the_prim_info` flips.

**Also on the stage index:**
- **Resync notices.** Hydra elides `Removed` for prims that are re-added and sends only `Added` (`stageSceneIndex.cpp:702-737`). Moho sends `Removed(root)` then `Added` (`stage/mod.rs:291-303`).
- **Asset path resyncs.** `GetResolvedAssetPathsResyncedPaths` (`stageSceneIndex.cpp:587-592`) has no Moho equivalent; openusd lacks it.

## 6. Rerooting

**Hydra** (`rerootingContainerDataSource.cpp:157-240`):
- It recursively wraps containers and vectors, and maps every `HdTypedSampledDataSource<SdfPath>` and `<VtArray<SdfPath>>` with a single-pair `PcpMapFunction`.
- Paths outside the source are left alone (51-56).
- It does not touch `SdfPathExpression`, strings or tokens.
- `primOrigin/scenePath` is an `HdPrimOriginSchema::OriginPath`, not an `SdfPath`, so it is untouched because of its type.
- It is applied to every prim served by `UsdImagingRerootingSceneIndex` (`rerootingSceneIndex.cpp` `GetPrim`): PI copies, NI prototypes, and the aggregation's binding-hash and binding-copy rerooting (`niInstanceAggregationSceneIndex.cpp:1393-1397`, 1420-1430).

**Moho** (`filter/rerooting.rs:26-127`):
- It reroots `Value::PathVec` everywhere except top-level `primOrigin` (36-47), whose `scenePath` Moho encodes as a `PathVec`.
- It is used by NI (`native_instancing.rs` `prim`, at the `remap_of` call) and PI (`point_instancing.rs:231-237`, 456-459).

**Verdict: parity in effect.**
- The `primOrigin` exception reproduces Hydra's type-based exclusion.
- Every Moho path is a `PathVec` because openusd's `sdf::Value` has no single-`Path` variant (`sdf/value.rs:143` has only `PathVec`; checked at rev `d7d2a29`). Hydra's single `SdfPath` fields (`materialPath`, `niPrototypePath`, `instance/instancer`, `binding/path`, `activeRenderSettingsPrim`) appear in Moho as one-element arrays. That is a representation divergence to document, or to take upstream.
- Moho does not reroot instance data for the binding hash. That is minor, and folds into item 8.

## 7. MergingSceneIndex

**Hydra** (`imaging/hd/mergingSceneIndex.cpp`):

| Aspect | Hydra | where |
|---|---|---|
| Prim type and data | first non-empty type wins; data are overlaid; empty container at roots and their ancestors | 350-406 |
| Children | a **`TfDenseHashSet`**, so no order | 409-432 |
| Adding an input | strict prefixes of the root, the **root always**, then **every child path recursively, defined or not** | 173-245, 30-50 |
| Removing an input | walks the removed input's children; still-present → Added, else Removed | 248-328 |
| Added from an input | forwarded **without filtering by the input's root**, retyped with the merged type | 435-515 |
| Removed from an input | forwarded unfiltered, then what still exists is re-added along with its subtree | 518-560 |
| Dirtied | forwarded unfiltered | 563-572 |
| Renamed | `ConvertPrimsRenamedToRemovedAndAdded` | 608-614 |

**Moho** (`filter/merging.rs`):

| Aspect | Moho | where |
|---|---|---|
| Children | in input order | 196-225 |
| Notices | **filtered to the sender's roots** (`under_any`) | 228-262, 128-150 |
| Adding an input | the root and its descendants only when *defined*; strict ancestors always | 56-75, 158-171 |
| Renamed | Removed plus Added, as Hydra | 250-262 |

**Verdict:**
- Order: a compatible refinement (Hydra's order is unspecified).
- Notices: divergence. Moho's root filtering is stricter than Hydra's; it is arguably a bug fix, but it is not parity.
- Undefined prims are omitted from Added, and a root not defined by its input is not notified.

**Parity requires** sending the root always, adding undefined children, and either dropping the root filter or arguing it in the change. Tests `merging.rs` `renames_outside_a_root_stay_hidden` and `an_input_at_the_root_notifies_the_root` are the likely ones to change.

## 8. Native instancing: naming, location and data

**Naming and location.**
- **Hydra:** `<enclosingPrototypeRoot>/UsdNiPropagatedPrototypes/<bindingHash>/<__Prototype_N>/UsdNiInstancer/UsdNiPrototype` (`niInstanceAggregationSceneIndex.cpp:871-907`; tokens: `usdImaging/tokens.h:45`, `niPrototypeSceneIndex.h` with `UsdNiInstancer` and `UsdNiPrototype`).
  - The **prototype is a child of the instancer**.
  - The binding hash is `NoPrimvars` or `Primvars%zx`, followed by `_<name>%zx` for each instance data source name (`niInstanceAggregationSceneIndex.cpp:737-796`).
  - The enclosing root is the `instancedBy/prototypeRoot`, so a PI copy root counts (`niInstanceAggregationSceneIndex.cpp:1360-1370`).
- **Moho:** `<scope>/MohoNiPropagatedPrototypes/<NoBindings|Bindings%016x>/P<identity>/{prototype, instancer}` (`native_instancing.rs:258-285`).
  - The prototype is a sibling of the instancer.
  - `<identity>` is Moho's stable openusd instancing-key hash (`stage/instancing.rs`, `__usdPrimInfo/niPrototypeIdentity` at `prim_info.rs:44-51`), not the `__Prototype_N` name.
  - The scope is never a PI copy, because NI runs first.
- **Verdict: divergence** (names, layout, and a Moho-only identity).

**Grouping key.**
- **Hydra:** computed on the instance prim's **flattened** data, rerooted to `/<protoName>`. It covers:
  - the constant-primvar names and roles;
  - `usdMaterialBindings`, `purpose`, `model` (geomModel), `model` (UsdImagingModelSchema) and `__usdUpAxis` (`sceneIndices.cpp:134-160`).
- **Moho:** the text of `usdMaterialBindings` and `purpose` on the instance and its ancestors within the scope (`native_instancing.rs:192-211`).
- **Divergence.**

**Binding data on the prototype.**
- **Hydra:** the `<bindingHash>` prim **carries a static copy** of the instance data sources, rerooted onto the prototype path (`niInstanceAggregationSceneIndex.cpp:839-858`, 1420-1430). The flattening inside the propagation then passes the instance's bindings and purpose down to the prototype prims.
- **Moho:** the directory prims carry **empty data** (`native_instancing.rs:429-431`).
- So in Moho a prototype bound only at the instance level (`rel material:binding` on the instanceable prim) resolves **no material**.
  - Not confirmed by a test: no Moho test checks a prototype prim's resolved binding. `native_instancing.rs::differing_bindings_split_groups` only checks the instancer path.
  - This looks like a real bug, beyond parity.

**Instancer data.**
- **Hydra** (`niInstanceAggregationSceneIndex.cpp:619-700`):
  - `instancerTopology {instanceIndices, prototypes, instanceLocations, mask}`, where `mask` comes from each instance's flattened visibility (600-616);
  - `primvars`: `hydra:instanceTransforms` (the flattened instance xform) **plus every constant primvar of the instances as an instance-rate primvar** (475-512);
  - `instancedBy`, when nested in a PI or NI prototype (677-693).
- **Moho** (`native_instancing.rs:325-376`): `prototypes`, `instanceIndices`, `instanceLocations` and `hydra:instanceTransforms` only.
  - There is no `mask`, so invisible instances are drawn.
  - There are no per-instance primvars, so per-instance `displayColor` and similar are lost.
  - There is no `instancedBy`.

**Prototype prims.**
- **Hydra:**
  - every prim in a prototype gets `instancedBy {paths:[UsdNiInstancer], prototypeRoots:[UsdNiInstancer/UsdNiPrototype]}` as an underlay (`niPrototypeSceneIndex.cpp:143-159`, 194-200);
  - instance prims with a renderable type are blanked (171-175).
- **Moho:**
  - **no `instancedBy` on NI prototype prims** (grep: `INSTANCED_BY` is only used in `point_instancing.rs:477`). A Hydra-style consumer that keys on `instancedBy` would draw prototypes in place and un-instanced.
  - instances keep their input type (`native_instancing.rs:457`, 477).

**`__usdPrimInfo` field by field** (`usdPrimInfoSchema.h:38-51`, `dataSourceUsdPrimInfo.cpp:58-155`, against `stage/prim_info.rs`):

| Field | Hydra | Moho | Verdict |
|---|---|---|---|
| `specifier`, `typeName`, `isLoaded`, `apiSchemas`, `kind` | yes | yes | parity |
| `variantSelections` | yes | **missing** | divergence |
| `niPrototypePath` | `SdfPath` | `PathVec` of one (openusd gap) | representation |
| `isNiPrototype` | root prototype only | root only | parity |
| `piPropagatedPrototypes` | written by PI propagation (`piPrototypePropagatingSceneIndex.cpp:101-177`) | missing | divergence |
| `niPrototypeIdentity` | — | Moho | **Moho-only** |

- `isNiPrototype`: Hydra uses `UsdPrim::IsPrototype`, which is true for the root only; Moho uses `is_prototype() && is_root_prim()`.

**PI naming.**
- **Hydra:** `ForInstancer%zx` of `TfHash::Combine(instancer.GetString(), propagatedPrototype.GetString())`, where `instancer` is the path in the prototype's input namespace and `propagatedPrototype` is the enclosing copy root, `/` at top level (`piPrototypePropagatingSceneIndex.cpp:406-433`).
- **Moho:** `ForInstancer%016x` of `DefaultHasher(instancer output path)` (`point_instancing.rs:171-173`). Nested instancers hash their output path, which already contains the enclosing copy.
- **Verdict:** same scheme and uniqueness, but different hash inputs and functions. The exact names can only match with a TfHash port, which is not recommended; the change should declare names opaque.

**Other PI differences** (`piPrototypeSceneIndex.cpp:59-131`, 158-257):

| Aspect | Hydra | Moho |
|---|---|---|
| Copy root | `xform.resetXformStack=true` as an overlay, **`visibility=true` as an underlay**. Prototypes kept under an invisible scope still draw. | overrides the xform but adds no visibility (`point_instancing.rs:483-491`), so copies under an invisible scope inherit invisibility through Flattening |
| `instancedBy` | an underlay | an override (`point_instancing.rs:476-482`); same effect, since Moho also takes the innermost |
| Prims under any instancer **or under an `over`** | made unrenderable, with `niPrototypePath` blocked (`HdBlockDataSource`) and `applyDrawMode=false` | — |
| Overs at all | included by the stage index: predicate `active && !abstract [&& loaded]`, **not `defined`** (`stageSceneIndex.cpp:484-505`) | **excluded**: `PrimPredicate::DEFAULT`, or ACTIVE\|DEFINED (`stage/mod.rs:183-189`, 464-475) |

- Because Moho excludes overs, **prototypes authored under an `over "Prototypes"` (a common idiom) come out as empty copies in Moho**. Not confirmed by a test.

## 9. Everything else

**Root and globals.**
- **Hydra root** (`dataSourceStage.cpp:25-80`): `system/assetResolution`; `sceneGlobals {activeRenderSettingsPrim, startTimeCode, endTimeCode, timeCodesPerSecond}`; `__usdUpAxis/upAxis`, flattened down to every prim. The full `HdSceneGlobalsSchema` (`sceneGlobalsSchema.h:37-46`) has no `metersPerUnit` or `upAxis`.
- **Moho** (`stage/globals.rs:73-116`):
  - adds `sceneGlobals/metersPerUnit` and `sceneGlobals/upAxis` (**not in Hydra**);
  - adds `__usdStageMetadata/<field>/{value, authored}` (**Moho-only**, and not sanctioned: the sanctioned addition is `__usdAttributes` and `__usdRelationships` only);
  - has no `__usdUpAxis`;
  - encodes `activeRenderSettingsPrim` as a `PathVec`.

**Inside the sanctioned containers.** `__usdAttributes/<a>/{value, authored, mightBeTimeVarying, typeName, colorSpace, interpolation, elementSize, connections}` and `__usdRelationships/{targets, forwardedTargets}` (`tokens.rs:33-63`). Hydra has no generic attribute container: the only `__` schemas are `__usdPrimInfo`, `__usdUpAxis`, `__usdRender{Settings, Product, Var}` and `__dependencies`. Every field here is therefore Moho's, and `authored` and `mightBeTimeVarying` stand only on the sanctioned addition.

**Render prims.**
- **Hydra:** `__usdRenderSettings`, `__usdRenderProduct`, `__usdRenderVar` (`usdRender*Schema.h`), then flattened to `renderSettings` (`HdRenderSettingsSchema`) with `__dependencies` (`renderSettingsFlatteningSceneIndex.cpp:32-41`, 455-520).
- **Moho:**
  - the stage index exposes them as `renderSettings`, `renderProduct`, `renderVar` (`tokens.rs:310-325`, `stage/render.rs`);
  - adds `instantaneousShutter` to the settings base (`render.rs:37`), which is not in UsdImaging's schemas;
  - tracks product and var dependents internally instead of emitting `__dependencies`.
- **Verdict: token divergence;** the flattened output is mostly at parity.

**Material bindings.**

| Aspect | Hydra | Moho |
|---|---|---|
| Per-level resolution (`materialBindingsResolvingSceneIndex.cpp:184-268`) | a stronger collection binding stops; **else a stronger direct binding stops (even when a weaker collection binding matched)**; else the collection binding wins; else the direct one | `chosen = collection.or_else(direct)` (`material_binding.rs:152`). With a non-stronger matching collection and a stronger direct binding on the same prim, Moho picks the collection and continues. **Edge-case divergence.** |
| Membership | `HdCollectionExpressionEvaluator(_si, expr)` on the collection prim's `collections/<name>/membershipExpression`, in the Hydra namespace (106-160, 274-288) | the stage oracle, else a `membershipExpression` that Moho copies into each collection binding (`tokens.rs:185-187`, **Moho-only**), compiled without predicates (`material_binding.rs:116-134`) |
| Dirty notices | `usdMaterialBindings` is **replaced** by the `materialBindings` prefix (444-479) | **adds** `materialBindings` and keeps the original (`material_binding.rs:243-256`), and re-dirties collection dependents (257-275) |

- Hydra's resolving index tracks no collection dependency. Whether another Hydra component re-dirties on a collection edit was **not confirmed**.

**Flattening.** Moho flattens `xform`, `visibility`, `purpose`, `primvars` and `usdMaterialBindings` (`flattening.rs:38-44`). Hydra's providers also flatten `model`, `__usdUpAxis` and `coordSysBinding`. Missing.

**Time.** Both read relative to the index time and dirty only time-varying data on `set_time`. Parity; Hydra's `forceDirtyingTimeDeps` (`stageSceneIndex.cpp:337-352`) is absent, which is minor.

**Not audited field by field:** mesh, curves, primvars, camera, light, volume and the implicit adapters. `nurbsCurves/pointWeights` is a known Moho addition (`tokens.rs:144`).

---

## Proposed change `scene-index-hydra-parity` (ordered)

1. **Groundwork.**
   - Decide on the single-`SdfPath` encoding: keep the one-element `PathVec`, documented, and file an openusd upstream candidate for `Value::Path`.
   - Add a block data source to `Overlay` so the PI step can block `niPrototypePath`.
   - Add a port of Hydra's collection predicate library over a `SceneIndex`, if openusd's `PredicateLibrary` permits it; otherwise list it as an upstream candidate.
2. **Stage scene index.**
   - Use the predicate `active && !abstract [&& loaded]`, so overs are exposed.
   - Drop the `kind` dirtying; ignore built-in prim fields.
   - Resync with `Added`, without `Removed`, for re-added prims.
   - In `__usdPrimInfo`: add `variantSelections`; drop `niPrototypeIdentity` and `stage/instancing.rs`.
   - At the root: drop `__usdStageMetadata`, `sceneGlobals/metersPerUnit` and `sceneGlobals/upAxis`; add `__usdUpAxis`.
   - Rename the render prims to `__usdRender*`; drop `instantaneousShutter`.
   - Drop `membershipExpression` from collection bindings.
   - Add `extentsHint` and `model` (geomModel) data.
3. **ExtentResolving filter** (port).
4. **PI propagation**, first in the chain.
   - Overs and everything under an instancer become unrenderable, with `niPrototypePath` blocked.
   - Copy root: `visibility=true` as an underlay; `instancedBy` as an underlay.
   - Write `__usdPrimInfo/piPropagatedPrototypes` on prototypes.
   - Hash inputs as Hydra: (instancer, propagated prototype).
   - **Remove M12** (proxy resolution and its scope).
5. **NI propagation**, after PI.
   - Layout: `/…/UsdNiPropagatedPrototypes/<bindingHash>/<__Prototype_N>/UsdNiInstancer/UsdNiPrototype`, with the enclosing root taken from `instancedBy/prototypeRoots`.
   - Hydra's binding hash, plus a binding-copy prim.
   - Flattening inside: either the propagation runs flattening per scope, or a top-level flattening stays and the binding-copy prims make it equivalent.
   - Instancer: `mask`, instance-rate primvars, `instancedBy`.
   - Prototype prims: `instancedBy` underlay; instance types blanked.
6. **DrawMode filter** inside NI, optional: `addDrawModeSceneIndex` defaults to true in Hydra. Or defer it explicitly, with a note.
7. **InstanceProxyPathTranslation** for `usdMaterialBindings`.
8. **MaterialBindingsResolving.**
   - Hydra's per-level order.
   - Membership by expression over the input index.
   - Drop the `CollectionMembership` oracle.
   - Replace dirty locators with Hydra's prefix replacement.
9. **Material networks.**
   - Add the `all` context, `config`, `interface`, `nodeTypeInfo` and `renderContextNodeIdentifiers`.
   - Delete `__materialNodes`, `__materialNodeGraph`, the OffStage connections, `MaterialNodeResolvingSceneIndex` and `streaming::unresolved`.
10. **Light linking**, a port of `HdsiLightLinkingSceneIndex`.
    - `group_N` ids; the scene-index evaluator.
    - Skip instanced geometry; `categories` on point instancers; `__dependencies`.
    - `filterLink`/`lightFilterLink` and light filters.
    - Remove it from `chain::standard`; consumers append it.
11. **RenderSettingsFlattening.** Read `__usdRender*` and emit `__dependencies`, or argue Moho's direct dirtying in the change.
12. **Merging.** Send the root always; include undefined descendants in Added; decide on the root filtering of notices; keep the ordered children.
13. **`chain::standard`** = Hydra's order. Delete `chain::merged` and `chain::input`, or reduce them to "standard over a stage" with no streaming semantics.
14. **Docs and specs.** `architecture.md` (chain, the generic data section without `__usdStageMetadata`, the Streaming section), `openusd-notes.md` (gaps: `Value::Path`, changed fields, predicate registration), ROADMAP upstream candidates.

**Streaming consequence.** Dropping the stable NI identity means chunk stages number `__Prototype_N` independently. Their `UsdNiPropagatedPrototypes/<hash>/__Prototype_1` subtrees collide under a merge. **The S0 masked-chunk streaming design cannot survive NI parity** unless each chunk is merged at its own root (not possible for chunks of one namespace) or the consumer goes back to one LoadAll stage.

## Tests likely to change (about 150 integration tests in total)

| Impact | Tests |
|---|---|
| **Deleted (about 17)** | `material_nodes.rs` (7); `streaming.rs` (4, or rewritten without side inputs; `light_links_reach_across_chunks` may survive); `instancing_chunks.rs` (1); `prototype_identity.rs` (2); the M12 tests in `point_instancing.rs` (3: `a_prototype_inside_a_native_instance_…`, `a_proxy_prototype_coming_and_going_…`, `an_authored_child_named_like_the_scope_…`) |
| **Rewritten (paths and naming)** | `native_instancing.rs` (5); `rerooting.rs` (2); `point_instancing.rs` (about 4 of the remaining 5: naming, `instance_categories_are_judged_per_instance` flips to `categories` on the point instancer) |
| **Light linking** | `light_linking.rs` (4: ids and empty-categories semantics); `collections.rs` (1) |
| **Stage and prim info** | `prim_info.rs` (`a_kind_edit_dirties_the_prim_info` inverted; `niPrototypeIdentity` gone); `stage.rs` (`inactive_and_abstract_prims_are_not_exposed` and `unloaded_prims_included`: overs); `stage_updates.rs` (1-3: Added without Removed); `scene_globals.rs` (2-3: `stage_metadata_says_what_is_authored`, `metersPerUnit`/`upAxis`) |
| **Materials** | `material.rs` (1-3: the `all` context and `interface` in names); `material_resolve.rs` (2-3: the oracle and expression fallback, per-level order); `material_binding.rs` / `collection.rs` (1-2: `membershipExpression`) |
| **Render** | `render.rs` (3) and `render_flatten.rs` (6): token renames, mostly mechanical |
| **Merging and chain** | `merging.rs` (2-3); `chain.rs` (2: order and dirty locators) |
| **New tests needed** | overs and invisible-scope prototypes; NI inside a PI prototype; an instance-level binding reaching the prototype; instance visibility mask and instance primvars; `instancedBy` on NI prototypes; extent resolving; proxy path translation; Hydra predicates in linking and binding |

**Estimate:** about 17 tests deleted, about 40-50 modified, and 10 or more new.

---

## 10. Every name and layout mismatch on the observable surface

This section takes the strictest reading: what an `hdcrust` render delegate, reading Hydra's own scene index in Solaris or usdview, would see differently from Moho.
- "PV1" means a single `SdfPath` in Hydra versus a one-element `PathVec` in Moho. openusd has no `Value::Path`; see item 6.
- Adapters I did not compare field by field are listed at the end of the section.

### Prim paths and prim types

| # | Hydra | Moho | Source |
|---|---|---|---|
| 1 | `/UsdNiPropagatedPrototypes` | `/MohoNiPropagatedPrototypes` | `tokens.rs` `PROPAGATED_ROOT`; `usdImaging/tokens.h:45` |
| 2 | `<bindingHash>` = `NoPrimvars` or `Primvars%zx`, then `_<name>%zx` per data source | `NoBindings` or `Bindings%016x` | `native_instancing.rs:262-266`; `niInstanceAggregationSceneIndex.cpp:737-796` |
| 3 | `<__Prototype_N>` (the USD prototype name) | `P<identity hex>` | `native_instancing.rs:270` |
| 4 | `…/UsdNiInstancer` (`instancer`), prototype at `…/UsdNiInstancer/UsdNiPrototype` (a **child** of the instancer) | `…/instancer`, prototype `…/prototype` (a **sibling**) | `tokens.rs` `PROTOTYPE`; `niPrototypeSceneIndex.h` tokens |
| 5 | NI scope for instances in a PI prototype = the PI copy root (`P/ForInstancerX/UsdNiPropagatedPrototypes/…`) | always `/`, or the enclosing NI prototype | item 1 |
| 6 | PI copy `P/ForInstancer%zx` from TfHash(instancer, propagatedPrototype); `%zx` is not zero-padded | `P/ForInstancer%016x` from DefaultHasher(output path) | `point_instancing.rs:171-173` |
| 7 | (no such prims) | `I/ForInstancer<h>[_n]/prototype<i>` (M12 scope and proxy copies) | `point_instancing.rs:269-317` |
| 8 | prims with specifier `over`, and their subtrees, are present but typeless | absent | `stage/mod.rs:183-189` |
| 9 | an NI instance prim with a renderable type is blanked | keeps its type | `native_instancing.rs:457,477` |
| 10 | the PI copy of a proxy target is an empty prim | a full copy | item 2 |

### `__usdPrimInfo`

| # | Hydra | Moho |
|---|---|---|
| 11 | has `variantSelections` | missing |
| 12 | has `piPropagatedPrototypes/<instancerHash>` → SdfPath | missing |
| 13 | — | `niPrototypeIdentity` (Moho-only) |
| 14 | `niPrototypePath` is an SdfPath | PV1 |
| 15 | a prim under a PI instancer or under an `over` has `niPrototypePath` blocked | not blocked |

### Root

| # | Hydra | Moho |
|---|---|---|
| 16 | `sceneGlobals` has no `metersPerUnit` or `upAxis` | adds both (`globals.rs:103-113`) |
| 17 | `__usdUpAxis/upAxis` on the root, flattened onto every prim | missing |
| 18 | — | `__usdStageMetadata/<field>/{value, authored}` (Moho-only, not sanctioned) |
| 19 | `system/assetResolution/resolverContext` | missing (no ArResolverContext in openusd; acceptable gap, document it) |
| 20 | `sceneGlobals/activeRenderSettingsPrim` is an SdfPath | PV1 |

### Instancing data

| # | Hydra | Moho |
|---|---|---|
| 21 | NI instancer `instancerTopology` has `mask` (instance visibility) | missing |
| 22 | NI instancer `primvars` hold each instance's constant primvars at `interpolation=instance` | only `hydra:instanceTransforms` |
| 23 | NI instancer has `instancedBy` when nested | missing |
| 24 | NI prototype prims carry `instancedBy {paths, prototypeRoots}` | missing |
| 25 | `instance/instancer` is an SdfPath | PV1 |
| 26 | the `<bindingHash>` prim carries a static copy of `usdMaterialBindings`, `purpose`, `model` and `__usdUpAxis` | an empty container |
| 27 | PI copy root underlays `visibility/visibility=true` | missing |
| 28 | PI copy root overlays only `xform/resetXformStack` | overrides the `xform` container, rebuilding `matrix` |
| 29 | PI `instancerTopology/prototypes` and `instanceIndices` are pruned for fully inactive prototypes (`inactiveIds` metadata, `dataSourcePointInstancer.cpp:98-125,307-380`) | not handled; not confirmed beyond grep (no `inactiveIds` in Moho) |

### Materials

| # | Hydra | Moho |
|---|---|---|
| 30 | `material/all` context | missing |
| 31 | `material/<ctx>/config` | missing |
| 32 | `material/<ctx>/interface/{parameters, parameterOrder}` | missing |
| 33 | `material/<ctx>/nodes/<n>/nodeTypeInfo` and `renderContextNodeIdentifiers` | missing |
| 34 | — | `__materialNodes` and `__materialNodeGraph` (Moho-only) |
| 35 | — | dangling `upstreamNodePath` connections to off-stage nodes (Moho-only) |
| 36 | `materialBindings/<purpose>/path` is an SdfPath | PV1 |
| 37 | `usdMaterialBindings/<purpose>[i]/collectionMaterialBindings[j]` has no `membershipExpression` | adds it |
| 38 | `materialPath` and `collectionPrimPath` are SdfPaths | PV1 |

### Lights

| # | Hydra | Moho |
|---|---|---|
| 39 | `light` lists `{filters, isLight, materialSyncMode}`; any other parameter is answered by name on `Get` (`lightAPIAdapter.cpp:40-102`) | lists every `inputs:*` (stripped) plus `poleAxis`, `treatAsPoint`, `treatAsLine` and `filters`; no `isLight` or `materialSyncMode` (`stage/light.rs:19-80`) |
| 40 | a light also carries `material`: a network with terminal `light`, the light prim as its node (`lightAPIAdapter.cpp:155-161`) | missing |
| 41 | `light/lightFilterLink` | missing |
| 42 | light filter prims | not modelled |
| 43 | `categories` is absent when empty | an empty container |
| 44 | instanced geometry has no `categories` | has them |
| 45 | a PI instancer gets `categories` | gets `instanceCategories` |
| 46 | category ids `group_<n>` | `<collection>:<expr>` |
| 47 | instancers carry `__dependencies` (`dep_<i>`) | missing |

### Render prims

| # | Hydra | Moho |
|---|---|---|
| 48 | stage-level `__usdRenderSettings`, `__usdRenderProduct`, `__usdRenderVar` | `renderSettings`, `renderProduct`, `renderVar` (`tokens.rs:310-325`) |
| 49 | — | `instantaneousShutter` in the settings base (`render.rs:37`) |
| 50 | the flattened `renderSettings` has `__dependencies` | none; Moho dirties directly |

### Curves

| # | Hydra | Moho |
|---|---|---|
| 51 | — | `nurbsCurves/pointWeights` (Moho-only; `tokens.rs:144`) |

### Dirty locators emitted

| # | Hydra | Moho |
|---|---|---|
| 52 | `kind` edit: nothing | `__usdPrimInfo/kind`, and for every prim-level info-only change |
| 53 | prim resync: `Added` only (re-adds elided) | `Removed(root)` then `Added` |
| 54 | `usdMaterialBindings` dirty is **replaced** by `materialBindings` | both, plus collection dependents |
| 55 | a light property edit dirties `light/<name>` and `material` | the whole `light` (`light.rs:36-39`) |
| 56 | light linking dirties `categories` on the targets of a changed expression only (the cache's `_InvalidateCategoriesOnTargets`), plus `instanceCategories` through dependencies | `categories` and `instanceCategories` on **every** geometry prim and instancer |
| 57 | merging: added, removed and dirtied are not filtered by input root; added includes undefined children; the root is always added | filtered, defined only, the root only when defined |
| 58 | NI: adding an instance dirties the instancer's `instancerTopology/instanceIndices` and `primvars`, and all instances' `instance` | resyncs the whole model; dirties `instance` on all instances, and `primvars/hydra:instanceTransforms` on xform edits |

**Not compared field by field** (they need the same table before implementation):
- the mesh, curves, primvars, xform, camera, volume and implicit adapters;
- their per-property invalidation locators, e.g. Hydra's `UsdImagingDataSourceMesh::Invalidate` against `stage/mesh.rs:60`;
- `geomSubset` children;
- `coordSysBinding`, `extent` and `extentsHint`, which Moho lacks.

## 11. Moho shaped around Crust and Mantle (blocks or complicates upstreaming)

| Item | Where | Why it was added | Upstream impact |
|---|---|---|---|
| `__usdStageMetadata` with `authored` | `stage/globals.rs:17-71` | Crust's scene globals (authored versus fallback) | Not Hydra; drop it, or Crust reads the stage through openusd |
| `sceneGlobals/metersPerUnit` and `upAxis` | `globals.rs:103-113` | Crust unit and axis conversion | Not Hydra; use `__usdUpAxis`; metersPerUnit has no Hydra home |
| `__usdAttributes` / `__usdRelationships`, incl. `authored`, `mightBeTimeVarying`, `crust:*` | `stage/generic.rs`, `tokens.rs:33-63` | Crust M9 and renderer attributes | The sanctioned addition, but **Hydra in Solaris will not provide it**, so hdcrust must not depend on it. Hydra's route for renderer attributes is primvars and namespaced data (e.g. `ri:` attributes via adapters). For an upstream `openusd-imaging` it needs its own justification, or should be made optional (a feature or a separate adapter) |
| `streaming` module, `chain::input` and `chain::merged` | `streaming.rs`, `chain.rs:24-55` | Crust's chunked import (S0/S2) | Not Hydra; drop it, or move it into the consumer |
| `MaterialNodeResolvingSceneIndex`, `__materialNodes`, `__materialNodeGraph`, OffStage connections | `filter/material_nodes.rs`, `stage/material.rs` | M10 for Crust streaming | Drop |
| `niPrototypeIdentity` and `stage/instancing.rs` (rebuilds openusd's private instancing key) | `prim_info.rs:44-51` | chunk agreement for Crust streaming | Drop; it is also a heavy openusd-internals workaround |
| M12 proxy-prototype resolution | `point_instancing.rs:239-317` | crust-render `moho-import` on Moana | Drop (Hydra yields empty) |
| Light-link ids from expression text, and the expression-only fallback | `light_linking.rs:141-170` | chunk agreement | Drop; port Hydra's evaluator |
| `CollectionMembership` public trait, and `membershipExpression` in bindings | `scene_index.rs:105-112`, `tokens.rs:185-187` | resolving without a stage after a merge | Not Hydra API; drop |
| `instantaneousShutter`, `pointWeights` | `render.rs:37`, `tokens.rs:144` | Crust reads them | Drop, or argue each |
| `moho::openusd` re-exports and openusd feature forwarding (`mmap`, `serde`, schema families) as "the stack's one pin" | `lib.rs:29`, root `Cargo.toml`, CLAUDE.md | Mantle, crust and Obsidian version policy | Inappropriate for a crate living inside the openusd workspace. Keep it in a downstream shim if needed |
| Relative asset-path anchoring for unresolved paths (UDIM) | `stage/attribute.rs:33-60` | Crust textures | An openusd gap workaround; it emulates USD, so it is fine, but it belongs upstream in openusd's resolver |
| Thread-confined `Rc` data sources | `docs/architecture.md` "Model" | the `!Send` openusd stage | Not a parity issue for data, but Hydra's data sources are thread-safe; note it for upstream API review |
| Docs and comments naming Crust, Mantle, Moana and gap ids (M8-M12) | architecture.md, module docs, specs | provenance | Rewrite for an upstream audience |

**Consequence for goal (2).** An hdcrust delegate in Solaris or usdview reads C++ Hydra, which has none of the Moho-only data above.
- Crust's import must be written only against Hydra's surface (section 10).
- On Moana it will hit the M12 gap: empty coral prototypes under UsdImaging 2.0.
- Crust will also need to handle `instancedBy`, `mask`, instance-rate primvars and the `material` light networks, which Moho does not emit today.

# Architecture

Moho is a renderer-agnostic scene index over openusd stages, in the model of
Hydra 2.0. It sits between a stage and its renderers: whatever authors the
stage (a DCC mirroring its edits, or nothing at all for a file a batch
renderer opens) feeds it, and renderers consume it as a Hydra 2.0 render
delegate consumes Hydra's terminal scene index.

```
  stage (openusd) --> StageSceneIndex --> chain::standard --> renderer filters
                                                                     |
            render delegate <-- managing observer <-- prim type batching
```

This page records the decisions that bind Moho itself; provenance lives in
the archived OpenSpec changes.

## Principle: Hydra parity

Moho is UsdImaging's and Hydra's behaviour at the reference revision, quirks
included: the chain order, prim paths, schema and token names, data layout
under each locator, and dirty locators. Behaviour Hydra lacks is dropped.
The one addition is the generic attribute data (below). A renderer that
needs a fix over Hydra ships it as a renderer filter of its own (as
arnold-usd ships `fixInvalidationSIP` and the like); Moho does not fold it
in. When the reference revision moves and Hydra fixes a quirk, Moho follows
in a change of its own.

## Model (Hydra 2.0)

```
SceneIndex: prim(path) -> Prim { ty, data: Option<Container> }, children(path),
            observers: added(path, ty) | removed(path) | dirtied(path, LocatorSet) | renamed
Container:  names(), get(name) -> DataSource
Vector:     len(), get(i) -> DataSource
Sampled:    value(shutter_offset) -> sdf::Value, sample_times(interval) -> Vec<f32>
Locator:    a path of tokens ("primvars/points/primvarValue")
schemas:    typed accessors over containers (MeshSchema, XformSchema, ...)
```

- **Lazy and thread-confined.** Data sources read the stage on demand and
  are thread-confined (`Rc`): a `usd::Stage` is `!Send`. A consumer pulls on
  the thread that owns the index. Only values (`sdf::Value`, shared arrays)
  cross threads.
- **The index owns the time.** A sampled data source is read relative to it.
  `value(0.0)` is the value at the index's time, and a shutter offset reads
  around it for motion blur. Changing the time dirties exactly the locators
  whose data might vary over time. As `UsdImagingStageSceneIndex`'s, a new
  index is at time code 0 (not the stage's default time), and replacing a
  held stage resets it to `EarliestTime`.
- **Paths are path arrays.** openusd's `sdf::Value` has no single-path
  variant, so a Hydra field typed as one `SdfPath` (a resolved binding's
  `path`, `instance/instancer`, `__usdPrimInfo/niPrototypePath`,
  `sceneGlobals/activeRenderSettingsPrim`, a dependency's
  `dependedOnPrimPath`) is a one-element `Value::PathVec`, absent where
  Hydra's path would be empty. A field Hydra types as a path array is a
  `PathVec` of any length.
- **Locators are token arrays.** A field Hydra types as a data source
  locator (`__dependencies`' locators) is a `Value::TokenVec` of its
  elements (`schema::locator_value`).
- **The renderer decides.** Dirty locators say what changed. Whether to
  update in place is the consumer's call, and it refuses what it cannot
  prove.

## Chain

A Hydra 2.0 renderer consumes Hydra in three steps; Moho offers each.

1. **The standard chain.** `moho::chain::standard(stage_index)` builds
   exactly the filters `UsdImagingCreateSceneIndices` builds with
   `addDrawModeSceneIndex = false`, `displayUnloadedPrimsWithBounds = false`,
   no overrides callback and no UsdImaging scene index plugins, in its order:

   ```
   StageSceneIndex --> ExtentResolving --> PointInstancing (propagation)
     --> NativeInstancing (propagation, flattening inside)
     --> InstanceProxyPathTranslation (usdMaterialBindings)
     --> MaterialBindingResolving --> RenderSettingsFlattening
   ```

   It leaves out what changes nothing it exposes or notifies: the locator
   caching scene index on `material` (a cache), the notice batching scene
   index (a pass-through until a client enables batching) and the selection
   scene index (nothing until something is selected). Several chains may be
   merged (`MergingSceneIndex`), as a render index merges its inputs.
2. **Renderer filters.** `moho::chain::renderer_filters(index, &filters)`
   applies a consumer's `RendererFilter { phase, order, build }` list in
   `HdSceneIndexPluginRegistry`'s order: phase, then `AtStart` before
   `AtEnd`, then list position (Hydra's plugin id). There is no global
   registry: the consumer knows its renderer's filters. Moho ships two, not
   applied by default: light linking (`RendererFilter::light_linking`, phase
   4 `AtStart`, as hdPrman and arnold-usd register it) and dependency
   forwarding (`RendererFilter::dependency_forwarding`, phase 1000 `AtEnd`,
   as arnold-usd does). Without light linking no prim has `categories`;
   without dependency forwarding nothing is dirtied through `__dependencies`
   (render settings, instance categories).
3. **The delegate.** As hdPrman's `renderDelegate.cpp`, the delegate wraps
   the terminal index in `PrimTypeNoticeBatchingSceneIndex`
   (`HdsiPrimTypeNoticeBatchingSceneIndex`: notices held until `flush`,
   consolidated, removals first, then added and dirtied entries by the
   priority a `PrimTypePriority` gives their prim types) and observes it
   with `moho::prim_managing::PrimManagingObserver`
   (`HdsiPrimManagingSceneIndexObserver`: a `PrimFactory` makes a managed
   prim per added prim, dirtied entries reach it with their locators,
   removals release it). Its `Update` is `flush`. The chain module's docs
   run the three steps.

Update latency ranks above first-load time: an edit dirties exactly what
Hydra dirties (a camera edit its camera at `camera/<field>` or `xform`, a
light with trivial collections nothing but its own addition), and
`crates/moho-spike`'s `update` benchmark times edits up to the managing
observer (`docs/update-latency.md`).

- **`StageSceneIndex`** has `UsdImagingStageSceneIndex`'s shape: built
  empty (`new`), then `set_stage`, `set_time` and `apply_pending_updates`.
  - It traverses with UsdImaging's predicate: active, not abstract, and
    loaded unless unloaded prims are included. A prim need not be defined,
    so `over`s and their descendants are prims of the index; the point
    instancing filter makes what lies under them unrenderable.
  - It observes its stage through openusd's `StageSink`, so any author of
    the stage (a DCC mirror replaying `usd::Diff`s, or nothing at all for a
    file opened by a batch renderer) produces the same notices.
  - Notices are queued and become added, removed and dirtied entries only in
    `apply_pending_updates`. A resynced subtree is notified added, and only
    the prims that are not re-added are notified removed. A prim-level
    info-only change (`kind`, stage metadata) notifies nothing, as
    UsdImaging ignores built-in fields (openusd does not report the changed
    fields, so the resync UsdImaging does for a plugin field is missed).
  - Per-type adapters build each prim's data and map a changed property to
    the locators it dirties, as UsdImaging's data sources and `Invalidate`.
  - Open stages with `moho::stage::stage_builder()`, which installs
    openusd-schemas' registry, so schema fallbacks and typed views work.
  - A material represents its descendants, as UsdImaging's material adapter
    (`RepresentsSelfAndDescendents`): population and `children` do not
    reach its shaders and node graphs (`prim` answers them, typeless, as
    UsdImaging's `GetPrim`), and an edit to one dirties the material (see
    "Materials and lights"). A prim of another type resynced under a
    material is populated, as UsdImaging does.
- **`ExtentResolving`** gives a prim with no `extent` of its own one from
  its `extentsHint` (purposes `geometry`, `render`, `proxy`).
- **`PointInstancing`** is `UsdImagingPiPrototypePropagatingSceneIndex`.
  - A whole-stage pass makes every prim strictly under an instancer or an
    `over` unrenderable: an empty type unless it is a material,
    `__usdPrimInfo/niPrototypePath` blocked and `geomModel/applyDrawMode`
    false.
  - Each prototype `P` an instancer targets is copied to
    `P/ForInstancer<hash>`, `<hash>` being `TfHash::Combine` of the
    instancer's path in the prototype's input namespace and the enclosing
    copy root (`/` at the top level), printed `%zx` (a port of `TfHash`
    gives Hydra's digits). The instancer's `instancerTopology/prototypes`
    names the copies, and the original prototype records them in
    `__usdPrimInfo/piPropagatedPrototypes/ForInstancer<hash>`.
  - A copy's root resets the transform stack (an overlay) and is visible
    unless it says otherwise (an underlay); every prim of the copy is
    `instancedBy` the instancer and the copy root (an underlay); its path
    values are re-rooted to the copy.
  - Each copy searches for instancers of its own, so a nested instancer is
    re-instanced in every copy, its own copies made at its prototype's
    input path and named from the enclosing copy root.
  - A target inside a native instance (an instance proxy) copies as an
    empty prim, as in UsdImaging 2.0.
- **`NativeInstancing`** is `UsdImagingNiPrototypePropagatingSceneIndex`, run
  after point instancing.
  - The stage without its USD prototypes, instances' renderable types
    blanked, is flattened (`HdFlatteningSceneIndex` with UsdImaging's
    providers); the instance aggregation reads the flattened data.
  - Instances sharing their enclosing prototype root (a point instancer
    copy, a native prototype, or `/`), USD prototype, constant primvar names
    and roles, and flattened `usdMaterialBindings`, `purpose`, `geomModel`,
    `model` and `__usdUpAxis` (re-rooted to `/<prototype name>`) share an
    instancer at
    `<root>/UsdNiPropagatedPrototypes/<bindingHash>/__Prototype_N/UsdNiInstancer`.
    `<bindingHash>` is `NoPrimvars` or `Primvars<TfHash>`, then
    `_<name><hash>` per instance data source (those digits are Moho's
    own); its prim carries a static copy of the group's instance data.
  - The instancer has `instancerTopology` (`prototypes`, `instanceIndices`,
    `instanceLocations` in path order, `mask` from each instance's
    flattened visibility), `primvars` (`hydra:instanceTransforms` and every
    constant primvar of the instances, instance-rate) and `instancedBy`
    when nested; each instance has `instance`.
  - The USD prototype is propagated under the instancer at
    `.../UsdNiInstancer/UsdNiPrototype`, its root given the group's
    instance data, an identity transform resetting the stack and
    `instancedBy`, its other prims `instancedBy` as an underlay, then
    flattened and aggregated in turn, so nested instances work at any
    depth. An optional append callback runs on each flattened prototype
    (UsdImaging adds its draw mode scene index there).
- **`InstanceProxyPathTranslation`** translates a path in
  `usdMaterialBindings` that names a prim inside a native instance to the
  same prim in the instance's propagated prototype.
- **Draw mode is deferred.** `UsdImagingDrawModeSceneIndex` replaces
  geometry with standins (bounds, cards, origin) for viewport level of
  detail. It is optional in Hydra's own API, a render delegate under usdview
  or Solaris receives it already applied, and its card generation is a
  sizeable port of its own. The stage index and the flattening carry
  `geomModel` and native propagation takes the append callback, so adding
  it later changes no layout.
- **`Flattening`** (`FlatteningSceneIndex`) resolves inherited data: the
  world transform, visibility, purpose, constant primvars, binding
  opinions, `model`, `geomModel` and `__usdUpAxis`. It runs inside native
  instancing, and stays a public filter.
- **The resolving filters** turn binding opinions and the stage's
  `__usdRender*` data into `materialBindings` and `renderSettings`.
  - Material binding resolving is `UsdImagingMaterialBindingsResolvingSceneIndex`.
    Per purpose, walking the flattened opinions from the root down, at each
    level: a matching stronger-than-descendants collection binding wins and
    stops; else a stronger-than-descendants direct binding wins and stops
    (even over a weaker matching collection binding); else a matching
    collection binding, else the direct binding, wins for now. A collection
    matches when the collection prim's
    `collections/<name>/membershipExpression`, read from the filter's input
    and judged over it with Hydra's predicates (`moho::collection`), matches
    the prim's path. Only a prim with `usdMaterialBindings` is given
    `materialBindings` (every prim names it), and a dirtied
    `usdMaterialBindings` is forwarded as `materialBindings`.
  - Nothing re-resolves a binding when a collection elsewhere changes:
    Hydra tracks no such dependency. A `collection:*` edit on a prim
    applying `MaterialBindingAPI` dirties its opinions, which flattening
    carries to its descendants.
- **`MergingSceneIndex`** is `HdMergingSceneIndex`: several inputs over one
  namespace (`add_input_scene(index, root)`, `remove_input_scene`), earlier
  inputs stronger. Adding an input notifies its root and every descendant it
  lists, defined or not (not the root's strict ancestors: Hydra's test is
  inverted); removing one walks its prims, re-adding what another input
  holds. Inputs' notices are forwarded without filtering by root, an added
  entry with its merged type. A single input answers every path. Children
  come in no promised order.
- **Every prim carries `primOrigin/scenePath`**, Hydra's prim origin: the
  USD prim's path, relative to its prototype root inside a USD prototype.
  The instancing propagations keep it on the prims they re-root.
- **Blocks.** `DataSource::Block` is Hydra's `HdBlockDataSource`: in an
  `Overlay` (`HdOverlayContainerDataSource`) it hides what weaker containers
  give at its name.

### Renderer filters

- **`LightLinkingSceneIndex`** is `HdsiLightLinkingSceneIndex` and its
  cache. It reads lights' `lightLink` and `shadowLink` and light filters'
  `filterLink` and `shadowLink` from `collections/<name>/membershipExpression`
  and judges them over its input with Hydra's predicates
  (`moho::collection`). Equal non-trivial expressions share a category id
  `group_<n>`; a trivial one (everything, or everything but properties) has
  the empty id and is not tracked. A light or filter that transports the
  collection gives the id at `light/lightLink`, `light/shadowLink` or
  `light/lightFilterLink`. Geometry no instancer instances gets
  `categories` (absent when empty); a native instancer gets
  `instanceCategories` per instance location and `__dependencies` on the
  instances' `categories`; a point instancer gets `categories` judged at its
  own path. A changed collection dirties the matches of its old and new
  expressions at `categories` and its light at `light/<collection name>`.
  Built over a populated input, it reads its lights first, as a first
  population would.
- **`DependencyForwardingSceneIndex`** is `HdDependencyForwardingSceneIndex`.
  A prim's `__dependencies` (`HdDependenciesSchema`: `dependedOnPrimPath`,
  `dependedOnDataSourceLocator`, `affectedDataSourceLocator`) count once the
  prim has been read through it. A dirtied prim dirties its dependents at
  their affected locators, transitively, each (prim, locator) once per
  notice; a depended-on prim removed or added again dirties its dependents.
  Tracked dependencies are answered as a block.

### The absolute root

The absolute root carries what `UsdImagingDataSourceStage` gives:
`sceneGlobals` (`startTimeCode`, `endTimeCode`, `timeCodesPerSecond`, and
`activeRenderSettingsPrim` when the stage authors a non-empty
`renderSettingsPrimPath`) and `__usdUpAxis/upAxis`. UsdImaging's
`system/assetResolution` is left out: openusd has no resolver context to
give. A consumer that needs other stage metadata (`metersPerUnit`, or
whether a field is authored) reads it from the stage it opened, through
`moho::openusd`.

### Materials and lights

A material's `material` is `UsdImagingDataSourceMaterial`'s: one network per
namespace of its authored outputs (`""` for `outputs:surface`, `mtlx` for
`outputs:mtlx:surface`), and always `__all`.
- A named context holds the nodes the shader-output walk reaches from its
  terminals (node graphs' outputs and interface inputs followed, a
  connection the stage cannot follow dropped), its terminals named without
  the context. `__all` holds every `Shader` under the material, connected
  or not, its terminals keeping their namespaced names.
- Each network has `nodes` (named by path relative to the material),
  `terminals`, `interface` (each material input's `displayName`,
  `displayGroup` and `mappings`, the node inputs it feeds; and
  `parameterOrder`) and `config` (the `config:*` attributes).
- Each node names `parameters`, `inputConnections`, `nodeIdentifier`,
  `renderContextNodeIdentifiers` (lights only) and `nodeTypeInfo` (outside
  `id` mode: `sdrMetadata` and the other `info:*` attributes). openusd has
  no shader registry, so a node outside `id` mode has an empty
  `nodeIdentifier`, as UsdImaging gives it when Sdr finds none.
- Dirtying is the material adapter's: a shader edit dirties the terminals
  whose connections reach it and the shader's node (by its name) in that
  context and `__all`, else the whole `material`; an input or output edit on
  the material dirties `material`, a `config:*` edit nothing; a removed
  shader notifies nothing.

Light filters (`LightFilter` and the types derived from it) are UsdImaging's
light filter adapter: type `lightFilter`, and `material`, the filter as the
one node, by its absolute path, feeding the terminal `lightFilter`,
identified by `<context>:lightFilter:shaderId`. Like a material, a filter
represents its descendants. It has no `light` of its own (light linking gives
it one); its collections come from the collection adapter.

Lights are UsdImaging's light API adapter. `light` names `filters`,
`isLight` and `materialSyncMode` and answers any other name as
`LookupLightParamAttribute` (Hydra's tokens read their `inputs:` attribute,
other names the attribute itself); dome lights add `portals`, and
`DomeLight_1` `domeOffset`. Every light has `material`: the light prim as
the one node, by its absolute path, feeding the terminal `light`, in the
contexts `""`, `__all` and each `<context>:light:shaderId` names. An
`inputs:*` edit dirties `light` and `material`, a `light:*` edit `light`.

### Render prims

Render settings, products and vars expose UsdImaging's `__usdRenderSettings`,
`__usdRenderProduct` and `__usdRenderVar`; a pass exposes `renderPass`. The
render settings flattening filter gives a settings prim `renderSettings`
with `HdRenderSettingsSchema`'s fields, its products and their vars
flattened into `renderProducts`, and declares `__dependencies` as UsdImaging
does (each flattened field on its `__usdRenderSettings` field,
`renderSettings/renderProducts` on each product and var, `__dependencies`
on `__usdRenderSettings/products`). It dirties nothing itself: the settings
are dirtied through dependency forwarding.

### Generic attribute data, Moho's one addition to Hydra

Every prim also exposes `__usdAttributes` and `__usdRelationships`. They
carry three things Hydra's schemas do not:
- each value, and whether a layer authors it or it is a schema fallback;
- the metadata a renderer reads (`colorSpace`, `interpolation`,
  `elementSize`);
- every renderer attribute (`myRenderer:*`) without a schema of its own.

This is the only data Moho adds to UsdImaging's. The leading underscores
follow Hydra's convention for private containers (`__usdPrimInfo`).

## Boundaries

- Moho depends on **openusd only**. It holds no consumer's type. Adapters
  (a DCC's changes to a mirror stage, a renderer's import) live in the
  consumers.
- **One openusd for the stack.** Consumers take openusd only through
  `moho::openusd` and `moho::openusd_schemas`, and turn openusd features on
  through Moho's (`mmap`, `serde`, and the schema families Moho does not
  read). One pin guarantees one openusd in every build. This is a downstream
  policy that changes no Hydra behaviour: if Moho moves into openusd's
  workspace (as an `openusd-imaging` crate), the re-exports move to a thin
  downstream crate, or consumers depend on openusd at the same rev.
- Moho is shaped to be offered upstream as that crate once its API has
  settled.

## Moho does not stream

Moho reads one stage, as UsdImaging does: it has no chunked inputs, no
merged chain and no resolution of material nodes across inputs. Those
(`streaming`, `chain::input`, `chain::merged`,
`MaterialNodeResolvingSceneIndex`) were removed for Hydra parity: Hydra has
no counterpart for population-masked chunk stages merged after the fact,
and native instancing names prototypes by their name on one stage.
Chunking a scene, and finding and loading what a chunk depends on, is the
consumer's business; `MergingSceneIndex` stays as a general filter.

The S0 spike measured streaming strategies (masked stages per chunk against
payload `load_and_unload` on one stage); its numbers and reasoning are in
`streaming-spike.md`. Its strategy (D) now reads one standard chain over a
`LoadAll` stage, whole: the benchmark of Moho's per-prim cost against direct
reading.

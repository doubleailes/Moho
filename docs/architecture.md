# Architecture

Moho is a renderer-agnostic scene index over openusd stages, in the model of
Hydra 2.0. It sits between the stage and the renderers: Mantle (the DCC) feeds
it, and Crust (the path tracer) and Obsidian (the GPU viewport) consume it.

```
  openusd (core) --> Moho (scene index) --> Crust (renderer)
        ^                ^             \--> Obsidian (viewport)
        |                |
       Mantle (DCC: graph, cook, session; its IPR owns a Moho index)
```

The program that introduces Moho is planned in Mantle's OpenSpec change
`scene-index` (`doubleailes/mantle`, archived in
`openspec/changes/archive/2026-10-25-scene-index/`). This page records the
decisions that bind Moho itself.

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
  whose data might vary over time.
- **Paths are path arrays.** openusd's `sdf::Value` has no single-path
  variant, so a Hydra field typed as one `SdfPath` (a resolved binding's
  `path`, `instance/instancer`, `__usdPrimInfo/niPrototypePath`,
  `sceneGlobals/activeRenderSettingsPrim`) is a one-element
  `Value::PathVec`, absent where Hydra's path would be empty. A field Hydra
  types as a path array is a `PathVec` of any length.
- **The renderer decides.** Dirty locators say what changed. Whether to
  update in place is the consumer's call, and it refuses what it cannot
  prove.

## Chain

`moho::chain::standard(stage_index)` builds the standard chain:

```
StageSceneIndex --> ExtentResolving --> NativeInstancing --> PointInstancing
  --> Flattening --> MaterialBindingResolving --> RenderSettingsFlattening
  --> LightLinking
```

It is not yet `UsdImagingCreateSceneIndices`' order. The OpenSpec changes
`hydra-parity-instancing` and `hydra-parity-chain` put point instancing
before native instancing, run flattening inside native instancing, and move
light linking out of the chain (a Hydra renderer appends it as a scene
index plugin).

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
- **`ExtentResolving`** gives a prim with no `extent` of its own one from
  its `extentsHint` (purposes `geometry`, `render`, `proxy`).
- **`NativeInstancing`** turns native instances into instancers over
  prototypes re-rooted under `MohoNiPropagatedPrototypes` (Hydra's layout
  comes with `hydra-parity-instancing`).
- **`PointInstancing`** re-roots a point instancer's prototypes under
  `<prototype>/ForInstancer<hash>`, and, as UsdImaging's whole-stage
  prototype pass, makes every prim strictly under an instancer or an `over`
  unrenderable: an empty type unless it is a material,
  `__usdPrimInfo/niPrototypePath` blocked and `geomModel/applyDrawMode`
  false.
- **`Flattening`** resolves inherited data: the world transform,
  visibility, purpose, constant primvars, and binding opinions.
- **The resolving filters** turn binding opinions, light-link collections
  and the stage's `__usdRender*` data into `materialBindings`, categories
  and `renderSettings`. Collection membership comes from the stage index,
  through `CollectionMembership`, until the later changes judge collection
  expressions over the scene index (`moho::collection`, Hydra's evaluator
  and predicate library).
- **`MergingSceneIndex`** combines several inputs over one namespace
  (`add_input_scene(index, root)`, `remove_input_scene`). It lists children
  in input order, strongest first, then the prims implied by deeper roots;
  Hydra's gives no order.
- **Every prim carries `primOrigin/scenePath`**, Hydra's prim origin: the
  USD prim's path, relative to its prototype root inside a USD prototype.
  The instancing filters keep it on the prims they re-root.
- **Blocks.** `DataSource::Block` is Hydra's `HdBlockDataSource`: in an
  `Overlay` (`HdOverlayContainerDataSource`) it hides what weaker containers
  give at its name.

### The absolute root

The absolute root carries what `UsdImagingDataSourceStage` gives:
`sceneGlobals` (`startTimeCode`, `endTimeCode`, `timeCodesPerSecond`, and
`activeRenderSettingsPrim` when the stage authors a non-empty
`renderSettingsPrimPath`) and `__usdUpAxis/upAxis`. UsdImaging's
`system/assetResolution` is left out: openusd has no resolver context to
give. A consumer that needs other stage metadata (`metersPerUnit`, or
whether a field is authored) reads it from the stage it opened, through
`moho::openusd`.

### Render prims

Render settings, products and vars expose UsdImaging's `__usdRenderSettings`,
`__usdRenderProduct` and `__usdRenderVar`; a pass exposes `renderPass`. The
render settings flattening filter gives a settings prim `renderSettings`
with `HdRenderSettingsSchema`'s fields, its products and their vars
flattened into `renderProducts`.

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

- Moho depends on **openusd only**. It holds no Mantle type (`CookedStage`,
  `ChangeSet`) and no Crust or Obsidian type. Adapters live in the consumers.
- **Moho is the stack's openusd.** Its consumers take openusd only through
  `moho::openusd` and `moho::openusd_schemas`, and turn openusd features on
  through Moho's (`mmap`, `serde`, and the schema families Moho does not
  read). One pin guarantees one openusd in every build, where a `[patch]`
  per consumer and a check used to: in OpenUSD, imaging and core ship as one
  build, and Moho plays that role for a pure-Rust stack.
- Moho is shaped to be offered upstream (as an `openusd-imaging` crate) once
  its API has settled.

## Moho does not stream

Moho reads one stage, as UsdImaging does: it has no chunked inputs, no
merged chain and no resolution of material nodes across inputs. Those
(`streaming`, `chain::input`, `chain::merged`,
`MaterialNodeResolvingSceneIndex`) were removed for Hydra parity: Hydra has
no counterpart for population-masked chunk stages merged after the fact,
and native instancing's prototype names cannot agree between chunk stages.
Chunking a scene, and finding and loading what a chunk depends on, is the
consumer's business; `MergingSceneIndex` stays as a general filter.

The S0 spike measured streaming strategies (masked stages per chunk against
payload `load_and_unload` on one stage); its numbers and reasoning are in
`streaming-spike.md`. Its strategy (D) now reads one standard chain over a
`LoadAll` stage, whole: the benchmark of Moho's per-prim cost against direct
reading.

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
- **The renderer decides.** Dirty locators say what changed. Whether to
  update in place is the consumer's call, and it refuses what it cannot
  prove.

## Chain

`moho::chain::standard(stage_index)` builds the standard chain, in
`UsdImagingCreateSceneIndices`' order, with light linking last, as Hydra
appends `HdsiLightLinkingSceneIndex` after scenes are merged:

```
StageSceneIndex --> NativeInstancing --> PointInstancing --> Flattening
  --> MaterialBindingResolving --> RenderSettingsFlattening      (chain::input)
  --> LightLinking                                               (chain::merged)
```

`chain::input(stage_index)` is the chain without light linking, what each
input of a streamed scene runs; `chain::merged(index)` is light linking over
a merging index of input chains (Streaming, below).

- **`StageSceneIndex`** has `UsdImagingStageSceneIndex`'s shape: built
  empty (`new`), then `set_stage`, `set_time` and `apply_pending_updates`.
  - It observes its stage through openusd's `StageSink`, so any author of
    the stage (a Mantle mirror replaying `usd::Diff`s, or nothing at all for
    a file opened by `crust render`) produces the same notices.
  - Notices are queued and become added, removed and dirtied entries only in
    `apply_pending_updates`.
  - Per-type adapters build each prim's data and map a changed property to
    the locators it dirties.
  - Open stages with `moho::stage::stage_builder()`, which installs
    openusd-schemas' registry, so schema fallbacks and typed views work.
- **`NativeInstancing`** turns native instances into instancers over
  prototypes re-rooted under `MohoNiPropagatedPrototypes`.
  - Prototypes are named by openusd's instancing key, rebuilt with
    stage-independent layer names, so chunks agree on them.
- **`PointInstancing`** re-roots a point instancer's prototypes under
  `<prototype>/ForInstancer<hash>`.
- **`Flattening`** resolves inherited data: the world transform,
  visibility, purpose, constant primvars, and binding opinions.
- **The resolving filters** turn binding opinions, light-link collections
  and render products into `materialBindings`, categories and
  `renderSettings`.
  - Collection membership comes from the stage index, through
    `CollectionMembership`. After a merge, light linking judges the lights'
    complete membership expressions instead (path patterns, no predicates).
- **`MergingSceneIndex`** combines several inputs over one namespace
  (`add_input_scene(index, root)`, `remove_input_scene`). It lists children
  in input order, strongest first, then the prims implied by deeper roots;
  Hydra's gives no order.
- **Every prim carries `primOrigin/scenePath`**, Hydra's prim origin: the
  USD prim's path, relative to its prototype root inside a USD prototype.
  The instancing filters keep it on the prims they re-root.

### Generic attribute data, Moho's one addition to Hydra

Every prim also exposes `__usdAttributes` and `__usdRelationships`.
Crust needs three things Hydra's schemas do not carry, so these containers
provide them:
- each value, and whether a layer authors it or it is a schema fallback;
- the metadata a renderer reads (`colorSpace`, `interpolation`,
  `elementSize`);
- every renderer attribute (`crust:*`) without a schema of its own.

The absolute root carries the same for the stage's metadata,
`__usdStageMetadata/<field>/{value, authored}`, for the fields the scene
globals read (time range and rates, metrics, the active render settings):
Hydra's `sceneGlobals` gives values only.

The leading underscores follow Hydra's convention for private containers
(`__usdPrimInfo`).

## Boundaries

- Moho depends on **openusd only**. It holds no Mantle type (`CookedStage`,
  `ChangeSet`) and no Crust or Obsidian type. Adapters live in the consumers.
- Moho is shaped to be offered upstream (as an `openusd-imaging` crate) once
  its API has settled.

## Streaming (decided in S0)

Moho streams the way Crust does, with one population-masked `LoadAll` stage
per chunk (Crust's `subtree_roots`, from 4 chunks up). It does so in Hydra's
way, by composing standard indices rather than giving `StageSceneIndex` a
streaming mode:

- the weakest, permanent input is a `chain::input` over the scene opened
  `LoadNone`, with `set_include_unloaded_prims(true)` so each chunk's root
  is present: render settings, products, cameras, passes and everything
  else outside the payloads;
- each chunk is a `chain::input` over its own masked stage, a stronger
  input of the same `MergingSceneIndex`, and adding or removing one adds or
  removes that chunk's payload contents;
- `chain::merged` puts light linking over the merging index, so a receiver
  in one chunk gets the categories of lights in any input;
- a prim can depend on prims no loaded input holds: a material bound from
  inside another chunk's payload, or a network node outside the material.
  `streaming::unresolved(index)` lists them; the consumer adds a side input
  (a `chain::input` over a stage masked to them, payloads loaded), weaker
  than the chunks and stronger than the scene input;
- the consumer decides when a chunk comes and goes: an offline import adds,
  pulls and removes each in turn, and a viewport keeps what it shows.

Below 4 chunks, one standard chain over one `LoadAll` stage serves the scene.
On the S0 fixture, the merged index exposes exactly what one chain over the
fully loaded stage does (`crates/moho/tests/streaming.rs`).

Payload `load_and_unload` on a single stage was rejected. On Moana its peak
was 2.56× the masked stages' (5.05 against 1.97 GiB), because unloading does
not give back memory. The numbers, the notice probe and the reasoning are in
`streaming-spike.md`.

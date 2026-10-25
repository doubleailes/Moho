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
`scene-index` (`doubleailes/mantle`, `openspec/changes/scene-index/`). This
page records the decisions that bind Moho itself.

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

```
stage --> StageSceneIndex --> FlatteningSceneIndex --> (instancing, material resolve, ...)
          local data, lazy,    world xform, inherited visibility / purpose,
          time, change          resolved material binding
          notices -> dirty
```

`StageSceneIndex` has `UsdImagingStageSceneIndex`'s shape: built empty
(`new`), then `set_stage`, `set_time` and `apply_pending_updates`. It
observes its stage through openusd's `StageSink`, so any author of the stage
(a Mantle mirror replaying `usd::Diff`s, or nothing at all for a file opened
by `crust render`) produces the same notices. Notices are queued and become
added, removed and dirtied entries only in `apply_pending_updates`, as in
Hydra. Several inputs combine under a `MergingSceneIndex`
(`add_input_scene(index, root)`, `remove_input_scene`).

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

- each chunk is a `StageSceneIndex` over its own masked stage;
- a `MergingSceneIndex` combines them, and adding or removing an input adds
  or removes that chunk's prims;
- the consumer decides when a chunk comes and goes: an offline import adds,
  pulls and removes each in turn, and a viewport keeps what it shows.

Below 4 chunks, one stage index over one `LoadAll` stage serves the scene.

Payload `load_and_unload` on a single stage was rejected. On Moana its peak
was 2.56× the masked stages' (5.05 against 1.97 GiB), because unloading does
not give back memory. The numbers, the notice probe and the reasoning are in
`streaming-spike.md`.

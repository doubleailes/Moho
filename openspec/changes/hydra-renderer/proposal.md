## Why

Hydra 2.0 names the consumer of a terminal scene index: `HdRenderer`,
built by `HdRendererPlugin::CreateRenderer(sceneIndex, rendererCreateArgs)`
after the application has appended the renderer's scene index plugins
(`UsdImagingGLEngine::_CreateSceneIndexChainAndRenderer`). Moho offers the
three steps (`chain::standard`, `chain::renderer_filters`, then
`PrimTypeNoticeBatchingSceneIndex` with `PrimManagingObserver`) but no type
that ties them together. Each consumer (crust-render, Obsidian) wires the
batching, the observer, its filter list and its `Update` by hand, as the
`chain` module's example does. The host also has no single object to hold
and update, which is what `UsdImagingGLEngine` holds as `_renderer`.

Porting the part of the API that Hydra has settled removes that wiring and
gives Moho's consumers Hydra's shape. Hydra has not settled the render
control side: `HdRenderer` is a stub whose only method returns
`HdLegacyRenderControlInterface`. That side stays out of this change.

## What Changes

- A `Renderer` trait, the counterpart of `HdRenderer`. It is the object a
  plugin creates over a terminal scene index. It has one method, `update`,
  the Hydra 2.0 hook `HdRenderDelegate::Update` (called before sync to
  process the terminal index's notices).
- A `RendererPlugin` trait, the counterpart of `HdRendererPlugin`'s Hydra
  2.0 API. `is_supported(args)` gives a reason when it answers no.
  `renderer_filters()` gives the renderer's scene index plugins, which
  Hydra registers per renderer in `HdSceneIndexPluginRegistry`.
  `create_renderer(terminal, args)` returns the renderer.
- Renderer create args, the counterpart of `HdRendererCreateArgsSchema`: a
  container with `gpuEnabled`. The schema's `drivers` (Hgi handles) are
  left out because openusd has no Hgi.
- `chain::create_renderer(input, host_filters, plugin, args)`, the
  counterpart of `_CreateSceneIndexChainAndRenderer`. It checks support,
  applies the host's and the plugin's renderer filters in one
  `HdSceneIndexPluginRegistry` order, and creates the renderer over the
  terminal index. It returns the terminal index with the renderer.
- `PrimManagingRenderer`, a ready-made `Renderer` that composes
  `PrimTypeNoticeBatchingSceneIndex` and `PrimManagingObserver` as hdPrman's
  `_RileySceneIndices` does. `update` flushes the batching.
- Not part of this change: the render control side (execute, AOVs and
  render buffers, render settings, pause, stop and restart, commands,
  stats), scene index create args (`HdSceneIndexCreateArgsSchema`: no Moho
  filter takes any), a plugin registry, and plugin discovery. Moho has no
  global registry. The consumer names its plugin.
- No breaking change. `chain::renderer_filters`, `PrimManagingObserver`
  and the batching index keep their API, and the manual wiring keeps
  working.

## Capabilities

### New Capabilities

- `renderers`: the consumer side of a terminal scene index, in Hydra 2.0's
  shape. It covers the renderer and its `update`, the renderer plugin
  (support check, its renderer filters, creation), renderer create args,
  creating a renderer over a chain with the host's and the plugin's
  filters, and the prim-managing renderer.

### Modified Capabilities

None. The requirements for renderer filter ordering, notice batching and
prim managing are unchanged. The new capability builds on them.

## Impact

- `crates/moho/src/renderer.rs` (new): `Renderer`, `RendererPlugin`,
  `RendererCreateArgs`, `PrimManagingRenderer`. `crates/moho/src/lib.rs`
  (the module). `crates/moho/src/chain.rs` (`create_renderer`, the module
  example rewritten over it). `crates/moho/src/tokens.rs` (`gpuEnabled`).
- New public API only. crust-render and Obsidian can replace their manual
  batching, observer and filter wiring with a plugin.
- Docs: `docs/architecture.md` ("Chain", step 3, and the model).
  `ROADMAP.md` ("Later": the render control side, waiting for Hydra's
  replacement of `HdLegacyRenderControlInterface`).

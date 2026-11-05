## Context

The reference is OpenUSD at 26.11 (`pxr/imaging/hd/renderer.h`,
`rendererPlugin.{h,cpp}`, `renderDelegateAdapterRenderer.h`,
`rendererCreateArgsSchema.h`, `renderDelegate.h`'s Hydra 2.0 section) and
`pxr/usdImaging/usdImagingGL/engine.cpp`. hdPrman's native path is in
`third_party/renderman/plugin/hdPrman/renderDelegate.cpp`.

What the reference does:
- `HdRenderer` is a stub. It has a virtual destructor and
  `GetLegacyRenderControl()`, with a `TODO` to replace
  `HdLegacyRenderControlInterface` with an API of its own. The only
  subclass is `HdRenderDelegateAdapterRenderer`, which wraps a Hydra 1
  render delegate in a render index fed by back-end emulation.
- `HdRendererPlugin::CreateRenderer(sceneIndex, rendererCreateArgs)`
  raises a coding error and returns null when `IsSupported` answers no,
  then calls the virtual `_CreateRenderer`. Its default falls back to the
  adapter. `IsSupported(args, std::string *reasonWhyNot)` gives the
  reason. `GetSceneIndexCreateArgs()` gives arguments for the plugin scene
  indices.
- `UsdImagingGLEngine::_CreateSceneIndexChainAndRenderer` appends the
  renderer's scene index plugins with
  `HdSceneIndexPluginRegistry::AppendSceneIndicesForRenderer`. That covers
  the plugins registered for every renderer, including the app's scene
  globals at phase 0, and those registered for this renderer. The engine
  then calls `CreateRenderer` over the result and keeps both the terminal
  index and `_renderer`.
- `HdRendererCreateArgsSchema` has `gpuEnabled` (bool) and `drivers` (a
  container of `HdDriver` handles: Hgi and the like).
- Hydra 2.0 render delegates process notices in `HdRenderDelegate::Update()`,
  which runs at the start of `HdRenderIndex::SyncAll`. hdPrman's
  `_RileySceneIndices` is `HdsiPrimTypeNoticeBatchingSceneIndex` over the
  terminal index, observed by `HdsiPrimManagingSceneIndexObserver`, and its
  `Update` is `_noticeBatchingSceneIndex->Flush()`.

Moho already has every piece except the binding: `chain::renderer_filters`
(registry order, list position at ties), `PrimTypeNoticeBatchingSceneIndex`
(empty until its first flush) and `PrimManagingObserver` (populates from
what its index holds when made). There is no plugin registry and no
dynamic loading. A consumer names its renderer statically.

## Goals / Non-Goals

**Goals:**
- One call from a chain to a renderer, with the host's and the renderer's
  filters in Hydra's order.
- The hdPrman composition as a reusable renderer, so a consumer writes only
  its prim factory and priorities.
- The host keeps the concrete renderer type, so whatever the consumer's
  renderer offers (rendering, AOVs, settings) needs no downcast and no
  Moho API.

**Non-Goals:**
- A render control API. Hydra has not defined one.
- `HdSceneIndexCreateArgsSchema` and `GetSceneIndexCreateArgs`: no Moho
  filter takes create args. Motion blur samples are lazy, and no filter
  needs `legacyRenderDelegateInfo`.
- A registry, plugin ids, display names, and discovery by `plugInfo`.
- Adapting Hydra 1 render delegates. Moho has no `HdRenderIndex`.

## Decisions

**D1. `RendererPlugin` has an associated renderer type, not
`Box<dyn Renderer>`.** Hydra returns an `HdRenderer` pointer because plugins
load dynamically and the host only knows the base class. That is also why
the host reaches render control through `GetLegacyRenderControl()`. Moho's
consumer names its plugin at compile time. With
`type Renderer: Renderer`, `create_renderer` returns the concrete type, and
the host calls the renderer's own render API directly.
- Alternative: `Box<dyn Renderer>` plus `as_any` downcasting. This matches
  Hydra's shape but makes every host downcast to reach anything beyond
  `update`, and it buys nothing without a registry. A consumer that needs
  heterogeneous plugins can box them in its own enum or trait object.
- Cost: `RendererPlugin` is not object safe. That is acceptable because
  there is no registry to hold a list of them.

**D2. `Renderer::update(&self)` is the one method.** `HdRenderer` has none
yet. `update` is taken from the Hydra 2.0 hook `HdRenderDelegate::Update`,
which is the method a native Hydra 2.0 consumer uses to process notices.
It takes `&self` because Moho's scene indices and observers use interior
mutability on one thread (`Rc`, `RefCell`), and a renderer holding them
does the same. `Drop` releases the renderer, as destroying the
`HdPluginRendererUniqueHandle` does.

**D3. Support is checked as `Result<(), Unsupported>`.** `Unsupported`
carries the reason (`reasonWhyNot`). `chain::create_renderer` returns
`Result<CreatedRenderer<R>, CreateRendererError>` with two cases:
`Unsupported(reason)` and `Declined`. Declined covers a supported plugin
whose `create_renderer` gives `None`, the counterpart of a null
`_CreateRenderer`. Hydra raises a coding error when the check is skipped.
Moho checks it itself, so the case cannot arise, and support is checked
before any filter is built because filter constructors may have side
effects (the scene globals `on_built` callback).

**D4. Create args are a `ContainerHandle` read through
`RendererCreateArgsSchema`, following `moho::schema`'s style.** The host
builds them with `RetainedContainer`. Only `gpuEnabled` is read and offered.
`drivers` holds `HdDriver` objects, which `vt::Value` cannot carry and
openusd has no Hgi for. A renderer that needs a device takes it in its
plugin's constructor. The token `gpuEnabled` goes in `tokens.rs`.
- Alternative: a Rust struct `{ gpu_enabled: Option<bool> }`. It is simpler,
  but it would leave the data source model that every Hydra argument
  container follows, and adding fields later would be an API break instead
  of a new name.

**D5. Host filters come before the plugin's in one list, then
`renderer_filters` sorts them.** `AppendSceneIndicesForRenderer` merges the
plugins registered for every renderer with the renderer's own and orders
them all by phase and insertion order. Concatenating `host ++ plugin` and
reusing `renderer_filters`'s stable sort gives the same phase order, with
ties broken by list position (Moho's documented substitute for Hydra's
plugin-id order). A host filter therefore precedes a plugin filter at an
equal phase and order. `RendererPlugin::renderer_filters(&self)` returns a
fresh `Vec<RendererFilter>` on each call because `RendererFilter` holds a
boxed constructor and is not `Clone`.

**D6. `chain::create_renderer` returns the terminal index with the
renderer** (`CreatedRenderer { terminal, renderer }`), as the engine keeps
`_terminalSceneIndex` beside `_renderer`. The host reads the terminal root
(`sceneGlobals/currentFrame`) and builds collection queries over it. The
function lives in `chain` next to `standard` and `renderer_filters`. It
takes the input already built (the standard chain or a merge), so the host
still owns the stage index and drives `set_time` and
`apply_pending_updates`.

**D7. `PrimManagingRenderer` lives in `renderer.rs`.** It holds
`Rc<PrimTypeNoticeBatchingSceneIndex>` and `Rc<PrimManagingObserver>` and
offers `new(terminal, priority, factory)`, `update` (flush), and
`observer()` / `prim(path)`. The observer is made over the batching index,
which is empty until flushed. As a result the first `update` populates
every prim in priority order. Factories created before that update see
nothing, which is the behaviour hdPrman relies on. A plugin's
`create_renderer` typically returns `Some(PrimManagingRenderer::new(..))`
or wraps it in its own type with render methods.

**D8. Docs and the chain example.** The `chain` module example is rewritten
over a small plugin and `create_renderer`, so the three steps read as one
call. The manual composition stays documented under `PrimManagingRenderer`.
The architecture doc's "Chain" step 3 names the plugin and the renderer.

## Risks / Trade-offs

- [Hydra later defines `HdRenderer`'s API differently, for example with an
  update of its own or render control] → `update` keeps the name and
  meaning of `HdRenderDelegate::Update`, the hook Hydra uses today. When
  the reference revision gains a real `HdRenderer` API, Moho follows in a
  change of its own, as the parity principle requires.
- [The associated type departs from Hydra's polymorphic return] → it is
  an API shape, not behaviour: notices, filter order and prim management
  are Hydra's. The decision is recorded here and in the architecture doc.
- [A host that already wires batching and the observer by hand] → nothing
  is removed, and the manual path keeps working.
- [Filters with side effects (`RendererFilter::scene_globals`'s callback)
  run on every `create_renderer`] → this matches Hydra, which appends the
  plugins again for each renderer. The host recreates its scene globals
  handle with the renderer.

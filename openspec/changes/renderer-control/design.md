# Design

## Context

See proposal.md for why this change exists. The current state:

- `moho::renderer` (change `hydra-renderer`) has `Renderer { fn update(&self) }`,
  `RendererPlugin` (whose `type Renderer` is named by the plugin), and
  `PrimManagingRenderer`. `chain::create_renderer` returns
  `CreatedRenderer<P::Renderer>`, so a richer renderer type reaches the
  host without a downcast.
- Scene indices, data sources and observers are `Rc` and confined to one
  thread. Data source values are `openusd::vt::Value`, which has no
  variant for opaque host data, so pixels cannot be data source values.
- What to render is already scene data:
  - `SceneGlobalsSceneIndex` sets `activeRenderSettingsPrim`,
    `activeRenderPassPrim`, `primaryCameraPrim` and `currentFrame`.
  - `RenderSettingsFlatteningSceneIndex` gives a settings prim
    `renderSettings`, with its products and their vars flattened into
    `renderSettings/renderProducts`.
  - `RenderSettingsSchema` / `RenderProductSchema` read them, including
    `namespacedSettings`.
- The reference (OpenUSD 26.11) has nothing to follow: `HdRenderer` only
  returns `HdLegacyRenderControlInterface`. The C++ delegate API
  (`HdRenderDelegate` settings, `HdRenderPass::Execute`, `HdRenderBuffer`,
  pause/stop, commands, stats) is the vocabulary to translate when a
  consumer bridges a C++ delegate, not a shape to copy.

## Goals / Non-Goals

**Goals:**
- One host-side API that a DCC's interactive rendering, a farm command line
  and a bridge to a C++ Hydra delegate can all serve.
- The owning-thread rule stays intact, while stop, wait and frame reads
  work from any thread.
- Progress is observed through the same observer machinery as the scene.
- Implementing a renderer that ignores optional features (pause,
  continuing, product writing, commands) is cheap.

**Non-Goals:**
- Running a render loop for the renderer: threads, sampling and scheduling
  belong to each renderer.
- Image file formats: product writing is the renderer's own, or the host's
  from frames.
- Render pass collections (`renderPass` prims select what renders):
  deferred until a consumer needs them.
- Following Hydra's eventual `HdRenderer` automatically: when it lands,
  a follow-up change reconciles the two.

## Decisions

### D1. A separate crate, `moho-render`, depending on `moho` only
Render control goes beyond Hydra, so it cannot sit in `moho`, whose rule
drops behaviour Hydra lacks and which is meant to move upstream as
`openusd-imaging`. A crate in the same workspace shares the pin, lints and
CI.
*Alternative:* a `moho::renderer::control` module behind an
`unstable-render-control` feature. Rejected: it puts behaviour that goes
beyond Hydra inside the crate intended for upstream.

### D2. `ControlledRenderer: moho::renderer::Renderer`, with `invalidation()` beside `update()`
`moho`'s `update(&self)` returns nothing and stays that way (D1). The
renderer records the strongest invalidation that its updates since the last
`render` caused, and `invalidation()` reads it. `render` resets it.
`Invalidation` derives `Ord` (`None < Continue < Restart < Recreate`), so
recording is a `max`. A helper, `RestartOnAnyNotice`, wraps a
`PrimManagingRenderer` and records `Restart` for any non-empty flush. That
covers the default in the spec. A renderer that classifies changes inspects
the dirtied locators in its `ManagedPrim::dirty` and records `Continue`
itself.
*Alternative:* a new `update` that returns `Invalidation`. Rejected: it
shadows `moho`'s method and breaks `chain::create_renderer` generics.

The trait:

```rust
pub trait ControlledRenderer: moho::renderer::Renderer {
    fn capabilities(&self) -> &Capabilities;
    fn invalidation(&self) -> Invalidation;
    fn render(&self, request: &RenderRequest) -> Result<(), RenderError>;
    fn output(&self) -> SceneIndexHandle;
    fn control(&self) -> ControlHandle;
    /// Sends the output index's queued notices. `update` also calls it.
    fn poll(&self);
    fn invoke(&self, command: &Token, args: Option<ContainerHandle>) -> Result<(), RenderError> {
        Err(RenderError::UnknownCommand(command.clone()))
    }
}
```

`RenderError` variants: `NoCamera`, `UnknownProduct(sdf::Path)`,
`Unsupported(&'static str)` (the missing capability), `RecreateRequired`,
`UnknownCommand(Token)`, `InvalidRequest(String)`, and `Failed(String)`. It
implements `std::error::Error`.

### D3. The output index holds metadata; pixels go through the control handle
`vt::Value` cannot hold an `Arc<Frame>`, and putting pixels into Moho's data
model would be wrong anyway. Each var's output prim (type `renderBuffer`)
gives:
- `renderBuffer/format` (token);
- `renderBuffer/dimensions` (int2);
- `renderBuffer/multiSampled` (bool);
- `renderBuffer/generation` (int64);
- `renderBuffer/converged` (bool).

The names follow `HdRenderBufferSchema`, plus `generation`. A host that
sees `generation` dirtied calls `control().frame(var)`. Each product prim
has type `renderProductOutput` and gives:
- `renderProductOutput/product` (path);
- `renderProductOutput/written` (asset path, once written).

The root gives:
- `renderer/state` (token);
- `renderer/samples` (int);
- `renderer/progress` (float, absent when unknown);
- `renderer/failure` (string, when `failed`);
- `renderer/stats`, an open container the renderer fills;
- `renderer/capabilities`, the declared capabilities as data.

The output index uses the stage paths of products and vars, so a host maps
between the two indices without a table.
*Alternative:* a `Value::Bytes` frame in the data source. Rejected: it
copies every snapshot into the scene data model and still cannot carry
format or windows without a schema of its own.

### D4. Render threads queue, the owning thread notifies
Render threads write to an `Arc<Shared>` holding:
- a `Mutex` with the run state, the latest `Arc<Frame>` per var and a set
  of vars whose snapshot is unseen;
- a `Condvar` for `wait`.

`poll` runs on the owning thread. It takes the unseen set and the state
change under the lock, releases the lock, updates the `Rc` output index's
retained data, and sends one `dirtied` batch. This keeps one notice per var
per poll, sends nothing without a poll, and never calls an observer under
the lock. The output index is a small retained index of the crate's own:
`moho::scene_index::RetainedSceneIndex` would also serve, but the crate
needs its prims' data rebuilt from `Shared` at each poll.
*Alternative:* notify from render threads. Rejected: observers and indices
are `Rc` and confined to one thread.

### D5. `ControlHandle(Arc<dyn Control>)`, frames as `Arc<Frame>`
```rust
pub trait Control: Send + Sync {
    fn state(&self) -> RunState;
    fn stop(&self);
    fn pause(&self) -> bool;
    fn resume(&self) -> bool;
    fn frame(&self, var: &sdf::Path) -> Option<Arc<Frame>>;
    fn wait(&self, seen: &RunState, timeout: Duration) -> RunState;
}

pub struct Frame {
    pub generation: u64,
    pub samples: u32,
    pub format: PixelFormat,
    pub display_window: PixelRect,
    pub data_window: PixelRect,
    pub pixels: Arc<[u8]>,
}
```

`sdf::Path` must be `Send + Sync`. openusd's path is an interned handle,
which the task 1.1 check confirms. If it is not, the handle takes the
var's path string. A renderer publishes a frame by swapping the `Arc` in
`Shared`, so readers never block the renderer for longer than a pointer
swap, and no Map/Unmap exists. `PixelFormat` has `F16x4`, `F32x4`,
`F32x3`, `F32`, `I32` and `U8x4`, named after `HdFormat`'s. Multisample
resolve happens before publishing.
`RunState` has `Idle`, `Loading`,
`Rendering { samples, progress: Option<f32> }`, `Paused { samples }`,
`Converged { samples }`, `Stopped { samples }` and
`Failed { reason: Arc<str> }`. Its token for `renderer/state` is the
lowercase variant name.

### D6. Selecting what to render
A crate helper, `RenderTargets::resolve(terminal, request, default_aov)`,
gives what the renderer must render. It reads the root's
`sceneGlobals/activeRenderSettingsPrim` and that prim's
`RenderSettingsSchema`:
- **Products:** each product's path, resolution, camera (the product's,
  else the settings') and vars (path, name, data type, source), filtered by
  `request.products`. A requested product that is missing is
  `UnknownProduct`.
- **No settings prim:** a single synthetic product at
  `/__moho_render__/product`, holding one var at
  `/__moho_render__/product/<default_aov>`, through
  `sceneGlobals/primaryCameraPrim`.
- **No camera:** `NoCamera`.

Every renderer resolves its targets the same way, and the spec's selection
scenarios are tested once, in the crate.

### D7. `SettingsOverrideSceneIndex` replaces a settings map
It is a filter, offered as a `RendererFilter` at phase 0, `AtEnd`, so it
sits above `RenderSettingsFlatteningSceneIndex`, which runs earlier in
`chain::standard`. It has these methods: `set(key, Value)`, `clear(key)`
and `target(Option<sdf::Path>)` (the default `None` follows the active
settings prim).

`prim()` overlays a `namespacedSettings` container with the overrides on
the target's `renderSettings`, using `data_source::Overlay`. Each call
compares against the current override and dirties only
`renderSettings/namespacedSettings/<key>`, or nothing when the value is
unchanged. When the active settings prim changes, both the old and the new
target are dirtied at every overridden key.
*Alternative:* a `HashMap` passed to `render`. Rejected: it is a second
source of truth that bypasses invalidation, so the renderer could not tell
`Continue` from `Restart` for a settings change.

### D8. `Capabilities` is plain data, also published as `renderer/capabilities`
`Capabilities` has:
- the flags `pause`, `stop`, `progressive`, `continue_on_edit` and
  `writes_products`;
- `material_contexts: Vec<Token>`;
- `aovs: Vec<AovDescriptor>`, each with a name, `PixelFormat`, a clear value
  and `multi_sampled`;
- `settings: Vec<SettingDescriptor>`, each with a namespaced key, a
  `SettingType` (`Bool`, `Int`, `Float`, `String`, `Token`, `Enum(Vec<Token>)`),
  a default `Value`, and an optional range;
- `commands: Vec<CommandDescriptor>`, each with a name, a description and
  argument descriptors.

Publishing them as data lets a UI build a settings panel by observing the
output index alone.

### D9. The test renderer
`moho_render::testing::TestRenderer` (feature `testing`) wraps
`PrimManagingRenderer` and runs one sampling thread per render. It works
like this:
- At each render it computes a scene digest on the owning thread: a hash of
  every prim's path, type and sampled values at shutter offset 0, walked
  from the terminal root.
- At each sample it fills each var with
  `f(digest, var name, samples)`, using a configurable per-sample delay so
  that tests can stop, pause and wait mid-render.
- It writes products as a small raw format (header plus pixels) at the
  product name, so the file can be checked without an image library.
- It declares one command, `flushTextureCache`, which does nothing.

Because the digest is computed on the owning thread, the sampling thread
needs no access to the index. This is the pattern real renderers follow:
import on the owning thread, sample elsewhere.

## Risks / Trade-offs

- [Hydra's eventual `HdRenderer` differs] → The crate is separate and
  documented as ahead of Hydra. A follow-up change maps or adapts it, and
  `moho` stays untouched either way.
- [Hosts forget to poll, so the UI never updates] → `update` polls, and
  the docs show the loop. `wait` gives batch hosts progress without
  polling.
- [A snapshot per sample copies whole images] → Renderers decide when to
  publish (per pass, or throttled). The spec only requires the latest frame
  and growing generations.
- [`Invalidation::Continue` misused, leaving stale samples] → It is the
  renderer's own claim, gated by `continue_on_edit`. The test renderer
  exercises both paths.
- [The scene digest walk in the test renderer is slow on large scenes] →
  It is test support only, behind a feature.

## Migration Plan

Additive: a new crate, no change to `moho`. Hosts adopt it renderer by
renderer. To roll back, delete the crate.

## Open Questions

- Whether `renderer/stats` keys should be standardised (time, memory, rays).
  For now each renderer chooses its keys, and the keys can be standardised
  later without changing this API.

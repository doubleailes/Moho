## Why

`moho::renderer::Renderer` (change `hydra-renderer`) has one method,
`update`, because Hydra's `HdRenderer` is still a stub: the only way to
control a render is `HdLegacyRenderControlInterface`, which Hydra plans to
replace. Moho's consumers each invent their own control API in the meantime:
start, stop, AOVs, frames, settings and progress. A host such as a DCC's IPR,
a farm command line, or a bridge to a C++ Hydra delegate such as Karma or
RenderMan has to be rewritten for every renderer. This change drafts Moho's
own answer ahead of the reference, in a separate crate, so Moho's core stays
at Hydra parity.

## What Changes

- A new workspace crate, `moho-render`, that depends on `moho` only. The
  `moho` crate does not change, so its Hydra parity rule and the
  `openusd-imaging` path are unaffected.
- A `ControlledRenderer` trait that extends `moho::renderer::Renderer`:
  - `capabilities()`: what the renderer supports.
  - `invalidation()`: what the updates since the last render cost the image
    (`None`, `Continue`, `Restart` or `Recreate`).
  - `render(RenderRequest)`: start or continue rendering, returning at once.
  - `output()`: an output scene index.
  - `control()`: a thread-safe control handle.
  - `poll()`: deliver output notices.
  - `invoke(command, args)`.
- What to render is scene data. A render reads the active render settings
  prim (`sceneGlobals/activeRenderSettingsPrim`, flattened by
  `RenderSettingsFlatteningSceneIndex`) and its products and vars. Without
  one, it reads the primary camera with the default AOV. A `RenderRequest`
  carries only per-call overrides: which products, a pixel crop, when to
  stop (`Convergence`), and who writes products (`ProductWriter::Host` or
  `ProductWriter::Renderer`).
- A `Send + Sync` `ControlHandle`:
  - the run state (`Idle`, `Loading`, `Rendering`, `Paused`, `Converged`,
    `Stopped`, `Failed`);
  - `stop`, and `pause` / `resume` where supported;
  - `wait`, which blocks until the state changes;
  - `frame(var)`, which returns immutable `Arc<Frame>` snapshots with a
    growing generation. There is no Map/Unmap.
- An output scene index: the renderer's state, samples, progress, stats and
  capabilities at its root, one `renderProductOutput` prim per product, and
  one `renderBuffer` prim per var. A var's prim is dirtied at
  `renderBuffer/generation` for each new snapshot, and its root at
  `renderer/state` for each transition. Notices are queued by render threads
  and sent on the owning thread by `poll` (and `update`).
- `SettingsOverrideSceneIndex`, a renderer filter that overlays host
  overrides on the active settings prim's `renderSettings/namespacedSettings`
  and dirties exactly the keys it changes. It replaces Hydra's settings map.
- `Capabilities`:
  - flags for pause, stop, progressive rendering, continuing on edit, and
    writing products;
  - material render contexts;
  - AOV, setting and command descriptors.
- A test renderer (`moho_render::testing`, behind a `testing` feature) that
  fills each var with a value derived from the scene and its sample count. It
  is used by this crate's tests and is available to hosts for theirs.
- Not part of this change:
  - Hgi or GPU drivers.
  - Zero-copy GPU frames.
  - Render pass collections.
  - Multiple devices.
  - A plugin registry.
  - Any bridge to C++ Hydra (consumers build their own).
  - Adapting Moho's consumers.
- No breaking change.

## Capabilities

### New Capabilities

- `render-control`: controlling a renderer created over a Moho terminal
  scene index. It covers the controlled renderer and its capabilities,
  invalidation after updates, render requests and how a render selects what
  to render, run states with stop, pause and wait, frame snapshots, the
  output scene index and when its notices are sent, product writing,
  settings overrides as a filter, commands, and the test renderer.

### Modified Capabilities

None. The `renderers` capability (from change `hydra-renderer`, implemented
and awaiting archive) keeps its requirements. `render-control` builds on its
`Renderer` and `RendererPlugin`, and a controlled renderer is created through
`chain::create_renderer` unchanged.

## Impact

- New crate `crates/moho-render` (a workspace member through `crates/*`),
  under the workspace's edition, lints, `unsafe_code = "forbid"` and MIT
  license. It has no dependency beyond `moho` and `std`.
- `moho` is unchanged.
- Docs:
  - `docs/architecture.md`: a "Render control" section that marks the API
    as ahead of Hydra and kept outside the parity core.
  - `ROADMAP.md`: the "Later" entry for render control points at
    `moho-render`, to be revisited when Hydra replaces
    `HdLegacyRenderControlInterface`.
  - `CLAUDE.md`: the layout lists the crate.
- Order: archive `hydra-renderer` first, so `renderers` is in
  `openspec/specs/` when this change's specs refer to it.

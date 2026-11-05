## Context

See `proposal.md` for why, and `specs/` for the requirements. The Hydra
reference is OpenUSD v26.08 (`ee47c679a`), as in
`docs/hydra-parity-check.md`. The code this design relies on, all read at
that tag:

- `pxr/imaging/hd/renderDelegate.h`:
  - `SetTerminalSceneIndex`: "a hook point for the render delegate to
    register an observer of the terminal scene index";
  - `Update`: "called at the beginning of `HdRenderIndex::SyncAll`, before
    render index prim sync".
- `pxr/imaging/hd/renderIndex.cpp`:
  - given a terminal scene index, it disables the emulation API, builds
    `HdSceneIndexAdapterSceneDelegate` over the index (back-end emulation:
    Hydra 1.0 `Sync` of supported prim types), and calls
    `SetTerminalSceneIndex`;
  - `SyncAll` calls `Update` before bprim, sprim and rprim sync.
- `pxr/imaging/hd/renderer.h`: `HdRenderer`, the "Hydra 2.0 replacement of
  the HdRenderDelegate", is still a stub. Its only method is
  `GetLegacyRenderControl`. `HdRendererPlugin::_CreateRenderer` defaults to
  `_CreateRendererFromRenderDelegate`, whose
  `HdRenderDelegateAdapterRenderer` builds the render index with
  `HdRenderIndex::NewForBackendEmulation(terminalSceneIndex)`.
- `pxr/usdImaging/usdImagingGL/engine.cpp`: scene index mode is on by
  default (`USDIMAGINGGL_ENGINE_ENABLE_SCENE_INDEX`). Tasks, render buffers
  and the free camera come from `HdxTaskControllerSceneIndex`, so they are
  prims of the terminal scene index. The engine asks the renderer plugin
  for its `HdRendererCreateArgsSchema` (Hgi, GPU enabled).
- `pxr/imaging/hd/renderBuffer.h` and `.cpp`:
  - `HdRenderBuffer::Sync` reads the buffer descriptor and calls
    `Allocate`;
  - `Map`/`Unmap` give CPU pixels;
  - `GetResource` may give an `HgiTextureHandle`, which
    `hdx/aovInputTask.cpp` uses directly. Otherwise it uploads the mapped
    pixels itself.

On the Moho side:
- `moho::SceneIndex`, `data_source::{Container, Vector, Sampled}` and
  `Observer` are object-safe traits behind `Rc`;
- `prim_managing::PrimManagingObserver` is
  `HdsiPrimManagingSceneIndexObserver`;
- `chain::renderer_filters` applies `RendererFilter`s over any
  `SceneIndexHandle`.

The workspace forbids `unsafe_code`, and Moho depends on openusd alone
(`CLAUDE.md`).

## Goals / Non-Goals

**Goals:**
- A Rust delegate is written once against Moho's traits and runs over
  either producer: Moho's chain, or C++ Hydra through this crate.
- The `unsafe` code and the C++ build sit in one crate that the rest of the
  workspace never builds.
- Every hook usdview uses is reachable from Rust: passes, buffers, AOVs,
  picking, settings, progressive control, commands, stats and material
  choices.

**Non-Goals:**
- The opposite direction: Moho's chain as a C++ `HdSceneIndexBase` that
  feeds Storm or another C++ delegate.
- Pure `HdRenderer` plugins. The API is a stub at 26.08. This crate moves
  when Hydra fills it in.
- Hydra 1.0 data. Geometry is never read through `HdSceneDelegate`.
- Sharing a GPU texture the delegate made with its own API (Vulkan, wgpu,
  CUDA) without a copy. GPU presentation goes through an Hgi texture the
  shim owns.
- Supplying Moho's `__usdAttributes` and `__usdRelationships` under C++
  Hydra. A delegate that wants both producers does not read them, as the
  `hydra-parity-stage` audit already concluded.
- Windows and macOS builds. Linux first, and the C++ is kept portable.

## Decisions

### D1. A crate outside the root workspace
`crates/moho-hydra` is a workspace of its own, listed in the root
`exclude`. It depends on `moho` by path and reaches openusd only through
`moho::openusd`, so the stack keeps one pin.
- Rejected: a member of the root workspace. `cargo test --workspace` and
  `clippy --workspace` would then need a C++ OpenUSD install everywhere,
  including on every consumer's machine that builds Moho.
- Rejected: a separate repository. That loses the shared review of the
  parity claim, and it means a second pin to bump.

### D2. The `unsafe` code is confined to the FFI module
The crate does not inherit the workspace lints. It sets `unsafe_code =
"deny"`, and `#[allow(unsafe_code)]` appears only on the `ffi` module the
bridge lives in. Every other module and the example delegate keep
`#![forbid(unsafe_code)]`. `clippy::unwrap_used` stays denied. `CLAUDE.md`
names this crate as the single exemption.

### D3. `cxx` over a narrow C++ shim
The shim exposes about 40 plain functions over opaque handles. The handles
are `HdSceneIndexBaseRefPtr` and `HdDataSourceBaseHandle` held in
`std::shared_ptr` wrappers, and `cxx` binds them. Neither side sees the
other's templates.
- Rejected: `bindgen` on pxr headers. Tf and Vt are template-heavy and
  their ABI is unstable across versions.
- Rejected: `autocxx`. It gives less control over the generated
  boundary, and its handling of pxr's namespace macros is untested.

### D4. Values are converted in C++, copied into owned openusd values
For a `VtValue`, the shim reports a type tag and, for arrays, a pointer and
length into the `VtArray`'s data. Rust copies the elements into the
matching `vt::Value` variant before the call returns, so a value never
borrows C++ memory (`hydra-scene-index-bridge`, "Values outlive the C++
handles"). Converting the other way (setting values, command arguments,
render stats, descriptors) goes through the same tags.
- `SdfPath` becomes a one-element `PathVec`.
- `HdDataSourceLocator` becomes a `TokenVec`.
- An empty path is reported to Rust as an absent value.
- Types with no `vt::Value` variant (for example `HdRenderBuffer*`,
  `GfRange3d`) are reported as unknown and read as `None`. `GfRange3d`
  appears only as `extent`, which Hydra already splits into `min` and
  `max`.

The rejected alternative is zero-copy: wrapping a `VtArray` in a Rust view
tied to the data source's lifetime. Moho's `vt::Value` owns its data, so a
view would need a new value type that every consumer handles. The copy
costs one `memcpy` per array read, which is small next to a renderer's own
upload, and the cost is measured (task 7.3).

### D5. Tokens and paths are interned at the boundary
`TfToken` and `SdfPath` become `tf::Token` and `sdf::Path` through a
per-bridge cache keyed by the C++ pointer identity (`TfToken::Hash`,
`SdfPath` prim and property part pointers). `prim()` and `children()` pay
one hash lookup per element instead of building a string. Going the other
way, a Moho path converts by its string once and is cached the same way.

### D6. Notices are forwarded live, and the delegate is driven at `Update`
A C++ `HdSceneIndexObserver` owned by the bridge forwards each batch at
once, converted, to the Moho observers registered on the wrapped index.
That makes the bridge an honest Moho scene index.

The plugin registers its own Moho observer, which queues the batches. At
`HdRenderDelegate::Update` it hands the queue to the Rust delegate, in
order, followed by a `sync` call. Usually the delegate drives a
`PrimManagingObserver` (built over a `PrimTypeNoticeBatching` filter, as
hdPrman does) over the bridged index.

Both happen on the thread that calls the engine (usdview's GUI thread).
This satisfies Moho's thread confinement without locks. A debug assertion
checks the thread id on each bridged call.

### D7. Hydra 1.0 prims are kept only where Hydra's tasks need them
- **Geometry, lights and materials.** The plugin reports the delegate's
  declared types so the render index tracks them and gives them ids. It
  creates stub `HdRprim` and `HdSprim` subclasses whose `Sync` only clears
  dirty bits and whose initial dirty bits are clean. The data is read
  through the scene index.
- **`renderBuffer` bprims.** These are real `HdRenderBuffer` subclasses.
  Hydra's `Sync` allocates them from the task controller's descriptor,
  which the shim forwards to Rust as `allocate(dims, format,
  multisampled)`.
- **`camera` sprims.** The plain `HdCamera` is used, because
  `HdRenderPassState` reads its matrices and framing for the pass.

The rejected alternative is to drop Hydra 1.0 entirely through a custom
`HdRenderer`. `HdxTaskController`'s tasks still call into
`HdRenderDelegate` and `HdRenderBuffer`, and `HdRenderer` cannot replace
them at 26.08.

### D8. Prim ids for picking
The stub rprims register in the render index, which gives each one its
`GetPrimId`. The plugin keeps a path→id map, filled when a stub is created
and cleared when it is destroyed. It gives the delegate the map's id for a
path with each pass (`hydra-render-delegate-plugin`, "Picking and
selection highlight"). usdview's pick and selection tasks then work
unchanged on the delegate's `primId` and `instanceId` AOVs.

### D9. Buffers: Rust memory, CPU mapped, optionally uploaded to Hgi
A render buffer's pixels live in a Rust `Vec`. `Map` locks it and gives
the pointer, and `Unmap` releases it. Rendering threads write to a Rust
staging buffer and publish under the same lock, so `Map` never sees a torn
frame. This is the `hdEmbree` model.

A delegate that sets `gpu_present` makes `GetResource` answer an
`HgiTextureHandle`. The shim creates that texture from the `Hgi` driver
(`SetDrivers`) and uploads the published pixels into it at `Resolve`. This
covers the spec's "GPU texture" path and lets usdview skip its own upload.

### D10. One C++ type per plugin, generated at build time
`plugInfo.json` and `TF_REGISTRY_FUNCTION(TfType)` need a distinct C++
class name for each plugin. A delegate crate:
- is a `cdylib`;
- calls `moho_hydra_build::plugin("HdExampleRendererPlugin", "Example")`
  from its `build.rs`, which compiles the shim's registration unit with
  that name and writes `plugInfo.json`;
- invokes `moho_hydra::export_render_delegate!(ExampleDelegate)` once.

The macro defines the `extern "C"` factory the registration unit calls.

`cargo xtask package` (in this crate) lays out `<plugin>/plugInfo.json`
and `<plugin>/lib<name>.so` ready for `PXR_PLUGINPATH_NAME`.

### D11. Panics stop at the boundary
Every Rust entry point the shim calls runs under `catch_unwind`. A panic
is reported through `TF_RUNTIME_ERROR` with its message, marks the
delegate failed (later passes clear to the clear colour, and settings
calls are no-ops), and never unwinds into C++.

### D12. Renderer filters run in Rust
A delegate applies `moho::chain::renderer_filters` over the bridged
terminal index. Hydra's C++ registry already applied usdview's
application filters, such as scene globals at phase 0, before the index
was handed over. The chain C++ builds for other renderers is not touched.

The consequence is that a Rust filter runs after every C++ filter
registered for this renderer, whatever their phases. There are none,
because the plugin registers no C++ scene index plugins.

### D13. Testing without a GUI
- **Bridge tests.** They build an `HdRetainedSceneIndex` and
  UsdImaging's chain over a fixture in C++ (through a test-only shim
  function), wrap it, and compare prims, data and notices with Moho's
  `chain::standard` over the same fixture. This reuses the normalizations
  of `docs/hydra-parity-check.md`.
- **Plugin tests.** They run `usdrecord --renderer Example` (OpenUSD's
  headless recorder, through the same engine as usdview) under `xvfb-run`
  on the fixtures, and compare images with tolerances. They also cover
  frames over time, settings through `--renderSettingsPrimPath`, and AOVs.
- **Interactive hooks.** Pause, stop, commands, picking and AOV menus go
  through a Python test that drives `UsdImagingGL.Engine` directly, the
  API usdview calls.

### D14. CI
A separate job installs conda-forge `openusd=26.08` and runs:
- `cargo fmt --check`;
- `clippy -D warnings`;
- `cargo test` in `crates/moho-hydra`;
- the `usdrecord` tests under `xvfb-run`.

The root job is unchanged.

## Risks / Trade-offs

- **Hydra 2.0's delegate API moves** (`HdRenderer`,
  `HdLegacyRenderControlInterface`) → Pin one OpenUSD version per release
  of the crate, check the version at build time, and keep the C++ that
  touches these classes in one file.
- **ABI mismatch with the OpenUSD that usdview runs** (another compiler,
  another USD build, Python-only `usd-core` wheels without imaging) → Build
  against the install that runs usdview (`PXR_USD_LOCATION`). Refuse a
  build when its `pxr/pxr.h` version differs from the one pinned.
  Document that conda-forge or a source build is required.
- **Copy cost of large arrays (D4)** → Measure on Moana's largest chunk
  (task 7.3). If it dominates, a borrowed array view is a later, additive
  change in Moho.
- **Notices arriving off the main thread**, from an application other than
  usdview that applies stage updates on a worker → The bridge's C++
  observer queues off-thread batches and delivers them at the next call on
  the owning thread. The debug assertion catches violations in tests.
- **Stub prims hide real costs.** The render index still tracks dirty bits
  and runs (empty) sync for every geometry prim → Report these prim types
  with `IsParallelSyncEnabled` false, and measure. Making them untracked
  needs Hydra API, so it goes to upstream candidates.
- **Divergence between producers.** A delegate tested only over Moho may
  read Moho-only data or depend on quirks Moho documents → Run the bridge
  tests (D13) against the corpus of `docs/hydra-parity-check.md`. The
  reference delegate reads nothing Moho-only.
- **A `unsafe` exemption in a crate that forbids it everywhere else** → It
  is confined to one module (D2) and reviewed as such, and the exemption
  is written down in `CLAUDE.md`.

## Migration Plan

This change is additive. `moho` and its consumers are untouched, and
rolling back means deleting `crates/moho-hydra`, its CI job, and the
`CLAUDE.md` and `ROADMAP.md` lines. A consumer's delegate adopts the
crate by depending on `moho-hydra` and building a plugin next to its
Moho-native entry point.

## Open Questions

- The exact Hgi upload path in D9 (`HgiBlitCmds::CopyTextureCpuToGpu`
  against a texture that is recreated on every resize) is settled when D9
  is implemented. It changes no requirement or task.
- Whether `HdRendererCreateArgsSchema`'s `gpuEnabled = false` (usdview
  without GL) must disable `gpu_present` automatically. The expected
  answer is yes, and it is confirmed against the engine at implementation
  time.

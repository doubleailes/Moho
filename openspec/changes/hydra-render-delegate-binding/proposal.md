## Why

A render delegate written in Rust against Moho's scene index contract runs
only where Moho builds the chain. usdview, the reference viewer studios
debug scenes in, runs OpenUSD's C++ Hydra 2.0, and has no way to load a
Rust delegate. Moho already matches Hydra's
paths, schemas, data layout and dirty locators at USD 26.08
(`docs/hydra-parity-check.md`). A delegate reading `moho::SceneIndex`
could therefore read C++ Hydra's terminal scene index unchanged, if
something bridged the two. That bridge also gives Moho a second,
independent producer of the same contract, which keeps the parity claim
honest.

## What Changes

- A new crate, `crates/moho-hydra`, outside the root workspace and outside
  `moho`'s dependency tree. It holds three parts:
  - a C++ shim, built by its `build.rs` against an installed OpenUSD;
  - a `cxx` bridge;
  - a Rust API.
  It reaches openusd only through `moho::openusd`.
- **Scene index bridge.** A C++ `HdSceneIndexBase` wrapped as a
  `moho::SceneIndex`, with its data sources wrapped as Moho containers,
  vectors and sampled values. Values convert between `VtValue` and
  `vt::Value`. Moho's conventions hold over it: one `SdfPath` becomes a
  one-element `PathVec`, an empty path becomes an absent field, and a
  locator becomes a `TokenVec`. Moho observers can register on the
  wrapped index.
- **Render delegate plugin.** A Rust `RenderDelegate` trait and an export
  macro. Together they build a Hydra renderer plugin (a cdylib with its
  `plugInfo.json`) that usdview lists in its renderer menu. The plugin
  takes the scene through Hydra 2.0's `SetTerminalSceneIndex` and
  `Update`, and stubs the Hydra 1.0 prim sync. It exposes the delegate
  surface usdview uses:
  - render passes with camera, framing and AOV bindings;
  - render buffers, both CPU-mapped and as a GL texture through Hgi;
  - AOV descriptors;
  - render setting descriptors and values;
  - render stats;
  - pause, resume, stop and restart;
  - commands;
  - material binding purpose and render contexts;
  - the renderer's own scene index filters.
- **A reference delegate** in the crate's examples. It renders meshes,
  cameras, lights and display colour on the CPU, progressively, and
  exercises every hook above in usdview.
- `CLAUDE.md` exempts `moho-hydra`, and only that crate, from
  `forbid(unsafe_code)` and from "openusd only". `unsafe` stays confined
  to its FFI module. `ROADMAP.md` gains the step.
- A CI job builds and tests `moho-hydra` against conda-forge OpenUSD
  26.08. The root workspace's checks are unchanged.

## Capabilities

### New Capabilities
- `hydra-scene-index-bridge`: a C++ Hydra scene index read through Moho's
  scene index contract (prims, children, data sources, values, notices,
  time), with Moho's value conventions.
- `hydra-render-delegate-plugin`: a Rust render delegate packaged as a Hydra
  renderer plugin. This covers loading in usdview, taking the scene
  through the terminal scene index, render passes and buffers, settings,
  progressive control, commands and renderer scene index filters.

### Modified Capabilities
None. `scene-index` is unchanged: the bridge is one more implementation
of it.

## Impact

- New: `crates/moho-hydra/` (Rust, C++ shim, `build.rs`, example
  delegate, tests).
- Edited: root `Cargo.toml` (`exclude`), `CLAUDE.md` (the exemption and
  the layout), `ROADMAP.md`, `docs/architecture.md` (a section on the
  bridge), and `.github/workflows/ci.yml` (a job with C++ OpenUSD).
- Dependencies, for `moho-hydra` only: `cxx`, `cxx-build`, and a C++
  OpenUSD 26.08 install (headers and `usd_hd`, `usd_hdx`, `usd_hf`,
  `usd_hgi`, `usd_tf`, `usd_vt`, `usd_sdf`, `usd_gf`, `usd_plug`).
- `moho` is unchanged, and no consumer is named.

## 1. Crate and build

- [ ] 1.1 Create `crates/moho-hydra` as its own workspace: `moho` by path, `cxx`, `cxx-build`, `unsafe_code = "deny"`, and `clippy::unwrap_used` denied (design D1, D2). Add it to the root `exclude`. Verify that `cargo test --workspace` at the root does not build it.
- [ ] 1.2 Write `build.rs`:
  - find OpenUSD through `PXR_USD_LOCATION`;
  - check that `pxr/pxr.h` is 26.08 and fail otherwise, naming both versions;
  - compile an empty shim against `usd_hd` and `usd_hdx`.

  Verify that `cargo build` succeeds against conda-forge `openusd=26.08`, and fails with that message against another version.
- [ ] 1.3 Add the CI job (design D14): conda-forge OpenUSD 26.08, then `fmt`, `clippy -D warnings` and `cargo test` in `crates/moho-hydra`. Verify the job is green on the scaffold and the root job is unchanged.
- [ ] 1.4 Update `CLAUDE.md`:
  - the layout line for `moho-hydra`;
  - the single exemption from `forbid(unsafe_code)` and "openusd only" (confined to `ffi`);
  - its own commands.

  Add the step to `ROADMAP.md`. Verify that both files describe the commands that 1.1 to 1.3 run.

## 2. Scene index bridge

- [ ] 2.1 Write the `ffi` module and C++ shim with opaque `HdSceneIndexBase` and `HdDataSourceBase` handles (design D3), and a test-only C++ function that builds an `HdRetainedSceneIndex` from entries. Verify with a test that a handle round-trips and is released (the C++ refcount returns to its start).
- [ ] 2.2 Write the token and path interning cache (design D5). Verify with tests that a `TfToken` or `SdfPath` converts to the same `tf::Token` or `sdf::Path` twice with one string conversion, and that property paths and the absolute root convert correctly.
- [ ] 2.3 Implement `moho::SceneIndex` on the wrapped index: `prim`, `children` (in C++ order), and the undefined prim. Verify the spec scenarios of "A C++ scene index as a Moho scene index" over a retained index.
- [ ] 2.4 Wrap containers, vectors, sampled values, blocks and null data sources, read lazily. Verify the scenarios of "Data sources map one to one", including laziness through a C++ data source that counts its reads.
- [ ] 2.5 Convert `VtValue` to `vt::Value` for every type that has a variant (design D4), plus `SdfPath`, `SdfPathVector`, locators, the empty path, and unknown types. Verify with a table test over every type, and the scenarios of "Values convert to openusd values" and "Moho's path and locator conventions".
- [ ] 2.6 Read sampled values at an offset, and answer contributing sample times with uniform or the C++ times. Verify the scenarios of "Sample times over a shutter" with a C++ test data source that holds samples.
- [ ] 2.7 Write the C++ observer that forwards added, removed, dirtied and renamed batches to Moho observers, held weakly (design D6), with the thread-id debug assertion and the off-thread queue. Verify the scenarios of "Notices reach Moho observers", that a dropped observer receives nothing, and that a batch sent from another thread arrives at the next call on the owning thread.
- [ ] 2.8 Verify "Values outlive the C++ handles" with a test: read points, drop the container, remove the prim in C++, and read the points on a worker thread.
- [ ] 2.9 Add a section on the bridge to `docs/architecture.md`, and record the C++ API relied on (render delegate hooks, `NewForBackendEmulation`, render buffer and pass state) in a `crates/moho-hydra/README.md`. Verify that every API the shim calls is listed with its header.

## 3. Bridge parity with Moho's chain

- [ ] 3.1 Add a test-only shim function that builds `UsdImagingCreateSceneIndices` over a `UsdStage`, and a dumper that walks any `moho::SceneIndex` and prints prims, types and data under every locator. Verify that it dumps a fixture through both the bridge and `moho::chain::standard`.
- [ ] 3.2 Compare the two dumps on Moho's fixtures with the normalizations of `docs/hydra-parity-check.md`, and with Moho-only data dropped. Verify that the differences are exactly the classified ones already in that document. Record any new one there.
- [ ] 3.3 Compare notices for the same edits on both sides. Verify that added, removed and dirtied entries match after the same normalizations.

## 4. Renderer plugin packaging

- [ ] 4.1 Write the C++ `HdRendererPlugin` and `HdRenderDelegate` subclasses that forward to a Rust `RenderDelegate` trait object through an `extern "C"` factory, with every Rust entry under `catch_unwind` (design D11). Verify with a test that a panicking delegate reports `TF_RUNTIME_ERROR` and later calls are no-ops.
- [ ] 4.2 Write the `moho_hydra_build::plugin(type_name, display_name)` build helper and the `export_render_delegate!` macro (design D10), which generate the registration unit and `plugInfo.json`. Verify that a minimal example `cdylib` builds and produces both.
- [ ] 4.3 Write `cargo xtask package`, which lays out the plugin directory. Verify that `Hd.RendererPluginRegistry` in Python lists the display name with `PXR_PLUGINPATH_NAME` set, and that a delegate reporting unsupported is not offered (spec "A loadable renderer plugin").

## 5. Delegate surface

- [ ] 5.1 `SetTerminalSceneIndex` and `Update`: bridge the terminal index, queue its notices, and hand them to the delegate in order at `Update`, then `sync` (design D6). Verify with an engine test that the first update carries the stage's added prims, and that a visibility edit arrives dirtied at `visibility` (spec "The scene through the terminal scene index").
- [ ] 5.2 Report the declared rprim, sprim and bprim types, with stub prims whose `Sync` does no work, and with `IsParallelSyncEnabled` false (design D7). Verify that the supported types are exactly those declared, and that stub `Sync` never reads the scene delegate (counted).
- [ ] 5.3 Write the render pass: camera matrices, camera path, framing or viewport, render tags, AOV bindings, and convergence. Verify with an engine test that orbiting the free camera changes the matrices the delegate receives, and that a scene camera's path is given.
- [ ] 5.4 Write render buffers in Rust memory with `Allocate`, `Map`/`Unmap`, `Resolve` and convergence, published under a lock (design D9). Verify that a resize reallocates before the next pass, and that a CPU buffer is presented by `usdrecord`.
- [ ] 5.5 Add the optional Hgi texture from `GetResource`, uploaded at `Resolve`, and disabled when the create args say the GPU is off. Verify that `hdx`'s AOV input task takes the handle (no CPU upload counted), and the image matches the CPU path.
- [ ] 5.6 Answer AOV descriptors, including unsupported ones. Verify that usdview's engine lists the delegate's AOVs, and that choosing `normal` binds and shows it.
- [ ] 5.7 Add the path→prim id map for picking (design D8). Verify with an engine test that a pick at a mesh's pixel returns its path, and that selection highlighting covers it.
- [ ] 5.8 Add render settings: descriptors, get, set, and the version. Verify that a set through the engine reaches the delegate and increases the version.
- [ ] 5.9 Add pause, resume, stop and restart, with their supported flags and state. Verify through the engine that pause and resume keep accumulated samples, and that stop ends the delegate's threads.
- [ ] 5.10 Add commands (descriptors, invocation with arguments) and render stats. Verify that an engine command invocation reaches the delegate with its arguments, and that the stats dictionary round-trips through value conversion.
- [ ] 5.11 Report the material binding purpose, render contexts, network selector, settings namespaces and shading systems. Verify that a `preview` delegate reads preview-purpose bindings through the bridge.
- [ ] 5.12 Apply Moho renderer filters over the bridged terminal index (design D12). Verify that a delegate with `RendererFilter::dependency_forwarding` reads its forwarded dirties, and that Storm in the same process sees the C++ chain unchanged.
- [ ] 5.13 Teardown: stop the delegate, then release its handles, buffers and threads before the render index is destroyed. Verify that switching renderer in an engine test leaves no live Rust threads and no reads after destruction (counted on the bridge).

## 6. Reference delegate

- [ ] 6.1 Write `examples/example_delegate`: a progressive CPU ray caster with a BVH, built on `PrimManagingObserver` over `PrimTypeNoticeBatching`. It handles meshes (points, topology, transforms, visibility, display colour), cameras, distant, dome and sphere lights, and the AOVs `color`, `depth`, `normal`, `primId` and `instanceId`. It reads no Moho-only data, and `#![forbid(unsafe_code)]` stays on. Verify that it renders each fixture through Moho's chain in a Rust test without C++.
- [ ] 6.2 Expose a setting (`maxSamples`), a command (`clearCaches`), stats (`samples`, `primCount`), pause and stop. Verify each through the tests of group 5.

## 7. Integration

- [ ] 7.1 Run `usdrecord --renderer Example` under `xvfb-run` on the fixtures, and frames over time on an animated one. Compare against reference images rendered by the same delegate through Moho's chain, with tolerances. Verify the images match and the CI job runs this.
- [ ] 7.2 Open a fixture in usdview with the plugin. Check the renderer menu, AOV menu, settings dialog, commands, pause and stop, picking, live edits and time scrubbing. Record the run in `crates/moho-hydra/README.md`, and verify every hook of `hydra-render-delegate-plugin` is exercised.
- [ ] 7.3 Measure the bridge's value copy (design D4) and the stub prims' sync cost (design D7) on Moana's largest chunk. Record both in `crates/moho-hydra/README.md`, and add any upstream need to `ROADMAP.md`'s upstream candidates.

## Workflow follow-up

- Archive the change once the CI job is green and the usdview run of 7.2 is recorded.

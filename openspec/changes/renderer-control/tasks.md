# Tasks

## 1. Crate scaffold

- [ ] 1.1 Create `crates/moho-render` (`Cargo.toml` with workspace package fields, `[lints] workspace = true`, dependency on `moho` by path, a `testing` feature; `src/lib.rs` with `#![forbid(unsafe_code)]` and crate docs stating the API is ahead of Hydra and names no consumer). Check whether openusd's `sdf::Path` and `vt::Value` are `Send + Sync` at the pinned rev and record the answer in `docs/openusd-notes.md` (design D5). Verify that `cargo build -p moho-render` and `cargo clippy --workspace --all-targets -- -D warnings` pass, and that `cargo tree -p moho` does not list `moho-render`
- [ ] 1.2 Add the crate to `CLAUDE.md`'s layout and verify that `grep moho-render CLAUDE.md` finds it

## 2. Core types

- [ ] 2.1 Add `Invalidation` (`Ord`: `None < Continue < Restart < Recreate`), `RunState` with its `renderer/state` token, `PixelFormat`, `PixelRect`, `Frame`, `Convergence`, `ProductWriter`, `RenderRequest` and `RenderError` (`std::error::Error`, with each variant's message naming its subject), documented with their `HdFormat`/`HdRenderBuffer` counterparts (design D2, D5). Verify with unit tests: invalidation ordering, `max` folding, state tokens, and `RenderError` display texts naming the camera, product path and command
- [ ] 2.2 Add `Capabilities`, `AovDescriptor`, `SettingDescriptor` (`SettingType`, default `Value`, range) and `CommandDescriptor`, with `Capabilities::to_container()` giving the `renderer/capabilities` data (design D8). Verify with a unit test reading `ri:hider:maxsamples` (int, default 64) back from the container

## 3. Shared state and the control handle

- [ ] 3.1 Add the `Control` trait, `ControlHandle`, and a ready-made `Shared` (a `Mutex` holding the state, the latest frame per var and the unseen set, plus a `Condvar`) that implements `Control`, with `publish_frame`, `set_state` and `take_changes` for renderers (design D4, D5). Verify with unit tests: `wait` returns on a change and on a timeout; `stop` from a spawned thread is seen by the owner; a held `Arc<Frame>` is unchanged after a newer publish whose generation is greater; an unknown var gives `None`

## 4. The output scene index

- [ ] 4.1 Add `OutputSceneIndex` (an `Rc` scene index implementing `moho::scene_index::SceneIndex`): root data (`renderer/state`, `samples`, `progress`, `failure`, `stats`, `capabilities`), `renderProductOutput` prims at product paths, and `renderBuffer` prims at var paths (`format`, `dimensions`, `multiSampled`, `generation`, `converged`), with tokens in a `tokens` module (design D3). Verify with tests: layout after targets are set; dropping a product removes its prim and vars; no data source holds pixels
- [ ] 4.2 Add `OutputSceneIndex::poll(&Shared)`: take the changes under the lock, release it, update the data, and send one `dirtied` batch (generation per changed var once, `renderer/state` on a transition). Verify with `moho`'s recording observer: three snapshots give one notice at the latest generation; no poll gives no notice; convergence dirties the root's state and reads `converged`

## 5. Selecting what to render

- [ ] 5.1 Add `RenderTargets::resolve(terminal, request, default_aov)` over `sceneGlobals` and `RenderSettingsSchema` / `RenderProductSchema`: products filtered by request, camera precedence (product, then settings, then primary camera), the synthetic `/__moho_render__/product` without settings, and `NoCamera` / `UnknownProduct` (design D6). Verify with tests over stages through `chain::standard` covering each selection scenario in the spec

## 6. The controlled renderer trait and settings overrides

- [ ] 6.1 Add `ControlledRenderer: moho::renderer::Renderer` (`capabilities`, `invalidation`, `render`, `output`, `control`, `poll`, `invoke` defaulting to `UnknownCommand`) and `RestartOnAnyNotice`, which wraps a `PrimManagingRenderer` and records `Restart` on a non-empty flush (design D2). Verify with a unit test that an update with no notice leaves `None` and an update with a dirtied mesh gives `Restart`
- [ ] 6.2 Add `SettingsOverrideSceneIndex` (`set`, `clear`, `target`) and its `RendererFilter` (phase 0, `AtEnd`) (design D7). Verify with tests through `chain::create_renderer`: an override reads 4 over an authored 64 with other keys unchanged; a change dirties only `renderSettings/namespacedSettings/ri:hider:maxsamples`; clearing restores 64 and dirties the key; the same value sends nothing; changing the active settings prim dirties both targets' overridden keys

## 7. The test renderer

- [ ] 7.1 Add `testing::TestRenderer` and its plugin (feature `testing`): a scene digest on the owning thread, one sampling thread per render with a configurable delay, pixels `f(digest, var, samples)`, convergence by `Convergence`, pause/resume/stop, `invalidation` following `continue_on_edit`, `Recreate` reported for a change to a setting declared create-only, a raw product writer, and the `flushTextureCache` command (design D9). Verify that `cargo build -p moho-render --features testing` and clippy pass
- [ ] 7.2 Integration tests (`tests/render_control.rs`, `--features testing`) through `chain::create_renderer` over a stage covering each spec scenario: capabilities before rendering; `render` returning at once; convergence at 4 samples; restart after an edit versus continue when declared; `RecreateRequired`; stop from another thread with frames still readable; pause and resume keeping the count; failure keeping its reason; crop windows; product subset; renderer-written products and the `written` path; unsupported writer; commands known and unknown; predictable pixels; pixels changing after an edit. Verify with `cargo test -p moho-render --features testing`
- [ ] 7.3 Add a crate-level doc example of the host loop (create, `render`, `update` and `poll` per tick, read frames on `generation` dirtied, `wait` for batch) over the test renderer, and verify that `cargo test -p moho-render --features testing --doc` passes

## 8. Docs

- [ ] 8.1 Add a "Render control" section to `docs/architecture.md` (the separate crate and why, output index layout, threading, overrides as a filter, its status ahead of Hydra) and point `ROADMAP.md`'s "Later" render control entry at `moho-render`, to be reconciled when Hydra replaces `HdLegacyRenderControlInterface`. Verify that every type named in both files exists (`grep` each in `crates/moho-render/src`)

## 9. Integration

- [ ] 9.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo test -p moho-render --features testing`, and verify that all pass
- [ ] 9.2 Run `openspec validate renderer-control --strict` and verify that it passes

## Workflow follow-up

- Archive `hydra-renderer` before this change, so `openspec/specs/renderers/` exists.
- Archive this change with `/opsx:archive` after review, then check that `openspec/specs/render-control/spec.md` exists with its Purpose.

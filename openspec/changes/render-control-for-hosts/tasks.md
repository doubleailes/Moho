# Tasks

## 1. Constructors before new fields

- [ ] 1.1 Make `Frame` and `SettingDescriptor` `#[non_exhaustive]` with `Frame::new` and `SettingDescriptor::new` and builders; move the crate's own literals (test renderer, unit tests, doctest) to them; verify `cargo test -p moho-render --features testing` passes unchanged

## 2. Setting descriptors for a person

- [ ] 2.1 Add `label` (defaulting to the key), `help` and `group` to `SettingDescriptor` and its builders, written into `renderer/capabilities/settings` as `label`, `help`, `group` (tokens in `moho_render::tokens`); verify unit tests for the "Setting described for a person" and "Undescribed setting" scenarios
- [ ] 2.2 Give the test renderer's settings display names, help and groups; verify a test reads them back from the output index

## 3. Frames name their render and their view

- [ ] 3.1 Add `render` to `Frame`, set by `Shared::publish_frame` from the epoch; verify the "Frames name their render" scenario in `control.rs` tests
- [ ] 3.2 Add `RenderView` (camera path, transform, projection, apertures and offsets, focal length, clipping range, resolution, pixel aspect ratio, conform policy, time code, rendering colour space; `None` where the index gives nothing) to `targets`, resolved per product in `RenderTargets::resolve` by locator with `HdCameraSchema` names; verify a targets test over a stage camera with every lens field authored, and the "Unauthored values" scenario
- [ ] 3.3 Carry `Arc<RenderView>` on `ProductTarget` and on every frame the test renderer publishes; verify the "Camera edited after the render started" and "Time and colour space" scenarios through the test renderer
- [ ] 3.4 Assert `RenderView` is `Send + Sync` beside the existing thread-crossing assertion; record any openusd API newly relied on in `docs/openusd-notes.md`

## 4. Identifier frames

- [ ] 4.1 Add `IdTable` (sorted identifier → absolute prim path, lookup by identifier) and `Frame::ids`, and the identifier AOV names (`primId`, `instanceId`) in tokens; verify unit tests for lookup and the background value's absence
- [ ] 4.2 Make the test renderer declare `primId` (`I32`), give each prim it digests an identifier and publish one shared table per render; verify the "Pick from a frame", "Background" and "Identifiers name scene prims" scenarios

## 5. Memory and stats

- [ ] 5.1 Add `hostBytes` and `deviceBytes` tokens, seed both at zero in `Shared::new`, and add `Control::stats` / `ControlHandle::stats`; verify the "Before any render" scenario and a cross-thread read
- [ ] 5.2 Make the test renderer report its held bytes (its digest and layout) after each update; verify the "Read from another thread" scenario, including the output root after a poll

## 6. Every var refines

- [ ] 6.1 Add a test rendering `color` and `depth` through the test renderer, stopping at a mid-render sample count, that checks both vars have frames of the same sample count; verify it passes 10 consecutive runs

## 7. Documentation and integration

- [ ] 7.1 Update `docs/architecture.md` ("Render control": frames, views, identifier tables, memory stats, setting descriptors) and the crate and `testing` module docs, including the host-loop doctest reading a frame's view; verify `cargo test -p moho-render --features testing --doc` passes
- [ ] 7.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` (plus `-p moho-render --features testing`) and verify all pass; verify `openspec validate render-control-for-hosts --strict`

## Workflow follow-up

- Bump Moho in Rekall and move its renderers to the new fields (Rekall change `ipr-over-moho-render`).
- Archive this change once implemented.

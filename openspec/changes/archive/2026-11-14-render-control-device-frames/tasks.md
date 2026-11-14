# Tasks

## 1. Device frames in render control

- [x] 1.1 Add `DeviceImage`, `DeviceImageHandle` and `ReadbackError` to `moho-render` (`types.rs`), with `backend`, `format`, `size`, `read(rect)` and `as_any`
- [x] 1.2 Replace `Frame::pixels` with `FramePixels::{Host, Device}`; add `Frame::on_device`, `Frame::device_image`, `Frame::read_pixels` (cached once for device frames) and `Frame::read_rect` (checked against the data window)
- [x] 1.3 Move `Shared`, the output index and the existing tests to the accessors; host frames read with no copy
- [x] 1.4 Tests: a held device frame stays valid; whole-frame read twice transfers once; one-pixel read of an identifier frame names its prim through the id table; a rectangle outside the window is an error; a host frame reads through the same accessor

## 2. Drivers

- [x] 2.1 Add `Drivers` (named, typed drivers, `HdDriverVector`) and `tokens::driver::RENDER_DRIVER` (`renderDriver`)
- [x] 2.2 Update `RendererCreateArgsSchema`'s documentation in `moho` to point at `moho_render::Drivers` (documentation only; no API change)
- [x] 2.3 Tests: a driver is reached by name and type, the wrong type gives nothing, an empty list is valid; a test plugin needing `renderDriver` makes `chain::create_renderer` fail as unsupported naming it, before any filter is built

## 3. Test backend

- [x] 3.1 Add `testing::HostImage` (backend `test`) and a test renderer option publishing device frames through it
- [x] 3.2 Test: the same scene rendered to host and device frames reads back equal pixels per var; downcasting a test image to another type gives nothing

## 4. The wgpu backend crate

- [x] 4.1 Add `wgpu` to `[workspace.dependencies]` at the consumers' version and create `crates/moho-render-wgpu` (depends on `moho-render`, `wgpu`; `forbid(unsafe_code)`; `pub use wgpu`)
- [x] 4.2 `WgpuDriver` (adapter, device, queue) and its `Drivers` entry as `renderDriver`
- [x] 4.3 Pixel format mapping (`F32x3` refused, `I32` as `R32Sint` or `R32Uint`) and `WgpuImage::new` refusing a mismatched texture, naming both formats
- [x] 4.4 `WgpuImage` readback: region copy to a padded `MAP_READ` buffer, map, wait, strip padding; device loss and map failure as errors
- [x] 4.5 `WgpuImage::of(&Frame)` and a same-device check against a host's driver
- [x] 4.6 Tests on the software adapter, one at a time, skipping with a message without an adapter: format mapping and refusals, unpadded rows of a 3 by 2 frame, `0xFFFFFFFF` reading as -1, a host copy of the texture reading as the frame's readback
- [x] 4.7 A dependency-graph test (or `cargo tree` check in CI) that `moho` and `moho-render` do not depend on wgpu

## 5. CI, documentation and integration

- [x] 5.1 CI: install a software Vulkan driver on Linux so the wgpu tests run
- [x] 5.2 `docs/architecture.md` ("Render control": device frames, drivers, the wgpu backend) and `CLAUDE.md` (layout; wgpu confined to `moho-render-wgpu`)
- [x] 5.3 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

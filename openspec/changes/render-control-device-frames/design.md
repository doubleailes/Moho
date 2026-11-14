# Design

## Context

`moho-render`'s `Frame` holds `pixels: Arc<[u8]>`, built by `Frame::new` and
published through `Shared::publish_frame`; hosts read `frame.pixels`
directly. `moho::schema::RendererCreateArgsSchema` reads `gpuEnabled` only
and documents that Hydra's `drivers` cannot be carried by a data source
(`vt::Value` holds no device), so a renderer that needs a device takes it in
its plugin's constructor. `moho::chain::create_renderer` calls the plugin's
`is_supported` before building any filter. `moho-render` depends on `moho`
and the standard library only, and stays outside the parity core.

The stack's GPU renderer and its host both use wgpu (one wgpu version in
the consumers' workspace). See proposal.md for why.

## Goals / Non-Goals

**Goals:**
- One frame type for CPU and GPU renderers: a host that does not care where
  pixels live reads bytes; a host that does reaches the texture.
- The host owns the device; the renderer borrows it, as under Hgi.
- No GPU needed to test render control's own contract.

**Non-Goals:**
- A graphics abstraction (Hgi itself). The backend exposes wgpu's own types;
  Moho does not wrap textures, command encoders or pipelines.
- Presenting, colour conversion, or tone mapping on the device: the host's.
- Fences or explicit synchronization objects (see D4).

## Decisions

### D1. `FramePixels`: host bytes or a device image

`Frame::pixels` becomes `FramePixels::{Host(Arc<[u8]>), Device(DeviceImageHandle)}`.
`Frame::new` keeps building host frames; `Frame::on_device(format, display,
data, samples, image)` builds device frames. Readers use
`Frame::read_pixels() -> Result<Arc<[u8]>, ReadbackError>` (the whole data
window; a host frame returns its `Arc` with no copy; a device frame reads
back once and caches the bytes in a `OnceLock` on the frame, so the
"twice, one transfer" requirement holds and frames stay immutable) and
`Frame::read_rect(rect)`, which reads the rectangle only (one pixel for a
pick, never the whole frame). `Frame::device_image()` gives the handle.

*Alternative:* keep `pixels: Arc<[u8]>` and add `device: Option<…>`, leaving
`pixels` empty for device frames. Rejected: an empty slice that means
"look elsewhere" is a silent bug for every existing reader; the enum makes
the break a compile error, which the two consumer adapters fix in a line.

### D2. `DeviceImage`: a trait object naming its backend

```rust
pub trait DeviceImage: Send + Sync + 'static {
    fn backend(&self) -> &Token;               // "wgpu", "test"
    fn format(&self) -> PixelFormat;
    fn size(&self) -> (u32, u32);              // the data window's
    fn read(&self, rect: PixelRect) -> Result<Vec<u8>, ReadbackError>;
    fn as_any(&self) -> &dyn Any;
}
pub type DeviceImageHandle = Arc<dyn DeviceImage>;
```

A backend reaches its own type with `frame.device_image()?.as_any()
.downcast_ref::<WgpuImage>()`, wrapped by the backend as
`WgpuImage::of(&Frame) -> Option<&WgpuImage>`. This is
`HdRenderBuffer::GetResource` returning a `VtValue` that holds an
`HgiTextureHandle`: opaque to Hydra, typed to the host that asks. `read` is
`Map` / `Unmap` collapsed into one call returning owned bytes. `Frame::read_rect`
takes a rectangle in display window coordinates, as the data window itself
is given, checks it against the data window (so the error is uniform across
backends), and hands the backend the rectangle in the image's own
coordinates.

*Alternative:* a generic `Frame<I>` over the image type. Rejected: the
control handle, `Shared` and every host would become generic, and a host
reading a CPU renderer would have to name a GPU type.

### D3. Drivers: a list handed at plugin construction

`moho_render::Drivers` is a list of `(Token, Arc<dyn Any + Send + Sync>)`,
`HdDriverVector`'s `(name, VtValue driver)`. `Drivers::get::<T>(name)`
downcasts; `tokens::driver::RENDER_DRIVER = "renderDriver"` is Hydra's
`HgiTokens->renderDriver`. A plugin takes `Drivers` in its constructor and
checks for its driver in `is_supported`, returning `Unsupported` naming
`renderDriver`; `chain::create_renderer` already calls it first, so no
filter is built. `moho` and `RendererCreateArgsSchema` do not change: the
schema keeps reading `gpuEnabled` only, its documentation points at
`moho_render::Drivers`, and the `renderers` capability's "no `drivers`"
requirement still holds.

*Alternative:* a `moho-render` wrapper `create_controlled_renderer(…,
drivers)` with a plugin trait taking drivers in `create_renderer`. Rejected:
it duplicates `chain::create_renderer` and would diverge from it; the
constructor route is what Moho already documents, and keeps one creation
path.

### D4. Synchronization through the queue, immutability through a ring

The renderer draws on the host's device and submits on its queue before it
publishes the frame. wgpu orders submissions on a queue, so any command the
host submits after reading the frame sees the writes: no fence crosses the
API. A published texture is never written again: a renderer keeps a small
ring of textures per var and reuses one only when no frame holds it
(`Arc::strong_count` of its image, or the backend's `WgpuImage` kept in the
ring and checked on reuse). Readback waits on the device (`poll` until the
map completes) and may run on any thread; wgpu's `Device`, `Queue` and
`Texture` are `Send + Sync` cheap clones.

*Alternative:* explicit fences in `DeviceImage`. Rejected: wgpu exposes
none across one queue, and Hgi's interop on one device does not need them.

### D5. `moho-render-wgpu`: the backend crate

Contents: `WgpuDriver { adapter, device, queue }` (handed as
`renderDriver`; it also serializes readbacks' blocking polls, which some
drivers deadlock on when concurrent); `WgpuImage { texture, format, size, driver }`
implementing `DeviceImage` (backend `wgpu`; readback copies the asked region
to a `MAP_READ` buffer with rows padded to `COPY_BYTES_PER_ROW_ALIGNMENT`,
maps it, waits, strips the padding); `texture_formats(PixelFormat)` (the
mapping: `F16x4 → Rgba16Float`, `F32x4 → Rgba32Float`, `F32 → R32Float`,
`I32 → R32Sint | R32Uint`, `U8x4 → Rgba8Unorm`, `F32x3` refused);
`pub use wgpu;`. The workspace gains `wgpu` in `[workspace.dependencies]`
at the version the consumers use, and only this crate names it. It is
outside the parity core like `moho-render`, and `CLAUDE.md`'s "openusd
only" boundary is restated as "`moho` depends on openusd only; wgpu is
confined to `moho-render-wgpu`".

### D6. The test backend

`moho_render::testing` gains `HostImage`, a `DeviceImage` of backend `test`
over host bytes, and a test renderer option to publish every frame through
it. The render control tests then cover device frames, readback, rectangles
and downcasting with no GPU; the wgpu crate's tests cover only what is
wgpu's (format mapping, padding, the host's device).

### D7. wgpu tests on the software adapter

The wgpu tests request a fallback (software) adapter, lavapipe on Linux and
WARP on Windows, and run one at a time per binary. Without an adapter a
test prints that it was skipped and why, and passes. CI installs
`mesa-vulkan-drivers` on Linux so the tests run there rather than skip.

## Risks / Trade-offs

- [The host and the renderer must use the same wgpu] → the re-export, and
  the consumers taking wgpu only through `moho_render_wgpu::wgpu`, as they
  take openusd only through `moho::openusd`.
- [A frame created on another device used by the host] → `WgpuImage` holds
  its driver; the host compares it with its own before using the texture,
  and reads back otherwise. Several devices are out of scope.
- [A host holding frames starves the renderer's ring] → the renderer
  allocates a new texture when every ring slot is held, and its device bytes
  stat counts them, so the host sees the cost.
- [Readback blocks the calling thread] → it is the host's choice to read;
  picks read one pixel; the cached whole-frame read happens once per frame.
- [wgpu major bumps break the stack together] → that is the point of one
  pin; bumping wgpu is bumping Moho, as with openusd.

## Migration Plan

1. `moho-render`: `FramePixels`, `DeviceImage`, `Drivers`, the test backend;
   existing tests move to `read_pixels`.
2. `moho-render-wgpu` with its tests; CI's software adapter.
3. Docs and `CLAUDE.md`.
4. Consumers bump Moho: CPU renderers keep `Frame::new` and change reads of
   `frame.pixels` to `read_pixels`; a GPU renderer adopts `WgpuDriver` and
   publishes device frames in a change of its own.

Rollback is reverting the crate; no data format is affected.

## Open Questions

- Whether the cached whole-frame readback should be dropped under memory
  pressure (a weak cache) rather than kept with the frame. It changes no
  behaviour the specs state.

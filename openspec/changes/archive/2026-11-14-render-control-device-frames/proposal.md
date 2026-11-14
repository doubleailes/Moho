# Proposal

## Why

`moho-render` frames hold host bytes. A renderer that draws on a GPU must
copy every frame to the host to publish it, and a host that shows it in a
GPU viewport copies it back: two transfers per frame a viewport never needed,
at the rate a rasterizer draws. A GPU renderer therefore keeps a frame
transport of its own beside render control, and a host that wants its frames
on the device reaches for that renderer's types, which is the coupling
render control exists to remove.

Hydra solves this with Hgi. The host creates the graphics device and hands
it to the renderer at creation (`HdDriver` named `renderDriver` in
`HdRendererCreateArgs::drivers`); the renderer draws into textures of that
device, and its render buffers give them out (`HdRenderBuffer::GetResource`
returns an `HgiTextureHandle`), with `Map` reading the pixels back when the
host needs them on the CPU. `HgiInterop` presents them. Moho has no
counterpart: its create args schema documents that `drivers` cannot be
carried, and that a renderer needing a device takes it in its plugin's
constructor, with no shared type to say which device.

This change gives render control that role: a frame whose pixels stay on
the device, readable back on demand, and a driver the host creates and hands
the renderer. A backend crate implements both for wgpu, as HgiWgpu backs Hgi,
so a wgpu renderer and a wgpu host share frames through Moho and nothing
else.

## What Changes

- **Device frames.** A frame holds either host pixels, as today, or a
  device image: an opaque, immutable image on a graphics device that names
  its backend, its pixel format and size, and reads its pixels (all of them,
  or a rectangle, such as one pixel under a pick) back to host memory on any
  thread. A host that knows the backend reaches the native image (the
  texture) through it; any other host reads the frame back and is served as
  if the renderer had published host pixels. Identifier frames may be
  device frames too: their id table is unchanged.
- **Drivers.** A host creates the device and gives the renderer plugin a
  list of named drivers (`HdDriverVector`'s counterpart), the `renderDriver`
  among them. A plugin that needs a driver the host did not give answers not
  supported, naming it. A renderer draws on the host's device, so its device
  frames are usable by the host without any copy.
- **The wgpu backend (`moho-render-wgpu`)**, a new crate outside the parity
  core: the wgpu driver (adapter, device and queue a host created), the wgpu
  device image (a texture of that device, read back through a copy and a
  mapped buffer), the pixel format mapping, and `wgpu` re-exported so every
  crate of a stack names the same wgpu, as `moho::openusd` does for openusd.
- **The test renderer** can publish device frames over a host-memory test
  backend, so render control's own tests cover the contract with no GPU.

`moho` is unchanged: drivers are a render control type, handed at plugin
construction, as the create args schema already documents; the schema still
offers no `drivers`. **BREAKING** for code that reads `Frame::pixels`
directly: it now holds host pixels or a device image, and a reader that
wants bytes asks the frame for them.

Out of scope: presenting (the host's), several devices or sharing a texture
across devices (external memory interop), backends other than wgpu, render
pass collections, and any change to a CPU renderer, which keeps publishing
host frames.

## Capabilities

### New Capabilities

- `render-control-wgpu`: the wgpu backend of render control's device
  frames and drivers: the driver a host makes from its wgpu device, device
  images of wgpu textures and their readback, the format mapping, and the
  stack's one wgpu.

### Modified Capabilities

- `render-control`: frames may hold a device image read back on demand;
  plugins receive the host's drivers at construction and answer not
  supported without one they need; the test renderer publishes device
  frames over a test backend.

## Impact

- `crates/moho-render`: `Frame` (host or device pixels, readback), a device
  image trait, `Drivers`, the test renderer and its test backend, tokens
  (`renderDriver`).
- New `crates/moho-render-wgpu`, depending on `moho-render` and `wgpu`
  (its version is the stack's wgpu, as Moho's openusd rev is the stack's
  openusd).
- CI: the wgpu crate's tests need a software adapter (lavapipe on Linux,
  WARP on Windows) and skip, saying so, where none exists.
- `CLAUDE.md` (layout, and that the wgpu dependency is confined to the
  backend crate), `docs/architecture.md` ("Render control").
- Consumers reading `Frame::pixels` (the renderers' controlled adapters and
  hosts) read through the frame's accessor when they bump Moho.
- Follows `render-control-for-hosts`, which it builds on; its requirements
  are added beside that change's, not over them.

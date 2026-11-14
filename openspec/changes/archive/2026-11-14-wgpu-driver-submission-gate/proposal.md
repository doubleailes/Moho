# Proposal

## Why

A host that shares its wgpu device with a renderer presents to a surface on
that device. Reconfiguring a surface (a viewport resized) while another
thread submits work can fail inside wgpu's idle wait, so every submission,
present and reconfiguration on a shared device must exclude the others.
Today each party keeps its own lock (the host's context, the renderer's
context), so they do not exclude each other once the device is shared
through `WgpuDriver`, which carries the device and queue but no gate.

## What Changes

- `WgpuDriver` carries one submission gate shared by all its clones, and
  offers `submit`, `present` and `configure_surface` under it. Everyone
  holding the driver (the host, its presenter, every renderer it handed the
  driver to) submits through it.
- `WgpuImage`'s readback submits its copy through the gate.
- Error scopes stay the caller's: `configure_surface` only orders the call.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `render-control-wgpu`: the wgpu driver's submissions, presents and
  surface reconfigurations exclude each other across every clone.

## Impact

- `crates/moho-render-wgpu`: `WgpuDriver` (the gate and three methods),
  `WgpuImage::read`.
- Consumers sharing a device submit through the driver instead of the queue.

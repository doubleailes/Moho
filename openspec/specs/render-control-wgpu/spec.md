# render-control-wgpu Specification

## Purpose

The wgpu backend of render control: the driver a host makes from the wgpu
device it created, device frames holding textures of that device and their
readback, so a wgpu renderer and a wgpu host share frames with no copy.

## Requirements

### Requirement: Outside the parity core, one wgpu
The wgpu backend SHALL be a crate of its own, depending on the render
control crate, the `moho` crate render control already depends on, and
wgpu only. Neither `moho` nor the render control crate
SHALL depend on it or on wgpu. It SHALL re-export wgpu, so every crate of a
stack names the same wgpu through it.

#### Scenario: Core crates without wgpu
- **WHEN** the workspace's dependency graph is read
- **THEN** `moho` and the render control crate do not depend on wgpu or on the backend crate

#### Scenario: The stack's wgpu
- **WHEN** a renderer and a host both name wgpu through the backend's re-export
- **THEN** a texture the renderer created is a texture of the type the host reads

### Requirement: The wgpu driver
The wgpu driver SHALL hold the adapter, device and queue a host created,
and SHALL be handed as the `renderDriver`. A renderer drawing through it
SHALL create its textures on that device and submit on that queue.

#### Scenario: Renderer on the host's device
- **WHEN** a host hands its wgpu driver as `renderDriver` and the renderer publishes a device frame
- **THEN** the frame's texture belongs to the host's device

### Requirement: wgpu device images
A wgpu device image SHALL hold a texture of the driver's device, the
frame's pixel format and size, and name the backend `wgpu`. Its texture
SHALL be usable by any command the host submits on the driver's queue after
the frame was published, with the renderer's writes visible to it.

#### Scenario: Sampled by the host
- **WHEN** a host copies a published wgpu frame's texture to a texture of its own and reads that back
- **THEN** it gets the pixels the frame reads back

### Requirement: Pixel formats on the device
Each render control pixel format SHALL map to the wgpu texture formats
holding its bytes unchanged (a 32-bit identifier to a signed or an
unsigned 32-bit texture), and a device image SHALL refuse a texture whose
format does not hold the frame's. A format with no wgpu counterpart SHALL
be refused, naming it.

#### Scenario: Matching formats
- **WHEN** a device image is made of an `Rgba16Float` texture for a four-half-float var
- **THEN** it is made, and its readback is eight bytes per pixel

#### Scenario: Unsigned identifiers
- **WHEN** a device image is made of an `R32Uint` texture for an identifier var
- **THEN** it is made, and a texel holding `0xFFFFFFFF` reads back as the identifier -1

#### Scenario: Mismatched format
- **WHEN** a device image is made of an `R32Uint` texture for a four-float var
- **THEN** it is refused with an error naming both formats

#### Scenario: Three floats
- **WHEN** a renderer makes a device image for a three-float var
- **THEN** it is refused, naming the format, since wgpu has no three-channel texture format

### Requirement: wgpu readback
A wgpu device image SHALL read back by copying its texture, or the asked
rectangle of it, to a host-visible buffer and waiting for it, on any
thread, removing the device's row padding. A device lost or a failed map
SHALL be an error, not a panic.

#### Scenario: Rows unpadded
- **WHEN** a 3 by 2 four-byte frame is read back
- **THEN** the bytes are 24 long, rows top to bottom, with no row padding

#### Scenario: No adapter
- **WHEN** the wgpu crate's tests run where no adapter is available
- **THEN** they report that they were skipped and why, and do not fail

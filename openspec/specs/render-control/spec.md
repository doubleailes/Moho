# render-control Specification

## Purpose

Controlling a renderer created over a Moho terminal scene index: what it
can do, what to render, starting, stopping and pausing it, reading its
images from any thread, and observing its progress as a scene index. It is
an API drafted ahead of Hydra's own `HdRenderer` and is kept outside
Moho's Hydra-parity core.

## Requirements

### Requirement: Outside the parity core
Render control SHALL be offered by a crate separate from `moho`, depending
on `moho` and the standard library only. The `moho` crate's API and
behaviour SHALL NOT change for it, and the crate's documentation SHALL name
no consumer.

#### Scenario: Moho unchanged
- **WHEN** the render control crate is added to the workspace
- **THEN** the `moho` crate's public API and its test suite are unchanged, and `moho` does not depend on the new crate

### Requirement: A controlled renderer is a renderer
A controlled renderer SHALL be a `moho` renderer, so a plugin that creates
it works with the existing renderer creation over a chain, and its `update`
processes the terminal index's notices as before.

#### Scenario: Created over a chain
- **WHEN** a host creates a controlled renderer through the existing chain creation with its plugin
- **THEN** it receives the terminal index and the controlled renderer, and calling `update` processes the pending notices

### Requirement: Capabilities
A controlled renderer SHALL declare, without rendering, whether it can
pause, stop, render progressively, continue after an edit and write
products, its material render contexts, and its AOV, setting and command
descriptors. A setting descriptor SHALL carry a name, a type and a default;
an AOV descriptor a name and a pixel format; a command descriptor a name
and its arguments.

#### Scenario: Capabilities read before any render
- **WHEN** a host asks a newly created renderer for its capabilities
- **THEN** it gets the flags and descriptors, and the run state is still `Idle`

#### Scenario: Setting descriptor
- **WHEN** a renderer declares a setting `ri:hider:maxsamples` of type int with default 64
- **THEN** the descriptor gives that namespaced name, the int type and the default 64

### Requirement: Invalidation after updates
After updates, a controlled renderer SHALL report the strongest invalidation
since its last render, in the order `None` < `Continue` < `Restart` <
`Recreate`. Starting a render SHALL reset it to `None`. A renderer that
does not classify changes SHALL report `Restart` for any processed notice.

#### Scenario: Strongest invalidation wins
- **WHEN** two updates since the last render cost `Continue` and then `Restart`
- **THEN** the renderer reports `Restart`

#### Scenario: Nothing changed
- **WHEN** an update processes no notice
- **THEN** the renderer reports `None`

#### Scenario: Reset by a render
- **WHEN** the renderer reported `Restart` and the host starts a render
- **THEN** the renderer reports `None` until the next update that processes a notice

#### Scenario: Unclassified edit
- **WHEN** a renderer that does not classify changes processes a notice for a dirtied mesh
- **THEN** it reports `Restart`

### Requirement: What a render selects
A render SHALL read what to render from the terminal index: the products
and vars of the active render settings prim named by the scene globals.
Without an active settings prim, it SHALL render the primary camera into one
product holding the renderer's default AOV. Without a camera, it SHALL
fail and name the missing camera.

#### Scenario: Active settings
- **WHEN** the scene globals name a render settings prim with one product holding `color` and `depth` vars, and the host starts a render
- **THEN** the renderer renders those two vars at the product's resolution through the product's camera

#### Scenario: Primary camera only
- **WHEN** there is no active render settings prim, the scene globals name a primary camera, and the host starts a render
- **THEN** the renderer renders the default AOV through that camera

#### Scenario: No camera
- **WHEN** there is no active render settings prim, no settings camera and no primary camera
- **THEN** starting a render fails with an error naming the missing camera, and the run state stays `Idle`

### Requirement: Render request overrides
A render request SHALL carry only per-call overrides: a subset of products,
a pixel crop within the display window, a stop condition (converged, a
sample count, a time budget, or never) and who writes products. A product
not in the stage SHALL fail the request.

#### Scenario: Product subset
- **WHEN** the active settings hold two products and the request names one
- **THEN** only that product's vars are rendered and appear in the output index

#### Scenario: Crop
- **WHEN** a request crops to a rectangle inside the display window
- **THEN** each frame's data window is that rectangle and its display window is the product's

#### Scenario: Sample count
- **WHEN** a request stops at 4 samples
- **THEN** the run state becomes `Converged` with 4 samples and no further snapshot is made

#### Scenario: Unknown product
- **WHEN** a request names a product path the terminal index does not hold
- **THEN** starting the render fails with an error naming that path

### Requirement: Starting a render does not block
Starting a render SHALL return before the first sample is done. Starting a
render while one runs SHALL apply the current scene and request, keeping the
samples when the reported invalidation was `None` or `Continue` and the
renderer can continue, and restarting otherwise. After `Recreate` it SHALL
fail and ask the host to recreate the renderer.

#### Scenario: Returns at once
- **WHEN** the host starts a render with no stop condition
- **THEN** the call returns while the run state is `Loading` or `Rendering`

#### Scenario: Restart after an edit
- **WHEN** a render is running, an update reports `Restart`, and the host starts a render again
- **THEN** sampling restarts from zero samples

#### Scenario: Recreate needed
- **WHEN** an update reports `Recreate` and the host starts a render
- **THEN** the render fails with an error asking for the renderer to be recreated

### Requirement: Run states
A controlled renderer SHALL be in one run state at a time: `Idle`,
`Loading`, `Rendering`, `Paused`, `Converged`, `Stopped` or `Failed`, with
the sample count where it applies. A failed render SHALL keep the reason.
The run state SHALL be readable from any thread.

#### Scenario: Convergence
- **WHEN** a render meets its stop condition
- **THEN** the run state becomes `Converged` with the sample count reached

#### Scenario: Failure
- **WHEN** the renderer fails while rendering
- **THEN** the run state becomes `Failed` with the reason, and frames already made stay readable

### Requirement: Stop, pause and resume
The control handle SHALL be usable from any thread. Stopping SHALL end
sampling and keep the last frames readable. Pausing and resuming SHALL
work only where declared: otherwise they SHALL answer that they did nothing
and leave the run state unchanged.

#### Scenario: Stop from another thread
- **WHEN** another thread stops a running render
- **THEN** the run state becomes `Stopped`, and the last frame of each var stays readable

#### Scenario: Pause unsupported
- **WHEN** a renderer that does not declare pausing is asked to pause
- **THEN** pausing answers false and the run state is unchanged

#### Scenario: Pause and resume
- **WHEN** a renderer that declares pausing is paused and then resumed
- **THEN** the run state goes `Paused` with the sample count kept, then `Rendering`, and sampling continues from that count

### Requirement: Waiting for a state change
The control handle SHALL let a thread wait until the run state differs from
one it has seen, or until a timeout, and SHALL return the current state.

#### Scenario: Wait for convergence
- **WHEN** a thread waits from `Rendering` and the render converges before the timeout
- **THEN** the wait returns `Converged`

#### Scenario: Timeout
- **WHEN** a thread waits and the state does not change before the timeout
- **THEN** the wait returns the unchanged state

### Requirement: Frame snapshots
The control handle SHALL give the latest frame of a rendered var as an
immutable snapshot readable from any thread. It holds its pixel format, its
display and data windows, its sample count and a generation that grows with
each snapshot of that var. Later snapshots SHALL NOT alter one already held.

#### Scenario: Snapshot stays valid
- **WHEN** a thread holds a frame and the renderer makes a newer one
- **THEN** the held frame's pixels and generation are unchanged, and the newer frame's generation is greater

#### Scenario: Unknown var
- **WHEN** a frame is asked for a var that is not being rendered
- **THEN** no frame is given

### Requirement: The output scene index
A controlled renderer SHALL give an output scene index. Its root holds the
run state, sample count, progress, stats and capabilities. It holds one prim
per rendered product, at that product's path, naming its source product and,
once written, its written file. It holds one prim per var, at that var's
path, with format, dimensions, generation and convergence. It SHALL hold no
pixels.

#### Scenario: Layout after a render starts
- **WHEN** a render of one product with `color` and `depth` vars starts
- **THEN** the output index holds the product's prim and its two var prims at their stage paths, each var giving its format and dimensions

#### Scenario: Products leave
- **WHEN** a later render request drops a product
- **THEN** the output index removes that product's prim and its vars

### Requirement: Output notices on the owning thread
The output index SHALL notify only on the thread that owns the renderer,
when the host polls it and during `update`. A poll SHALL dirty each var with
a new snapshot at its generation, once per poll whatever the snapshot count,
and dirty the root's state when the run state changed.

#### Scenario: Progress observed
- **WHEN** a var gets three snapshots between two polls
- **THEN** the next poll dirties that var's generation once, and its generation equals the latest snapshot's

#### Scenario: No poll, no notice
- **WHEN** the render thread makes snapshots and the owning thread does not poll or update
- **THEN** no observer of the output index is called

#### Scenario: State transition
- **WHEN** the render converges between two polls
- **THEN** the next poll dirties the root's state, and the root then reads `converged`

### Requirement: Product writing
When a request asks the renderer to write products, a renderer declaring
that ability SHALL write each product to its product name and record the
written file in the product's output prim. A renderer without it SHALL fail
the request. When the host writes, the renderer SHALL write nothing.

#### Scenario: Renderer writes
- **WHEN** a renderer that writes products converges on a request that asks it to
- **THEN** each product's file exists at its product name and its output prim names it

#### Scenario: Unsupported writer
- **WHEN** a renderer that does not write products gets a request asking it to
- **THEN** the request fails with an error naming the missing capability

### Requirement: Settings overrides as a filter
A settings override filter SHALL overlay host values on the active render
settings prim's namespaced settings, or a named settings prim's, leaving
other data unchanged. Setting, changing or clearing a key SHALL dirty only
that key's locator, and setting a key to the value it already holds SHALL
dirty nothing.

#### Scenario: Override a setting
- **WHEN** the host overrides `ri:hider:maxsamples` to 4 on the active settings prim that authors 64
- **THEN** the terminal index reads 4 at that key, and other namespaced settings are unchanged

#### Scenario: Dirty only the key
- **WHEN** the host changes that override to 8
- **THEN** the settings prim is dirtied at `renderSettings/namespacedSettings/ri:hider:maxsamples` and nowhere else

#### Scenario: Clear
- **WHEN** the host clears the override
- **THEN** the terminal index reads the authored 64 again, and that key is dirtied

#### Scenario: Same value
- **WHEN** the host sets an override to the value it already holds
- **THEN** no notice is sent

### Requirement: Commands
A controlled renderer SHALL run a command it declares, with arguments as a
data source container, and SHALL fail with an unknown-command error for any
other name.

#### Scenario: Declared command
- **WHEN** the host invokes a declared command `flushTextureCache` with no arguments
- **THEN** the call succeeds

#### Scenario: Unknown command
- **WHEN** the host invokes a command the renderer does not declare
- **THEN** the call fails with an unknown-command error naming it

### Requirement: Test renderer
The crate SHALL offer, behind a feature, a renderer meeting every
requirement above. Its pixels depend only on the scene, the var and the
sample count, so tests can predict them. It SHALL declare pausing, stopping,
progressive rendering, continuing after an edit and product writing.

#### Scenario: Predictable pixels
- **WHEN** the test renderer renders a var to 2 samples twice over the same scene
- **THEN** both frames hold the same pixels

#### Scenario: Pixels follow the scene
- **WHEN** a prim the test renderer reads is edited, updated, and rendered again
- **THEN** the new frame's pixels differ from the frame before the edit

### Requirement: Device frames
A frame SHALL hold either host pixels or a device image: an image on a
graphics device naming its backend, pixel format and size. A device image
SHALL be immutable once its frame is published: a later frame of the same
var SHALL use another image or leave a held one unchanged.

#### Scenario: A device frame names its backend
- **WHEN** a renderer publishes a frame of a var as a device image of backend `wgpu`
- **THEN** the frame held from the control handle names the backend `wgpu`, the var's pixel format and the data window's size

#### Scenario: A held device frame stays valid
- **WHEN** a thread holds a device frame and the renderer publishes a newer frame of the var
- **THEN** reading the held frame's pixels gives the pixels it held when published

### Requirement: Reading a device frame back
A device frame SHALL read its pixels back to host memory on any thread, in
the layout of host frames: all of them, or a rectangle of its data window.
Reading the whole frame twice SHALL transfer it once. A rectangle outside
the data window SHALL be an error, not a partial read.

#### Scenario: The whole frame
- **WHEN** a host reads a device frame's pixels
- **THEN** it gets the data window's pixels, rows top to bottom, tightly packed in the frame's format, as a host frame of the same image holds them

#### Scenario: One pixel under a pick
- **WHEN** a host reads the one-pixel rectangle at (3, 5) of an identifier device frame
- **THEN** it gets that pixel's identifier, and the frame's id table names its prim as for a host frame

#### Scenario: Outside the window
- **WHEN** a host reads a rectangle reaching past the data window
- **THEN** the read fails with an error naming the rectangle and the window

#### Scenario: A host frame
- **WHEN** a host reads a host frame's pixels through the same accessor
- **THEN** it gets the frame's bytes without any transfer

### Requirement: Reaching the native image
A host SHALL reach a device frame's native image only by naming its
backend's image type, and SHALL get nothing for a host frame or another
backend's image.

#### Scenario: The right backend
- **WHEN** a host asks a `wgpu` device frame for the wgpu image
- **THEN** it gets the image, and through it the texture

#### Scenario: Another backend
- **WHEN** a host asks a device frame of the test backend for the wgpu image
- **THEN** it gets nothing and can still read the frame back

### Requirement: Drivers
A host SHALL hand a renderer plugin, at construction, a list of named
drivers, as `HdDriverVector`: the graphics device it renders with is named
`renderDriver`. A driver SHALL be reachable only by naming its name and its
type. An empty list SHALL be valid.

#### Scenario: The render driver
- **WHEN** a host builds drivers holding its device as `renderDriver` and constructs a plugin with them
- **THEN** the plugin gets that device by the name `renderDriver` and its type

#### Scenario: Wrong type
- **WHEN** a plugin asks for `renderDriver` as a type the host's driver is not
- **THEN** it gets nothing

### Requirement: A plugin without its driver
A renderer plugin that needs a driver SHALL answer not supported when its
drivers lack it, with a reason naming the driver, before any renderer
filter is built. The create args schema SHALL still offer no `drivers`.

#### Scenario: A GPU renderer without a device
- **WHEN** a plugin needing a `renderDriver` was constructed with no drivers, and a host creates a renderer over a chain with it
- **THEN** creation fails as unsupported, with a reason naming `renderDriver`

### Requirement: Device frames from the test renderer
The test renderer SHALL be able to publish its frames as device images of
a test backend held in host memory, whose readback gives the pixels a host
frame of the same render would hold.

#### Scenario: Same pixels on either side
- **WHEN** the same scene is rendered by a test renderer publishing host frames and by one publishing device frames
- **THEN** reading each var's frame back gives equal pixels, and the device frames name the test backend

## ADDED Requirements

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

# Spec Delta

## MODIFIED Requirements

### Requirement: Capabilities
A controlled renderer SHALL declare, without rendering, whether it can
pause, stop, render progressively, continue after an edit and write
products, its material render contexts, and its AOV, setting and command
descriptors. A setting descriptor SHALL carry a name, a type, a default, a
display name, a help text and a group; an AOV descriptor a name and a pixel
format; a command descriptor a name and its arguments.

#### Scenario: Capabilities read before any render
- **WHEN** a host asks a newly created renderer for its capabilities
- **THEN** it gets the flags and descriptors, and the run state is still `Idle`

#### Scenario: Setting descriptor
- **WHEN** a renderer declares a setting `ri:hider:maxsamples` of type int with default 64
- **THEN** the descriptor gives that namespaced name, the int type and the default 64

#### Scenario: Setting described for a person
- **WHEN** a renderer declares a setting with the display name `Max Samples`, a help text and the group `Sampling`
- **THEN** the descriptor and the output index's capabilities give that display name, help text and group

#### Scenario: A path setting
- **WHEN** a renderer declares a setting naming a camera prim, with no default
- **THEN** the descriptor's type is a prim path, and its default is the empty value

#### Scenario: Undescribed setting
- **WHEN** a renderer declares a setting without a display name
- **THEN** the descriptor's display name is the setting's name, and its help text and group are empty

### Requirement: Frame snapshots
The control handle SHALL give the latest frame of a rendered var as an
immutable snapshot readable from any thread. It holds its pixel format, its
display and data windows, its sample count, a generation that grows with
each snapshot of that var, the render that made it and the view that render
read. Later snapshots SHALL NOT alter one already held.

#### Scenario: Snapshot stays valid
- **WHEN** a thread holds a frame and the renderer makes a newer one
- **THEN** the held frame's pixels and generation are unchanged, and the newer frame's generation is greater

#### Scenario: Unknown var
- **WHEN** a frame is asked for a var that is not being rendered
- **THEN** no frame is given

#### Scenario: Frames name their render
- **WHEN** the host starts a render, then starts another after an edit
- **THEN** every frame of the second render names a render id greater than every frame of the first

### Requirement: Test renderer
The crate SHALL offer, behind a feature, a renderer meeting every
requirement above. Its pixels depend only on the scene, the var and the
sample count, so tests can predict them. It SHALL declare pausing, stopping,
progressive rendering, continuing after an edit and product writing, an
identifier AOV, and settings with display names, help and groups.

#### Scenario: Predictable pixels
- **WHEN** the test renderer renders a var to 2 samples twice over the same scene
- **THEN** both frames hold the same pixels

#### Scenario: Pixels follow the scene
- **WHEN** a prim the test renderer reads is edited, updated, and rendered again
- **THEN** the new frame's pixels differ from the frame before the edit

#### Scenario: Identifiers name scene prims
- **WHEN** the test renderer renders a `primId` var
- **THEN** every identifier in the frame maps, through its table, to a prim the terminal index holds

## ADDED Requirements

### Requirement: The view a frame shows
Each frame SHALL carry the view its render read when it started: the camera
prim path, the camera's transform and lens (projection, apertures and their
offsets, focal length, clipping range), the product's resolution, pixel
aspect ratio and aspect ratio conform policy, the index's time code and the
rendering colour space, each as the terminal index gave it.

#### Scenario: Camera edited after the render started
- **WHEN** a render starts through `/cam`, then the camera is moved and updated without a new render
- **THEN** the frames of that render still give the camera transform the render started with

#### Scenario: Time and colour space
- **WHEN** the scene globals' current frame is 12 and the active settings author `renderingColorSpace` `acescg`, and the host starts a render
- **THEN** each frame of that render gives time code 12 and the colour space `acescg`

#### Scenario: Unauthored values
- **WHEN** no current frame and no rendering colour space are given
- **THEN** each frame gives no time code and no colour space

### Requirement: Identifier frames name their prims
A frame of a var whose AOV is an identifier (`primId`, `instanceId`) SHALL
carry a table from each identifier its pixels hold to the absolute path of
the prim the renderer attributes it to. The value that marks no hit SHALL
not be in the table.

#### Scenario: Pick from a frame
- **WHEN** a render with a `primId` var converges over a scene whose mesh `/World/A` covers the image's centre
- **THEN** the identifier at the centre pixel of the `primId` frame maps to `/World/A` in that frame's table

#### Scenario: Background
- **WHEN** a pixel of a `primId` frame hits nothing
- **THEN** its identifier has no entry in the table

### Requirement: Every var refines
A renderer declaring progressive rendering SHALL publish a frame of every
var it renders as it refines, not only at convergence. The frames a pass
publishes SHALL share their sample count.

#### Scenario: A data var while rendering
- **WHEN** a progressive render of `color` and `depth` reaches 2 of 8 samples
- **THEN** both vars have a frame, each holding 2 samples

### Requirement: The memory a renderer holds
A controlled renderer SHALL report the host and device bytes it holds as
the stats `hostBytes` and `deviceBytes`, from its creation on, zero where it
holds nothing on a side. The control handle SHALL give the stats from any
thread, and the output index's root SHALL hold them.

#### Scenario: Read from another thread
- **WHEN** another thread asks the control handle for the stats of a renderer that has loaded a scene
- **THEN** it gets `hostBytes` and `deviceBytes`, and the output index's root gives the same values after the next poll

#### Scenario: Before any render
- **WHEN** a host asks a newly created renderer's control handle for its stats
- **THEN** `hostBytes` and `deviceBytes` are present

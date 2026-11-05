## Purpose

Packages a render delegate written in Rust against Moho's scene index
contract as a renderer plugin of OpenUSD's C++ Hydra, so that usdview and
other Hydra applications render with it.

## ADDED Requirements

### Requirement: A loadable renderer plugin
A Rust delegate exported through the binding SHALL build into a Hydra
renderer plugin: a shared library with its `plugInfo.json`. Placed on
`PXR_PLUGINPATH_NAME`, Hydra SHALL discover it, report it supported, and
list it under the display name the delegate declares.

#### Scenario: Listed in usdview
- **WHEN** usdview starts with the plugin's directory on `PXR_PLUGINPATH_NAME`
- **THEN** its renderer menu lists the delegate's display name, and choosing it renders with the delegate

#### Scenario: Unsupported environment
- **WHEN** the delegate reports itself unsupported (for example, a device it needs is missing)
- **THEN** Hydra does not offer it, and no other renderer is affected

### Requirement: The scene through the terminal scene index
The delegate SHALL receive the render index's terminal scene index,
bridged as a Moho scene index, once the render index is built. It SHALL
receive the notices sent since its last update in order, applied on the
thread that drives Hydra's sync, before any Hydra 1.0 prim sync runs.

#### Scenario: Initial population
- **WHEN** usdview opens a stage with the delegate selected
- **THEN** the delegate is given the terminal scene index and receives added entries for the stage's prims at its first update

#### Scenario: An edit in usdview
- **WHEN** a prim's visibility is toggled in usdview
- **THEN** at the next update the delegate receives that prim dirtied at `visibility`, and reads the new value through the scene index

### Requirement: Supported prim types
The plugin SHALL report the rprim, sprim and bprim types the delegate
declares. Prims of those types SHALL get no Hydra 1.0 sync work: the
delegate reads them only through the scene index. Render buffers and
cameras SHALL also be available to render passes, as in the next
requirements.

#### Scenario: Declared types
- **WHEN** the delegate declares `mesh`, `basisCurves`, `camera`, `distantLight` and `renderBuffer`
- **THEN** Hydra reports exactly those types as supported, and the scene index gives prims of other types too

### Requirement: Render passes
A render pass SHALL hand the delegate, on each execution:
- the camera's world-to-view and projection matrices;
- the camera's scene path when it has one;
- the framing (data window, display window, pixel aspect) or the viewport;
- the render tags;
- each AOV binding's name, clear value and render buffer.
A render pass SHALL report converged when the delegate does.

#### Scenario: usdview's free camera
- **WHEN** the user orbits usdview's free camera
- **THEN** the next execution gives the delegate the new view matrix, and the image follows the camera

#### Scenario: A scene camera
- **WHEN** the user looks through `/World/cam` in usdview
- **THEN** the execution gives the delegate the camera path `/World/cam`, whose data it reads through the scene index

### Requirement: Render buffers
The delegate SHALL own each render buffer's memory and format. A buffer
SHALL be readable by mapping it on the CPU. A delegate that offers a GPU
texture SHALL also give it as an Hgi texture handle. usdview SHALL present
either kind without a copy made by the application.

#### Scenario: CPU presentation
- **WHEN** the delegate writes `color` into a CPU buffer of `float16x4`
- **THEN** usdview shows that image, and the `depth` AOV it requests is resolved from the delegate's `depth` buffer

#### Scenario: Resize
- **WHEN** usdview's viewport is resized
- **THEN** the delegate is asked to reallocate its buffers at the new size before the next execution

### Requirement: AOVs
The plugin SHALL answer each AOV name with the delegate's descriptor
(format, multisampling, clear value), or with an unsupported descriptor
when the delegate has none. usdview SHALL offer the delegate's AOVs in its
AOV menu.

#### Scenario: An extra AOV
- **WHEN** the delegate describes `normal` and the user picks it in usdview's AOV menu
- **THEN** the render pass is given a `normal` binding, and usdview shows that buffer

### Requirement: Picking and selection highlight
The plugin SHALL give the delegate, for each prim path it draws, the id
Hydra knows the prim by. A delegate writing those ids into its `primId`
AOV, and instance indices into `instanceId`, SHALL let usdview pick prims
in the viewport and highlight the selection.

#### Scenario: Picking a mesh
- **WHEN** the user clicks a mesh drawn by the delegate in usdview
- **THEN** usdview selects that mesh's prim and highlights it in the viewport

### Requirement: Render settings
The plugin SHALL report the delegate's render setting descriptors, read
and write their values, and report a settings version that changes when a
value does. usdview SHALL show them in its settings dialog.

#### Scenario: A setting changed in usdview
- **WHEN** the user changes the delegate's `maxSamples` in usdview's renderer settings
- **THEN** the delegate receives the new value and the settings version increases

### Requirement: Progressive control
When the delegate supports it, the plugin SHALL report pause and stop as
supported, and SHALL pass pause, resume, stop and restart to the delegate.
It SHALL report the delegate's paused and stopped state.

#### Scenario: Pause in usdview
- **WHEN** the user pauses rendering in usdview
- **THEN** the delegate is paused, reports paused, and resumes from where it was when resumed

### Requirement: Commands and stats
The plugin SHALL report the delegate's commands, with their names,
descriptions and arguments, and SHALL invoke them with their argument
values. It SHALL report the delegate's render stats as a dictionary.

#### Scenario: A command from usdview
- **WHEN** the user runs a delegate command from usdview's renderer commands menu
- **THEN** the delegate's command runs with the arguments given

### Requirement: Material and renderer choices
The plugin SHALL report the delegate's material binding purpose, material
render contexts, material network selector, render settings namespaces
and shading systems.

#### Scenario: Preview materials
- **WHEN** the delegate declares the `preview` binding purpose and no render context
- **THEN** the scene index gives its prims the preview-purpose bindings and UsdPreviewSurface networks

### Requirement: Renderer scene index filters
A delegate SHALL be able to apply Moho scene index filters, its renderer
filters included, over the bridged terminal scene index. It SHALL see
their output as its scene. The C++ chain SHALL be left as Hydra builds it.

#### Scenario: Moho's dependency forwarding over the bridge
- **WHEN** a delegate applies a Moho renderer filter over the bridged index
- **THEN** it reads that filter's prims and notices, and other renderers in the same application see the C++ chain unchanged

### Requirement: Teardown
Switching renderer, closing the stage, or quitting SHALL stop the
delegate's work and release its scene index handles, buffers and threads
before Hydra destroys the render index. Nothing SHALL be read from C++
after that.

#### Scenario: Switching renderer
- **WHEN** the user switches from the delegate to Storm in usdview
- **THEN** the delegate's render threads stop, and usdview keeps running with Storm

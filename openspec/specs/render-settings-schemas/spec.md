# render-settings-schemas Specification

## Purpose
How a scene index exposes UsdRender: render settings, products, vars and
passes, flattened into Hydra 2.0's render settings schema, with renderer
settings kept as namespaced data.

## Requirements

### Requirement: Render settings
A `RenderSettings` prim SHALL have type `renderSettings` and SHALL expose
`__usdRenderSettings`, as `UsdImagingUsdRenderSettingsSchema` defines it:
`resolution`, `pixelAspectRatio`, `aspectRatioConformPolicy`,
`dataWindowNDC`, `disableMotionBlur`, `disableDepthOfField`, `camera`,
`includedPurposes`, `materialBindingPurposes`, `renderingColorSpace`,
`instantaneousShutter` (UsdImaging takes `UsdRenderSettings`' attribute
names with the inherited `UsdRenderSettingsBase` ones,
`dataSourceRenderPrims.cpp`), `products` (the products relationship's
targets) and `namespacedSettings` (every other authored attribute and
relationship by full name, such as `myRenderer:samplesPerPixel`).

A render settings flattening index SHALL give it `renderSettings`, with
exactly the fields `UsdImagingRenderSettingsFlatteningSceneIndex` fills:
`namespacedSettings`, `camera`, `disableMotionBlur`, `disableDepthOfField`,
`includedPurposes`, `materialBindingPurposes`, `renderingColorSpace` and
`renderProducts`, the settings' products flattened with their vars. The
flattened `renderSettings` SHALL carry no field outside those, so no
`instantaneousShutter`, which stays in `__usdRenderSettings` only.

#### Scenario: Settings with a product
- **WHEN** `/Render/settings` names camera `/cam`, resolution 1920×1080, `myRenderer:samplesPerPixel = 64` and one product
- **THEN** its `__usdRenderSettings` gives the camera and the resolution; its flattened `renderSettings` gives the camera, `myRenderer:samplesPerPixel` 64 in `namespacedSettings`, and one product in `renderProducts` whose resolution is 1920×1080

#### Scenario: An instantaneous shutter
- **WHEN** `/Render/settings` authors `instantaneousShutter = true`
- **THEN** its `__usdRenderSettings` gives `instantaneousShutter` true, and its flattened `renderSettings` has no `instantaneousShutter`

### Requirement: Products and vars
A `RenderProduct` SHALL have type `renderProduct` and SHALL expose
`__usdRenderProduct` (`resolution`, `pixelAspectRatio`,
`aspectRatioConformPolicy`, `dataWindowNDC`, `instantaneousShutter`, `disableMotionBlur`,
`disableDepthOfField`, `camera`, `productType`, `productName`, `orderedVars`
and `namespacedSettings`). A `RenderVar` SHALL have type `renderVar` and
SHALL expose `__usdRenderVar` (`dataType`, `sourceName`, `sourceType` and
`namespacedSettings`).

Each flattened product SHALL give, as `HdRenderProductSchema`:
- its `path`, `type`, `name` and `cameraPrim`;
- `resolution`, `pixelAspectRatio`, `aspectRatioConformPolicy` and `dataWindowNDC`;
- `disableMotionBlur` and `disableDepthOfField`;
- its own `namespacedSettings`, including `driver:parameters:*`;
- `renderVars`, in `orderedVars` order.

A product's own opinion SHALL override the settings' for camera,
resolution, aspect and motion. Each flattened var SHALL give its `path`,
`dataType`, `sourceName`, `sourceType` and `namespacedSettings`.

#### Scenario: A product overrides the camera
- **WHEN** a product targets camera `/cam2` while its settings target `/cam`
- **THEN** the flattened product's `cameraPrim` is `/cam2`

#### Scenario: Vars in order
- **WHEN** a product's `orderedVars` targets `color` then `depth`
- **THEN** its `renderVars` list them in that order with their source names and data types

#### Scenario: Stage-level data
- **WHEN** a consumer reads a render var prim at the stage index
- **THEN** it finds `__usdRenderVar` and no `renderVar` container

### Requirement: Render passes
A `RenderPass` prim SHALL have type `renderPass` and SHALL expose
`renderPass` with `passType` and `renderSource`, and SHALL expose every
collection it applies as `collections`, its schema's built-in ones
included: `renderVisibility`, and `prune`, `matte` and `cameraVisibility`
when applied.

#### Scenario: A pass and its source
- **WHEN** a render pass targets `/Render/settings` as its render source
- **THEN** its `renderPass/renderSource` is `/Render/settings`

#### Scenario: A pass's collections
- **WHEN** a render pass applies `prune`, `matte` and `cameraVisibility` collections
- **THEN** its `collections` hold `renderVisibility`, `prune`, `matte` and `cameraVisibility`, each with its complete membership expression

### Requirement: Active settings
The scene globals SHALL name the active render settings prim, from the
stage's `renderSettingsPrimPath` metadata.

#### Scenario: Stage metadata
- **WHEN** the stage authors `renderSettingsPrimPath = "/Render/final"`
- **THEN** the scene globals' active render settings prim is `/Render/final`

### Requirement: Dependencies across prims
The flattened render settings SHALL declare, as
`UsdImagingRenderSettingsFlatteningSceneIndex` does, in `__dependencies`:
each flattened field on the settings' own `__usdRenderSettings` field it
comes from, `renderSettings/renderProducts` on each product's
`__usdRenderProduct` and each var's `__usdRenderVar`, and `__dependencies`
itself on `__usdRenderSettings/products` (so that a change to the product
list re-reads the dependencies). The flattening index SHALL NOT dirty
`renderSettings` itself: a consumer that adds dependency forwarding gets
the settings dirtied for the affected locators.

#### Scenario: A var changes
- **WHEN** a var's `sourceName` changes and updates are applied, with dependency forwarding after the chain
- **THEN** the settings that list it are dirtied for `renderSettings/renderProducts`

#### Scenario: Without forwarding
- **WHEN** the same edit is read through `chain::standard` alone
- **THEN** the var is dirtied for `__usdRenderVar/sourceName` and the settings are not dirtied

### Requirement: Typed views over render settings
`moho::schema` SHALL give typed views, as Hydra's schema classes:
`RenderSettingsSchema` over a prim's `renderSettings`
(`HdRenderSettingsSchema`), `RenderProductSchema` over each entry of its
`renderProducts` in order (`HdRenderProductSchema`), and
`UsdRenderSettingsSchema` over `__usdRenderSettings`
(`UsdImagingUsdRenderSettingsSchema`). The views SHALL change no data,
layout or notice.

#### Scenario: Settings with products
- **WHEN** `/Render/settings` names camera `/cam`, resolution 1920×1080 and products `beauty` (no resolution of its own) and `aovs` (camera `/cam2`, resolution 960×540), and its flattened data is read through `RenderSettingsSchema`
- **THEN** the camera is `/cam`, there are two products in that order, `beauty`'s resolution is 1920×1080 and camera `/cam`, and `aovs`'s resolution is 960×540 and camera `/cam2`

#### Scenario: Settings without products
- **WHEN** a settings prim authors resolution 640×480 and conform policy `cropAperture` and no products
- **THEN** `RenderSettingsSchema` gives no product, and `UsdRenderSettingsSchema` gives 640×480 and `cropAperture`

### Requirement: Members read by their Hydra names
Each view SHALL read its members by their Hydra names, SHALL answer `None`
for a member that is absent or holds another type, and SHALL read a path
member as the first element of its path array.

#### Scenario: A schema fallback
- **WHEN** product `beauty` and its settings author no pixel aspect ratio
- **THEN** its `RenderProductSchema` gives UsdRender's fallback, 1.0

#### Scenario: An absent member
- **WHEN** a settings prim authors no camera relationship
- **THEN** `RenderSettingsSchema` and `UsdRenderSettingsSchema` answer `None` for the camera

#### Scenario: A camera path
- **WHEN** the settings' camera is the one-element path array `[/cam]`
- **THEN** `RenderSettingsSchema` gives the camera `/cam`

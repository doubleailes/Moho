## MODIFIED Requirements

### Requirement: Render settings
A `RenderSettings` prim SHALL have type `renderSettings` and SHALL expose
`__usdRenderSettings`, as `UsdImagingUsdRenderSettingsSchema` defines it:
`resolution`, `pixelAspectRatio`, `aspectRatioConformPolicy`,
`dataWindowNDC`, `disableMotionBlur`, `disableDepthOfField`, `camera`,
`includedPurposes`, `materialBindingPurposes`, `renderingColorSpace`,
`products` (the products relationship's targets) and `namespacedSettings`
(every other authored attribute and relationship by full name, such as
`myRenderer:samplesPerPixel`).

A render settings flattening index SHALL give it `renderSettings`, with
exactly the fields `UsdImagingRenderSettingsFlatteningSceneIndex` fills:
`namespacedSettings`, `camera`, `disableMotionBlur`, `disableDepthOfField`,
`includedPurposes`, `materialBindingPurposes`, `renderingColorSpace` and
`renderProducts`, the settings' products flattened with their vars. Settings
SHALL carry no field outside UsdImaging's schemas (no
`instantaneousShutter`).

#### Scenario: Settings with a product
- **WHEN** `/Render/settings` names camera `/cam`, resolution 1920×1080, `myRenderer:samplesPerPixel = 64` and one product
- **THEN** its `__usdRenderSettings` gives the camera and the resolution; its flattened `renderSettings` gives the camera, `myRenderer:samplesPerPixel` 64 in `namespacedSettings`, and one product in `renderProducts` whose resolution is 1920×1080

### Requirement: Products and vars
A `RenderProduct` SHALL have type `renderProduct` and SHALL expose
`__usdRenderProduct` (`resolution`, `pixelAspectRatio`,
`aspectRatioConformPolicy`, `dataWindowNDC`, `disableMotionBlur`,
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

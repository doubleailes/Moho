# render-settings-schemas Specification

## Purpose
How a scene index exposes UsdRender: render settings, products, vars and
passes, flattened into Hydra 2.0's render settings schema, with renderer
settings kept as namespaced data.

## Requirements

### Requirement: Render settings
A `RenderSettings` prim SHALL have type `renderSettings` and SHALL expose
`renderSettings` with:
- `camera`, `includedPurposes`, `materialBindingPurposes` and `renderingColorSpace`;
- `disableMotionBlur` and `disableDepthOfField`;
- `renderProducts`, the settings' products flattened with their vars;
- `namespacedSettings`, every other authored attribute and relationship by full name, such as `crust:samplesPerPixel`.

Settings SHALL also give `resolution`, `pixelAspectRatio`,
`aspectRatioConformPolicy`, `dataWindowNDC` and `instantaneousShutter`.

#### Scenario: Settings with a product
- **WHEN** `/Render/settings` names camera `/cam`, resolution 1920×1080, `crust:samplesPerPixel = 64` and one product
- **THEN** its `renderSettings` gives the camera, the resolution, `crust:samplesPerPixel` 64 in `namespacedSettings`, and one product in `renderProducts`

### Requirement: Products and vars
Each flattened product SHALL give:
- its `path`, `type`, `name` and `cameraPrim`;
- `resolution`, `pixelAspectRatio`, `aspectRatioConformPolicy` and `dataWindowNDC`;
- `disableMotionBlur` and `disableDepthOfField`;
- its own `namespacedSettings`, including `driver:parameters:*`;
- `renderVars`, in `orderedVars` order.

A product's own opinion SHALL override the settings' for camera,
resolution, aspect and motion. Each var SHALL give its `path`, `dataType`,
`sourceName`, `sourceType` and `namespacedSettings`.

#### Scenario: A product overrides the camera
- **WHEN** a product targets camera `/cam2` while its settings target `/cam`
- **THEN** the flattened product's `cameraPrim` is `/cam2`

#### Scenario: Vars in order
- **WHEN** a product's `orderedVars` targets `color` then `depth`
- **THEN** its `renderVars` list them in that order with their source names and data types

### Requirement: Render passes
A `RenderPass` prim SHALL have type `renderPass` and SHALL expose
`renderPass` with `passType` and `renderSource`, and SHALL expose its
`matte` and `cameraVisibility` collections as `collections`.

#### Scenario: A pass and its source
- **WHEN** a render pass targets `/Render/settings` as its render source
- **THEN** its `renderPass/renderSource` is `/Render/settings`

### Requirement: Active settings
The scene globals SHALL name the active render settings prim, from the
stage's `renderSettingsPrimPath` metadata.

#### Scenario: Stage metadata
- **WHEN** the stage authors `renderSettingsPrimPath = "/Render/final"`
- **THEN** the scene globals' active render settings prim is `/Render/final`

### Requirement: Dependencies across prims
A change to a product, a var, or the settings' camera relationship SHALL
dirty the settings whose flattened `renderSettings` includes it.

#### Scenario: A var changes
- **WHEN** a var's `sourceName` changes and updates are applied
- **THEN** the settings that list it are dirtied for `renderSettings`

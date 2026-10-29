## ADDED Requirements

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

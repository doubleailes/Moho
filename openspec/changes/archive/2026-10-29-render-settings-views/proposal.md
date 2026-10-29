## Why

The Hydra parity changes moved render settings to `HdRenderSettingsSchema`'s
layout: the flattened `renderSettings` keeps the camera, while resolution,
pixel aspect ratio, conform policy and data window live on each entry of
`renderProducts`, resolved over the settings'. A settings prim with no
product carries them only in its stage data, `__usdRenderSettings`. A
consumer that still read `renderSettings/resolution` got nothing, without
an error, and the edits reach `renderSettings` only through dependency
forwarding. The layout is Hydra's and stays. What is missing is the typed
view Moho gives every other schema (`MeshSchema`, `XformSchema`, ...), which
names where each field lives, and a note in the architecture on what the
parity changes moved.

## What Changes

- Typed views in `moho::schema`, as Hydra's schema classes:
  `RenderSettingsSchema` (`HdRenderSettingsSchema`: camera, namespaced
  settings, purposes, colour space, motion blur and depth of field, and the
  products), `RenderProductSchema` (`HdRenderProductSchema`: path, type,
  name, camera, resolution, pixel aspect ratio, conform policy, data window,
  motion blur, depth of field, namespaced settings) and
  `UsdRenderSettingsSchema` (`UsdImagingUsdRenderSettingsSchema`: the stage
  data, for a settings prim with no product).
- `docs/architecture.md`: what the parity changes moved (render settings
  fields, the up axis, light parameters, subsets, native instancing paths)
  and the renderer filters a consumer adds to be notified of them.
- No data, notice or layout changes: the views read what is already there.

## Capabilities

### Modified Capabilities
- `render-settings-schemas`: typed views over the flattened settings, their
  products and the stage's settings data.

## Impact

- `crates/moho/src/schema.rs`; a test file `crates/moho/tests/render_schema.rs`.
- `docs/architecture.md`.
- New public API only.

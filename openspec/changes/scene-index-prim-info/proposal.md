## Why

Moving Crust's importer onto Moho (crust-render's `moho-import`, S2) stopped
at two gaps that block its lights and transforms (M8 and M9 in its design):

- Crust needs each prim's USD type name and applied API schemas. It warns
  on unsupported types, tells `DomeLight` from `DomeLight_1`, and applies
  ShapingAPI's cone fallback. Hydra's `UsdImagingUsdPrimInfoSchema` carries
  them; Moho's `__usdPrimInfo` holds only the native instancing fields.
- Crust warns about time-sampled transforms when a render gives no frame. At
  the default time a sampled data source reports no sample times, so the
  index cannot say whether an attribute is time-sampled.

## What Changes

- **M8.** `__usdPrimInfo` gains `specifier`, `typeName`, `isLoaded`,
  `apiSchemas` and `kind`, as `UsdImagingUsdPrimInfoSchema` defines them,
  and every prim carries it.
- **M9.** Each attribute's generic entry gains `mightBeTimeVarying`, USD's
  `ValueMightBeTimeVarying`, which does not depend on the index's time.
  This extends Moho's generic data; Hydra's schemas have no counterpart.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `stage-scene-index`: prim info as Hydra defines it; time variance in the
  generic attribute data.

## Impact

- `crates/moho`: `stage/prim_info.rs`, `stage/generic.rs`, `tokens.rs`.
- crust-render's `moho-import` reads them for lights (10.x) and transforms
  (5.1, 5.2) once Moho's rev is bumped.

## Why

Moving Crust's instancing onto Moho (crust-render's `moho-import`, 12.1)
found gap M11. The native and point instancing filters serve a prototype's
prims from a new place in the namespace (`MohoNiPropagatedPrototypes/...`,
`<prototype>/ForInstancer<hash>`), but the path values in their data still
name the source: a material bound inside a native prototype names
`/__Prototype_N/Looks/red`, which the native instancing filter hides. The
binding then resolves to a material that is not in the index. Hydra's
prototype propagation re-roots path values with the prims
(`UsdImagingRerootingContainerDataSource`).

## What Changes

- The native instancing filter wraps each propagated prototype prim's data
  so that every path value at or under its `/__Prototype_N` root names the
  same prim under the propagated root. The point instancing filter does the
  same from the original prototype to its `ForInstancer` copy.
- Paths outside the source root are left alone, as Hydra leaves them, and
  `primOrigin` keeps naming the USD prim.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `instancing-schemas`: path values inside propagated prototypes.

## Impact

- `crates/moho`: new `filter/rerooting.rs`, `filter/native_instancing.rs`,
  `filter/point_instancing.rs`.
- crust-render's `moho-import` reads prototype materials directly once its
  Moho rev is bumped.

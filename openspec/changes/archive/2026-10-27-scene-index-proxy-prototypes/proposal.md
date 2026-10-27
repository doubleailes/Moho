## Why

crust-render's `moho-import` (12.4) found gap M12 on Moana: in
`isCoral`, a point instancer's prototypes target prims inside a native
instance (`Prototypes/xgStaghorn` is instanceable, and each target is an
instance proxy). The stage scene index holds no instance proxies, so the
point instancing filter copied each target as an empty, typeless prim, and
about 2.9 million geometries disappeared. No fixture covered a point
instancer targeting prims inside an instance.

## What Changes

- A point instancer's prototype target that the index does not hold, but
  that lies inside a native instance, is copied from the same prim in that
  instance's propagated prototype (native instancing runs first in Moho's
  chain, so the propagated prototype is in the point instancing filter's
  input).
- Such copies go under the instancer, at
  `I/ForInstancer<hash>/prototype<i>` (a typeless scope holds them), not
  under the propagated prototype, whose native instances would otherwise
  draw them too.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `instancing-schemas`: point instancer prototypes inside native instances.

## Impact

- `crates/moho/src/filter/point_instancing.rs`.
- crust-render's `moho-import` reruns Moana (12.4) on the Moho rev.

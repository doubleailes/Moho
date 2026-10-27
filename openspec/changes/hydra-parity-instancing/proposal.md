## Why

Moho's instancing diverges from UsdImaging's in order, layout and data
(`hydra-parity-stage/audit.md`, sections 1, 2, 8 and 10). Native instancing
runs before point instancing, the reverse of Hydra, which leaves a native
instance inside a point instancer's prototype drawn once instead of
scattered. The native instancer lacks the visibility mask, so invisible
instances are drawn, and its prototype carries none of the instance's
binding data, so a prototype bound only at the instance resolves no
material. Prototype paths, names and `instancedBy` differ from what an
`hdcrust` delegate would read from Hydra. This is the second of four parity
changes; it depends on `hydra-parity-stage` (block data source, exposed
overs, `model`/`geomModel` data).

## What Changes

- **BREAKING** Point instancer propagation runs first, before native
  instancer propagation, as `UsdImagingCreateSceneIndices` chains them.
- **BREAKING** Native instancing takes Hydra's layout:
  `<scope>/UsdNiPropagatedPrototypes/<bindingHash>/<__Prototype_N>/UsdNiInstancer`
  with the prototype at `.../UsdNiInstancer/UsdNiPrototype`, the scope being
  the enclosing prototype root (a point instancer's copy, an enclosing
  native prototype, or `/`). `MohoNiPropagatedPrototypes`, `NoBindings`,
  `Bindings<hash>`, `P<identity>`, `instancer` and `prototype` go.
- **BREAKING** Instances are grouped by Hydra's key: the USD prototype, the
  set of constant primvar names and roles, and the flattened
  `usdMaterialBindings`, `purpose`, `geomModel`, `model` and `__usdUpAxis`,
  re-rooted to the prototype. `niPrototypeIdentity` and
  `stage/instancing.rs` (the rebuilt openusd instancing key) are deleted.
- Flattening runs inside native instancer propagation, for the stage and
  for each prototype, as Hydra's does; the `<bindingHash>` prim carries a
  copy of the group's instance data, so a binding authored only on the
  instance reaches the prototype's prims. Flattening also covers `model`,
  `geomModel` and `__usdUpAxis`. The standalone `FlatteningSceneIndex`
  stays a public filter, no longer a step of `chain::standard`.
- The native instancer gains `instancerTopology/mask` (each instance's
  flattened visibility), every constant primvar of its instances as an
  instance-rate primvar, and `instancedBy` when nested. Prototype prims
  carry `instancedBy`; an instance prim's renderable type is blanked.
- **BREAKING** Point instancers as UsdImaging: `instanceIndices` keep
  inactive instances (masked out), and a prototype whose every instance is
  inactive is pruned with its indices; copies are named
  `ForInstancer<hash>` from Hydra's hash inputs (the instancer path in the
  prototype's input namespace and the enclosing copy root), hashed with a
  port of `TfHash`; the copy root gets `resetXformStack` as an overlay and
  `visibility = true` as an underlay; `instancedBy` is an underlay; prims
  under a nested instancer or an `over` inside a copy are unrenderable; the
  original prototype records its copies in
  `__usdPrimInfo/piPropagatedPrototypes`.
- **BREAKING** M12 is removed: a point instancer prototype target inside a
  native instance (an instance proxy) is copied as the empty prim the stage
  index gives for it, as in UsdImaging 2.0.
- An instance proxy path translation filter (a port of
  `UsdImaging_InstanceProxyPathTranslationSceneIndex`), configured for
  `usdMaterialBindings` only, follows the propagation.
- Draw mode (`UsdImagingDrawModeSceneIndex`) is explicitly deferred:
  `chain::standard` matches `UsdImagingCreateSceneIndices` with
  `addDrawModeSceneIndex = false`. Native propagation takes Hydra's
  per-prototype append callback so it can be added later without changing
  the layout.
- Bugs the audit found, fixed by parity: a binding only at the instance
  resolves on the prototype's prims; a native instance inside a point
  instancer prototype is scattered by the point instancer; invisible native
  instances are masked out.

Pending changes this change supersedes (archived, unchanged, before
`hydra-parity-stage` is):
- `scene-index-proxy-prototypes`: "Point instancer prototypes inside native
  instances" is removed.
- `scene-index-rerooting`: "Nested point instancers" is modified (copy
  names from Hydra's hash inputs); "Path values inside propagated
  prototypes" stands.
- `scene-index-prim-info`: nothing further (its fields were settled by
  `hydra-parity-stage`).

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `instancing-schemas`: native instancing layout, grouping and data; stable
  prototype paths removed; nested instances inside point instancer
  prototypes; point instancer topology, naming and copies; nested point
  instancers; M12 removed; instance proxy path translation.
- `filtering-scene-indices`: flattening inside native instancer
  propagation; flattened model data and up axis.
- `stage-scene-index`: prim origin on the new prototype paths.

## Impact

- `crates/moho/src/filter/{native_instancing,point_instancing,flattening,rerooting}.rs`,
  new `filter/instance_proxy_path_translation.rs`, a `TfHash` port (new
  private module), `chain.rs`, `tokens.rs`; `stage/instancing.rs` and the
  private identity accessor deleted.
- Tests: 6 deleted, about 14 modified, about 15 added (`tasks.md`).
- Consumers (crust-render, Mantle, Obsidian) read `instancedBy`, `mask`,
  instance-rate primvars and the new prototype paths; crust-render's Moana
  coral prototypes come out empty, as under UsdImaging 2.0, unless it keeps
  its own workaround.

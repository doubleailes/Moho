# Stage adapters against UsdImaging, field by field

Task 2.11 of `hydra-parity-stage`. Moho's stage adapters
(`crates/moho/src/stage/`) are compared with UsdImaging at the reference
revision (`/home/philippe.llerena/Workspace/github/doubleailes/OpenUSD` at
`70c45e83b`; paths below are relative to `pxr/usdImaging/usdImaging/` unless
stated). Each row gives the data names, their layout and the locators a
property edit dirties. The "Moho before" column is the state at the start of
the task; "Now" is after the commits listed at the end. Every divergence
found is numbered (A0 to A28), and is either fixed, reproduced as a
UsdImaging quirk, or recorded as a deviation with its reason.

General facts that hold for every family:
- A type's `Invalidate` is its own locators plus
  `UsdImagingDataSourceGprim::Invalidate` or `UsdImagingDataSourcePrim::Invalidate`
  (`visibility`, `purpose`, `xformOp:*`/`xformOpOrder` to `xform`,
  `extent`, `extentsHint`, `primvars:*`), as Moho's base adapters
  (`XformAdapter`, `PrimvarsAdapter`, `ImageableAdapter`, `ModelAdapter`)
  add theirs to every prim's.
- An attribute data source built without a locator
  (`UsdImagingDataSourceAttribute<T>::New(attr, globals)`) is never flagged
  time-varying, so a time change never dirties it. Moho now has unflagged
  members (`Member::Unflagged`, `AttributeValue::unflagged`) for those.

## 1. Mesh (`dataSourceMesh.cpp`, `stage/mesh.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| prim type | `mesh` | `mesh` | parity |
| `mesh/topology/{faceVertexCounts, faceVertexIndices, holeIndices}` | int[], flagged at their leaf | same | parity |
| `mesh/topology/orientation` | token, unflagged | flagged | **A0**, fixed |
| `mesh/subdivisionScheme`, `mesh/doubleSided` | unflagged | flagged | **A0**, fixed |
| `mesh/subdivisionTags/*` (8 members) | unflagged | flagged | **A0**, fixed |
| invalidation | topology attributes to `mesh/topology`; tags to `mesh/subdivisionTags`; `subdivisionScheme`, `doubleSided` to their leaf (`:204-247`) | same | parity |
| points, normals, velocities, accelerations | custom primvars (section 3) | | |

## 2. Curves (`dataSourceBasisCurves.cpp`, `dataSourceNurbsCurves.cpp`, `stage/curves.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| prim types | `basisCurves`, `nurbsCurves` | same | parity |
| `basisCurves/topology/{curveVertexCounts, type, basis, wrap}` | only `curveVertexCounts` flagged | all flagged | **A0**, fixed |
| basis curves invalidation | each member dirties `basisCurves/topology/<member>` (`:141-155`) | `basisCurves/topology` | **A1**, fixed |
| `nurbsCurves/{curveVertexCounts, knots, order, ranges}` | mapped, sorted names, per-leaf flags and invalidation | same members, schema order | fixed (order) |
| `pointWeights` | custom primvar `primvars/pointWeights`, vertex unless authored (`:66-120`) | `nurbsCurves/pointWeights` | **A2**, fixed (task 2.9) |
| `widths` | custom primvar on Curves | same | parity |

## 3. Primvars (`dataSourcePrimvars.cpp`, `dataSourceGprim.cpp`, `primvarUtils.cpp`, `stage/primvars.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| `primvars` named | when the prim has authored primvars; always on a gprim (`dataSourceGprim.cpp:73-83`) | when any primvar had a value | **A3**, fixed |
| `primvars` given | `null` without authored primvars, unless the gprim has custom primvars to overlay | | fixed |
| generic names | `GetAuthoredPrimvars`, less `points`, `velocities`, `accelerations` (`_RejectPrimvar`) | `primvars_with_values` (fallbacks counted), nothing rejected | **A4**, **A5**, fixed |
| generic entry given | only with an authored value | any value | **A4**, fixed |
| custom primvars | by family: PointBased `points`, `velocities`, `accelerations`, `normals`; Curves `widths`; NurbsCurves `pointWeights`; always named, given when authored, empty arrays kept; weaker than `primvars:*` | given for a non-empty value, any prim of the family | **A6**, fixed |
| `primvars:*` relationship | answered (forwarded targets, constant), not named | missing | **A7**, fixed |
| entry names | `interpolation`, `role`, [`colorSpace`], value(s), [`elementSize`] | value(s), `interpolation`, [`role`], `elementSize`, [`colorSpace`] | fixed |
| `role` | `point`, `normal`, `vector`, `color`, `textureCoordinate` only; a generic primvar without one gives `null`; a custom primvar always gives one, empty when none | also `pointIndex`, `edgeIndex`, `faceIndex`; custom omitted | **A8**, **A9**, fixed |
| `elementSize` | only when not 1; never on a custom primvar | always, 1 by default | **A10**, fixed |
| `colorSpace` | colour role only: the attribute's own, else the nearest prim from its own up applying `ColorSpaceAPI` with a non-empty, valid `colorSpace:name` (an unknown name ends the search with none), else the attribute's fallback (`usd/usd/colorSpaceAPI.cpp:212-280`), a token | own, else any ancestor's `colorSpace:name`, a string | **A11**, fixed; the validity check uses `GfColorSpace`'s built-in names (openusd has no `ComputeColorSpaceName`, `GfColorSpace` or `ColorSpaceDefinitionAPI` lookup: upstream candidate) |
| time-varying flags | per leaf: `primvars/<n>/primvarValue`, or `indexedPrimvarValue` and `indices` | `primvars/<n>` | **A12**, fixed |
| `primvars:<n>:indices` edit | `primvars/<n>:indices` (the rest of the name appended verbatim, `dataSourcePrim.cpp:899-902`) | `primvars/<n>` | **A13**, reproduced (UsdImaging's quirk) |
| a `primvars:*` property added or removed | `primvars` (`Resync` invalidation, `dataSourcePrim.cpp:895-897`) | `primvars/<n>` | **A14**, fixed (`Adapter::invalidate_resynced`) |
| custom primvar edits | only for the prim's family | any prim | **A15**, fixed |

A point instancer's instance data read as primvars
(`hydra:instanceTranslations`, ...) are left as they were; they belong to
`hydra-parity-instancing`.

## 4. Visibility, purpose, extent (`dataSourcePrim.cpp`, `stage/imageable.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| `visibility` named | every Imageable; given when authored (`:710-711`, `:772-784`) | only when authored | **A3**, fixed |
| `visibility/visibility` | `false` only when `invisible` at the globals' time, without shutter offset, retained; flagged at `visibility` | sampled | **A16**, fixed |
| `purpose` | on an Imageable with an authored or non-`default` purpose; names `purpose`, `inheritable`, `fallback` | any prim; names the present members | fixed |
| `extent` named | every Boundable; given when authored | any prim authoring `extent` | **A17**, fixed |
| missing extent corner | `(0, 0, 0)` with a warning (`:199-216`) | none | **A18**, fixed (also `extentsHint`) |
| `extentsHint`, `model` | `:733-736`, `:808-827` | missing | fixed (task 2.6) |
| invalidation | `visibility`, `purpose`, `extent`, `extentsHint` to their locators | same | parity |

## 5. Xform (`dataSourcePrim.cpp`, `stage/xform.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| `xform` named | every Xformable; given when the ops might have an effect | only when they have an effect | **A3**, fixed |
| `xform/{matrix, resetXformStack}`, flags, invalidation | `:408-474`, `:881-884` | same | parity |
| `geomXformVectors` | on an Xformable authoring `xformOpOrder`: `translation`, `rotation`, `scale`, `pivot`, `rotationOrder` (`TfEnum` name, such as `RotationOrderXYZ`) by `GetXformVectorsByAccumulation` at the globals' time; never flagged nor invalidated (`:834-858`) | missing | **A19**, fixed, quirks reproduced. openusd's `xform_vectors_by_accumulation` reads a folded rotation's angles in the stack's own order, where C++ reads them in ZYX order (its documented difference) |

## 6. Camera (`dataSourceCamera.cpp`, `stage/camera.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| prim type | `camera` | `camera` | parity |
| apertures, focal length | float × 0.1 | same | parity |
| `clippingPlanes` | float4[] to double4[] | same | parity |
| shutter and exposure members, `linearExposureScale` | Hydra names mapped to `shutter:*`, `exposure:*`; the scale computed | same | parity |
| `stereoRole` | token, listed and answered | missing | **A20**, fixed |
| `camera`'s names | `UsdGeomCamera::GetSchemaAttributeNames(false)`: USD names (`shutter:open`), no `linearExposureScale`; `Get` answers both the Hydra names and any prim attribute by name (`:196-268`) | Hydra names plus `linearExposureScale` | **A21**, reproduced (UsdImaging's quirk) |
| invalidation | each schema attribute to `camera/<hd name>`, exposure inputs also to `linearExposureScale` | same, without `stereoRole` | fixed |

## 7. Volume and field assets (`dataSourceVolume.cpp`, `usdVolImaging/dataSourceFieldAsset.cpp`, `usd/usdVol/volume.cpp`, `stage/volume.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| `volumeFieldBinding/<field>` | a single `SdfPath` | a one-element path array | parity under design D1 (A22) |
| which relationships bind | `field:` relationships with exactly one forwarded target that is a prim path; named by the relationship's base name, sorted; `Get(name)` reads `field:<name>` | the first target, any count, `field:` stripped | **A23**, **A24**, fixed; a nested `field:a:b` is named `b` and not found again, UsdImaging's quirk, reproduced |
| `field:*` edit | the whole `volumeFieldBinding` (`:95-106`) | `volumeFieldBinding/<field>` | **A25**, fixed |
| `volumeField` names | `UsdVolVolumeFieldAsset`'s attributes, then the subclass's (`fieldClass` for OpenVDB, `fieldPurpose` for Field3D); `Get` answers any attribute | five base members | **A26**, fixed. UsdImaging lists `fieldDataType` twice (base and subclass); Moho lists it once |
| field attribute edit | the whole `volumeField` | `volumeField/<member>` | **A27**, fixed |

## 8. Implicit shapes (`dataSourceImplicits-Impl.h`, `stage/implicit.rs`)

Sphere (`radius`), Cube (`size`), Cone, Cylinder and Capsule (`height`,
`radius`, `axis`), Cylinder_1 and Capsule_1 (`height`, `radiusTop`,
`radiusBottom`, `axis`), Plane (`doubleSided`, `width`, `length`, `axis`):
each attribute but `extent` at `<shape>/<attribute>`, flagged and dirtied per
leaf, as Moho's tables do; UsdImaging's mapped data source names them
sorted (`axis`, `height`, `radius`, ...), which Moho now follows (it listed
them in schema order). A shape edit does not dirty `extent`
in either. As gprims, they now name `primvars` (A3).

## 9. Geom subsets (`geomSubsetAdapter.cpp`, `stage/geom_subset.rs`)

| Item | UsdImaging | Moho before | Now |
|---|---|---|---|
| prim | its own prim, type `geomSubset`, with the prim data every prim has (material bindings among them) | typeless | **A28**, fixed |
| `geomSubset/type` | `elementType` `face` to `typeFaceSet`, `point` to `typePointSet`, anything else the empty token; unflagged | missing | fixed |
| `geomSubset/indices` | int[], unflagged | missing | fixed |
| invalidation | `indices` to the locator `indices`, `elementType` to `type`, at the top of the prim (not under `geomSubset`) | | reproduced (UsdImaging's quirk) |
| the mesh | not dirtied by a subset edit; a consumer gathers subsets from the mesh's children | | parity |

## Found outside the families compared

Recorded, not fixed here:
- UsdImaging's render settings, product, var and pass prims skip
  `UsdImagingDataSourcePrim`'s data (`dataSourceRenderPrims.cpp`, "Skip
  properties on UsdImagingDataSourcePrim"), so they carry no
  `__usdPrimInfo` or `primOrigin`; Moho's base adapters give every prim
  those. For `hydra-parity-chain`, which reworks the render settings
  flattening.
- Points, HermiteCurves, TetMesh and NurbsPatch prims have UsdImaging
  adapters (`points`, `tetMesh`, `nurbsPatch`, ...) and none in Moho, so they
  are typeless. Not in this change's scope.

## Commits

- `moho: primvars as UsdImaging's primvars data sources` (A3 primvars,
  A4 to A15)
- `moho: mesh data flagged time-varying as UsdImaging flags it` (A0 mesh)
- `moho: basis curves topology dirtied per member, as UsdImaging` (A0 and
  A1 curves)
- `moho: NURBS curve members in UsdImaging's mapped order`
- `moho: implicit shape members in UsdImaging's mapped order`
- `moho: NURBS point weights are the primvar primvars/pointWeights` (A2)
- `moho: camera data as UsdImaging's camera data source` (A20, A21)
- `moho: volumes and field assets as UsdImaging's data sources` (A23 to A27)
- `moho: xform names and geomXformVectors, as UsdImaging` (A3 xform, A19)
- `moho: visibility, purpose and extent named as UsdImaging names them`
  (A3, A16 to A18)
- `moho: geom subsets as prims, as UsdImaging's geom subset adapter` (A28)

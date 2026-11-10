# Design

## Context

UsdImaging's particle field adapter is a gprim adapter registered for
`ParticleField` with `includeDerivedPrimTypes`. Its Hydra 2.0 data source,
`UsdImagingDataSourceParticleFieldPrim`, is the gprim data source with
`primvars` overlaid by `UsdImagingDataSourceCustomPrimvars` mappings, and its
`Invalidate` maps both attributes of each pair to the one primvar. Moho's
stage index has per-type adapters looked up by exact type name, a single
derived-type case (`LightFilter`), and custom primvars for point-based prims,
curves and point instancers in `stage/primvars.rs`.

openusd (`54db9fd`, and `26c81d8` on main) ships
`openusd_schemas::vol::ParticleField3DGaussianSplat::attribute_in_use(SplatData)`,
a port of C++ `_UsesFloatAttr`: the float attribute when its value at
`EarliestTime` is a non-empty array, else the half one.

## Goals / Non-Goals

**Goals:**
- UsdImaging's data for a particle field, quirks included: names, types,
  interpolations, roles and dirty locators.
- No change to any other prim.

**Non-Goals:**
- A `particleField` schema (kernel type, projection and sorting hints):
  UsdImaging's data source leaves it as an `XXX`. The attributes are in the
  generic attribute data.
- `elementSize` or `colorSpace` on the coefficients primvar beyond what
  UsdImaging gives (see D3).
- Validating the schema's length rules: that is the renderer's job, as it is
  in Hydra.

## Decisions

**D1. Derived types through the registry.** `AdapterRegistry::set_type_and_derived`
records a type whose adapter also serves its derived types; a prim whose
type has no adapter of its own takes the first such type it `is_a`. This is
`includeDerivedPrimTypes`. `LightFilter` moves onto it (it was the one hand
written case), and `ParticleField` is the second.
*Alternative:* listing `ParticleField3DGaussianSplat` by name. A schema
deriving from `ParticleField` later would get no adapter, where UsdImaging
serves it.

**D2. The pairs live with the other custom primvars.** `stage/primvars.rs`
already overlays gprim and instancer custom primvars under `primvars:*`; the
particle field's are a third family, gated on `is_a("ParticleField")`, with
the mapping's interpolation (vertex or constant) rather than the attribute's
metadata. `particle_field.rs` holds the table and the type adapter, whose
`invalidate` is the data source's `Invalidate`.
The float-or-half question reads the float attribute's whole array (openusd
has no size-only read, a `TODO(perf)` upstream). It is asked only when a
pair's primvar is fetched, never when names are listed.

**D3. Parity over convenience for the coefficients' metadata.**
`UsdImagingDataSourceCustomPrimvars` passes no `elementSize` and computes a
colour space only for the colour role; the coefficients are `float3[]`. So
the primvar has neither. A consumer derives the element size from the
degree, `(degree + 1)²`, as the schema says; the authored `elementSize` and
`colorSpace` remain in `__usdAttributes`. A consumer that needs a colour
space Hydra does not carry adds it in a renderer filter of its own.

## Risks / Trade-offs

- [Choosing a pair reads the float array] → once per primvar fetch, as C++
  does; recorded as an upstream candidate.
- [A renderer filter keyed on prim types sees a new type] → additive; a prim
  factory ignores types it does not manage.

## Migration Plan

Additive. Merge into Moho; consumers that pin a Moho branch take it there.

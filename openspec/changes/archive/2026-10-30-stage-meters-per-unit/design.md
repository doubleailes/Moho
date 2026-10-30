# Design

## Context

`stage/globals.rs` (`SceneGlobalsAdapter`) gives the absolute root
`sceneGlobals` and `__usdUpAxis`, as `UsdImagingDataSourceStage`, reading
the stage on each `get`. `hydra-parity-stage` D7 removed
`sceneGlobals/metersPerUnit` and `__usdStageMetadata`, and D5 made root
info-only changes notify nothing. `__usdUpAxis` is flattened and is one of
the instance data sources (`INSTANCE_DATA_SOURCE_NAMES`), as in UsdImaging.

## Goals / Non-Goals

**Goals:**
- A delegate reads the stage's authored unit from the index alone.
- `sceneGlobals` stays exactly Hydra's, so the scene globals filter and
  every Hydra-shaped reader are unaffected.

**Non-Goals:**
- Other stage metadata (`framesPerSecond`, custom layer data). Each would
  need its own case; none has a consumer that cannot read the stage.
- Notifying stage metadata edits. That is a parity rule (D5) for the whole
  root, and changing it belongs to its own change.
- Converting lens values. Hydra's camera data keeps UsdImaging's fixed
  `0.1` lens unit; what a consumer does with the unit is its own business.

## Decisions

**D1. A root container of its own, `__usdMetersPerUnit/metersPerUnit`.**
It follows `__usdUpAxis`, the shape UsdImaging uses for a stage metric
(a `__usd*` container on the absolute root holding one field). The token
module `tokens::usd_meters_per_unit` mirrors `usd_up_axis`.
*Alternatives:* `sceneGlobals/metersPerUnit`, which D7 removed: it would
put a non-Hydra field into Hydra's schema, where the scene globals filter
overlays the root and a Hydra reader expects only Hydra's fields. A generic
`__usdStageMetadata`, also removed by D7: it re-opens every field for one
consumer's need.

**D2. Absent when unauthored, not the fallback.** The value is the stage's
`metersPerUnit` only when its session or root layer authors one, as openusd
reports it (`Stage::meters_per_unit`, before
`openusd_schemas::geom::stage_meters_per_unit` applies the centimetre
fallback). Crust warns when a scene with depth of field leaves the unit
unauthored, so the index must keep that distinction.
*Alternative:* the fallback included, as `__usdUpAxis` does. A consumer
could no longer tell an authored centimetre from none.

**D3. Root only: not flattened, not an instance data source.** A unit is a
property of the stage, read once per commit at `/`. Flattening it to every
prim, or keying prototypes on it, adds cost and changes instancing hashes
for nothing: every prim of one stage has the same unit.

**D4. Notification as the rest of the root.** A `metersPerUnit` edit is an
info-only change at the absolute root and notifies nothing (D5 of
`hydra-parity-stage`). The adapter reads the stage on each `get`, so a
reader that re-reads after `apply_pending_updates` sees the new value; a
structural change (a sublayer added, the stage replaced) resyncs the root
as before.

## Risks / Trade-offs

- [A consumer caching the unit misses an edit] → Same as the time range and
  the up axis today. Documented in "The absolute root", and Crust re-reads
  it on each commit.
- [Moho's root is no longer exactly UsdImaging's] → One named extension,
  spec'd as such, outside every Hydra schema container. The parity table in
  `docs/architecture.md` records it.

## Migration Plan

Additive: nothing reads `__usdMetersPerUnit` today. Merge Moho, then bump
crust-render (`hydra-only-renders`), then Mantle. Rollback: revert; no
consumer depends on it until Crust's change lands.

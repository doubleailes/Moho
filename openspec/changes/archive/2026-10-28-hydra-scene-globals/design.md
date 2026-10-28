## Context

The reference is `pxr/imaging/hdsi/sceneGlobalsSceneIndex.{h,cpp}` at
OpenUSD `70c45e83b`, identical to USD 26.08's installed header
(`HDSI_SGSI_API_VERSION 2`: a filtering scene index). Its users are
usdview's `UsdImagingGLEngine` (`_AppSceneIndices`) and Solaris. The stage
index already gives the root `sceneGlobals` as `UsdImagingDataSourceStage`
does (`startTimeCode`, `endTimeCode`, `timeCodesPerSecond`,
`activeRenderSettingsPrim` when authored); that stays as it is.

What the reference does:
- Holds six `std::optional`s: `activeRenderPassPrim`,
  `activeRenderSettingsPrim`, `primaryCameraPrim` (`SdfPath`),
  `currentFrame`, `timeCodesPerSecond` (`double`), `sceneStateId` (`int`).
  `HdSceneGlobalsSchema` also has `startTimeCode` and `endTimeCode`, but the
  filter neither holds nor sets them: they come from the input.
- `GetPrim(/)` overlays `{sceneGlobals: _SceneGlobalsDataSource}` over the
  input's root data (`HdOverlayContainerDataSource`, filter first), or uses
  it alone when the input root has none. `_SceneGlobalsDataSource` holds a
  pointer to the index: it names all six fields always and reads them live;
  an unset field is `nullptr`, so the overlay falls through to the input.
- Each setter returns early on an equal value (`currentFrame` through
  `_IsEqualTimeCode`, NaN equal to NaN; the others with `optional ==`,
  where an unset optional differs from any value and NaN differs from
  itself), stores, and when observed sends one dirtied entry at `/` with
  its field's locator.
- Added, removed and dirtied notices pass through; renames take the base
  class's default (removed and added).
- usdview registers it, through a callback, for every renderer at insertion
  phase 0, `InsertionOrderAtStart` ("insert earlier so downstream scene
  indices can query and be notified of changes"), followed by its dome light
  camera visibility and material pruning indices.
- Each `UsdImagingGLEngine::Render` calls `_usdImagingSceneIndex->SetTime`
  then `_SetSceneGlobalsCurrentFrame(params.frame)`: the host keeps the two
  together; neither scene index knows the other.

## Goals / Non-Goals

**Goals:**
- The reference's fields, setters, notices, overlay and live reads.
- A placement that matches usdview's.

**Non-Goals:**
- The host-specific indices usdview appends after it (dome light camera
  visibility, scene material pruning).
- Coupling the stage index's time to the current frame inside Moho.
- `startTimeCode` / `endTimeCode` setters: the reference has none.

## Decisions

**D1. A filter with shared, live state.** `SceneGlobalsSceneIndex` holds its
fields in an `Rc<RefCell<..>>` that the root's `sceneGlobals` container
also holds, so a container read before a set answers the new value, as
`_SceneGlobalsDataSource`'s pointer to its index does. Alternative: a
retained snapshot per `prim(/)`; rejected, it would differ from Hydra for a
consumer holding the container.

**D2. Overlay through `data_source::Overlay`.** Moho's `Overlay` is
`HdOverlayContainerDataSource` (nested containers overlaid, first data
source ending the search, unset fields `None` falling through), so the root
is `Overlay[{sceneGlobals: live}, input root]`, or the live container alone
when the input root has no data.

**D3. Values.** Paths follow Moho's convention (`docs/architecture.md`): a
one-element `Value::PathVec`. Hydra's empty `SdfPath` is still a data
source that hides the input's, so an empty path (`sdf::Path::default()`)
reads as an empty `PathVec`, not as absent (absent would let the input's
value through). `currentFrame` and `timeCodesPerSecond` are
`Value::Double`, `sceneStateId` `Value::Int`. Setters take `sdf::Path`,
`f64` and `i32`.

**D4. Equality as the reference.** `currentFrame`: equal when both NaN or
`==`, and only once set. The others: unset differs from anything, then
`==` (so `timeCodesPerSecond` set to NaN twice notifies twice). Moho's
`Observers` sends nothing without observers, which is `_IsObserved()`.

**D5. Renames.** Converted to removals and additions
(`renamed_as_removed_and_added`), the base class default the reference
inherits.

**D6. Placement: a renderer filter at phase 0, `AtStart`.** usdview
registers it as a scene index plugin for all renderers, so it fits
`chain::renderer_filters` as is: `RendererFilter::scene_globals(on_built)`
builds it at phase 0 `AtStart` (`SCENE_GLOBALS_PHASE`) and calls `on_built`
with the index, the way usdview's callback stores it in `_AppSceneIndices`
for the engine to drive. It is therefore below light linking (phase 4) and
dependency forwarding (phase 1000), not at the terminal: the brief's "near
the terminal" does not hold for usdview. A host that builds it by hand
(`SceneGlobalsSceneIndex::new(input)`) and passes it as the renderer
filters' input gets the same order unless another filter sits at a
negative phase or at phase 0 `AtStart`. Not in `chain::standard`, which is
`UsdImagingCreateSceneIndices`' chain.

**D7. The host pattern is documented, not coded.** The host calls
`stage_index.set_time(t)` and `scene_globals.set_current_frame(t.value())`
together, as `UsdImagingGLEngine::Render` does; Moho couples neither.

## Risks / Trade-offs

- [The empty-path representation (D3) is a choice Hydra cannot make for
  us] → documented in the module and here; a consumer that reads an empty
  array treats it as "no prim", as Hydra's consumers treat an empty path.
- [The reference revision moves (e.g. adds fields or handles removal of the
  active render settings prim, its `XXX` notes)] → follow in a change of
  its own, as the parity principle says.
- [A host forgets to set the current frame] → `currentFrame` reads nothing,
  as under Hydra without `SetCurrentFrame`; the docs give the pattern.

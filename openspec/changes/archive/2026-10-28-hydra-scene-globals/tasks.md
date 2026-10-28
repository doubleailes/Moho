## 1. The filter

- [x] 1.1 Add the remaining `HdSceneGlobalsSchema` tokens (`activeRenderPassPrim`, `primaryCameraPrim`, `currentFrame`, `sceneStateId`) to `tokens::scene_globals`
- [x] 1.2 Add `filter::scene_globals::SceneGlobalsSceneIndex`: pass-through of prims, children and notices (renames as removed and added), and the root's live `sceneGlobals` overlaid over the input root's data
- [x] 1.3 Add its six setters with the reference's equality and one dirtied entry at `/` per change
- [x] 1.4 Tests: unset fields (names listed, input's value or nothing), the overlay keeping the input's root data, a set field hiding the input's, an empty path, live reads, a root with no input data, pass-through notices

## 2. Notices

- [x] 2.1 Tests: each setter's notice (exact locator at `/`), no notice for an equal value, the first set notifying, `currentFrame` NaN equal to NaN, `timeCodesPerSecond` NaN notifying again
- [x] 2.2 Test: a `currentFrame` round trip (set, set again, change, read back)

## 3. Placement

- [x] 3.1 Add `RendererFilter::scene_globals(on_built)` at `SCENE_GLOBALS_PHASE` (0), `AtStart`, handing the built index to the host
- [x] 3.2 Tests: its place among light linking and dependency forwarding; the stage index's time not setting the current frame

## 4. Docs and parity

- [x] 4.1 Module docs and `docs/architecture.md`: the filter, its placement, the root's scene globals, and the host pattern (`set_time` with `set_current_frame`)
- [x] 4.2 Check against USD 26.08 with a small C++ oracle of `HdsiSceneGlobalsSceneIndex` over `UsdImagingCreateSceneIndices` (names, values and notices), and record the result

# Design

## D1. A literal port

The fallbacks are an underlay (`HdOverlayContainerDataSource::New(prim,
underlay)`): a prim's own `displayStyle` member wins. The fallback
`displayStyle` container names `refineLevel` and `cullStyle` whether set or
not, as Hydra's does, and reads the fallbacks when asked, so data read
before a change sees it. A prim with no data stays without. The refine
level change dirties `displayStyle/refineLevel`; the cull style change
dirties all of `displayStyle`, Hydra's wider locator ("insufficient
invalidation" otherwise). Every prim of the input is dirtied, the absolute
root first, as `HdSceneIndexPrimView` walks; nothing is walked when nothing
observes. Renames become removals and additions, the filtering base's
default.

## D2. Option for the fallbacks

`SetRefineLevelFallback` takes `std::optional<int>`: Moho takes
`Option<i32>`. `SetCullStyleFallback` takes a token, empty for none: Moho
takes `Option<Token>`, the empty token being `None`.

## D3. Where it runs, and the complexity

`UsdImagingGLEngine::_CreateUsdImagingSceneIndices` puts it right after
`UsdImagingSceneIndex`, before the renderer's scene index plugins, and sets
the refine level fallback in `_PreSetTime` from `_GetRefineLevel` of its
complexity. Moho documents that place (`chain` and `architecture.md`)
rather than adding a `StandardOptions` switch, which would change the
chain `UsdImagingCreateSceneIndices` builds. `refine_level_for_complexity`
ports `_GetRefineLevel` exactly (the 0.01 nudge, `f32` bins); a complexity
below 1.0 or NaN, a coding error there, is level 0, what the engine then
answers.

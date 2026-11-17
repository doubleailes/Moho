# Design

## D1. A literal port, standins as one type

`UsdImaging_DrawModeStandin`'s three subclasses become one `Standin` with a
`Kind` (`Origin`, `Bounds`, `Cards`): the prims, types, data sources and
dirty locators are UsdImaging's, quirks included (the replaced prim keeps
no type and no data; the card materials' colour locators start at the
universal render context, not at `material`; a cross card's faces sit
2^-23 apart). The cards' data is computed once at offset 0 and kept until a
card attribute or the up axis is dirtied (`_CardsDataCache`); the
`displayColor` and texture fallback read `geomModel/drawModeColor` when
asked, so a colour edit needs no new data source.

## D2. Where it runs

As `UsdImagingCreateSceneIndices`: the draw mode index is native instancer
propagation's append callback, applied to the flattened scene of the stage
and of every USD prototype, before instance aggregation; the unloaded
filter runs right after the stage index (before extent resolving), with the
stage index including unloaded prims. Hydra's default for
`addDrawModeSceneIndex` is true; Moho's `chain::standard` keeps it off (as
it was specified), and `chain::standard_with` turns either option on.

## D3. Lookup of a prim's standin

UsdImaging finds the standin of a path or its ancestor with
`std::map::lower_bound` over `SdfPath` order. Moho looks each ancestor up
in the map instead, which does not depend on how `sdf::Path` orders.

## D4. `fromTexture`

C++ opens each texture with `HioImage` to read its `worldtoscreen` (or
`worldToNDC`) metadata. openusd reads no images, so the lookup always
fails, and a `fromTexture` card has no face and an empty extent, exactly
what UsdImaging gives when no image opens. The geometry for a found matrix
is ported, so adding an image reader changes only the lookup.

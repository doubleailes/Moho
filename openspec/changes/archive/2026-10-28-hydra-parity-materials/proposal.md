## Why

Material binding resolution and material networks diverge from UsdImaging
(`hydra-parity-stage/audit.md`, sections 3, 9 and 10): Moho resolves
collection membership through the stage (USD semantics) instead of over
the scene index with Hydra's predicates, picks the wrong binding in one
per-level edge case, keeps both `usdMaterialBindings` and
`materialBindings` dirty locators, and re-dirties prims on collection
edits, which Hydra does not. Its networks lack Hydra's `__all` context,
`config`, `interface`, `nodeTypeInfo` and `renderContextNodeIdentifiers`,
and carry M10's streaming data (`__materialNodes`, `__materialNodeGraph`,
off-stage connections). Lights lack the `material` network a Hydra render
delegate reads them through. This is the third of four parity changes; it
depends on `hydra-parity-stage` (collection evaluator) and
`hydra-parity-instancing` (instance proxy path translation, flattening
inside propagation).

## What Changes

- **BREAKING** Material binding resolution as
  `UsdImagingMaterialBindingsResolvingSceneIndex`:
  - per level, ancestors first: a stronger-than-descendants collection
    binding wins and stops; else a stronger-than-descendants direct binding
    wins and stops (even when a weaker collection binding matched); else a
    matching collection binding wins and the walk continues; else the direct
    binding does;
  - collection membership is the collection prim's
    `collections/<name>/membershipExpression` judged over the input scene
    index with Hydra's predicates (`hydra-parity-stage`'s evaluator), not the
    stage's `CollectionMembership`;
  - a dirty `usdMaterialBindings` locator is replaced by `materialBindings`,
    and nothing is re-dirtied when a collection's membership changes (Hydra
    tracks no such dependency);
  - `materialBindings` is given only to prims that have
    `usdMaterialBindings`.
- **BREAKING** Material networks as `UsdImagingDataSourceMaterial`: an
  `__all` context (every shader under the material, connected or not,
  terminals keeping their namespaced names), `config` (the material's
  `config:*` attributes), `interface` (`parameters` with their mappings,
  display name and group, and `parameterOrder`), `nodeTypeInfo` for nodes
  not in `id` mode, `renderContextNodeIdentifiers` for light and light
  filter nodes. A connection to a prim the stage cannot reach is dropped, as
  UsdShade's walk drops it.
- **BREAKING** M10's stage-side data is removed: `__materialNodes`,
  `__materialNodeGraph` and connections to nodes outside the stage's mask.
- **BREAKING** Lights as UsdImaging's light API adapter: `light` lists
  `filters`, `isLight` and `materialSyncMode`, and answers any other
  parameter by name as `LookupLightParamAttribute` does; every light
  carries `material`, a network whose single node is the light prim (by its
  absolute path) feeding the terminal `light`, in the `""` and `__all`
  contexts and each context a `<context>:light:shaderId` names. An
  `inputs:*` edit dirties `light` and `material`; a `light:*` edit dirties
  `light`.

Pending changes this change supersedes (archived, unchanged, before
`hydra-parity-stage` is):
- `scene-index-material-nodes`: "Connections to shaders the stage does not
  hold" and "Shader nodes" are removed (its filtering requirement went in
  `hydra-parity-stage`).
- `scene-index-prim-info`, `scene-index-rerooting`,
  `scene-index-proxy-prototypes`, `openusd-through-moho`: untouched.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `material-schemas`: resolved binding; material networks; connections to
  shaders the stage does not hold and shader nodes removed.
- `light-schemas`: light data and its `material` network; light parameter
  dirtying.

## Impact

- `crates/moho/src/filter/material_binding.rs`, `stage/material.rs`,
  `stage/light.rs`, `tokens.rs`. The binding filter no longer takes a
  `CollectionMembership`; the trait itself is removed by
  `hydra-parity-chain`, once light linking stops using it.
- Tests: 2 deleted, about 10 modified, about 10 added (`tasks.md`).
- Consumers read materials from `materialBindings` and a context of their
  choice (or `__all`), and lights from `material` or `light`.

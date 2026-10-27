## Why

After the first three parity changes, what remains between Moho and Hydra
is the shape of the chain around UsdImaging's: light linking sits inside
`chain::standard` although Hydra's renderers add it, with its own category
ids and semantics (`hydra-parity-stage/audit.md`, sections 1, 4, 7, 10 and
11); `__dependencies` and dependency forwarding, which Hydra's light linking
and render settings flattening rely on, are missing; merging filters its
inputs' notices; and nothing gives a consumer what a Hydra 2.0 render
delegate consumes (`HdRenderDelegate::SetTerminalSceneIndex` and `Update`,
observing the terminal scene index through
`HdsiPrimTypeNoticeBatchingSceneIndex` and
`HdsiPrimManagingSceneIndexObserver`, as hdPrman's Riley path does). Crust's
delegate code is meant to run unchanged over Moho in Rust and, through
`hdcrust`, over Hydra's terminal scene index in Solaris and usdview; this
change completes the surface that makes both the same. It also states
Moho's boundaries for an upstream `openusd-imaging` crate. Fourth of four;
it depends on the other three.

## What Changes

- **BREAKING** `chain::standard` is exactly `UsdImagingCreateSceneIndices`'
  chain with `addDrawModeSceneIndex = false`,
  `displayUnloadedPrimsWithBounds = false`, no overrides callback and no
  UsdImaging plugins: stage, extent resolving, point instancer propagation,
  native instancer propagation, instance proxy path translation, material
  binding resolving, render settings flattening. Light linking leaves it.
- Renderer filters by phase: a consumer passes an ordered list of
  `(phase, order, constructor)` entries, applied over its terminal index
  (after any merge of its own) in `HdSceneIndexPluginRegistry`'s order:
  by phase, then `AtStart` before `AtEnd`, then list position. No global
  registry.
- **BREAKING** Light linking is a port of `HdsiLightLinkingSceneIndex`,
  shipped as a filter a consumer adds (hdPrman and arnold-usd add it at
  phase 4, `AtStart`): `group_<n>` category ids shared by equal
  expressions, the empty id for trivial ones; membership judged over its
  input scene index with Hydra's predicates; `categories` only on
  geometry that is not instanced, absent when empty; `instanceCategories`
  on native instancers, `categories` on point instancers;
  `light/{lightLink, shadowLink, lightFilterLink}` from the `lightLink`,
  `shadowLink` and `filterLink` collections; `__dependencies` on
  instancers; dirtying only the targets of a changed collection.
- Light filter prims (`LightFilter`, type `lightFilter`) as UsdImaging's
  light filter adapter, with their collections and `material` network.
- `__dependencies` (`HdDependenciesSchema`) and a port of
  `HdDependencyForwardingSceneIndex`, shipped as a filter a consumer adds
  (arnold-usd at phase 1000). **BREAKING** Render settings flattening emits
  `__dependencies` and stops dirtying its settings itself.
- A port of `HdsiPrimTypeNoticeBatchingSceneIndex` (notices held until
  `flush`, consolidated, removals first, then added and dirtied entries by
  prim type priority) and of `HdsiPrimManagingSceneIndexObserver` (a prim
  factory creates a managed prim per added prim; dirtied entries reach it
  with their locator set; removals release it): what a Hydra 2.0 delegate
  consumes.
- **BREAKING** Merging as `HdMergingSceneIndex`: adding an input notifies its
  root and every descendant, defined or not; inputs' notices are forwarded
  without filtering by root; no child order is promised.
- Out of scope, with reasons in `design.md`: `HdNoticeBatchingSceneIndex`
  (a pass-through until batching is enabled), `HdsiLocatorCachingSceneIndex`
  (a cache), `UsdImagingSelectionSceneIndex` (adds nothing until something
  is selected), draw mode (see `hydra-parity-instancing`), Hydra 1.0 dirty
  bits.
- **BREAKING** `CollectionMembership` (public trait) is removed: no filter
  uses it any more, and Hydra has no counterpart.
- The `moho::openusd`/`moho::openusd_schemas` re-exports and feature
  forwarding stay for the stack's consumers; they are documented as a
  downstream policy that leaves Moho if it becomes `openusd-imaging` inside
  openusd's workspace.
- Docs: `architecture.md` (the chain in Hydra's order, renderer filters,
  the Hydra 2.0 consumer path), Crust, Mantle, Moana and gap ids (M1–M12)
  removed from crate docs and comments; `CLAUDE.md` Boundaries: "Hydra
  parity; the only addition is generic attribute data"; `ROADMAP.md`
  upstream candidates.

Pending changes: all five were dealt with by the first three changes;
nothing further here (`openusd-through-moho` has no requirements; its
policy is kept, see above).

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `scene-index`: dependencies data; managing prims from notices.
- `filtering-scene-indices`: merging as Hydra's (inputs, notices, no
  order); the standard chain; renderer filters by phase; dependency
  forwarding; notice batching by prim type.
- `light-schemas`: resolved light linking as Hydra's; light filters.
- `render-settings-schemas`: dependencies across prims through
  `__dependencies`.
- `volume-schemas`: renderer-defined volumes without a consumer's names.

## Impact

- `crates/moho/src`: `chain.rs` (standard, renderer filters),
  `filter/light_linking.rs` (rewritten), new
  `filter/{dependency_forwarding,prim_type_notice_batching}.rs`, a prim
  managing observer module, `filter/merging.rs`, `filter/render_settings.rs`,
  `stage/light.rs` (light filters), `scene_index.rs` (trait removed),
  `lib.rs` and module docs.
- `docs/architecture.md`, `docs/openusd-notes.md`, `CLAUDE.md`,
  `ROADMAP.md`; the main specs' Purpose lines that mention chunks or
  streaming (`filtering-scene-indices`, `instancing-schemas`), edited at
  archive.
- Tests: 2 deleted, about 14 modified, about 16 added (`tasks.md`).
- Consumers build `chain::standard`, then their renderer filters (light
  linking at phase 4, dependency forwarding at 1000, their own fixes), then
  batch and observe the terminal index as a Hydra 2.0 delegate does.

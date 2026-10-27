## Context

See `proposal.md` for why, and `../hydra-parity-stage/audit.md` (sections
1, 4, 7, 10 and 11). Hydra references, OpenUSD `70c45e83b`:
`pxr/usdImaging/usdImaging/sceneIndices.cpp`,
`pxr/imaging/hd/{mergingSceneIndex,dependencyForwardingSceneIndex,sceneIndexPluginRegistry,renderIndex,renderDelegate}.{h,cpp}`,
`pxr/imaging/hdsi/{lightLinkingSceneIndex,primTypeNoticeBatchingSceneIndex,primManagingSceneIndexObserver}.{h,cpp}`,
and hdPrman (`third_party/renderman/plugin/hdPrman/`: `renderDelegate.cpp`,
`rileyPrimFactory.cpp`, `lightLinkingSceneIndexPlugin.cpp`). arnold-usd
(`plugins/scene_index/*.cpp`, `libs/render_delegate/reader.cpp`) was read as
a second renderer's practice.

### How a Hydra 2.0 renderer consumes Hydra

1. The application builds UsdImaging's chain (`UsdImagingCreateSceneIndices`)
   and inserts it into a render index, which merges its inputs
   (`HdMergingSceneIndex`).
2. The render index applies the renderer's scene index plugins, ordered by
   `HdSceneIndexPluginRegistry` (phase, insertion order): hdPrman and
   arnold-usd add `HdsiLightLinkingSceneIndex` at phase 4 `AtStart`;
   arnold-usd adds its dependency declarations at 900 and
   `HdDependencyForwardingSceneIndex` at 1000, and fixes of its own at 0.
3. The delegate receives the terminal index (`SetTerminalSceneIndex`) and,
   in `Update`, processes notices directly: hdPrman wraps it in
   `HdsiPrimTypeNoticeBatchingSceneIndex` (a priority per prim type) and
   observes that with `HdsiPrimManagingSceneIndexObserver` and a prim
   factory (`rileyPrimFactory.cpp`), each managed prim receiving its dirty
   locators. (`HdRenderer`, the API meant to replace this, is still a stub
   in `hd/renderer.h`.)

arnold-usd's standalone procedural (`reader.cpp`) drives the same delegate
through the same chain outside any DCC; that is the shape Crust takes for
batch renders. Crust's delegate code then runs unchanged over Moho (Rust),
and through `hdcrust` over Hydra's terminal index in Solaris and usdview.
This is why names, layout and dirty locators must match exactly, and why
Moho offers steps 2 and 3, not only step 1.

### Principle

Parity means UsdImaging's and Hydra's behaviour at the reference revision,
quirks included. A renderer that needs a fix over it (arnold-usd's
`fixPropagatedPrototypesVisibilitySIP`, `fixMaterialPruningSIP`,
`fixInvalidationSIP`) ships that fix as its own renderer filter; Moho does
not fold it in. When the reference revision moves and Hydra fixes a quirk,
Moho follows in a change of its own.

## Goals / Non-Goals

**Goals:**
- `chain::standard` = UsdImaging's chain, named configuration (spec).
- Steps 2 and 3 above, as Moho API: renderer filters by phase, light
  linking and dependency forwarding as shipped filters, prim type notice
  batching and a prim managing observer.
- Merging as `HdMergingSceneIndex`.
- Boundaries and docs stated for upstreaming.

**Non-Goals:**
- `HdNoticeBatchingSceneIndex`: a pass-through until a client enables
  batching; Moho's stage index already emits one batch per
  `apply_pending_updates`, and prim type batching (above) is what a delegate
  uses. Out of scope.
- `HdsiLocatorCachingSceneIndex` on `material`: a cache, no observable
  change; revisit with the per-prim cost profiling in ROADMAP's "Later".
- `UsdImagingSelectionSceneIndex`: adds `selections` only for prims a client
  selects; a UI concern. A consumer adds it if it has a selection.
- `HdsiSceneGlobalsSceneIndex`, prim type pruning, root overrides (what
  arnold-usd's procedural adds around the chain): application-side in Hydra
  too; the consumer builds them.
- Renderer-specific dependency declarations (material → bound geometry, as
  arnold-usd's phase 900 plugin and hdPrman's): renderer filters, not Moho.
- Hydra 1.0 dirty bits (`HdDirtyBitsTranslator`): Crust is a Hydra 2.0
  delegate; not offered.
- Tags and before/after constraints of the plugin registry: not needed for
  a list the consumer orders itself.

## Decisions

### D1. Renderer filters are a list, not a registry

Hydra discovers plugins through `plugInfo.json` and a process-wide registry.
Moho is a library, and its consumer knows its renderer's filters, so it
passes them: `chain::renderer_filters(index, &[RendererFilter { phase,
order, build }])`. Ordering is `HdSceneIndexPluginRegistry`'s sort (phase,
then order with `AtStart` first, then plugin id, for which list position
stands in). Alternative: a registry keyed by renderer name, as Hydra; it
adds global state for no gain in a single-renderer process. Light linking
and dependency forwarding are provided as constructors with Hydra's
customary phases documented (4 `AtStart`, 1000 `AtEnd`), not applied by
default.

### D2. Light linking ported, not patched

A port of `HdsiLightLinkingSceneIndex` and its cache: per (prim, collection)
the expression read from `collections`, ids handed out per distinct
non-trivial expression (`group_<n>`), membership through
`hydra-parity-stage`'s evaluator over the filter's input, categories
computed on read (`ComputeCategoriesForPrimPath`), invalidation by
evaluating the union of dirty expressions once and dirtying its matches'
`categories` (`_InvalidateCategoriesOnTargets`) and the lights' link
locators. Prim types: Hydra's rprim types for geometry, light types and
`lightFilter`, instancers by type `instancer`. The `CollectionMembership`
oracle and the stage-free fallback go with the old filter; the trait leaves
`scene_index.rs`.

### D3. `__dependencies` and forwarding

`HdDependenciesSchema`'s layout (entries of `dependedOnPrimPath`,
`dependedOnDataSourceLocator`, `affectedDataSourceLocator`; an empty path
meaning the prim itself) in `tokens.rs` and `schema.rs`; a port of
`HdDependencyForwardingSceneIndex` (reverse dependency map built lazily from
`__dependencies`, transitive dirtying with cycle protection, dependents of
removed or added prims dirtied). Render settings flattening declares
Hydra's entries (self, products, vars, and `__dependencies` on
`__usdRenderSettings/products`) and drops its internal dependents table.
Light linking's instancers declare theirs.

### D4. Notice batching by prim type and the managing observer

Ports of `HdsiPrimTypeNoticeBatchingSceneIndex` (a `PrimTypePriority` trait,
`flush`) and `HdsiPrimManagingSceneIndexObserver` (a `PrimFactory` trait
returning `Rc<dyn ManagedPrim>`, `ManagedPrim::dirty(&Dirtied)`, `prim(path)`).
Thread confinement stays Moho's (`Rc`, one thread), where Hydra guards
`Dirty` with a mutex for parallel sync; the observer's contract is
otherwise the same.

### D5. Merging as Hydra's, quirks included

Adding an input sends the strict prefixes of its root, the root and every
descendant the input lists, defined or not. Notices from inputs are
forwarded unfiltered by root (Hydra forwards an input's notice for a path
outside that input's root, though `GetPrim` there ignores the input);
Moho's root filtering goes, as a quirk reproduced. Children are a set; Moho
may keep a deterministic order internally but specifies none.

### D6. `moho::openusd` re-exports stay downstream policy

The re-exports and feature forwarding (`openusd-through-moho`) solve the
stack's one-pin problem and change no Hydra behaviour, so they stay while
Moho is its own crate. If Moho moves into openusd's workspace as
`openusd-imaging`, a re-export of its sibling crate is meaningless there:
the policy then moves to a thin downstream crate (or consumers depend on
openusd directly at the same rev). `CLAUDE.md` and `architecture.md` say so.

### D7. Boundaries and docs

`CLAUDE.md` Boundaries: "Hydra parity: UsdImaging's and Hydra's chain
order, prim paths, schema and token names, data layout and dirty locators;
behaviour Hydra lacks is dropped; the only addition is the generic
attribute data (`__usdAttributes`, `__usdRelationships`)". Crate docs and
comments drop Crust, Mantle, Moana and gap ids (M1–M12): provenance lives in
the archived changes. `architecture.md`'s chain section is rewritten in
Hydra's order (it claimed `UsdImagingCreateSceneIndices`' order while
running native instancing first), with the consumer path of Context.

### D8. Update latency is measured on a camera edit and a light addition

What matters most for interactive rendering is the time from an edit to the
renderer being updated, more than the time to the first pixel. Two edits
stand for it. A camera edit is the most frequent edit in a viewport, and it
must touch one prim. Adding a light is the edit most likely to fan out,
through light linking categories. Both are pinned down twice. First, the
update-scope requirement fixes exactly what is notified, matching what
Hydra notifies (camera locators from
`UsdImagingDataSourceCameraPrim::Invalidate`, and trivial collections
ignored in `HdsiLightLinkingSceneIndex::_ProcessAddedLightOrFilter`).
Second, a benchmark measures how long it takes. The benchmark stops at
Moho's managing observer, because the consumer's own update time (Crust
rebuilding its scene, restarting the render) belongs to the consumer's
benchmark.

### D9. The index starts at time 0, as UsdImaging's

`UsdImagingStageSceneIndex` holds its time in `_StageGlobals::_time`, a
default-constructed `UsdTimeCode`, so a new index reads at time code 0, not
at the stage's default time. Moho's earlier spec said "the stage's default
time", and that showed up while #18's review was being checked: a point
instancer whose `protoIndices` has only time samples read different
topology in Moho than in Hydra before the first `set_time`, and the first
`set_time(0)` sent an extra dirty. Hydra's other quirk is reproduced too:
replacing a stage already held clears the globals, which sets the time to
`UsdTimeCode::EarliestTime()` (`stageSceneIndex.cpp:966-970`). The
"Time variance" requirement is unaffected, since `mightBeTimeVarying` reads
the opinions and not the time.

## Risks / Trade-offs

- [Without dependency forwarding, render settings and instance categories
  stop updating live] → Hydra's behaviour; the docs and `chain` module say
  to add forwarding, and the shipped constructor makes it one line.
- [Light linking leaves `chain::standard`] → consumers that relied on
  categories add the filter; a test pins that `chain::standard` alone has
  none.
- [Unfiltered merged notices may mention prims `GetPrim` does not serve] →
  Hydra's quirk; consumers already re-query on notices.

## Migration Plan

Lands last, after `hydra-parity-materials`. Consumers build
`chain::standard`, apply their renderer filters (light linking at 4,
dependency forwarding at 1000), then batch by prim type and observe with a
prim factory.

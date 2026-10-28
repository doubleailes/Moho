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

## Deviations recorded while implementing

- **A light filter has no `light` at the stage index** (spec "Light
  filters" said it answers its parameters by name in `light`).
  `UsdImagingLightFilterAdapter::GetImagingSubprimData` gives a light
  filter `material` (terminal `lightFilter`) over the base prim data, and no
  `light`; checked with USD 26.08 (the comparison harness). Only the light
  linking filter gives one, holding the three link ids. Moho follows
  UsdImaging, as the user's parity decision says; the filter's parameters
  are in its `material` node's `parameters`. A light filter's descendants
  are represented by it (`RepresentsSelfAndDescendents`): a shader under it
  dirties its `material`.
- **Light linking reads its input when built.** `HdsiLightLinkingSceneIndex`
  learns lights and filters from added notices only (a render index's
  merging scene index sends them when the chain is inserted); built over an
  input already populated, it would know none. Moho's is often built over a
  populated chain, so it reads the lights and filters its input holds when
  made, as a first population (which dirties nothing, as Hydra's first added
  batch). Its notices and data then match Hydra's (checked with USD 26.08,
  the comparison harness, light linking and dependency forwarding over a
  merging index).
- **Light linking keeps its tables while unobserved.** Hydra's returns
  early from every notice while nothing observes it, so its cache goes
  stale. Moho's keeps the cache current and only skips computing the
  dirtied entries, which would go nowhere.
- **Category order.** Hydra lists a prim's categories in hash order; Moho
  lists them in id order (`group_0` first).
- **Merging does not notify the strict ancestors of a new root** (spec
  "Adding and removing an input" and D5 said it does).
  `HdMergingSceneIndex::_AddStrictPrefixesOfSceneRoots` skips every root
  for which `IsAbsoluteRootOrPrimPath()` holds, that is every valid root
  (its test is inverted at the reference revision), so Hydra never sends
  them; `hydra-parity-instancing` had already found and followed this. The
  spec scenario "An input comes and goes" is corrected: adding notifies
  `/world/elem` and its descendants, not `/world`.
- **Merging, other quirks reproduced.** With a single input, the merge is
  that input at every path (`GetPrim` does not check the root); an added
  entry outside every root takes the empty type once there are two inputs
  (no input gives one); a removed input's notices are dropped, since Moho's
  observers cannot be unregistered as Hydra's are.
- **Merging notices through the instancing propagations.** Checked against
  USD 26.08 on the instancing fixtures: what remains different comes from
  openusd renumbering prototypes when one is retired (an upstream candidate
  since `hydra-parity-instancing`) and, on removing a nested point
  instancer, from re-additions the propagation's own input handling makes;
  neither is merging's.
- **Adding a linked light also dirties its link id.** The spec's "Adding a
  light linked to a few prims" said only the prims whose categories change
  are dirtied. `HdsiLightLinkingSceneIndex` also dirties the added light at
  `light/lightLink` (`_InvalidateLights` for the collection it just
  processed), after the added entry; Moho does the same, and the test pins
  it.

### Found by the final parity check (`docs/hydra-parity-check.md`)

Fixed, each with a test: flattening dirties only the descendants whose
flattened data was read since it was last dirtied (`_DirtyHierarchy`,
flattened primvars tracked per primvar), and a prim added again dirties its
read descendants for every flattened name, after the added entry
(`_PrimsAdded`); the flattened `purpose` is
`HdFlattenedPurposeDataSourceProvider`'s (the input container as it is, an
inheritable parent's whole, or a fallback alone, without `inheritable`).

Accepted:

- **Category ids are numbered in another order.** Light linking numbers a
  new expression when it first meets it, in the order prims are added
  (Hydra) or read at construction (Moho), both depth first; Moho's merging
  lists children in path order, Hydra's in hash order, so on ALab a light's
  id differs (`group_0` against `group_3`). The ids stay consistent between
  lights and the prims they link.
- **An explicit collection's union order.** For `explicitOnly` (and a
  property path) C++ unions the included paths in hash map order
  (`/world/c /world/a`); openusd's port in path order. The same expression.
- **A path expression's text.** openusd prints a predicate's string
  argument bare when it can (`{hdType:mesh}`), C++ quoted
  (`{hdType:"mesh"}`). The same expression.

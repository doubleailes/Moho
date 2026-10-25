# Design

## Context

See proposal.md for why. What exists and what binds this design:

- **Moho.**
  - `crates/moho` is empty apart from re-exporting openusd.
  - S0 fixed the streaming: one masked stage per chunk under a merging
    index, over a `LoadNone` scene input (`docs/streaming-spike.md`,
    `docs/architecture.md`).
  - Conventions: `forbid(unsafe_code)`; no `unwrap` outside tests; openusd
    pinned at `68a867a`; never guess an openusd signature.
- **Hydra 2.0, the model** (local OpenUSD checkout, HEAD `70c45e83b`):
  - **Contract:** `HdSceneIndexBase` (`GetPrim`, `GetChildPrimPaths`) and
    `HdSceneIndexObserver` (added, removed, dirtied, renamed).
  - **Data sources:** `HdDataSource` containers, vectors and sampled values,
    with `GetContributingSampleTimesForInterval`.
  - **Locators:** `HdDataSourceLocator(Set)`.
  - **Schema tokens:** `xform`, `mesh`, `primvars`, `visibility`,
    `purpose`, `extent`, `materialBindings`, `material`, `light`,
    `collections`, `categories`, `camera`, `basisCurves`, the implicit
    shapes, `instancerTopology`, `instance`, `instancedBy`,
    `volumeFieldBinding`, `volumeField`, `renderSettings`, `renderPass`
    and `sceneGlobals`.
  - **USD reading:** `UsdImagingStageSceneIndex` and its adapters and data
    sources.
  - **Filters:** flattening, merging, material binding resolving,
    native-instance and point-instancer prototype propagation, light
    linking, and render settings flattening.
- **What Crust reads** (crust-render `usd_import/`):
  - Every family above.
  - Through one read path that tells an authored value from a schema
    fallback.
  - Many renderer attributes by name (`crust:*`, `driver:parameters:*`,
    light `inputs:*`).
  - Transforms over the shutter.
  - Collection-based material binding and light linking.
  - Native instances and point instancers, with prototypes renumbered per
    stage.
- **openusd and openusd-schemas at `68a867a`:**
  - stage sinks;
  - typed schemas for geom, lux, shade, render and vol;
  - `XformCache` and xformable queries;
  - `MaterialBindingAPI` (`direct_binding`, `collection_bindings`);
  - `CollectionAPI` with `complete_membership_expression` and membership
    queries.

## Goals / Non-Goals

**Goals:**
- Hydra 2.0's model and schema tokens, so that a reader of Hydra's
  documentation can read Moho, and Moho can be offered upstream.
- Every value equals what openusd resolves (parity), so that Crust's S2
  migration can be checked module by module.
- Exact dirty sets: an edit or a time change dirties what it changes and
  nothing else.
- Prototype paths that are stable across stages and chunks.

**Non-Goals:**
- **Rendering concerns:**
  - converting implicit shapes to meshes;
  - extent computation;
  - texture loading;
  - the shutter policy (which camera, and product overrides of motion blur
    stay the consumer's).
- **Crust's layer-level workarounds,** which keep using the stage: invisible
  overrides read from raw layer data, `.mtlx` references found by scanning
  layer specs, asset anchoring on a property's stack.
- **Threading:** parallel reads, or `Send` data sources.
- **Writing:** authoring through the index.

## Decisions

### D1. Rust shapes of the contract

Hydra's interfaces map to Rust like this:

```rust
pub trait SceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim;               // Prim { ty: Token, data: Option<ContainerHandle> }
    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path>;
    fn add_observer(&self, observer: Weak<dyn Observer>);
}
pub trait Observer {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]);
    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]);
    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]); // Dirtied { path, locators: LocatorSet }
    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]);
}
pub enum DataSource { Container(ContainerHandle), Vector(VectorHandle), Sampled(SampledHandle) }
pub trait Sampled {
    fn value(&self, offset: f32) -> Option<sdf::Value>;
    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>>;   // None: uniform
}
```

- **Handles are `Rc`.** Observers are held weakly, as Hydra holds them.
- **Values are `sdf::Value`,** which is `Send`, so values cross threads while
  the index does not.
- **A locator is a `Vec` of tokens.** A `LocatorSet` stays minimal, and
  `intersects` is a prefix test both ways.
- **Typed schemas are thin views over containers** (`MeshSchema::get(&container)`,
  `.topology()`, `.face_vertex_counts()`), with the token constants in one
  module, as in Hydra.

*Alternative:* an enum of typed prims (Hydra 1.0 style). This was rejected
in Mantle's `scene-index` D1.

### D2. `StageSceneIndex`: adapters, invalidation and time

The index follows `UsdImagingStageSceneIndex`.

**API.**
- `new()`, `set_stage(Stage)`, `set_time(TimeCode)` and
  `apply_pending_updates()`.
- `set_stage` removes everything and populates by the default traversal.
  Instances are traversed as prims; their content comes from D5.
- Stages must carry openusd-schemas' registry, or schema fallbacks and typed
  views are missing (`docs/openusd-notes.md`). `moho::stage::stage_builder()`
  opens stages with it; the index reads whatever stage it is given.

**Adapters.**
- One adapter per USD prim type (and per applied API schema: binding,
  collection, light) builds the prim's container and maps a changed
  property name to locators.
- The mapping table follows `UsdImagingDataSourcePrim::Invalidate` and each
  family's adapter. It is the "property dirties locators" requirement.

**Queued notices.**
- A `StageSink` records resynced prim paths, resynced property paths and
  info-only property paths, and notifies nothing.
- `apply_pending_updates` coalesces resyncs by prefix, repopulates them
  (removed, then added), and turns property changes into dirtied entries
  through the adapters.

**Time.**
- A sampled data source that `might_be_time_varying` records
  `(path, locator)` in the time-varying set when it is built.
- `set_time` to a new time dirties exactly that set.
- Sample times over an interval come from openusd's `time_samples_in`,
  which includes the bracketing samples, shifted to offsets.

**Load and unload notices.** `load_rules_changed` resyncs the reported
paths. Moho never loads anything itself; a consumer that loads chunks uses
merging inputs (D4).

### D3. Generic attribute data, Moho's one addition to Hydra

Crust needs three things Hydra's schemas do not carry:
- whether a value is authored or a fallback;
- renderer attributes it has no schema for;
- colour space and interpolation metadata on any attribute.

Every prim therefore also exposes `__usdAttributes` (and
`__usdRelationships`), with the leading underscores Hydra uses for its own
private containers (`__usdPrimInfo`).

- **Attributes:** keyed by attribute name. Each entry gives `value`
  (sampled), `authored`, `typeName`, `colorSpace`, `interpolation`,
  `elementSize` and `connections`.
- **Relationships:** `targets` and `forwardedTargets`.
- **Laziness:** both containers are lazy. Listing names reads the prim's
  property names; reading one entry resolves only that property.
- **Schema values stay Hydra's:** resolved, fallback included. Only the
  generic entry reports whether a value is authored.

*Alternative:* per-family `namespacedSettings`-style containers only. Hydra
does this for render settings, and Moho does too (D7). Generic data is still
needed for `crust:*` on geometry, lights and volumes.

### D4. Filters and the standard chain

- **`FlatteningSceneIndex`** has one provider per data source name, as in
  Hydra:
  - `xform`: local × parent, unless the stack resets;
  - `visibility`: the local value, else the parent's, else visible;
  - `purpose`: the local value, else the parent's if inheritable, else the
    fallback;
  - constant `primvars`: inherited when not defined locally;
  - `usdMaterialBindings`: the ancestors' entries, then the prim's.

  It dirties descendants on change.
- **`MergingSceneIndex`** follows Hydra:
  - earlier inputs are stronger, and data sources are overlaid;
  - the first non-empty type wins;
  - adding or removing an input sends added, removed and re-added entries.
- **Resolving filters:**
  - `MaterialBindingResolvingSceneIndex` turns `usdMaterialBindings` into
    `materialBindings`;
  - `LightLinkingSceneIndex` turns `collections` into categories;
  - `RenderSettingsFlatteningSceneIndex` flattens products and vars into
    `renderSettings`.
- **The standard chain** is `moho::chain::standard(stage_index)`, the same
  order as `UsdImagingCreateSceneIndices`:
  stage → native instancing (D5) → point instancing (D5) → flattening →
  material binding resolving → light linking → render settings flattening.

  Consumers can compose their own chain. A streamed scene puts one standard
  chain per input under a `MergingSceneIndex`.

### D5. Instancing follows Hydra's prototype propagation

- **Native instances.** The stage index marks instances with their
  prototype and does not expose `/__Prototype_N` prims.
  - A native-instancing filter groups instances by prototype and binding
    hash (material bindings, purpose), as
    `UsdImaging_NiInstanceAggregationSceneIndex` does.
  - It creates one instancer per group under
    `/MohoNiPropagatedPrototypes/<bindings>/<prototype>/instancer`.
  - It re-roots the prototype's prims there.
  - Nested instances recurse.
- **Stable paths.** The `<prototype>` segment is a hash of the prototype's
  identity, never openusd's numbering:
  - the asset path and prim path its prim index composes from;
  - the variant selections.

  So two chunk stages agree on it.
- **Point instancers.**
  - Expose `instancerTopology` and the `hydra:instance*` primvars.
  - A point-instancer filter re-roots each targeted prototype under
    `<prototype>/ForInstancer<hash>`, with `resetXformStack` and
    `instancedBy`, as `UsdImagingPiPrototypePropagatingSceneIndex` does.
  - It empties the type of untargeted prims under the instancer.

*Risk:* the identity hash must be computed from openusd's prim index graph.
If openusd does not expose enough of it (the arcs' layer and path), that is
an upstream candidate, and the fallback is the prototype's root prim stack
sites (`prim_stack()`).

### D6. Collections, material bindings and light linking use openusd

- **Membership and expressions.** Moho calls openusd's
  `complete_membership_expression` and membership queries. It does not
  reimplement them.
- **Binding opinions** come from openusd-schemas' `MaterialBindingAPI`
  (`direct_binding`, `collection_bindings`, strength).
- **Resolution** happens in the resolving filter, by Hydra's rules: the
  first `strongerThanDescendants` binding walking down wins, else the
  deepest; at one level, a collection binding beats a direct one.

### D7. Render settings keep namespaced data

- **Flattening.** Settings, products and vars flatten into `renderSettings`
  as in `UsdImagingRenderSettingsFlatteningSceneIndex`. Product opinions
  override settings opinions.
- **Namespaced data.** Every attribute outside the UsdRender schemas goes
  into `namespacedSettings` by full name: `crust:*`, `driver:parameters:*`
  and `ri:*`.
- **Dependencies.** The flattening filter records which prims each settings
  prim depends on, so a product or var change dirties its settings.

### D8. Dependencies

- `moho` gains `openusd-schemas` from the same git source at the same rev
  as `openusd`. It is part of the openusd project, so "openusd only" holds.
  `CLAUDE.md`'s rule is updated to say so.
- No other dependency.

### D9. Testing: fixtures, parity and notices

- **One fixture per family** is written as USDA text, as the S0 fixture is.
- **Parity tests** read every exposed value through the index and directly
  through openusd at the same time, and compare.
- **Notice tests** assert the exact entries, through a recording observer:
  set a stage; edit and apply; set a time; add and remove merging inputs.
- **Stable prototypes** are tested on two masked stages of one instanced
  scene.

## Risks / Trade-offs

- **[One large change]:** core, five filters, ten families. The project
  owner chose this. Tasks are ordered so the contract and the stage index
  land and are tested first. Each family is then a self-contained group, so
  the change can be reviewed and merged in slices of commits.
- **[openusd gaps]:** some USD behaviour may be missing at `68a867a`
  (prototype identity, collection expressions on instances, a property's
  colour space resolution). Each gap gets a workaround commented in code and
  an entry under "Upstream candidates", as `CLAUDE.md` requires.
- **[Parity is only as good as the fixtures]:** S2 adds the real scenes
  (ALab, Moana, Crust's test scenes) as parity gates. This change covers
  each family's fixtures.
- **[Performance]:** lazy data sources keep the cost per read. The stage
  index caches each prim's container until it is dirtied or resynced.
  Memory at Moana scale is S0's question, already answered by chunking.
- **[Hydra drift]:** Moho follows Hydra at the local checkout's HEAD. Later
  Hydra changes are not tracked automatically.

## Open Questions

- The exact identity that keys a stable native prototype (D5). It is
  settled by the first instancing task against openusd's prim index API, and
  recorded in `docs/openusd-notes.md`.
- Whether the native-instancing filter's binding hash also needs light
  linking categories, which Crust judges at the instance path. It is settled
  with the light linking task, against Crust's `light_links.rs`.

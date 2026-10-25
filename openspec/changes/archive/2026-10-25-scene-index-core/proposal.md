# Proposal

## Why

Every consumer of a USD stage in the stack reads it its own way. Crust's
importer reads `usd::Prim`s directly across 23k lines. Mantle's IPR can only
hand renderers files, and Obsidian needs a reader of its own. Moho exists to
give them one renderer-agnostic reader in Hydra 2.0's model, and S0 has fixed
how it streams. This change (S1, Moho side) builds that reader. It is what
Mantle's `scene-index` tasks and Crust's importer migration (S2) wait for.

## What Changes

The `moho` crate, empty since the bootstrap, gains Hydra 2.0's scene index,
complete enough to replace Crust's direct reads:

- **The model.**
  - A scene index answers `prim(path)` (a type and a container of data) and
    `children(path)`.
  - Observers receive batches of added, removed, dirtied (with locators) and
    renamed prims.
  - Data sources are containers, vectors and sampled values, read lazily on
    the thread that owns the index.
  - Locators and locator sets name data hierarchically.
- **`StageSceneIndex`**, in `UsdImagingStageSceneIndex`'s shape:
  - built empty, then `set_stage`, `set_time` and `apply_pending_updates`;
  - stage edits are queued and become notices only when updates are applied;
  - a time change dirties exactly the data that may vary over time.
- **Filtering indices:**
  - `FlatteningSceneIndex`: world transform, and inherited visibility,
    purpose and material binding;
  - `MergingSceneIndex`, whose inputs carry S0's streaming: a scene input
    plus one input per chunk.
- **Typed schemas for every family Crust imports:**
  - xform; mesh and subdivision; primvars; visibility, purpose and extent;
    basis curves; implicit shapes;
  - material bindings, and material networks (UsdPreviewSurface and
    MaterialX);
  - lights and light linking; cameras;
  - native instances and point instancers;
  - volumes and their fields;
  - render settings, products, vars and passes.
- **Instancing:** native instances keep prototype paths stable across stages
  and chunks, as Hydra's native-instancing filter does. S0 showed that
  openusd's `/__Prototype_N` numbering is not.
- A test fixture scene per family, and parity checks against what openusd
  resolves for the same data.

## Capabilities

### New Capabilities
- `scene-index`: the scene index contract, which covers prims, children,
  observers and their notice semantics, data sources, sampled values and
  locators.
- `stage-scene-index`: reading one openusd stage, which covers stage
  replacement, time and the time-varying set, queued edits, and the mapping
  of stage changes to added, removed and dirtied prims.
- `filtering-scene-indices`: flattening (inherited data) and merging (several
  inputs over one namespace, added and removed as a whole).
- `geometry-schemas`: xform, mesh and subdivision, primvars, visibility,
  purpose, extent, basis curves and implicit shapes.
- `material-schemas`: material bindings, and material networks from
  UsdPreviewSurface and MaterialX.
- `light-schemas`: lights, light filters and light linking.
- `camera-schema`: cameras and their projection, clipping and exposure.
- `instancing-schemas`: native instances with stable prototypes, and point
  instancers.
- `volume-schemas`: volumes and their fields.
- `render-settings-schemas`: render settings, products, vars and passes.

### Modified Capabilities
None. Moho has no specs yet.

## Impact

- `crates/moho`: the whole implementation. It stays openusd-only, and the
  public API is new.
- `docs/architecture.md`, `docs/openusd-notes.md` (every openusd API the
  index starts relying on), and `ROADMAP.md` (S1 done, gaps under "Upstream
  candidates").
- openusd stays pinned at `68a867a`. Missing APIs are worked around in code,
  with a comment, and listed as upstream candidates.
- Consumers: Mantle's `scene-index` tasks (1.2 onward) depend on this change.
  Crust's S2 migrates its importer module by module against these schemas.
- `crates/moho-spike` is unchanged and stays the S2 benchmark.

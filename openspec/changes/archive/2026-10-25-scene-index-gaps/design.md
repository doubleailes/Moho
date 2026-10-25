## Context

S1 (`scene-index-core`) gave Moho Hydra 2.0's model over one stage, and
streaming as S0 decided: a `MergingSceneIndex` of a `LoadNone` scene input
and masked chunk inputs, each a standard chain. Planning S2 against it found
seven gaps (crust-render, `openspec/changes/moho-import/design.md`, M1 to
M7). Hydra's sources (the local OpenUSD checkout) settle most of them.

## Goals / Non-Goals

**Goals:** close M1 to M7 with tests, keeping Hydra's names and placement
wherever Hydra has an answer.

**Non-Goals:**
- Choosing what is loaded for a consumer: Moho lists what is missing, the
  consumer opens stages.
- Light-link predicates: the merged chain judges complete membership
  expressions, path patterns only, as the expression fallback does today.

## Decisions

### D1. Light linking after the merge (M1)

In Hydra, `HdsiLightLinkingSceneIndex` is not part of
`UsdImagingCreateSceneIndices`. It is appended at the end of the chain, after
scenes are merged, so it sees every light. Moho does the same:

```
chain::input(stage)   native -> point -> flatten -> material bindings -> render settings
chain::merged(index)  light linking over `index`, judging complete membership expressions
chain::standard(stage) = chain::merged-style linking over chain::input(stage),
                         with the stage as membership oracle
```

Across inputs no single stage can judge every collection, so the merged
linking index has no oracle and judges the lights' complete membership
expressions, which are path-based and agree across chunk stages.
*Alternative:* a composite oracle asking each input's stage. Rejected: a
chunk stage asked about a path in another chunk can only answer from path
rules too, and the composite must track inputs as they come and go.

### D2. Prims outside the loaded inputs (M2)

`streaming::unresolved(index: &dyn SceneIndex) -> BTreeSet<sdf::Path>` walks
the index and returns:
- each resolved binding's material (`materialBindings/*/path`) that the
  index does not hold;
- each network node of a held material whose name is an absolute path (a
  node outside the material, as the material adapter names it) that the
  index does not hold.

A consumer opens a stage masked to those paths, payloads loaded, wraps it
in `chain::input`, and adds it as an input, weaker than the chunks and
stronger than the scene input. It repeats until nothing is unresolved.
*Alternative:* a hook on `MergingSceneIndex`. Rejected: deciding what to
load is the consumer's.

### D3. Stage metadata as generic data (M3)

Hydra's `sceneGlobals` has no authored flag, and Moho keeps Hydra's schemas
as Hydra names them. Moho's one addition, the generic data, already reports
authored-ness for attributes (`__usdAttributes/<name>/authored`). The
pseudo-root has no attributes but metadata, so the root gains
`__usdStageMetadata/<field>/{value, authored}` for `startTimeCode`,
`endTimeCode`, `timeCodesPerSecond`, `framesPerSecond`, `metersPerUnit`,
`upAxis` and `renderSettingsPrimPath`. The value is the resolved one, a
fallback when unauthored, as `__usdAttributes` gives. Authored-ness comes
from `Stage::has_authored_stage_metadata`. A change to the stage's metadata
dirties `__usdStageMetadata` with `sceneGlobals`.

### D4. Prim origin (M4)

Hydra's `HdPrimOriginSchema` carries `scenePath`;
`UsdImagingDataSourcePrimOrigin` fills it with the prim's path, made
relative to the prototype root for a prim inside a USD prototype
(`/__Prototype_1/MyXform/MySphere` gives `MyXform/MySphere`). Moho exposes
`primOrigin/scenePath` on every prim of a stage index the same way. The
native instancing filter builds prototype prims from the input's
`/__Prototype_N` prims, so they keep the prototype-relative path; the point
instancing filter's copies keep the original prototype's absolute path. A
consumer names a prototype prim by joining the instance's path and
`scenePath`.

### D5. Strategy (D) in the spike (M5)

(D) is (B) through Moho: a `LoadNone` scene input under a merging index,
each chunk a `chain::input` over a masked `LoadAll` stage added in turn,
read through `chain::merged`, and removed. Its digest hashes the index's
prims, types and values, so it is compared with an index over the fully
loaded stage, not with (A) to (C), which hash raw openusd reads. Its peak
memory on Moana is compared with (B)'s. `moho-spike` depends on `moho` for
it; `moho`'s dev-dependency on `moho-spike` stays, a cycle Cargo allows for
dev-dependencies.

### D6. Render pass collections (M6)

The collections adapter lists the prim's applied `CollectionAPI` instances
from its composed `apiSchemas`, which include the typed schema's built-in
ones. A render pass therefore exposes `renderVisibility`, and `prune`,
`matte` and `cameraVisibility` when applied. The requirement is widened to
say so and tested on a pass with all four.

### D7. Merged children order (M7)

`HdMergingSceneIndex::GetChildPrimPaths` collects children in a dense hash
set, so Hydra gives no order. Moho keeps its current order and specifies
it: inputs strongest first, each input's children in its own order, each
path once, then the intermediate prims implied by deeper roots, in input
order.

## Risks / Trade-offs

- **[Linking without an oracle misses predicates]** → the same limit as the
  expression fallback today; recorded in `ROADMAP.md`.
- **[A dev-dependency cycle between moho and moho-spike]** → allowed by
  Cargo; the spike is unpublished.

# S0 streaming spike: results

The record of OpenSpec change `streaming-spike` (S0 of the `scene-index`
program). It compares three ways of reading a stage chunk by chunk, checks
that they read the same data, probes change notices across loading, and
decides how Moho streams under the design's rule (D6).

**Decision: (B), a masked stage per chunk.** On Moana, (A)'s peak memory is
**2.56×** (B)'s (5.05 against 1.97 GiB), past D6's 1.25× bound.
`StageSceneIndex` keeps Hydra's shape (`new`, `set_stage`, `set_time`,
`apply_pending_updates`) over one stage. Streaming is built from standard
indices: one stage index per chunk, combined under a `MergingSceneIndex`.

## How it was measured

- Harness: `crates/moho-spike`, binary `streaming-spike`, driver
  `scripts/streaming-spike.sh`. One strategy per process. Memory is `VmHWM`
  (peak) and `VmRSS` from `/proc/self/status`.
- Strategies:
  - **(A)** one stage opened `LoadNone`. Per chunk, one `load_and_unload` loads
    it and unloads the previous one, then the chunk is read. The last chunk is
    unloaded at the end.
  - **(B)** Crust's import: a `LoadNone` hierarchy stage yields the chunks, and
    one `LoadAll` stage masked to each chunk is opened, read and dropped.
  - **(C)** one `LoadAll` stage: the reference.
- Chunks follow Crust's rule (D1): the children of the single top-level prim,
  or the top-level prims when there are several, streamed only from 4 up.
- The read set (D2): type name, `xformOpOrder` and its ops, `points`,
  `faceVertexCounts`, `faceVertexIndices`, `material:binding` targets, and
  `might_be_time_varying` on `points` and the ops, at the stage's start time.
  Traversal is `PrimPredicate::DEFAULT`, which stops at instances. Each
  instance also reads its prototype's subtree, once per stage, as an import
  does.
- Machine: `cdg2-phil-01`, Xeon E5-2699 v4 (72 threads), 94 GiB, Linux
  5.14 (Rocky 9). openusd `68a867a`. Release build. Each run is capped at 80 GB
  with no swap (`systemd-run --scope -p MemoryMax=80G -p MemorySwapMax=0`).
  Run on 2026-10-25, on a working tree based on Moho `23c0fc5`.
- Raw output: `docs/streaming-spike/{alab,moana}/` (one JSON line per phase
  and chunk, plus `meta.txt`).

## Results

### ALab 2.2 (`ALab/entry.usda`)

ALab does **not** stream under Crust's rule. Its top level holds two prims,
`/root` and the class `/_root_type`, below the threshold of 4. So every
strategy reads it as one `LoadAll` stage, as Crust's own import does
(single-stage, `skip_stage_teardown`). The three runs are the same work.

| strategy | chunks | total | peak | after | digest |
|---|---|---|---|---|---|
| a (→ c) | 1 | 49.98 s | 24.83 GiB | 1.28 GiB | `b21308cc5ae9cdce` |
| b (→ c) | 1 | 41.07 s | 24.83 GiB | 1.28 GiB | `b21308cc5ae9cdce` |
| c | 1 | 39.98 s | 24.83 GiB | 1.28 GiB | `b21308cc5ae9cdce` |

12 195 prims read, no read errors. The spread in time (40 to 50 s, and 72 s
for a first, cold-cache run) is noise between identical runs.

### Moana island (`island/usd/island.usda`)

22 chunks: the children of `/island`. The elements reach `/island` by
reference and carry payloads inside, so a `LoadNone` stage holds only the
hierarchy.

| strategy | total | peak | after last chunk | after the stage is dropped | digest |
|---|---|---|---|---|---|
| a | 40.39 s | **5.05 GiB** | 4.81 GiB | 2.43 GiB | `d8de81f228c46b76` |
| b | 39.98 s | **1.97 GiB** | 1.69 GiB | 1.69 GiB | `d8de81f228c46b76` |
| c | 43.76 s | 6.50 GiB | 6.26 GiB | 3.69 GiB | `d8de81f228c46b76` |

135 034 prims read, no read errors, equal digests. RSS after each chunk:

- (A): 0.73, 1.86, … 2.06, 2.73, 2.84, 4.02, 4.71, 4.81 GiB. It only grows.
  The final unload takes 0.1 ms and frees nothing measurable. The memory comes
  back only when the stage is dropped.
- (B): 0.62, 1.07 (chunks 2 to 15), 1.69 GiB (chunks 16 to 22). It is bounded
  by the largest chunk.
- (C): 6.08 GiB after the first chunk (openusd composes lazily, so opening
  costs nothing and the first reads compose), 6.26 GiB at the end.

(C) on Moana was not in the plan, which kept Crust's 75.74 GiB composition as
its baseline. It was run because it costs 44 s here. It shows that baseline
does not carry over: with this read set at `68a867a`, the whole island
composes in 6.5 GiB. Crust's figure measures its full import (normals, UVs,
primvars, curves, materials) at an older openusd. The comparison between
strategies is what this spike decides on, and it holds either way: (A) keeps
78% of (C)'s peak, while (B) keeps 30%.

### What (A) costs, and why

Each `load_and_unload` drops a bounded set of cached indices (the loaded and
unloaded paths and their ancestors whose load rule is not `All`). It does
not pay the two full-store recomposes the `find_loadable` comment warns of.
That comment is about `find_loadable`, not about loading (see
`openusd-notes.md`). Time is therefore not (A)'s problem (1.01× (B)). Memory
is: unloading a payload does not give back what composing it took.
`Stage::find_loadable`'s documentation says the codebase "has no
layer-eviction mechanism yet", so payload layers stay in the registry once
read. The growth seen here is consistent with that, plus allocator retention.
Which share is which was not measured.

## Notice probe (D4)

On the fixture, opened `LoadNone`, with a recording `StageSink`. Asserted in
`crates/moho-spike/tests/probe.rs` and recorded in `openusd-notes.md`:

| step | notices |
|---|---|
| `override_prim` inside an unloaded payload | `after_commit`: resynced the prim |
| author a new attribute there | `after_commit`: resynced the property, then changed-info-only |
| `load` the payload | `load_rules_changed`: resynced `/world` |
| edit while loaded | `after_commit`: changed-info-only |
| `unload` the payload | `load_rules_changed`: resynced `/world` |
| edit after unload | `after_commit`: changed-info-only |

- **Edits under an unloaded payload notify.** Yes, in stage namespace.
- **Load and unload can be told from edits.** Yes: they never fire
  `after_commit`, only `load_rules_changed`. But the path is the chunk's
  parent, not the chunk, so read as a subtree resync it covers every
  sibling. That is an upstream candidate (`ROADMAP.md`).

## Decision under D6

D6 chooses (A) only if, on both ALab and Moana, its peak is at most 1.25×
(B)'s, its time at most 1.5× (B)'s, and the probe passes.

| criterion | ALab | Moana |
|---|---|---|
| peak (A) / (B) ≤ 1.25 | 1.00, vacuous: both are (C) | **2.56 (5.05 / 1.97 GiB): fails** |
| time (A) / (B) ≤ 1.5 | 1.22, noise between identical runs | 1.01 (40.39 / 39.98 s): passes |
| probe | passes | passes |

(A) fails on memory, so the choice is **(B)**. The hybrid applies only when
(A) wins on memory and fails the probe, which is the opposite of what was
found.

ALab cannot separate the strategies under Crust's chunk rule, so the
decision rests on Moana alone. A finer chunk rule (for example the children
of `/root` on ALab) would make ALab informative. It would be a change to
Crust's rule, and so to (B) itself, and is not needed for this decision.

## The constructor, in Hydra's shape

S0 first sketched a `StageSceneIndex::from_source(source)` that would open
chunk stages itself. That puts streaming inside the stage index, which Hydra
does not do. Moho follows Hydra 2.0 instead, where streaming is composed from
standard scene indices and the application decides what is loaded:

```rust
impl StageSceneIndex {
    /// Empty, as `UsdImagingStageSceneIndex::New()`.
    pub fn new() -> Self;
    /// Removes every prim and reads `stage` from scratch.
    pub fn set_stage(&mut self, stage: Stage);
    /// Dirties what may vary over time.
    pub fn set_time(&mut self, time: TimeCode);
    /// Turns the stage edits queued since the last call into added, removed
    /// and dirtied notices.
    pub fn apply_pending_updates(&mut self);
}
```

- **The stage index reads one stage**, whatever its load set and mask.
  Choosing them is the caller's job, as it is in `usdImaging`.
- **Streaming composes standard indices.** The chunks come from Crust's rule
  on a `LoadNone` stage. Each chunk is a `StageSceneIndex` over a `LoadAll`
  stage masked to that chunk. A `MergingSceneIndex` combines them:
  `add_input_scene(chunk_index, chunk_root)` adds a chunk, and
  `remove_input_scene` drops it, which observers see as the chunk's prims
  added or removed. Below 4 chunks, one stage index over one `LoadAll` stage
  serves the whole scene, as in Crust's import.
- **The consumer decides when chunks come and go.** An offline import
  (Crust) adds a chunk, pulls it, and removes it, in turn. That is S0's
  strategy (B) and its memory bound. A viewport (Obsidian) keeps the chunks
  it shows, since removing one deletes its prims for every observer.
- **Edits apply at an explicit flush.** Stage notices are queued and become
  scene index notices in `apply_pending_updates`, so the IPR can replay a
  whole change and flush once before its delegate syncs.
- **A `LoadNone` stage hears edits under payloads it has not loaded** (the
  probe). The S1 specs decide whether a merging setup keeps a hierarchy stage
  for that, or forwards edits to the chunk stages that are open.
- **Prototype paths are numbered per stage**, so `/__Prototype_N` is never
  stable identity across chunks. S1 should place prototypes the way Hydra's
  native-instancing filter does, under paths derived from the instancer.

S1 turns this into requirements. Mantle's `scene-index` specs carry the
decision.

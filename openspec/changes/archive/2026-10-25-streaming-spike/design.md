# Design

## Context

See proposal.md for why. Relevant facts:

- **Crust's streaming** (crust-render `68560ae`, `usd_import/mod.rs`):
  - `subtree_roots` takes the children of the stage's single top-level prim,
    or the top-level prims when there are several.
  - `stream_roots` streams them only when there are at least
    `MIN_STREAM_CHUNKS = 4`, because each masked open re-composes the root
    layer.
  - Each chunk is `stage_builder().load(LoadAll).mask(StagePopulationMask::new(..))`.
  - `CRUST_STREAM_IMPORT=0` forces one stage, which is how Crust A/B-tests
    the two.
- **openusd (`68a867a`)**:
  - `Stage::load`, `unload` and `load_and_unload`, with `LoadPolicy`.
  - Builder `.load(InitialLoadSet)` and `.mask(StagePopulationMask)`.
  - `StageSink::after_commit` with `CommittedChange { resynced,
    changed_info_only }`.
  - A source comment says that under `LoadNone` a load or unload pays two
    full-store recomposes per call.
- **Moho (`bootstrap`)**: an empty `moho` crate, with `forbid(unsafe_code)`
  and `unwrap_used` denied outside tests.

## Goals / Non-Goals

**Goals:**
- Comparable numbers for strategies (A), (B) and (C) on the same scenes,
  taken the same way.
- Proof that the strategies read identical data.
- Knowledge of how change notices behave with payloads unloaded, loaded and
  unloaded again.
- A decision, and the `StageSceneIndex` constructor it implies.

**Non-Goals:**
- Any scene index code: data sources, observers or schemas (S1).
- Tuning openusd, or patching it. Gaps go to "Upstream candidates".
- Texture or asset loading, which is Crust's and is identical under every
  strategy.
- Parallel chunk reading. Crust's traversal is single-threaded, so the
  comparison is too.

## Decisions

### D1. The chunk unit is Crust's

The harness uses Crust's `subtree_roots` rule and its threshold of 4, so (B)
reproduces what Crust does today and (A) is compared like for like. Below the
threshold every strategy degenerates to (C), and the harness says so.

### D2. "Reading" approximates an import, with a digest

For every prim in a chunk, the harness reads:

- its type name;
- `xformOpOrder` and the ops it names;
- `points`, `faceVertexCounts` and `faceVertexIndices`;
- `material:binding` targets;
- `might_be_time_varying` on `points` and on the transform ops.

That covers composition and value resolution, the costs that differ between
strategies, without depending on openusd-schemas. Each prim's reads are
hashed (`std::hash::DefaultHasher::new()`, which has fixed keys). The digest
is the hash over the prims in path order, so chunk order does not matter.
Equal digests mean equal data read.

### D3. One process per strategy, Linux memory from `/proc`

Peak memory cannot be reset inside a process, so the binary runs one
strategy per invocation (`--strategy a|b|c`), and a driver script runs all
three. Memory comes from `/proc/self/status`: `VmHWM` for the peak and
`VmRSS` after each chunk, which shows whether unload actually frees memory.
This needs no `unsafe` and no libc. On other systems the memory columns read
"unavailable". The hand runs are assumed to happen on Linux.

The output is one JSON line per phase or chunk (`open`, `chunk <path>`,
`total`), with times, memory and the digest. That is easy to diff and to
paste into `docs/streaming-spike.md`.

### D4. Notice probe on a generated scene

On the generated fixture:

1. Open the stage `LoadNone` and install a recording `StageSink`.
2. Author an override on a prim **inside** an unloaded payload, through the
   stage's edit target.
3. Load that payload, edit again, then unload it.

The probe records every `CommittedChange` (resynced and changed-info-only
paths) at each step. Two questions decide whether (A) can serve Mantle's live
mirror:
- does an edit under an unloaded payload notify at all?
- do `load` and `unload` notify as resyncs that an index would have to tell
  apart from edits?

### D5. A generated fixture keeps CI honest

`moho-spike` generates a scene into a temporary directory: a root layer with
one `/world` prim holding 8 children, each with a payload to its own layer of
a few meshes (transforms, points, topology, a material binding, one animated
`points`). Tests run (A), (B) and (C) in-process on it and assert equal
digests and non-empty chunk lists. They also run the notice probe and assert
what it records. The probe's outcome is asserted only once it is known and
written down; until then it is printed. Memory is never asserted in CI.

### D6. Decision rule and the constructor it implies

Choose **(A)** when, on both ALab and Moana:

- its peak memory is at most **1.25×** (B)'s;
- its total time is at most **1.5×** (B)'s;
- the probe shows edits under unloaded payloads notify, and load or unload
  notices can be told apart from edits.

Otherwise choose **(B)**. If (A) wins on memory but fails the notice test, the
choice is a **hybrid**: (B) for offline imports and (A) for the live mirror.
The thresholds are this design's assumption and can be changed before the
runs.

The constructor follows from the outcome:

- **(A):** `StageSceneIndex::new(stage)` over a stage opened `LoadNone`. The
  index loads and unloads subtrees as consumers pull and release them.
- **(B):** `StageSceneIndex::from_source(source)`, where a `StageSource` opens
  stages on demand (`open(load, mask)`). This is the trait sketched in
  Mantle's `docs/crust-audit.md` §2. The index keeps a `LoadNone` hierarchy
  stage for structure and change notices, and opens masked chunk stages when
  a consumer pulls.

The record states the chosen signature and why.

### D7. The spike crate is unpublished and kept

`crates/moho-spike` (`publish = false`) depends on openusd. It is kept after
S0 as a regression benchmark for S2's parity work (Crust on Moho), and can be
removed when that is done.

## Risks / Trade-offs

- **[Moana needs a big machine]**: (B) alone peaked around 44 GiB in Crust's
  import. The harness reads less than an import, so it needs less, but still
  tens of GiB → run on the workstation Crust's numbers came from, and skip
  (C) on Moana, using Crust's recorded 75.74 GiB as its baseline.
- **[The read set is not a full import]** → the comparison is between
  strategies, with the same reads under each. S2's parity tests measure the
  real import later.
- **[`VmHWM` includes allocator retention]** → each strategy runs in a fresh
  process, and post-chunk `VmRSS` shows the trend.
- **[An openusd version effect]** → numbers are tied to `68a867a` and
  recorded with the rev. A later bump reruns the driver.

## Open Questions

None that change the plan. The numbers and the probe's outcome are the
spike's results, not inputs to it.

# Proposal

## Why

Moho's `StageSceneIndex` must serve Crust at production scale, and its
constructor cannot be fixed until we know how it streams. Crust's own
measurements (crust-render `usd_import/mod.rs`, `stream_roots`):

- composing the whole Moana island costs openusd **75.74 GiB and 6m19s**;
- a stage masked to one element costs **1.10 GiB and 2.6s**;
- so Crust imports element by element, each top-level subtree under its own
  `LoadAll` stage with a population mask, dropped when done: **117.10 GiB and
  13:20 become 43.76 GiB and 09:19**, pixel-identical.

A scene index that composes one whole stage would lose that. openusd offers
a second way to bound what is composed, payload `load`/`unload` on one stage.
Its source warns that each call under `LoadNone` pays two full-store
recomposes. Nobody has measured it. This spike is S0 of the `scene-index`
program (Mantle's OpenSpec change of that name). It measures the options and
decides `StageSceneIndex`'s constructor before S1 specifies it.

## What Changes

- **A measurement harness** in this repository: an example binary that walks
  a scene's prims and reads what an import reads (types, transforms, points,
  material bindings), under each strategy:
  - **(A) single stage, payload streaming:** opened `LoadNone`, then for each
    top-level subtree `load_and_unload` it, read it, and unload it.
  - **(B) masked stage per chunk (Crust's way):** a `LoadNone` hierarchy
    stage, plus one `LoadAll` stage masked to each subtree, opened, read and
    dropped in turn.
  - **(C) single stage, everything loaded:** the reference for correctness
    and for small scenes.

  For each it reports peak and post-chunk resident memory, wall time per
  phase and per chunk, and a digest of everything read.
- **Equivalence:** for every strategy, the digest must equal (C)'s on the
  same scene, so a faster strategy cannot be a wrong one. This is checked in
  CI on a generated scene with payloads, and on ALab and Moana by hand.
- **Change notices across loading:** whether a `StageSink` on a `LoadNone` or
  partly loaded stage reports edits under unloaded payloads, and whether
  loading or unloading fires notices a scene index would mistake for edits.
  This decides whether (A) can also serve Mantle's live mirror.
- **A results record**, `docs/streaming-spike.md`: numbers on ALab and Moana,
  notice behavior, and the decision with its constructor signature, ready for
  S1 to specify.

No library API ships. The harness is its own unpublished workspace crate,
`crates/moho-spike`, so the `moho` crate stays empty until S1.

## Capabilities

### New Capabilities
<!-- None: a spike produces measurements and a decision, not system behavior.
     The change sets skip_specs: true. The decision becomes requirements in
     S1's specs. -->

### Modified Capabilities
<!-- None. -->

## Impact

- `crates/moho-spike` (`publish = false`): the strategies, the digest, the
  memory probe, the generator for the CI fixture, a `streaming-spike` binary
  and its tests.
- `docs/streaming-spike.md` (the results), `docs/architecture.md` (the open
  streaming question resolved), `docs/openusd-notes.md` (loading and notice
  behavior as observed), and `ROADMAP.md` (S0 done).
- Datasets: ALab 2.2 and the Moana island are not in the repository or CI.
  The harness takes a scene path, and the hand runs happen on a workstation
  holding them. Moana's full-stage reference (C) is not re-run: Crust's
  recorded 75.74 GiB composition stands as its baseline.
- openusd: no change. Any gap found goes under "Upstream candidates".

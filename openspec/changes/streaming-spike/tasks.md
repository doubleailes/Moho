# Tasks

## 1. Spike crate and fixture

- [ ] 1.1 Add `crates/moho-spike` (`publish = false`, workspace lints, depends on the workspace `openusd`). Verify that `cargo build --workspace` passes and `cargo metadata` lists it as unpublished.
- [ ] 1.2 Write the fixture generator: a root layer with `/world` and 8 children, each payloading its own layer of meshes (transforms, points, topology, a material binding, one animated `points`), written to a temp directory. Verify with a test that opens it `LoadAll` and finds 8 chunks and the expected prim count.

## 2. Strategies and digest

- [ ] 2.1 Implement Crust's chunk rule (`subtree_roots` plus the threshold of 4) and the read set with its order-independent digest (D2). Verify with tests: the fixture yields 8 chunks, and two reads of the same stage give the same digest.
- [ ] 2.2 Implement strategy (C) (single stage, `LoadAll`), (B) (`LoadNone` hierarchy plus a `LoadAll` masked stage per chunk, dropped after reading) and (A) (single `LoadNone` stage, `load_and_unload` per chunk, then unload). Verify with a test that all three give equal digests on the fixture.

## 3. Measurement binary

- [ ] 3.1 Add the `streaming-spike` binary: `--strategy a|b|c <scene>`, emitting one JSON line per phase and chunk with wall time, `VmRSS`/`VmHWM` from `/proc/self/status` ("unavailable" elsewhere) and the digest (D3). Verify by running it on the fixture for each strategy and checking that the digests match and the JSON parses.
- [ ] 3.2 Add `scripts/streaming-spike.sh <scene>`, which runs the three strategies in separate processes and prints a comparison table (with `--skip-c` for Moana). Verify on the fixture.

## 4. Notice probe

- [ ] 4.1 Implement the probe (D4): a `LoadNone` stage with a recording `StageSink`, an edit under an unloaded payload, then load, edit and unload, recording every `CommittedChange`. Verify that it runs on the fixture and prints each step's resynced and changed-info-only paths, then add assertions for the observed behavior and record that behavior in `docs/openusd-notes.md`.

## 5. Runs and decision

- [ ] 5.1 Run the driver on ALab 2.2 (all three strategies) and on Moana (A and B), on Linux, at the pinned openusd rev. Verify that the outputs are saved under `docs/streaming-spike/` with the machine, rev and date.
- [ ] 5.2 Write `docs/streaming-spike.md`: the tables, the probe's outcome, the decision under D6's rule, and the chosen `StageSceneIndex` constructor signature. Update `docs/architecture.md` (open question resolved), `ROADMAP.md` (S0 done) and, if a gap was found, the "Upstream candidates" list. Verify that the decision cites the numbers that justify it.

## Workflow follow-up

- Archive this change, then carry the decision into Mantle's `scene-index` specs and Moho's S1 change.

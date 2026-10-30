# Update latency

How long an edit takes to reach a Hydra 2.0 consumer
(`openspec/changes/hydra-parity-chain`, design D8). Update latency ranks
ahead of first-load time in Moho's profiling. Reported, not gated.

## Method

`crates/moho-spike`'s `update` binary builds the consumer `moho::chain`
describes over a `LoadAll` stage: `chain::standard`, light linking and
dependency forwarding as renderer filters, `PrimTypeNoticeBatchingSceneIndex`
and a `PrimManagingObserver` whose factory manages every typed prim. A run
times one edit from authoring it on the stage to the observer having
received every notice (`apply_pending_updates`, then `flush`); the
consumer's own update is not included. An added light is removed after its
run, untimed. Each table gives the median (the mean of the middle two for an
even N) and worst of N runs, and what the observer received in the last run.

```sh
cargo build --release -p moho-spike --bin update
target/release/update --runs 50 --fixture 10000
target/release/update --runs 10 --camera /island/cam/shotCam <samples>/island/usd/island.usda
```

The fixture is a camera with a translate op and N meshes under `/world`
(the linked light includes `/world/a`).

Machine: `cdg2-phil-01`, Xeon E5-2699 v4 (72 threads), 94 GiB, Linux 5.14
(Rocky 9), release build, openusd `d7d2a29`. The fixtures were run on
2026-10-30, after the walk cost work of issue #27 (`walk.md`); Moana on
2026-10-28, with Moho `d8ce000`, before it (not re-run).
Moana ran under `systemd-run --scope -p MemoryMax=80G -p MemorySwapMax=0`.
"Authoring" is the part of the total spent authoring the edit on the stage
(openusd); the rest is Moho's.

## Fixture, 1 000 meshes (50 runs)

Setup (chain, filters, batching, observer, first flush): 0.03 s, 1 001
managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.020 | 0.217 | 0.009 | 0 | 1 | 1 |
| camera transform | 0.028 | 0.108 | 0.014 | 0 | 1 | 1 |
| light, trivial collections | 0.228 | 0.534 | 0.029 | 1 | 0 | 0 |
| light linked to one prim | 2.357 | 2.830 | 0.179 | 1 | 1 | 1 |

## Fixture, 10 000 meshes (50 runs)

Setup: 0.27 s, 10 001 managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.021 | 0.132 | 0.009 | 0 | 1 | 1 |
| camera transform | 0.025 | 0.059 | 0.013 | 0 | 1 | 1 |
| light, trivial collections | 0.762 | 1.466 | 0.038 | 1 | 0 | 0 |
| light linked to one prim | 24.871 | 30.163 | 0.746 | 1 | 1 | 1 |

Before issue #27 (Moho `8163843`, same day): setup 0.71 s, the linked light
48.3 ms (1 000 meshes: 4.23 ms), the other edits within noise.

## ALab (5 runs)

`ALab/entry.usda`, after issue #27. Setup: 10.0 s (41.2 s before), 8 416
managed prims, peak 5.23 GiB.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.212 | 16 619.368 | 0.050 | 0 | 1 | 1 |
| camera transform | 0.205 | 0.308 | 0.041 | 0 | 1 | 1 |
| light, trivial collections | 4.047 | 20.853 | 0.053 | 1 | 0 | 0 |
| light linked to one prim | 229.611 | 433.580 | 8.847 | 1 | 1 | 0 |

The first edit after setup (the worst focal length) takes 16.6 s, before
and after #27 alike; not yet located. The linked light was 250 ms before.

## Moana island (10 runs)

`island/usd/island.usda`, `LoadAll`, camera `/island/cam/shotCam`, the
linked light including `/island/isCoral/isCoral5/geometry/hires_part/starhornstem0004_geo`.
Setup: 265 s, 136 858 managed prims, peak 7.01 GiB.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.118 | 3.666 | 0.023 | 0 | 1 | 1 |
| camera transform | 0.298 | 0.551 | 0.035 | 0 | 1 | 1 |
| light, trivial collections | 100.819 | 169.936 | 0.320 | 1 | 0 | 0 |
| light linked to one prim | 2 942.480 | 3 129.935 | 139.423 | 1 | 1 | 1 |

## Reading

- **A camera edit does not grow with the scene**: one dirtied entry, one
  managed prim told, in tens of microseconds. The first runs of this
  benchmark found two costs that did: merging deduplicated children with a
  linear search (300 ms for a linked light on 10 000 meshes), and every
  update read the populated USD prototypes, whose subtrees hold most of
  Moana's prims (0.4 ms per camera edit on 10 000 meshes, 22 ms on Moana).
  Both are gone: a camera edit on Moana takes 0.1 to 0.3 ms.
- **A light with trivial collections** dirties nothing but its own
  addition, as Hydra's light linking ignores trivial collections. Its time
  still grows with the scene (0.2 ms on 1 000 meshes, 101 ms on Moana), on
  Moho's side, not openusd's: some filter's handling of an added prim walks
  more than the prim. Not yet located; the next profiling target.
- **A linked light** costs what Hydra's algorithm costs: the union of the
  dirty expressions is matched once over the whole input from the root
  (`HdsiLightLinkingSceneIndex::_ComputeAllMatches`), so it grows with the
  number of prims the walk visits. Since issue #27 the walk itself costs
  under 1 µs per prim on the fixture (`walk.md`); most of what remains
  (about 20 of the 25 ms on 10 000 meshes) is openusd rediscovering the
  stage's prototypes after the edit, when the walk lists the absolute
  root's children (an upstream candidate in `ROADMAP.md`). Its own dirt
  (`light/lightLink`) is absorbed by its added entry in the batching.

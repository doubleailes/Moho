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
(Rocky 9), release build, openusd `d7d2a29`, Moho `d8ce000`, 2026-10-28.
Moana ran under `systemd-run --scope -p MemoryMax=80G -p MemorySwapMax=0`.
"Authoring" is the part of the total spent authoring the edit on the stage
(openusd); the rest is Moho's.

## Fixture, 1 000 meshes (50 runs)

Setup (chain, filters, batching, observer, first flush): 0.04 s, 1 001
managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.023 | 0.100 | 0.008 | 0 | 1 | 1 |
| camera transform | 0.034 | 0.068 | 0.014 | 0 | 1 | 1 |
| light, trivial collections | 0.206 | 0.315 | 0.029 | 1 | 0 | 0 |
| light linked to one prim | 3.846 | 4.286 | 0.149 | 1 | 1 | 1 |

## Fixture, 10 000 meshes (50 runs)

Setup: 0.55 s, 10 001 managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.023 | 0.203 | 0.008 | 0 | 1 | 1 |
| camera transform | 0.036 | 0.073 | 0.014 | 0 | 1 | 1 |
| light, trivial collections | 0.664 | 1.772 | 0.040 | 1 | 0 | 0 |
| light linked to one prim | 45.970 | 55.649 | 0.770 | 1 | 1 | 1 |

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
  number of prims the walk visits (3 s on Moana, of which 0.14 s is
  openusd authoring the light). On the fixture, most of it is openusd listing children
  (about 1 µs per child, twice that on the first listing after an edit; an
  upstream candidate in `ROADMAP.md`). Its own dirt (`light/lightLink`) is
  absorbed by its added entry in the batching.

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
run, untimed. Each table gives the median and worst of N runs, and what the
observer received in the last run.

```sh
cargo build --release -p moho-spike --bin update
target/release/update --runs 50 --fixture 10000
target/release/update --runs 10 --camera /island/cam/shotCam <samples>/island/usd/island.usda
```

The fixture is a camera with a translate op and N meshes under `/world`
(the linked light includes `/world/a`).

Machine: `cdg2-phil-01`, Xeon E5-2699 v4 (72 threads), 94 GiB, Linux 5.14
(Rocky 9), release build, openusd `d7d2a29`, Moho `e387d60`, 2026-10-27.
Moana ran under `systemd-run --scope -p MemoryMax=80G -p MemorySwapMax=0`.
"Authoring" is the part of the total spent authoring the edit on the stage
(openusd); the rest is Moho's.

## Fixture, 1 000 meshes (50 runs)

Setup (chain, filters, batching, observer, first flush): 0.05 s, 1 001
managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.026 | 0.207 | 0.009 | 0 | 1 | 1 |
| camera transform | 0.039 | 0.085 | 0.015 | 0 | 1 | 1 |
| light, trivial collections | 0.225 | 0.522 | 0.033 | 1 | 0 | 0 |
| light linked to one prim | 4.083 | 4.712 | 0.160 | 1 | 1 | 1 |

## Fixture, 10 000 meshes (50 runs)

Setup: 0.52 s, 10 001 managed prims.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.025 | 0.180 | 0.009 | 0 | 1 | 1 |
| camera transform | 0.034 | 0.084 | 0.014 | 0 | 1 | 1 |
| light, trivial collections | 0.622 | 1.447 | 0.037 | 1 | 0 | 0 |
| light linked to one prim | 44.100 | 53.848 | 0.710 | 1 | 1 | 1 |

## Moana island (10 runs)

`island/usd/island.usda`, `LoadAll`, camera `/island/cam/shotCam`, the
linked light including `/island/isCoral/isCoral5/geometry/hires_part/starhornstem0004_geo`.
Setup: 248 s, 136 858 managed prims, peak 7.01 GiB.

| edit | median (ms) | worst (ms) | authoring (ms) | added | dirtied entries | prim dirty calls |
|---|---|---|---|---|---|---|
| camera focalLength | 0.102 | 1.809 | 0.017 | 0 | 1 | 1 |
| camera transform | 0.309 | 0.458 | 0.029 | 0 | 1 | 1 |
| light, trivial collections | 92.257 | 141.157 | 0.194 | 1 | 0 | 0 |
| light linked to one prim | 2 861.008 | 3 000.939 | 129.535 | 1 | 1 | 1 |

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
  still grows with the scene (0.2 ms on 1 000 meshes, 92 ms on Moana), on
  Moho's side, not openusd's: some filter's handling of an added prim walks
  more than the prim. Not yet located; the next profiling target.
- **A linked light** costs what Hydra's algorithm costs: the union of the
  dirty expressions is matched once over the whole input from the root
  (`HdsiLightLinkingSceneIndex::_ComputeAllMatches`), so it grows with the
  number of prims the walk visits (3 s on Moana, of which 0.13 s is
  openusd authoring the light). On the fixture, most of it is openusd listing children
  (about 1 µs per child, twice that on the first listing after an edit; an
  upstream candidate in `ROADMAP.md`). Its own dirt (`light/lightLink`) is
  absorbed by its added entry in the batching.

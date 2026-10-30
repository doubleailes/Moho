# Walk cost

What `prim()` and `children()` cost on every prim of the standard chain
(issue #27). A walk from the absolute root is what a light linking
invalidation does (`HdsiLightLinkingSceneIndex::_ComputeAllMatches`), and
what population and the managing observer's first flush do, so its
per-prim cost bounds both the linked-light update and the first load.

## Method

`crates/moho-spike`'s `walk` binary opens a scene `LoadAll` and builds
`chain::standard`'s filters by hand, in its order, then light linking and
dependency forwarding as renderer filters, keeping every layer. It walks
each layer from the absolute root, depth first, calling `prim()` then
`children()` once per prim (no data source is read): one first walk, then
the median and best of N more. The layer above a filter adds that filter's
cost. `--layer NAME` walks one layer only, for profiling.

```sh
cargo build --release -p moho-spike --bin walk
target/release/walk --runs 10 --fixture 10000
target/release/walk --runs 3 <samples>/ALab/entry.usda
```

The fixture is `update`'s: a camera and N meshes under `/world`.

Machine: `cdg2-phil-01`, Xeon E5-2699 v4 (72 threads), 94 GiB, Linux 5.14
(Rocky 9), release build, openusd `d7d2a29`, 2026-10-30. "Before" is Moho
`8163843`.

## What the profile found

Profiled with callgrind (the fixture) and stack sampling (ALab).

1. **openusd's `SchemaRegistry::is_a`** walks the schema base graph with a
   fresh SipHash `HashSet` on every call. The stage index asks it several
   times per prim (`skips_prim_data`, whether a prim represents its
   descendants, the adapters). Fixed by remembering the answer per
   registry, schema type and base (`stage::is_a`); the base graph is fixed
   once a registry is built.
2. **The stage index's `children()`** listed the stage's children and
   composed the prim's `typeName` on every call (openusd's child listing
   costs about 1 µs per child). It now holds each prim's children, as it
   holds its prims, until a resync reaches them: a resync forgets the
   children at and under its root, of its parent, and of the absolute root
   (whose children include the prototypes).
3. **`RetainedSceneIndex::children()` scanned every prim it held.** Native
   instancing's instance aggregation keeps its instancers in one, so each
   `children()` there was linear in the instancers, and a walk of ALab
   quadratic (about 630 µs per prim). It now links each prim to its
   parent, as `HdRetainedSceneIndex`'s `SdfPathTable` does; children keep
   path order.
4. **SipHash on paths.** The path-keyed caches of the stage index,
   flattening, material binding resolving and merging used `std`'s
   `HashMap`, hashing each path's text again on every lookup. They now use
   `sdf::PathMap` and `sdf::PathSet`, which reuse the hash a path holds.
5. **Native instancing's prototype pruning** built every ancestor path of
   a prim to find its root prim; it now reads the root prim's name.

None of these changes data, layout or notices: the test suite, and a dump
of every prim, type and data source, plus the notices of an edit set
(visibility, `displayColor`, a light added, linked, retargeted and edited,
a prim defined and one removed, a time change), with nothing read first and
with everything read first, are byte-identical before and after on
crust-render's 44 samples, `Kitchen_set` and `Kitchen_set_instanced`,
MaterialX's teapot and lion, Moana's `isCoral` and ALab (nothing read).

The Hydra comparison of `hydra-parity-check.md` was re-run (USD 26.08,
conda-forge `openusd-26.08-py313h6963fe8_0`) on its whole corpus, once
with Moho at `8163843` and once with these changes:

- **Static data:** 143 scenes, the same prims and types on both sides in
  each, no unexplained difference. The per-scene tables of the two Moho
  revisions are identical, and isCoral's full Moho dump (19.9 million
  lines) is byte-identical.
- **Notices:** with nothing read, 906 of 1 214 edits give Hydra's entries;
  with everything read, 874 of 1 192 (two edits more than the parity check
  counted, from fixtures added since). Every other edit falls in a class
  that check documents, and the totals per class are the same for both
  revisions.
- **Run to run:** an edit making a prim non-instanceable (openusd's
  prototype retirement, class (a)) varies from run to run in Hydra and in
  Moho, at either revision: how often an entry is notified depends on the
  order in which the replacement prototype is composed. Run 40 times per
  revision, the 11 such edits in the scenes where one run differed give
  the same sets of variants, or sets that differ only in variants seen
  once or twice in 40; every other edit of those scenes (164) is
  deterministic and identical.

## 10 000 meshes (10 runs)

Median per prim (µs):

| layer | before | after |
|---|---|---|
| stage | 4.88 | 0.11 |
| extent resolving | 5.08 | 0.15 |
| point instancing | 5.49 | 0.46 |
| native instancing | 6.35 | 0.51 |
| instance proxy path translation | 6.23 | 0.52 |
| material binding resolving | 6.76 | 0.39 |
| render settings flattening | 5.42 | 0.43 |
| light linking | 5.91 | 0.68 |
| dependency forwarding | 6.20 | 0.77 |

Building the chain over the set stage: 0.35 + 0.07 s before, 0.09 + 0.01 s
after (standard chain + renderer filters).

## ALab (3 runs)

`ALab/entry.usda`, 13 503 prims in the stage index, 17 681 at the
terminal. Median per prim (µs):

| layer | before | after |
|---|---|---|
| stage | 9.42 | 0.20 |
| extent resolving | 9.60 | 0.27 |
| point instancing | 85.28 | 4.19 |
| native instancing | 717.54 | 8.17 |
| instance proxy path translation | 627.08 | 8.11 |
| material binding resolving | 665.78 | 4.42 |
| render settings flattening | 662.00 | 4.33 |
| light linking | 638.81 | 4.72 |
| dependency forwarding | 637.84 | 7.48 |

Population (`walk`): `set_stage` 17.99 → 12.83 s, standard chain 6.12 →
2.04 s, renderer filters 11.90 → 0.16 s. The `update` benchmark's setup
(chain, renderer filters, batching, observer and first flush) went from
41.2 s to 10.0 s; most of what remains is `set_stage`, openusd composing
and traversing the stage.

## What remains

- **openusd's prototype discovery.** After any structural edit, the next
  `Stage::prototypes()` walks the whole populated namespace again (an
  upstream candidate in `ROADMAP.md`). The absolute root's children name
  the prototypes, so the first walk after a resync pays it: about 20 ms of
  the linked light's 25 ms on 10 000 meshes (`update-latency.md`).
- **Ancestor paths.** Merging's `inputs_at` and point instancing's
  instancer test build each ancestor `sdf::Path` (a new string and its
  hash) where USD's interned `SdfPath` parents are nearly free. This is
  most of point and native instancing's remaining cost on ALab.

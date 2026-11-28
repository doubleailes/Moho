# Moho

A renderer-agnostic scene index over [openusd](https://github.com/mxpv/openusd)
stages, in the model of Hydra 2.0. Moho sits between a composed USD stage and
the renderers that draw it: it hands out prims as lazily evaluated data
sources, reports what changed down to the data locator, and owns the time
everything is read at.

It is named after the Mohorovičić discontinuity, the boundary between the
Earth's crust and mantle. [Mantle](https://github.com/doubleailes/mantle), the
DCC, feeds it. [Crust](https://github.com/doubleailes/crust-render), the path
tracer, and Obsidian, the GPU viewport, consume it.

Status: bootstrap. See `ROADMAP.md` and `docs/architecture.md`.

## Toward an upstream `openusd-imaging`

Moho is meant to be offered to openusd as an `openusd-imaging` crate. It
already fits on the boundary: it depends on `openusd` and `openusd-schemas`
only, names no consumer, follows Hydra and UsdImaging quirk for quirk, and
shares openusd's license, edition and MSRV. What it still needs:

1. **openusd changes merged. Done.** Moho pins mxpv/openusd's main
   (`9ba0611`), which holds every change Moho needed, the `.mtlx` format
   included.
2. **Ports moved into openusd.** The eight pull requests of
   mxpv/openusd#195 (#187 to #194) deleted Moho's copies, and
   `sdf::Path::make_relative` builds `primOrigin`. `TfHash::Combine` over
   strings and over a sorted map (`tf_hash.rs`) moves into openusd's `tf`
   as public API, the first pull request. The rest is under "Upstream
   candidates" in `ROADMAP.md`.
3. **Scope agreed** in [mxpv/openusd#200](https://github.com/mxpv/openusd/issues/200):
   imaging is in scope, as one `openusd-imaging` crate, with a committed
   C++ dump and a regenerating test as its compliance test. The plan is in
   [the comment](https://github.com/mxpv/openusd/issues/200#issuecomment-7213936329)
   that follows; whether the test may normalize the three ordering
   differences below is open.
4. **A C++ layout.** Modules `hd` (`scene_index`, `data_source`,
   `locator`, `schema`, `tokens`, merging, flattening, dependency
   forwarding, the renderer plugin), `hdsi` (the other filters) and
   `usd_imaging` (`stage`, instancing propagation, `chain::standard`),
   landed as one pull request each in that order.
5. **Parity as a compliance test.** The Hydra comparison runs from
   `tools/parity/` (both dumpers, the edit generator and the comparator),
   and `docs/hydra-parity-check.md` has its results on `9ba0611`. What
   remains is the test openusd asked for: commit the scene index dumps,
   with the edits and notices applied, and a test that regenerates them,
   over Moho's fixtures and openusd's `vendor/usd-wg-assets`.
6. **Moho's additions to Hydra.** `__usdAttributes`, `__usdRelationships`
   and `__usdMetersPerUnit` stay downstream, or come behind an opt-in if
   openusd's maintainer prefers.
7. **openusd's conventions.** Under its lint table Moho has 14 clippy
   findings; it also needs `max_width = 120`, terse test names (most are
   longer than four words), comments free of planning steps and change
   names, `TODO(rayon)` markers where the `Rc`-based scene indices could
   run in parallel, and a home for `scene_index::testing`.
8. **Downstream policy split off.** The `moho::openusd`,
   `moho::openusd_schemas` and `moho::openusd_mtlx` re-exports and the
   feature forwarding move to a thin downstream crate for the stack's single
   pin, and the host gives the stage builder its formats (`.mtlx`).
   `moho-spike`, `moho-render` and `moho-render-wgpu` stay here.

## License

MIT

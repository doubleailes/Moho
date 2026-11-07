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

1. **openusd changes merged. Done.** Moho pins mxpv/openusd's main at the
   `openusd-imaging` placeholder (`26c81d8`), which holds the reorder
   resync, the asset path walk's plain data probe and the one-lookup
   prototype root Moho's fork pin carried.
2. **Ports moved into openusd. Done for mxpv/openusd#195.** Its eight pull
   requests (#187 to #194) deleted Moho's copies: the memoized `is_a`,
   sorted authored properties, the collection membership path expression,
   `TfHash::Combine`'s hashing, a single-path `Value::Path`, `ColorSpaceAPI`
   and `GfColorSpace`, the `typeName` resync, and the prototype resyncs.
   What remains (`SdfPath::MakeRelativePath` in `stage/prim_origin.rs`,
   among others) is under "Upstream candidates" in `ROADMAP.md`.
3. **A scope discussion.** openusd now carries an `openusd-imaging`
   placeholder crate. Agree the design, the layout and the parity results
   with its maintainer before any large pull request.
4. **A C++ layout.** Split Moho the way the C++ libraries are split: Hd
   (`scene_index`, `data_source`, `locator`, `schema`, `tokens`), Hdsi
   (`filter`) and UsdImaging (`stage`, `chain::standard`), as modules or
   crates agreed in that discussion.
5. **Parity as a compliance test.** The Hydra comparison in
   `docs/hydra-parity-check.md` runs from a harness outside the repository.
   Commit the C++ scene index dumps, with the edits and notices applied to
   them, and a test that regenerates them, as openusd does for composition.
6. **The generic attribute data.** `__usdAttributes` and
   `__usdRelationships` are Moho's one addition to Hydra; be ready to make
   them opt-in or keep them downstream.
7. **openusd's conventions.** Its lint table (mostly `doc_markdown`, fixed by
   adding `UsdImaging` to `doc-valid-idents`), `max_width = 120`, comments
   free of planning steps and change names, and `TODO(rayon)` markers where
   the `Rc`-based scene indices could run in parallel. `moho-spike` stays
   here.
8. **Downstream policy split off.** The `moho::openusd` and
   `moho::openusd_schemas` re-exports and feature forwarding move to a thin
   downstream crate for the stack's single pin.

## License

MIT

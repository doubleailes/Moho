## Why

Moho re-exports the openusd it pins (`moho::openusd`,
`moho::openusd_schemas`), but its consumers also depend on openusd
themselves. Keeping one openusd across Moho, crust-render and Mantle then
takes a git pin in each, a `[patch.crates-io]` in crust-render and Mantle
(crust-render asks for crates.io `openusd`), a bump order, and checks
(`cargo tree -i openusd`, crust's compile-time guard) to catch drift. In
OpenUSD, imaging and core ship as one build, so an application cannot mix
them. Moho can play that role: if consumers take openusd only through it,
one pin is the stack's, by construction.

## What Changes

- Moho's re-exports become the only supported way to reach openusd for its
  consumers, documented in `lib.rs`, `CLAUDE.md` and `architecture.md`.
- Moho forwards the openusd features consumers enable: `mmap` and `serde` to
  `openusd`, and `media`, `physics`, `proc`, `semantics`, `skel`, `ui` to
  `openusd-schemas` (the families Moho reads are always on). CI builds with
  every feature on.
- The consumers move afterwards, in their own repositories: crust-render
  (with S2, which already adds Moho) drops its `openusd` and
  `openusd-schemas` entries and its `[patch]`, then Mantle does the same.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
None: a dependency policy, no requirement changes (`skip_specs`).

## Impact

- `crates/moho/Cargo.toml` (features), `crates/moho/src/lib.rs` (docs),
  `.github/workflows/ci.yml`, `CLAUDE.md`, `docs/architecture.md`.
- crust-render and Mantle follow in their own changes.

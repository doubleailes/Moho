# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read this, then `docs/architecture.md`, then `docs/openusd-notes.md` before
changing code.

## What Moho is

A renderer-agnostic scene index over openusd stages, in the model of Hydra
2.0: scene indices, lazily evaluated data sources, locators, observers of
added, removed and dirtied prims, and the time owned by the index. Naming
lore: **openusd** is the core, **Mantle** the layer that moves (the DCC),
**Crust** the renderer on top, and **Moho** the boundary between crust and
mantle. **Obsidian** is the GPU viewport. Mantle feeds Moho, and Crust and
Obsidian consume it.

The program is planned in Mantle's OpenSpec change `scene-index`
(https://github.com/doubleailes/mantle). The steps that concern this
repository are in `ROADMAP.md`.

## Layout

```
crates/moho        the scene index: locator, data_source, scene_index, schema,
                   stage (StageSceneIndex and its adapters), filter, chain,
                   streaming, tokens
crates/moho-spike  S0 streaming spike harness (unpublished; S2 benchmark),
                   strategy (D) reading through Moho
docs/              architecture.md, openusd-notes.md, streaming-spike.md
scripts/           streaming-spike.sh (runs the spike's strategies on a scene)
openspec/          OpenSpec changes and specs for this repository
```

## Commands

```sh
cargo test -p moho            # one crate
cargo test --workspace        # everything
```

## Conventions

- Cargo workspace, edition 2024, stable toolchain (`rust-toolchain.toml`),
  `rustfmt` `max_width = 110`.
- `#![forbid(unsafe_code)]` in every crate (also `unsafe_code = "forbid"` in workspace lints).
- No `unwrap()` outside tests (`clippy::unwrap_used` is denied; integration
  test files opt out with `#![allow(clippy::unwrap_used)]`). Prefer returning
  errors over `expect`.
- MIT license.
- Before every commit, all three must pass:
  ```sh
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
- Work in small, tested increments; one concern per commit.

## Boundaries

- **openusd only** (openusd and openusd-schemas, one rev). Moho never depends
  on Mantle, Crust or Obsidian, and holds none of their types. Adapters
  (Mantle's `ChangeSet` to a mirror stage, a renderer's import) live in the
  consumers.
- **Hydra 2.0's model and names.** Schemas, filters and the standard chain
  (`moho::chain::standard`) follow UsdImaging. Moho's one addition is the
  generic data (`__usdAttributes`, `__usdRelationships`, and the root's
  `__usdStageMetadata`); anything else Hydra lacks is argued in the change
  that adds it.
- **The consumer decides.** Moho reports what changed (dirty locators). It
  never decides whether a renderer can update in place.

## openusd

- Pure-Rust USD (https://github.com/mxpv/openusd). **Its API is not stable.**
  It is pinned by git `rev` in the root `Cargo.toml`. `openusd` and
  `openusd-schemas` must use the same rev. Until mxpv/openusd#175 merges, the
  pin is on doubleailes/openusd's `perf/clip-time-varying` (upstream plus the
  clip fixes); move back to mxpv/openusd at its merge.
- **One rev across the stack.** Mantle depends on the same git source at the
  same rev, and crust-render patches its crates.io `openusd` to it. Bump in
  order: Moho, then crust-render, then Mantle.
- **Never guess a signature.** Read the source at the pinned rev
  (`~/.cargo/git/checkouts/openusd-*/<rev>/crates/...`) and record any API you
  start relying on in `docs/openusd-notes.md`.
- If openusd lacks something, don't work around it silently: add it to
  "Upstream candidates" in `ROADMAP.md` (and to the gaps in
  `docs/openusd-notes.md`), and comment the workaround in code.
- `moho` re-exports `openusd` (`moho::openusd`), so consumers build against
  exactly the pinned version, and `openusd-schemas` (`moho::openusd_schemas`)
  from the same rev. openusd-schemas is part of the openusd project, so
  depending on it keeps Moho "openusd only".

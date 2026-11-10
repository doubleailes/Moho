# Tasks

## 1. The adapter

- [x] 1.1 Add `prim_type::PARTICLE_FIELD` and `tokens::particle_field` (the six primvar names)
- [x] 1.2 Add `AdapterRegistry::set_type_and_derived` and `derived_base`; make `adapter_type` take the first derived base a prim `is_a` (LightFilter moved onto it); register `ParticleField` with its derived types
- [x] 1.3 Add `stage/particle_field.rs`: the pair table, `custom` choosing the float or half attribute through `attribute_in_use`, and the type adapter's `invalidate`
- [x] 1.4 In `stage/primvars.rs`, overlay the particle field custom primvars (named always, given when authored, the mapping's interpolation, a custom primvar's role, no element size)

## 2. Tests

- [x] 2.1 `tests/particle_field.rs`: float-only, half-only, both authored (float wins; an empty float array loses), nothing authored, a plain `ParticleField`, the coefficients' metadata in `__usdAttributes` only
- [x] 2.2 A `.usdc` fixture written by OpenUSD 26.11 (`scripts/particle-field-fixture.py`), read with equal values
- [x] 2.3 Invalidation: each twin dirties its one primvar, the degree its own, authoring the float twin switches the source; a time change dirties animated positions alone
- [x] 2.4 Through `chain::standard`, the prim keeps its type and primvars

## 3. Documentation and gate

- [x] 3.1 `docs/architecture.md` (particle fields, derived-type lookup), `docs/openusd-notes.md` (`attribute_in_use`), `docs/hydra-parity-check.md` (not in the corpus yet), `ROADMAP.md` (the array-size read upstream candidate)
- [x] 3.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

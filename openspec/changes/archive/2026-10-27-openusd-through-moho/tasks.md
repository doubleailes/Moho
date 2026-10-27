## 1. Moho

- [x] 1.1 Forward openusd's features (`mmap`, `serde`) and the schema families Moho does not read (`media`, `physics`, `proc`, `semantics`, `skel`, `ui`) as Moho features. Verify with `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and add that to CI.
- [x] 1.2 Document the re-exports as the only way consumers reach openusd, in `lib.rs`, `CLAUDE.md` (the one-rev rule) and `docs/architecture.md`.

## 2. Checks

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `openspec validate openusd-through-moho --strict`.

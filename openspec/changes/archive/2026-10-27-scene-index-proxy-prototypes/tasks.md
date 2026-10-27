## 1. Proxy prototypes (M12)

- [x] 1.1 Resolve point instancer prototype targets inside native instances through the propagated prototype, and copy them under the instancer's `ForInstancer<hash>` scope. Verify with a point instancer targeting a mesh inside an instanceable asset.

## 2. Checks

- [x] 2.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace` and `openspec validate scene-index-proxy-prototypes --strict`.

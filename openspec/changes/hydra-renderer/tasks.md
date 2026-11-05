# Tasks

## 1. Create args

- [ ] 1.1 Add the `gpuEnabled` token (`HdRendererCreateArgsSchemaTokens`) to `tokens.rs`, and `schema::RendererCreateArgsSchema` over a `ContainerHandle` with `gpu_enabled() -> Option<bool>`, following the existing schemas. Verify with unit tests: `gpuEnabled` true read back, and empty create args giving `None`
- [ ] 1.2 Document the schema (the reference header, and why `drivers` is left out) and verify that `cargo doc -p moho` builds without warnings

## 2. Renderer and plugin traits

- [ ] 2.1 Add `crates/moho/src/renderer.rs` and declare it in `lib.rs`. It holds `Renderer` (`update(&self)`), `RendererPlugin` (associated `type Renderer`, `is_supported(&RendererCreateArgsSchema) -> Result<(), Unsupported>`, `renderer_filters() -> Vec<RendererFilter>`, `create_renderer(SceneIndexHandle, &RendererCreateArgsSchema) -> Option<Self::Renderer>`) and `Unsupported { reason }`, with module docs mapping each item to `HdRenderer` / `HdRendererPlugin` / `HdRenderDelegate::Update` (design D1–D3). Verify that `cargo build -p moho` and `cargo clippy --workspace --all-targets -- -D warnings` pass
- [ ] 2.2 Test (`tests/renderer.rs`): a GPU-only test plugin answers `Unsupported` with a reason naming the GPU for `gpuEnabled` false, and is supported for true. Verify with `cargo test -p moho --test renderer`

## 3. Creating a renderer over a chain

- [ ] 3.1 Add `chain::create_renderer(input, host_filters, &plugin, &args) -> Result<CreatedRenderer<P::Renderer>, CreateRendererError>`. It checks support first, applies `host ++ plugin.renderer_filters()` through `renderer_filters`, then calls `create_renderer` on the terminal. `CreateRendererError` has `Unsupported(Unsupported)` and `Declined` (design D3, D5, D6). Verify that it builds and clippy is clean
- [ ] 3.2 Tests in `tests/renderer.rs`, with filters that record their build order and a plugin that records the index it was given: host scene globals (0, `AtStart`) with plugin dependency forwarding (1000, `AtEnd`) and light linking (4, `AtStart`) give forwarding over linking over globals over the input, and the plugin receives that terminal; a host filter precedes a plugin filter at equal phase and order; an unsupported plugin fails with its reason and no filter is built (the recording filter is never called); a plugin returning `None` fails with `Declined`. Verify with `cargo test -p moho --test renderer`

## 4. The prim-managing renderer

- [ ] 4.1 Add `renderer::PrimManagingRenderer` (`new(terminal, priority, factory)`, `update` flushing the batching, `observer()`, `prim(path)`) implementing `Renderer`, documented as hdPrman's `_RileySceneIndices` (design D7). Verify that it builds and clippy is clean
- [ ] 4.2 Tests over a stage through `chain::standard`: no managed prim before the first update, then the material's factory call comes before the mesh's at that update; a `doubleSided` edit reaches the managed mesh only at the next update, at `mesh/doubleSided`; `prim(path)` gives the made prim, or nothing for a declined type. Verify with `cargo test -p moho --test renderer`
- [ ] 4.3 Test an edit end to end through `create_renderer` with a plugin returning a `PrimManagingRenderer`: stage edit, `apply_pending_updates`, then `update`, and the dirtied entry is processed when `update` returns. Verify with `cargo test -p moho --test renderer`

## 5. Docs

- [ ] 5.1 Rewrite the `chain` module example over a small plugin and `create_renderer` (the three steps in one call), and verify that the doctest passes with `cargo test -p moho --doc`
- [ ] 5.2 Update `docs/architecture.md`: "Chain" step 3 names the plugin, `create_renderer` and `PrimManagingRenderer`, with the associated-type decision recorded. Update `ROADMAP.md` "Later": the render control side, waiting for Hydra's replacement of `HdLegacyRenderControlInterface`. Verify that both files name only types that exist (`grep` for each new item name in `crates/moho/src`)

## 6. Integration

- [ ] 6.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`, and verify that all three pass
- [ ] 6.2 Run `openspec validate hydra-renderer --strict` and verify that it passes

## Workflow follow-up

- Archive the change with `/opsx:archive` after review, then check that `openspec/specs/renderers/spec.md` exists with its Purpose.

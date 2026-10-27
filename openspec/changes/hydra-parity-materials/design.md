## Context

See `proposal.md` for why, and `../hydra-parity-stage/audit.md` (sections
3, 9 and 10) for the comparison. Hydra references are to OpenUSD
`70c45e83b`: `pxr/usdImaging/usdImaging/materialBindingsResolvingSceneIndex.cpp`,
`dataSourceMaterial.cpp`, `lightAPIAdapter.cpp`, `primAdapter.cpp`
(`LookupLightParamAttribute`), `pxr/imaging/hd/materialSchema.h` and the
material network, node, interface and connection schemas.

After `hydra-parity-instancing`, binding opinions reach every prim through
the flattening inside native propagation, and paths into instances are
translated before resolution. The resolving filter still asks the stage's
`CollectionMembership` oracle.

Principle, as in the other parity changes: parity means UsdImaging's
behaviour at the reference revision, quirks included; a fix a renderer
needs over it (as arnold-usd ships its own scene index plugins) belongs to
that renderer, not to Moho.

## Goals / Non-Goals

**Goals:**
- Binding resolution, network layout and light data as UsdImaging's, so
  an `hdcrust` delegate reads the same data from Moho and from Hydra.

**Non-Goals:**
- A shader registry (Sdr): openusd has none. `nodeIdentifier` of a shader
  in `sourceAsset` or `sourceCode` mode stays empty, as UsdImaging gives it
  when Sdr cannot resolve one; `nodeTypeInfo` carries what a consumer needs
  to resolve it.
- `HdsiMaterialBindingResolvingSceneIndex` (choosing among binding purposes)
  and material dependency declaration (a material change invalidating bound
  geometry): both are renderer-side in Hydra (Storm's and arnold-usd's
  plugins); a consumer adds them through `hydra-parity-chain`'s renderer
  filters.
- Material locator caching (`HdsiLocatorCachingSceneIndex`): see
  `hydra-parity-chain`.

## Decisions

### D1. Resolution over the input scene index

The filter keeps a handle on its input and judges a collection binding by
reading the collection prim's `collections/<name>/membershipExpression`
from the input and evaluating it there with `hydra-parity-stage`'s
evaluator. Evaluators are cached per (collection prim, name) and dropped
when that prim is dirtied for `collections` or removed. The per-level order
is the four steps of the spec, read from the flattened binding vector
(ancestors first). The constructor loses its `CollectionMembership`
argument.

### D2. Dirty locators as Hydra's

`usdMaterialBindings` in a dirty set is replaced by `materialBindings`
(prefix replacement); other locators pass. The collection-dependent
re-dirtying Moho does today is dropped: Hydra's resolving index tracks no
dependency on collections, and no other UsdImaging component does
(`DependenciesSchema` appears in none of the binding code). A live consumer
that edits collections declares its own dependency, as Hydra's renderers do.

### D3. Networks as `UsdImagingDataSourceMaterial`

- Context names: per output namespace, plus `__all` (the token value of
  `HdMaterialSchemaTokens->all`; the audit wrote `all`).
- `__all`: every `Shader` descendant of the material as a node, connected
  or not; terminals under their full output base name.
- `config`: `config:*` attributes, prefix stripped, on every context.
- `interface`: `parameters/<input>` with `displayName`, `displayGroup`,
  `mappings` (each `{nodePath, inputName}` consuming the material input),
  and `parameterOrder` (material inputs in property order).
- `nodeTypeInfo` (`info:*` other than `info:id`, and `sdrMetadata`) when the
  implementation source is not `id`; `renderContextNodeIdentifiers` for
  light and light filter nodes only.
- The walk (`value_producing_attributes`, shader outputs only) already
  drops sources the stage does not hold once the off-stage case of M10 is
  removed.

### D4. Lights as the light API adapter

`light` names three entries and answers others through a port of
`LookupLightParamAttribute` (its token-to-`inputs:` map, else the attribute
of that name). `material` is built by the material data source with a fixed
terminal (`light`): one node keyed by the light's absolute path,
`nodeIdentifier` from the light's `light:shaderId` (or
`<context>:light:shaderId`), contexts `""`, `__all` and each such context.
Invalidation as `InvalidateImagingSubprim`: `inputs:*` → `light` and
`material`; `light:*` → `light`. Light filters get the same treatment with
`lightFilter:shaderId` once `hydra-parity-chain` exposes them.

## Risks / Trade-offs

- [Collection edits no longer re-resolve bindings by themselves] → Hydra's
  behaviour; documented; consumers that need it add a dependency filter.
- [USD-only predicates in binding collections now match nothing] → as in
  Hydra; the predicates Hydra defines cover type, visibility, purpose, data
  and primvars.
- [Consumers reading `material/""` only see a light as a material] → every
  light has `material` in Hydra too; consumers filter by prim type.

## Migration Plan

Lands after `hydra-parity-instancing`. Consumers read `materialBindings`,
choose a network context (`__all` for every node), and read lights through
`light` or `material`.

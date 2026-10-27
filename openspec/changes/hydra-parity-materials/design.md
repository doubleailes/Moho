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

## Deviations recorded while implementing

Checked against USD 26.08's `UsdImagingCreateSceneIndices` (the comparison
harness): material, binding and light data match field for field, but for
the order of `interface/parameters` (C++ iterates an unordered map) and
Moho's single-path `PathVec`s; the notices of the edits below match, but
for `__usdAttributes`.

- **Task 4.1, a collection edit on the binding prim still re-resolves.**
  UsdImaging's `MaterialBindingAPI` adapter dirties `usdMaterialBindings`
  for a `collection:*` edit on a prim applying the API
  (`materialBindingAPIAdapter.cpp`, `InvalidateImagingSubprim`), and
  flattening carries it to the descendants, so `a_collection_edit_reresolves`
  stays, as `a_collection_edit_on_a_binding_prim_reresolves`.
  `editing_a_collection_elsewhere_reresolves` became its inverse
  (`editing_a_collection_elsewhere_dirties_no_bound_prim`).
- **Task 4.2, removing a shader notifies nothing.** UsdImaging resyncs a
  removed shader as a prim with no adapter: nothing reaches its material
  (checked with the harness). `removing_a_shader_dirties_its_material`
  became `removing_a_shader_notifies_nothing`; the network read again has
  lost the shader.
- **A material represents its descendants** (beyond the tasks, as the
  last of M10's stage-side support). UsdImaging's material adapter is
  `RepresentsSelfAndDescendents`: shaders and node graphs under a material
  are not populated nor listed as children (`GetPrim` still answers them,
  typeless, as UsdImaging's does), and their edits dirty the material as
  `InvalidateImagingSubprimFromDescendent` does: the terminals whose
  connections reach the shader, and its node in that context and `__all`,
  named by the prim's name alone (`material/mtlx/nodes/ss` for
  `graph/ss`, UsdImaging's quirk), else the whole `material`;
  `ui:nodegraph:*` edits alone dirty nothing. A prim of another type
  resynced under a material (a shader retyped to a `Scope`, say) is
  repopulated, its subtree added, and the material is not told, as
  `_ApplyPendingResyncs` does (PR #19 review). A retype is a resync: openusd
  reports a `typeName` edit as info-only, so the stage index promotes it
  (an openusd upstream candidate). Light filters, also
  `RepresentsSelfAndDescendents`, wait for `hydra-parity-chain`.
- **Task 2.5, `config:*` dirties nothing.** UsdImaging's material adapter
  invalidates an authored output, an input reached through a terminal's
  connections, or any `inputs:*` (all `material`); a `config:*` edit falls
  through to the prim's base invalidation, which ignores it. Moho does the
  same. Its adapters invalidate per property, so a batch editing an output
  and, say, a transform also dirties `xform`, which UsdImaging skips once
  the material's own locators are non-empty.
- **Binding names as UsdImaging** (beyond the tasks, the instancing
  implementer's note). `materialBindings` is named on every prim
  (`_PrimDataSource::GetNames`) and given only with opinions; it names
  every purpose its opinions name. The stage index's `usdMaterialBindings`
  names `""`, `preview` and `full`, and each level both kinds of binding,
  whatever is bound; flattening keeps a lone side's opinions as they are
  and leaves a purpose neither binds empty.
- **The binding edge case against UsdShade.** With a strong direct binding
  and a weaker matching collection binding on one prim, UsdImaging picks the
  direct binding and UsdShade's `ComputeBoundMaterial` the collection's.
  Moho follows UsdImaging; `equals_openusd_bound_material` leaves that
  prim out.
- **Collection evaluators are cached** per collection prim and name, and
  dropped when that prim is dirtied for `collections`, or added, removed
  or renamed; UsdImaging builds one per match.
- **Dome lights** (beyond the spec's three names). UsdImaging's dome
  adapters overlay `light` with `portals` (both domes) and `domeOffset`
  (`DomeLight_1`, a quarter turn about X for a `Z` pole), and dirty `light`
  for `portals` and `poleAxis`. Moho adds them; the spec is corrected.
- **Light quirks reproduced.** A `<context>:light:shaderId` edit and a
  plain attribute edit (`treatAsPoint`) dirty nothing; a light's
  `material` interface lists its authored inputs (the light viewed as a
  `UsdShadeMaterial`); a light input with no value answers its fallback in
  `parameters` but is not named there.
- **Parameter and terminal quirks reproduced.** A terminal's
  `upstreamNodeOutputName` is the output its material output connects to
  (a node graph's), not the resolved shader's; in `__all`, a terminal other
  than `surface`, `displacement` and `volume` names the connected prim
  itself (a node graph); `inputs:sourceColorSpace` is named among a
  texture's parameters but gives nothing, and becomes `inputs:file`'s
  `colorSpace`; a colour or asset parameter without a colour space gives
  the empty token.
- **Property order.** openusd lists authored properties in composed order;
  Moho sorts a material's inputs and outputs in dictionary order and
  applies `propertyOrder`, as C++ `GetAuthoredPropertiesInNamespace`
  (an openusd upstream candidate).

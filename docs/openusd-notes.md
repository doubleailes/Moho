# openusd notes

The openusd APIs Moho relies on, read from the source at the pinned rev
(`crates/openusd/src/usd/...`). openusd's API is not stable: re-read these
before every bump, and add any API Moho starts relying on. Sections written
before the bump to `d7d2a29` (doubleailes/openusd, upstream `0a4bb1b` plus
mxpv/openusd#175) were read at `68a867a`.

## Bump to `ca53e71`

doubleailes/openusd's `feat/mtlx-format-plugin`, one commit over `bac9080`:
`openusd-mtlx` embeds MaterialX 1.39's `nprlib` nodedefs beside `stdlib`,
`pbrlib` and `bxdf`, so a document's NPR nodes resolve their `info:id`. No
API change.

## Bump to `bac9080`

doubleailes/openusd's `feat/mtlx-format-plugin`: mxpv/openusd main
(`900ea31`) plus file formats a registry is given at run time
(mxpv/openusd#215) and the `.mtlx` format (mxpv/openusd#213). The
package reloads between `26c81d8` and `900ea31` change no API Moho uses.

- `usd::StageBuilder::format(&'static dyn sdf::FileFormat)` adds a format to
  the builder's registry; `resolver` keeps the formats already added.
- `openusd_mtlx::MTLX`, the read-only `.mtlx` format: a document composes as
  `UsdShade` prims in `usdMtlx`'s layout. `stage::stage_builder` gives it to
  every stage it builds. A repeated child name reads the first, as
  MaterialX's reader does.

## Bump to `26c81d8`

mxpv/openusd main at the `openusd-imaging` placeholder: back on upstream,
which now holds the three fixes the doubleailes pin carried (the reorder
resync, `AbstractData::holds_plain_data`, mxpv/openusd#186) and the imaging
parity changes of mxpv/openusd#195 (#187 to #194).

- `Stage::add_sink(..) -> Result<StageSinkId>` and `Stage::remove_sink(id)
  -> Result<()>`: both fail with `sdf::LayerError::Busy` while what is
  pending cannot be delivered first. The stage index then holds the stage
  without a sink (`stage/mod.rs`, `Current::sink`).
- `StageSink::load_rules_changed`, `layer_muting_changed`, and the new
  `loads_retried` and `recomposed` take `usd::Resynced<'_> { subtrees:
  &[sdf::Path], prims: &[sdf::Path] }`: subtree resyncs, minimal in
  ancestors, and prims recomposed in place with nothing beneath them
  implied. `CommittedChange::recomposed: &[sdf::Path]` is the same for an
  edit, disjoint from `resynced`. Moho repopulates each subtree; for a
  recomposed prim it reads what it held again without a notice, since USD
  names no such prim and UsdImaging sends nothing for it, or removes its
  subtree when the stage no longer exposes it (`stage/pending.rs`,
  `apply_pending_updates`).
- `usd::ColorSpaceAPI::compute_attribute_name(&Attribute, Option<&mut
  ColorSpaceCache>) -> Result<Option<Token>>` (C++
  `UsdColorSpaceAPI::ComputeColorSpaceName`, mxpv/openusd#192) and
  `gf::ColorSpace`: a colour primvar's or a material parameter's
  `colorSpace` (`stage/primvars.rs`). Names a `ColorSpaceDefinitionAPI`
  defines are valid now; the attribute's declared `colorSpace` wins over
  the prims' as in C++, where Moho tried it after them. Moho passes no
  cache, which nothing would evict on an edit.
- Prototypes follow C++ `Usd_InstanceCache` (mxpv/openusd#194): numbered
  from `/__Prototype_1`, and kept, resynced, when the instance they were
  composed from goes or their source changes. The stage index no longer
  resyncs re-pointed instances after an edit. A notice still leaves out
  the prototypes the change composes (see "Gaps"), which the stage index
  adds itself.
- `vt::Value::Path(sdf::Path)` (mxpv/openusd#191), in memory only: Hydra's
  single `SdfPath` fields are a `Value::Path` (they were one-element
  `Value::PathVec`s), so a single path and a path array are told apart.
- `tf::HashState` (C++ `Tf_HashState`, mxpv/openusd#190): `append(bytes)`
  and `code()`. Moho's `TfHash::Combine` names (`ForInstancer%zx`,
  `Primvars%zx`) use it in place of its own SpookyHash port (`tf_hash.rs`).
- `SpecSite.layer` is an `sdf::LayerHandle`; its identifier is
  `layer.identifier()`.
- Core metadata fields are typed: an attribute's `colorSpace` reads as a
  `Value::Token`, as USD types it.
- `CommittedChange::resynced` now holds a prim whose `typeName`,
  `apiSchemas`, `kind`, `specifier`, `active` or clip metadata a layer
  edits, as C++ `UsdStage` notifies (mxpv/openusd#193). A `kind` edit
  repopulates the prim, as in UsdImaging.

## Bump to `54db9fd`

doubleailes/openusd's `perf/path-prim-len`: `92b9066` plus mxpv/openusd#186.
A query below a prototype finds the enclosing `/__Prototype_N` with one
lookup, and skips it when the stage mask includes everything, where it
built every ancestor path. No API Moho uses changed.

## Bump to `92b9066`

doubleailes/openusd's `perf/walk-type-probe`: `689bfde` plus
`AbstractData::holds_plain_data`, which the asset path walk
(`usd_utils::all_dependencies`, `modify_asset_paths`, packaging) uses to
skip a field holding plain data without decoding it; a crate layer answers
from the value's type. No API Moho uses changed.

## Bump to `689bfde`

doubleailes/openusd's `prim-children-resync`: upstream `5b86332` (which holds
mxpv/openusd#175) plus a resync for a prim whose children or properties a
layer reorders.

- The value types moved from `sdf` to `vt`: `vt::Value`, `vt::Dictionary`,
  `vt::ValueKind`.
- `shade::MaterialBindingAPI::bound_material(purpose, legacy)` takes C++
  `supportLegacyBindings` per call; with `legacy` false it counts only prims
  with `MaterialBindingAPI` applied.

## Bump to `d7d2a29`

- `has_authored_metadata(key)` moved onto the `usd::Object` trait (with the
  other `UsdObject` queries), so callers import it (`use openusd::usd::Object
  as _`).
- `Shader::id()` is gone: the identifier is
  `shader.node_def().shader_id() -> Result<Option<Token>>` (C++
  `UsdShadeNodeDefAPI::GetShaderId`), `None` outside identifier mode.
- mxpv/openusd#175: `Attribute::might_be_time_varying` answers from a clip
  set's manifest and schedule without opening its clips, and an
  `AttributeQuery`'s value source no longer opens every clip. On ALab, a raw
  read with time-variance checks fell from 26.0 to 11.1 GiB peak.

## Change tracking

- `Stage::add_sink<S: StageSink + 'static>(&self, sink: S) ->
  Result<StageSinkId>` and `Stage::remove_sink(&self, id: StageSinkId) ->
  Result<()>` (`stage.rs`).
- `StageSink::after_commit(&self, stage: &Stage, change: &CommittedChange<'_>)`
  (`sink.rs`): the composed change after recomposition. It is the equivalent
  of C++ `UsdNotice::ObjectsChanged`. A plain `Fn(&Stage, &CommittedChange)`
  closure is a `StageSink`. Sinks must not add or remove sinks from inside a
  callback.
- `CommittedChange { resynced: &[sdf::Path], recomposed: &[sdf::Path],
  changed_info_only: &[sdf::Path], .. }`: prim and property paths in stage
  namespace; `recomposed` names prims recomposed in place, nothing beneath
  them implied.
- `CommittedChange::changed_fields(&self, path: &sdf::Path) -> impl
  Iterator<Item = &tf::Token>`: the fields authored at a stage path by the
  edit (C++ `GetChangedFields`).
- `Stage::apply_diff(&self, diff: &Diff, mode: ApplyMode<'_>) ->
  Result<(), StageAuthoringError>` (`diff.rs`). `ApplyMode::ExactLayer(identifier)`
  replays onto the named layer of the stage.

## Time

- `Attribute::might_be_time_varying(&self) -> Result<bool>` and
  `Stage::value_might_be_time_varying(&self, attr_path) -> Result<bool>`. Both
  are conservative for value clips with more than one active clip.
- `Attribute::time_samples(&self) -> Result<Option<sdf::TimeSampleMap>>`
  (`None` for clip-driven values), `time_sample_times(&self) -> Result<Vec<f64>>`,
  and `time_samples_in(&self, interval: impl Into<gf::Interval>) ->
  Result<Vec<f64>>`. **Correction (S1):** at `68a867a`, `time_samples_in`
  keeps only the samples inside the interval (its source notes C++ also
  stops at the bracketing sample). Moho takes `time_sample_times` and adds
  the bracketing samples itself (`data_source::bracketing_sample_times`).
- Generic reads (S1): `Prim::attributes()` lists the composed attributes,
  the schema's declared ones included; `Attribute::is_defined`,
  `has_authored_value`, `has_authored_metadata(key)`, `metadata::<Value>(key)`
  (raw: `colorSpace` is a `Value::String`), `type_name() -> Option<ValueTypeName>`
  (`as_token()`), `connections()`; `Prim::relationships()`,
  `Relationship::targets()` and `forwarded_targets()`.
- `usd::TimeCode` (`usd/timecode.rs`) is always a numeric time: the
  default time is the `None` arm of `impl Into<Option<TimeCode>>` at the
  call site (`Attribute::get_at`). `TimeCode::new(0.0)` is C++'s
  default-constructed `UsdTimeCode` (time code 0), where a stage index
  starts; `TimeCode::EARLIEST` (`f64::MIN`, C++ `EarliestTime()`) and
  `is_earliest()` are where it resets when its stage is replaced. A query
  at `EARLIEST` resolves the earliest sample; an offset added to it stays
  `EARLIEST` (`f64::MIN` absorbs it).
  `TimeCode::value()` is the time as an `f64`, what a host gives the scene
  globals filter's `set_current_frame`.
- `sdf::Path::default()` is the empty path (`SdfPath()`), which
  `Path::new` refuses to parse; `Path::is_empty` tests for it. The scene
  globals filter reads a set empty path as `Value::Path` of it.
- `Stage::start_time_code`, `end_time_code` and
  `has_authored_time_code_range` read the root and session layers only, as in
  USD.

## Loading and population (streaming, S0)

- `Stage::load(&self, path, policy: LoadPolicy) -> Result<(),
  sdf::PathParseError>`, `unload(&self, path)`, and
  `load_and_unload(&self, to_load: impl IntoIterator<Item = (impl IntoPath,
  LoadPolicy)>, to_unload: impl IntoIterator<Item: IntoPath>)`. `LoadPolicy` is
  `WithDescendants` (the default) or `WithoutDescendants`.
- Builder: `.load(InitialLoadSet)`, `.mask(StagePopulationMask)`, and
  `Stage::mask(&self) -> StagePopulationMask`.
  `StagePopulationMask::new(paths: impl IntoIterator<Item: sdf::IntoPath>) ->
  Result<Self, StagePopulationMaskError>` (absolute prim paths only).
- **Correction:** the "two full-store recomposes per call" comment is on
  `Stage::find_loadable` (and so `load_set`), which swaps in
  `LoadRules::all()` and back. `load`, `unload` and `load_and_unload` install
  one rule change and drop a bounded set of cached indices
  (`pcp/load_rules.rs`, `set_load_rules`): the touched paths and their
  ancestors whose effective rule is not `All`. The S0 spike measures what
  that costs.

## Notices across loading (S0 probe)

Observed on a stage opened `LoadNone`, with a sink installed, editing through
the root edit target (`crates/moho-spike/tests/probe.rs` asserts all of it):

- **Edits under an unloaded payload notify**, through `after_commit`, in stage
  namespace. `override_prim` on a prim inside the unloaded payload resyncs
  that prim. A new attribute spec resyncs the property path, then its value is
  `changed_info_only`.
- **Load and unload never fire `after_commit`.** They fire only
  `StageSink::load_rules_changed(&self, stage, resynced: Resynced<'_>)`, so
  an index can tell them from edits.
- **The reported paths are exact** (since `26c81d8`, mxpv/openusd#174).
  Loading or unloading `/world/element_0` reports it as a subtree, and
  `/world`, whose loaded state flipped with no payload of its own, as a
  prim recomposed in place, so sibling chunks are not covered. Before, the
  notice was `["/world"]` alone, a subtree resync covering every sibling.
- An `over` authored on the root layer composes its prim even while the
  payload above it is unloaded (undefined, so default traversal skips it), so
  a value authored there stays readable across unload.

## Reading (S0 harness)

- `Prim::filtered_subtree(&self, PrimPredicate) -> Result<PrimRange>` (an
  iterator of `Result<Prim>`), `filtered_children`, `children`. Under
  `PrimPredicate::DEFAULT` the walk stops at instances.
- `Prim::is_instance() -> Result<bool>`, `Prim::prototype() ->
  Result<Option<sdf::Path>>`. Prototype paths (`/__Prototype_N`) are numbered
  per composition, so they differ between stages and across load changes.
- `Prim::type_name() -> Result<Option<Token>>`,
  `Attribute::get_at::<vt::Value>(time: impl Into<Option<TimeCode>>)` (the
  raw value), `Relationship::targets() -> Result<Vec<sdf::Path>>` (targets
  authored in a payload come back mapped to stage namespace).
- `Stage::override_prim(path)`, `Stage::create_attribute(path, type_name)`
  then `Attribute::set(value)`.

## openusd-schemas (S1)

- `moho` depends on `openusd-schemas` from the same git source and rev as
  `openusd`, with the features `geom`, `lux`, `render`, `shade` and `vol`.
  Each schema family is opt-in; none is on by default.
- **Typed views and fallbacks need the schema registry on the stage.**
  `openusd_schemas::schema_registry() -> Arc<SchemaRegistry>` (a process-wide
  `OnceLock` over `ALL`) is handed to `Stage::builder().schema_registry(..)`.
  A stage opened without it knows only the core `usd` family: typed `get`
  constructors answer `None`, and `Attribute::fallback_value` finds no schema
  fallback.

## Stage scene index (S1)

- `Stage::traverse(&self, PrimPredicate, impl FnMut(&sdf::Path)) -> Result<()>`:
  depth-first, children in composed order; `PrimPredicate::DEFAULT` (active,
  loaded, defined, not abstract) stops at instances, which it still visits.
- `Stage::prim_matches(&self, &sdf::Path, PrimPredicate) -> Result<bool>`: the
  prim's status composed with its ancestors', so it answers for any path,
  masked or not.
- `Prim::filtered_children(&self, PrimPredicate) -> Result<Vec<Prim>>`: an
  instance has none under `DEFAULT`.
- `Prim::type_name(&self) -> Result<Option<Token>>` and
  `Prim::api_schemas(&self) -> Result<Vec<Token>>` (the prim definition's
  applied schemas, built-in ones included; a multiple-apply instance reads
  `CollectionAPI:lightLink`). `Prim::is_instance(&self) -> Result<bool>`.
- `StageBuilder::schema_registry(Arc<SchemaRegistry>)`: see openusd-schemas
  above; `moho::stage::stage_builder()` installs it.
- Scene globals: `Stage::start_time_code`, `end_time_code`,
  `time_codes_per_second`; `openusd_schemas::geom::stage_meters_per_unit` and
  `stage_up_axis -> Result<UpAxis>` (both fall back to the registry's
  declared value, else 0.01 and `Y`); `render::Settings::stage_settings_path
  -> Result<Option<sdf::Path>>` (the `renderSettingsPrimPath` metadata, empty
  as `None`).
- `vt::Value::Path` holds one path (C++ `VtValue` of `SdfPath`,
  mxpv/openusd#191); writers refuse it.

## Geometry schemas (S1)

- `openusd_schemas::geom::XformQuery::for_prim(&Prim) -> Result<XformQuery>`
  (the empty query for a prim that is not Xformable): `local_transform(time)
  -> Result<gf::Mat4d, SchemaError>` (row vectors, the last op most local),
  `resets_xform_stack()`, `transform_might_have_effect()` (false for no ops
  or a lone op beside its inverse), `transform_might_be_time_varying()`,
  `time_samples()` (the ops' unioned samples). The query fixes the op list
  when built; Moho builds one per read. `Xformable::affects_transform(name)`
  is C++ `IsTransformationAffectedByAttrNamed`.
- `PrimvarsAPI::from_prim_unchecked(Prim).primvars_with_values()` (authored
  or fallback; `:indices` attributes are not primvars). `Primvar`:
  `primvar_name()`, `interpolation_token()` (fallback `constant`),
  `element_size()` (fallback 1), `is_indexed()`, `indices_attr()`,
  `flattened::<Value>(time)` (C++ `ComputeFlattened`). `Attribute::role() ->
  Result<Option<sdf::Role>>` gives the Hydra role. `Prim::is_a("PointBased")`
  and `is_a("Curves")` select the plain attributes Hydra reads as primvars.

## Particle fields (`particle-field-adapter`)

- `openusd_schemas::vol::ParticleField3DGaussianSplat::from_prim_unchecked(Prim)`
  views any prim; `attribute_in_use(SplatData) -> Result<Attribute>` is C++
  `_UsesFloatAttr`: the float attribute when its value at
  `TimeCode::EARLIEST` is an array with elements, else the `half` one
  (also when neither is authored). `uses_float(SplatData)` answers the
  same as a bool. `SplatData` names the five pairs (`Positions`,
  `Orientations`, `Scales`, `Opacities`, `RadianceCoefficients`).
- The question reads the float attribute's whole array (its `TODO(perf)`:
  `Attribute` has no array-size read); the stage index asks it only for the
  primvar fetched.
- `Prim::is_a("ParticleField")` holds for `ParticleField3DGaussianSplat`
  (the registry's base chain: `ParticleField`, `Gprim`).
- `.usdc` decodes `point3h[]`, `half3[]`, `quath[]` and `half[]`
  (`Value::Vec3hVec`, `QuathVec`, `HalfVec`), checked against a layer
  OpenUSD 26.11 wrote (`crates/moho/tests/data/particle_field.usdc`).

## Coordinate systems (Moho issue #46)

- `openusd_schemas::shade::CoordSysAPI::all(&Prim) -> Result<Vec<CoordSysAPI>>`
  gives the applied instances in applied order (`api_schema_instance_names`);
  `name() -> &Token` is the instance name.
- `CoordSysAPI::local_binding() -> Result<Option<CoordSysBinding>>` is C++
  `GetLocalBinding`: the first forwarded target of `coordSys:<name>:binding`
  (`Relationship::forwarded_targets`), `None` when it targets nothing.
  `CoordSysBinding::coord_sys_path() -> &sdf::Path` is the target.
- Only the applied form is read, as UsdImaging 2.0's `CoordSysAPI` adapter
  reads it; a bare `coordSys:<name>` relationship gives nothing.
- `CoordSysAPI::owns_property` accepts `coordSys` and `coordSys:*` only,
  where C++ `CanContainPropertyName` (UsdImaging's invalidation test) is a
  plain `TfStringStartsWith(name, "coordSys")`. The stage index tests the
  prefix itself (`stage/coord_sys.rs`).
- `sdf::Path::is_prim()` is C++ `IsPrimPath` (`/` and property paths are
  not); `append_path("__coordSys_<name>")` appends a child, an `Err` for a
  name that is no identifier (a namespaced instance name), where C++
  `AppendChild` gives the empty path.

## Material schemas (S1)

- `openusd_schemas::shade::MaterialBindingAPI::get(&Stage, path) ->
  Result<Option<_>>` (only where the API is applied, as UsdImaging's binding
  adapter requires). `direct_binding(purpose) -> Result<Option<DirectBinding>>`
  (`material_path()`, `strength() -> Result<BindingStrength>`) and
  `collection_bindings(purpose) -> Result<Vec<CollectionBinding>>`
  (`collection_path()` such as `/world.collection:metal`, `material_path()`,
  `strength()`). `MATERIAL_PURPOSES` is `["", "preview", "full"]`;
  `owns_property(name)` tells a binding property.
- Collections: `CollectionAPI::from_prim_unchecked(prim, name)`,
  `membership_query() -> Result<MembershipQuery>`, `MembershipQuery::is_included
  (&Path) -> bool`, `uses_rule_map()`, `rule_map() -> &PathExpansionRuleMap`
  (`HashMap<Path, PathRule>`; `PathRule` is ExplicitOnly, ExpandPrims,
  ExpandPrimsAndProperties, Exclude). `complete_membership_expression` resolves
  only an authored `membershipExpression` (as C++
  `ResolveCompleteMembershipExpression`): it is empty for an includes/excludes
  collection. `rule_map_expression(&PathExpansionRuleMap) -> PathExpression`
  (C++ `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap`,
  mxpv/openusd#189, since `26c81d8`) converts such a collection's rule map;
  Moho ported it before.
- Material networks (superseded in part by `hydra-parity-materials`
  below): `shade::Material::get(&Stage, path)`; `Connectable::outputs()`
  lists declared outputs too; `Shader::get`, `Connectable::authored_inputs()`;
  `Input`/`Output::
  value_producing_attributes(ProducerFilter::{Any, ShaderOutputsOnly}) ->
  Vec<ShadingAttribute>` follows node-graph outputs and interface inputs (C++
  `UsdShadeUtils::GetValueProducingAttributes`); `ShadingAttribute::{Input,
  Output}` and `attribute()`.
- Asset values: `sdf::AssetPath` keeps `authored_path` and, from value
  resolution, `resolved_path()`, which openusd sets only when the anchored
  path names an existing file (so never for a `<UDIM>` set). Moho anchors an
  unresolved relative path on the strongest site of
  `Attribute::property_stack()` (`SpecSite.layer.identifier()`),
  as Crust does, and sets it with `set_resolved_path`.
- Stage-free matching: `sdf::path_expr::PathExpressionEval::<D>::build(&expr,
  &PredicateLibrary::new())` then `match_path(&path, &domain).value`. With an
  empty library, expressions with predicates do not compile.
- Building expressions: `PathExpression::make_atom_pattern`, `make_op(SetOp,
  ..)` (empty operands absorbed as in C++), `make_complement`,
  `make_atom_reference(ExpressionReference::weaker())`, `compose_over`,
  `parse`; `PathPattern::nothing()` + `set_prefix` + `append_stretch_if_possible`.

## Light schemas (S1)

- Every UsdLux light type's prim definition includes `LightAPI` in
  `Prim::api_schemas()`, so one API adapter covers all lights.
  `Prim::attributes_in_namespace("inputs")` lists declared inputs too (the
  LightAPI fallbacks); `Relationship::forwarded_targets()` reads
  `light:filters`.

## Instancing (S1)

- `Prim::is_instance()`, `Prim::prototype() -> Result<Option<sdf::Path>>`
  (`/__Prototype_N`, numbered per composition), `Prim::is_prototype()`.
  Native instancing names a propagated prototype by the prototype's name,
  as UsdImaging does (`hydra-parity-instancing` removed Moho's rebuilt
  instancing key).
- Parity references (S1 chain tests): `geom::XformCache::new(time)` and
  `local_to_world_transform(&Prim)`, `ImageableExt::compute_visibility(time)
  -> Visibility`, `shade::MaterialBindingAPI::bound_material(purpose, legacy)`.
- Point instancers: `geom::PointInstancer::get`, `mask_at(time, None)`
  (C++ `ComputeMaskAtTime`: inactive and invisible ids folded together,
  empty when nothing is masked), and `orientationsf` preferred over
  `orientations` when non-empty (as `uses_orientationsf`).
- `openusd_schemas::geom::PrimMetadata::inactive_ids()` (generated, on
  `usd::Prim`): the `inactiveIds` metadata as a `Value::Int64ListOp`, the
  strongest opinion (C++ composes list ops across layers);
  `sdf::ListOp::explicit_items()`, which UsdImaging reads
  (`GetExplicitItems`) to prune fully inactive prototypes. `mask_at`
  flattens the list op where C++ `ComputeMaskAtTime` reads its explicit
  items; the two differ only for a prepended or appended `inactiveIds`.
- `Relationship::targets()` (authored, unforwarded) for a point instancer's
  prototypes when it authors `inactiveIds`, as UsdImaging reads
  `GetTargets` there and `GetForwardedTargets` otherwise.
- `Stage::prototypes() -> Result<Vec<sdf::Path>>` (the stage's
  `/__Prototype_N` prims), which the stage index lists among the absolute
  root's children; `Stage::remove_prim(path)` in tests.

## Scene index gaps (`scene-index-gaps`)

- `Stage::has_authored_stage_metadata(field) -> Result<bool>`: whether the
  session or root layer authors a pseudo-root metadata field (C++
  `UsdStage::HasAuthoredMetadata`); a block counts, a fallback does not.
  (It was behind `__usdStageMetadata`, removed by `hydra-parity-stage`.)
- `Stage::stage_metadata(field) -> Result<Option<vt::Value>>`: composed
  pseudo-root metadata, falling back to a registered field's default. Read
  for `renderSettingsPrimPath`.
- `Stage::frames_per_second() -> f64`, beside `start_time_code`,
  `end_time_code` and `time_codes_per_second`.
- `sdf::Path::new` parses relative paths (`geo`, `a/b`) and the `.` anchor,
  which the prim origin's prototype-relative scene path uses.

## Prim info (`scene-index-prim-info`)

- `Prim::specifier() -> Result<Option<sdf::Specifier>>` (`Def`, `Over`,
  `Class`), `Prim::type_name() -> Result<Option<Token>>` (as authored, so
  `DomeLight_1` stays versioned), `Prim::kind() -> Result<Option<Token>>`
  (authored only) and `Prim::is_loaded() -> Result<bool>`.
- `Prim::api_schemas() -> Result<Vec<Token>>`: composed, the prim
  definition's built-in schemas first (C++ `GetAppliedSchemas`).
- `Attribute::might_be_time_varying() -> Result<bool>`, also exposed as
  `__usdAttributes/<name>/mightBeTimeVarying`.

## Hydra parity (`hydra-parity-stage`)

Read at `d7d2a29`.

- `sdf::path_expr::PredicateLibrary::<D>::new().define(name, binder)`
  (`predicate.rs`): `binder: Fn(&[FnArg]) -> Option<PredicateFn<D>>`, where
  `PredicateFn<D> = Rc<dyn Fn(&D) -> PredResult>`. Returning `None` fails
  the link ("invalid arguments"); an unknown name fails it too. There are no
  typed parameters or defaults (C++ `Define` with `NamesAndDefaults`): each
  binder reads `FnArg { name: Option<String>, value: PredicateArg }`
  (`Bool`, `Int`, `Float`, `String`) and supplies its defaults itself.
  `PredResult { value, constant }`, `PredResult::varying(bool)`,
  `constant(bool)`.
- `PathExpressionEval::<D>::build(&expr, &library) -> Result<_, EvalError>`
  requires a complete expression (references resolved, paths absolute) or
  an empty one; the empty evaluator matches nothing. `match_path(&path,
  &|path| -> D) -> PredResult` calls the domain function only when a
  predicate needs it.
- `Prim::variant_sets().selections() -> Result<Vec<(String, String)>>`
  (C++ `UsdVariantSets::GetAllVariantSelections`, which UsdImaging's prim
  info reads): the selections composed onto the prim, sorted by set name,
  read from the masked stage. (`Prim::prim_index().variant_selections()`
  answers the same from the prim index.)
- `openusd_schemas::geom::stage_up_axis(&Stage) -> Result<UpAxis>`: the
  authored `upAxis`, else the registry's fallback (`Y`); an authored token
  other than `Y` or `Z` is an error, where C++ returns the token.
  `UpAxis::as_token()`.
- `Prim::specifier() -> Result<Option<sdf::Specifier>>` (composed:
  `Def`, `Over`, `Class`).
- Model data: `Prim::is_model() -> Result<bool>` (C++ `UsdPrim::IsModel`);
  `ModelAPI::from_prim_unchecked(prim).is_kind("component",
  KindValidation::ModelHierarchy)` (C++ `UsdModelAPI::IsKind`, the default
  validation); `Object::has_authored_metadata("assetInfo")` (C++
  `UsdPrim::HasAssetInfo`) and `Prim::metadata::<Value>("assetInfo")`, a
  `Value::Dictionary` read with `Dictionary::get_at(key)` (`identifier`, an
  asset path; `name` and `version`, strings), as `UsdModelAPI`'s asset info
  getters.
- Primvars: `PrimvarsAPI::authored_primvars()` (C++ `GetAuthoredPrimvars`),
  `Primvar::from_attribute(attr) -> Option<Primvar>`, `Primvar::indices_attr()`
  with `Attribute::has_value()` (UsdImaging's `_IsIndexed`),
  `Prim::has_api_schema("ColorSpaceAPI")`, `Relationship::is_defined()`.
- `Stage::stage_metadata("upAxis")` gives the raw token when
  `stage_up_axis` refuses it.
- `Relationship::targets()` (authored targets, unforwarded) for UsdImaging's
  `ri:*` namespaced settings, `forwarded_targets()` for render relationships
  (C++ `GetForwardedTargets`).
- `PrimPredicate::new(required, rejected)` (`stage.rs`): status bits
  `ACTIVE`, `LOADED`, `DEFINED`, `ABSTRACT`; the walk stops at instances
  unless `with_instance_proxies(true)`. UsdImaging's predicate is
  `new(ACTIVE | LOADED, ABSTRACT)`, or `new(ACTIVE, ABSTRACT)` with unloaded
  prims included: `DEFINED` is not required, so `over`s are visited.

## Materials and lights (`hydra-parity-materials`)

Read at `d7d2a29`.

- `info:implementationSource`: `shade::NodeDefAPI::from_prim_unchecked(prim)`
  (any generated schema has `from_prim_unchecked`), `implementation() ->
  Result<ImplementationSource>` reads the raw token, and an unauthored,
  mistyped or unknown value is `Id` (C++ `GetImplementationSource`, which
  warns). `shader_id() -> Result<Option<Token>>` is `info:id` in `Id` mode
  only. C++ `UsdShadeNodeDefAPI(prim)` is valid only where the API is
  applied (`UsdAPISchemaBase::_IsCompatible` checks `HasAPI`): Moho asks
  `Prim::has_api_schema("NodeDefAPI")`, true for every `Shader`.
- `info:*` attributes: `Prim::property_names() -> Result<Vec<Token>>` lists
  authored and schema-declared names, sorted, then `propertyOrder` applied
  (C++ `GetPropertyNames`); `nodeTypeInfo` scans it for `info:`.
- `sdrMetadata`: the prim's raw metadata, `Prim::metadata::<Value>("sdrMetadata")`,
  a `Value::Dictionary` (`vt::Dictionary::new()` when absent, as C++ hands
  an empty `VtDictionary`). `Shader::sdr_metadata()` converts to a
  `HashMap<Token, String>`, which Hydra does not.
- `config:*`: `Prim::attributes_in_namespace("config:")` (a trailing `:` is
  accepted) and `Attribute::get_at::<Value>(None)`, the default time, as
  C++ `attr.Get(&value)`.
- Authored inputs and outputs: `Prim::authored_attributes_in_namespace(ns)`
  lists in C++ `GetAuthoredPropertiesInNamespace`'s order, dictionary order
  (`tf::dictionary_cmp`) then `propertyOrder` (mxpv/openusd#188, since
  `26c81d8`); Moho sorted them itself before, with `sdf::element_cmp`.
- Interface input consumers: `shade::NodeGraphInterface::interface_input_consumers(&self,
  transitive: bool) -> Result<InterfaceInputConsumersMap>` on `Material`
  (C++ `ComputeInterfaceInputConsumersMap`): every authored input of the
  container has an entry, consumers or not, in composed order (C++'s map is
  unordered); `consumers(&Input) -> Option<&[Input]>`, `iter()`.
  `Material::from_prim_unchecked(prim)` views any prim (a light) as C++
  `UsdShadeMaterial(prim)` does.
- Display data: `usd::Object::display_name()` and `usd::Property::display_group()`
  (`Result<Option<String>>`), composed with the schema's declared values
  (a light input's `Intensity`, `Basic`).
- Terminals: `Material::surface_source(&[&str]) -> Result<Option<ResolvedTerminal>,
  SchemaError>` (and `displacement_source`, `volume_source`), C++
  `ComputeSurfaceSource`'s context walk with the universal fallback;
  `ResolvedTerminal::sources()[0].attribute().attribute().prim()` is the
  shader. A context that is not a namespaced identifier is an error.
- Connections: `ShadingAttribute::connected_sources() -> Result<ConnectedSources>`
  (`into_sources()`; `ConnectionSource::{prim, path, name, full_name,
  source_type}`), valid when the source attribute is defined, as C++
  `GetConnectedSources`. `ConnectableAPI::from_prim(prim) -> Result<Option<_>>`
  is `Some` where the prim has a connectable behavior (C++'s validity).
  `Input::from_attribute(attr) -> Option<Input>`, `Input::base_name()`.
- Lights: `lux::LightAPI::from_prim_unchecked(prim)`;
  `shader_id_for(&[&str]) -> Result<Option<Token>>` (C++ `GetShaderId`: the
  first context whose `<context>:light:shaderId` is non-empty, else
  `light:shaderId`, `None` when empty) and `shader_id_attr_for(context) ->
  Result<Option<Attribute>>` (C++ `GetShaderIdAttrForRenderContext`).
  `light:shaderId`'s fallback is the light type's name (`SphereLight`) from
  the schema registry.
- `Attribute::role() -> Result<Option<sdf::Role>>` (`Role::Color` for a
  parameter's colour space); `Stage::stage_metadata("upAxis")` for a
  `DomeLight_1`'s `scene` pole; `gf::Mat4d::IDENTITY`, row-major in `.0`.

## Light filters and the Hydra 2.0 consumer path (`hydra-parity-chain`)

- `openusd_schemas::lux::LightFilter::from_prim_unchecked(prim)` and the
  `LightFilterExt` trait (`lux/light.rs`, for `LightFilter` and
  `PluginLightFilter`): `shader_id_for(&[context]) -> Result<Option<Token>>`
  (C++ `UsdLuxLightFilter::GetShaderId`, falling back to
  `lightFilter:shaderId`) and `shader_id_attr_for(context) ->
  Result<Option<Attribute>>` (`<context>:lightFilter:shaderId`).
- `Prim::is_a(schema) -> Result<bool>` (`usd/prim.rs`) tells a
  `LightFilter` or a type derived from it (C++ `UsdLuxLightFilter(prim)`).
  `PluginLightFilter` and `LightFilter` declare the built-in
  `CollectionAPI:filterLink`, which `Prim::api_schemas` lists.
- `sdf::PathExpression::make_op(op, left, right)` absorbs constants as C++
  `SdfPathExpression::MakeOp` does: an empty operand is the identity of a
  union, so light linking folds its dirty expressions from
  `PathExpression::default()`. `PathExpression` is `PartialEq` but not
  `Hash`: the light linking cache finds expressions by comparison.
- `StageBuilder::in_memory(identifier) -> Result<Stage>` (C++
  `UsdStage::CreateInMemory`), used by the chain module's doc test, and
  `Stage::create_relationship(path)` (the update benchmark's linked light).

## Final parity check (`docs/hydra-parity-check.md`)

- `sdf::ValueTypeName::default_value() -> Option<Value>` (C++
  `SdfValueTypeName::GetDefaultValue`: zero, empty, identity), with
  `Attribute::type_name() -> Result<Option<ValueTypeName>>`: the value an
  attribute data source answers when the attribute has neither a value nor
  a fallback, as UsdImaging's value-initialized `T`.
- `Attribute::get_at::<Value>(TimeCode::EARLIEST)` reads the earliest
  sample, as C++ `UsdGeomPointInstancer::UsesOrientationsf` does.
- `Prim::prim_definition() -> Result<Arc<PrimDefinition>>` and
  `PrimDefinition::applied_api_schemas() -> &[Token]`: C++
  `UsdPrim::GetAppliedSchemas`, registered schemas only. `Prim::api_schemas`
  also keeps the authored names no registered schema has.
- `Prim::create_attribute(name, type_name)` then `Attribute::set` notify a
  property resync, then an info change, as USD's two edits do.
- `PathExpression`'s `Display` prints a predicate's string argument bare
  when it re-reads as a token (`{hdType:mesh}`); C++ `GetText` quotes it
  (`{hdType:"mesh"}`). The same expression.

## Walk cost (Moho issue #27)

- `sdf::PathMap<V>` and `sdf::PathSet` (`sdf/path.rs`): a `HashMap` and
  `HashSet` keyed by `Path` with `FxBuildHasher`. A `Path` is an `Arc` of
  its text and the hash of that text, computed once, so a lookup mixes one
  word where `std`'s `RandomState` runs SipHash over the whole text. The
  path-keyed caches of the stage index, flattening, material binding
  resolving and merging use them.
- `Prim::is_a(schema) -> Result<bool>` answers from bases the schema
  registry computes at `build` (mxpv/openusd#187, since `26c81d8`), so
  `stage::is_a` calls it directly. Moho remembered the answer per registry,
  schema type and base before.
- `Stage::prototypes()` re-runs `discover_prototypes`, a walk of the whole
  populated namespace, the first time it is called after any structural
  edit (the population epoch moved; openusd's own `TODO(perf)` in
  `usd/stage.rs`). The stage index lists the prototypes among the
  absolute root's children, as UsdImaging does, so the first walk after a
  resync pays it: about 20 ms of a linked light's 25 ms on the 10 000 mesh
  fixture (`docs/update-latency.md`).

## Gaps

- Prototype discovery is not incremental: any structural edit makes the
  next `Stage::prototypes()` walk the whole stage, where C++'s
  `Usd_InstanceCache` is updated from the change set.
- Splines (`Ts`): a `.spline` block only round-trips as a dictionary. There
  are no typed knots, evaluation, authoring or usdc encoding.
- No layer eviction: unloading a payload leaves its layers resident
  (`Stage::find_loadable`'s documentation says so). In the S0 spike, RSS on
  one `LoadNone` stage only grew across Moana's 22 load/unload rounds
  (0.73 to 4.81 GiB), and the final unload freed nothing measurable
  (mxpv/openusd#173).

- `CollectionEvaluator::build` is `pub(super)`. No longer a gap for Moho:
  since `hydra-parity-chain` nothing reads USD collection membership
  (light linking judges collections over the scene index, as Hydra).
- No `ArResolverContext` on a stage (C++ `UsdStage::GetPathResolverContext`):
  the absolute root has no `system/assetResolution/resolverContext`
  (`stage/globals.rs`).
- Resolved-asset-path resyncs (C++
  `UsdNotice::ObjectsChanged::GetResolvedAssetPathsResyncedPaths`): openusd
  reports them since `26c81d8` (`CommittedChange::asset_paths_resynced`),
  and UsdImaging re-reads the asset-path-dependent data under those paths;
  Moho does not read them yet.
- `CommittedChange::changed_info_only` names the paths; their fields come
  from `CommittedChange::changed_fields` (C++ `GetChangedFields`).
  UsdImaging resyncs a prim for a plugin field and ignores built-in ones;
  openusd already resyncs for the fields `UsdStage` does (`typeName`,
  `kind`, ...), and Moho ignores every other prim-level info-only change,
  so a plugin field's resync (a `renderSettingsPrimPath` edit, chiefly) is
  missed (`stage/pending.rs`).
- No `SdfPath::MakeRelativePath`: the prim origin's prototype-relative scene
  path (`stage/prim_origin.rs`) is built from the path's text, parsed back
  with `sdf::Path::new`, which accepts relative paths and `.`.
- No shader registry (Sdr): a node outside `id` mode (`sourceAsset`,
  `sourceCode`) has an empty `nodeIdentifier`, where UsdImaging asks Sdr
  for one (`stage/material.rs`); its `nodeTypeInfo` carries what a consumer
  needs to resolve it.
- Authoring notices differ from USD's: a value set on an attribute without
  a spec is one resync (USD: resync, then info change); a write of the
  value already held notifies nothing; `over` specs created to author
  through a reference are resynced (USD: info-only).
- A notice (`StageSink::after_commit`, `load_rules_changed` and the other
  `Resynced` notices) names the prototypes that were composed before the
  change, not those it composes: making a prim the first instance of its
  prototype reports the prim, and loading a payload whose instances need a
  new prototype the payload, but neither `/__Prototype_N`. C++ `UsdStage`
  resyncs every prototype. The stage index resyncs, for each instance it
  repopulates, the instance's prototype (`Prim::prototype`) when it has
  not populated it (`StageSceneIndex::apply_pending_updates`), so that it
  is added and the prototype pruning hides it: a lookup per repopulated
  prim, where `Stage::prototypes()` would walk the stage after the edit.
  After a load (or an unload, a layer muted, a load retried) it also
  resyncs each prototype `Stage::prototypes()` holds that it has not
  populated: openusd's load can also give an unloaded instance outside the
  payload a new prototype, which no repopulated instance names (see the
  next gap).
- A load re-keys instances it does not touch. Loading ALab's
  `construction_cinder_block01_0003` (`LoadNone`) composes two prototypes
  in openusd, `/__Prototype_383` for the block and `382` for the unloaded
  `lab_projector01_0001/decor_sheets_projector01_0001`, which moves off
  its prototype, and retires two (`211`, `372`); C++ `Usd_InstanceCache`
  composes one (the block's) and retires one, the projector keeping its
  prototype.
- openusd-schemas' UsdGeom schema lacks `model:cardVisibility`.
- A value authored with another type than the one read is cast: a schema's
  declared type wins (`Attribute::type_name`), and
  `XformCommonAPI::xform_vectors` casts a `float3` translate or a `double3`
  rotate or scale. C++ typed `Get` fails there, and UsdImaging answers its
  default (identity xform vectors; no `extentsHint` for one authored
  `vector3d[]`).

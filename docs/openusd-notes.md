# openusd notes

The openusd APIs Moho relies on, read from the source at the pinned rev
(`68a867a`, `crates/openusd/src/usd/...`). openusd's API is not stable: re-read
these before every bump, and add any API Moho starts relying on.

## Change tracking

- `Stage::add_sink<S: StageSink + 'static>(&self, sink: S) -> StageSinkId` and
  `Stage::remove_sink(&self, id: StageSinkId)` (`stage.rs`).
- `StageSink::after_commit(&self, stage: &Stage, change: &CommittedChange<'_>)`
  (`sink.rs`): the composed change after recomposition. It is the equivalent
  of C++ `UsdNotice::ObjectsChanged`. A plain `Fn(&Stage, &CommittedChange)`
  closure is a `StageSink`. Sinks must not add or remove sinks from inside a
  callback.
- `CommittedChange { resynced: &[sdf::Path], changed_info_only: &[sdf::Path],
  .. }`: prim and property paths in stage namespace.
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
  `StageSink::load_rules_changed(&self, stage, resynced: &[sdf::Path])`, so an
  index can tell them from edits.
- **But the reported path is coarse.** Loading or unloading
  `/world/element_0` reports `resynced = ["/world"]`: the dropped indices
  include every ancestor whose effective rule is not `All` (under `LoadNone`,
  the chunk's parent), and the list is reduced to minimal ancestors. Read
  literally, as a subtree resync, it covers every sibling chunk. An index that
  loads a chunk itself knows what it loaded and must not dirty the siblings.
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
  `Attribute::get_at::<sdf::Value>(time: impl Into<Option<TimeCode>>)` (the
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
- `sdf::Value` has no single-path variant; Moho holds one path as a
  one-element `Value::PathVec`.

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
  collection.
- Material networks: `shade::Material::get(&Stage, path)`; `Connectable::outputs()`
  lists declared outputs too (Moho keeps those with authored connections or
  values); `Shader::get`, `Shader::id() -> Result<Option<String>>` (`info:id`
  in id mode), `Connectable::authored_inputs()`; `Input`/`Output::
  value_producing_attributes(ProducerFilter::{Any, ShaderOutputsOnly}) ->
  Vec<ShadingAttribute>` follows node-graph outputs and interface inputs (C++
  `UsdShadeUtils::GetValueProducingAttributes`); `ShadingAttribute::{Input,
  Output}` and `attribute()`.
- Asset values: `sdf::AssetPath` keeps `authored_path` and, from value
  resolution, `resolved_path()`, which openusd sets only when the anchored
  path names an existing file (so never for a `<UDIM>` set). Moho anchors an
  unresolved relative path on the strongest site of
  `Attribute::property_stack()` (`SpecSite.layer` is the layer identifier),
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
  (`/__Prototype_N`, numbered per composition), `Prim::prim_stack() ->
  Result<Vec<SpecSite>>` (`layer` identifier, `path`, `offset`),
  `Prim::prim_index().variant_selections()`, `Stage::layer_stack() ->
  Vec<String>` (the root layer stack's identifiers).
- The instancing key (`pcp::instancing::InstanceKey`: instanceable arcs,
  variant selections, relative load rules and mask) and
  `PrimIndex::instanceable_nodes` are crate-private, and the key names layer
  stacks by a stage-local `LayerStackId`. The public graph is enough to
  rebuild it: `PrimIndexRef::graph() -> Result<pcp::PrimIndex>`,
  `PrimIndex::root()`, `node(id)`, `children(id)`, `all_nodes()` (strength
  order, inert nodes skipped); `pcp::Node`: `arc() -> ArcType`, `parent()`,
  `path()`, `namespace_depth()` (the parent site's prim-element count minus
  it is `depth_below_introduction`), `is_inert()`, `is_culled()`,
  `has_specs()`, `map_to_root().time_offset()`; `Stage::node_layer_stack(node)`
  and `Stage::layer_identifier(LayerId)`.
- `prim_stack()` of a `/__Prototype_N` prim is empty.
- Parity references (S1 chain tests): `geom::XformCache::new(time)` and
  `local_to_world_transform(&Prim)`, `ImageableExt::compute_visibility(time)
  -> Visibility`, `shade::MaterialBindingAPI::bound_material(purpose)`.
- Point instancers: `geom::PointInstancer::get`, `mask_at(time, Some(&ids))`
  (inactive and invisible ids folded together; the inactive-id helper is
  private, so Moho separates them against `invisibleIds`), and
  `orientationsf` preferred over `orientations` when non-empty (as
  `uses_orientationsf`).

## Scene index gaps (`scene-index-gaps`)

- `Stage::has_authored_stage_metadata(field) -> Result<bool>`: whether the
  session or root layer authors a pseudo-root metadata field (C++
  `UsdStage::HasAuthoredMetadata`); a block counts, a fallback does not.
  Behind `__usdStageMetadata/<field>/authored`.
- `Stage::stage_metadata(field) -> Result<Option<sdf::Value>>`: composed
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

## Gaps

- Splines (`Ts`): a `.spline` block only round-trips as a dictionary. There
  are no typed knots, evaluation, authoring or usdc encoding.
- `load_rules_changed` reports the nearest ancestor whose load rule changes
  resolution (`/world`), not the loaded or unloaded path, as its resync. A
  notice naming the paths whose payload inclusion changed would let an
  observer that did not initiate the load avoid resyncing sibling subtrees
  (mxpv/openusd#174).
- No layer eviction: unloading a payload leaves its layers resident
  (`Stage::find_loadable`'s documentation says so). In the S0 spike, RSS on
  one `LoadNone` stage only grew across Moana's 22 load/unload rounds
  (0.73 to 4.81 GiB), and the final unload freed nothing measurable
  (mxpv/openusd#173).
- No counterpart of C++ `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap`
  (a rule-based collection as one path expression, which UsdImaging's
  collection adapter uses). Moho ports it in `stage/collection.rs`, checked
  against `MembershipQuery::is_included`.

- No public, stage-independent instancing key: Moho rebuilds openusd's key
  from the public prim-index graph, naming layer stacks by their layers'
  identifiers (`stage/instancing.rs`).
- `prim_stack()` of a prototype prim is empty (C++ `GetPrimStack` returns the
  prototype's specs).
- `CollectionEvaluator::build` is `pub(super)`: an expression can only be
  compiled against the stage of the collection that owns it
  (`MembershipQuery`). Light linking after a merge, whose lights and
  receivers sit on different input stages, therefore judges expressions
  without predicates (`PathExpressionEval` with an empty library).
- `CommittedChange::changed_info_only` names the paths, not the fields that
  changed on them (C++ `GetChangedFields`). A prim's info-only change is
  taken as a possible `kind` change (`stage/mod.rs`).
- No `SdfPath::MakeRelativePath`: the prim origin's prototype-relative scene
  path (`stage/prim_origin.rs`) is built from the path's text, parsed back
  with `sdf::Path::new`, which accepts relative paths and `.`.

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

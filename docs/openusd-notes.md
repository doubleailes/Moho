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
  Result<Vec<f64>>`. The last also returns the samples bracketing the
  interval's start, which is what motion-blur sampling needs.
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
- **Caution (source comment):** under `InitialLoadSet::LoadNone`, a load or
  unload pays two full-store recomposes per call. This is what the S0 spike
  measures.

## Gaps

- Splines (`Ts`): a `.spline` block only round-trips as a dictionary. There
  are no typed knots, evaluation, authoring or usdc encoding.

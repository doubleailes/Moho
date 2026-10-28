## MODIFIED Requirements

### Requirement: The index owns the time
The index SHALL hold a current time, as `UsdImagingStageSceneIndex` holds
one. A new index's time SHALL be time code 0, the value of a
default-constructed `UsdTimeCode`; it SHALL NOT be the stage's default
time. Replacing a stage the index already holds SHALL reset the time to
`UsdTimeCode::EarliestTime()`, as `_StageGlobals::Clear` does. Sampled
values SHALL be read relative to the current time. Setting a different
time SHALL dirty exactly the data that may vary over time, on the prims
that hold it. Setting the time it already has SHALL notify nothing.

#### Scenario: One animated mesh
- **WHEN** the time changes on a stage where only one mesh's points are sampled
- **THEN** that mesh alone is dirtied for `primvars/points`, and its points read at offset 0 equal the stage's at the new time

#### Scenario: The same time
- **WHEN** the time is set to its current value
- **THEN** observers receive nothing

#### Scenario: A new index reads at time 0
- **WHEN** a point instancer authors `protoIndices` only as time samples, at frames 0 and 10 with different values, and an index whose time was never set reads it
- **THEN** its `instancerTopology/instanceIndices` are those at frame 0, and setting the time to 0 afterwards notifies nothing

#### Scenario: Replacing the stage
- **WHEN** an index holding a stage is given another stage
- **THEN** its time is `UsdTimeCode::EarliestTime()`

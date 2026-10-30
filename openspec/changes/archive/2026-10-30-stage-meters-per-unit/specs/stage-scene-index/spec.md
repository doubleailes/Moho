## MODIFIED Requirements

### Requirement: Scene globals
The absolute root SHALL carry exactly the data `UsdImagingDataSourceStage`
gives, except `system` (openusd has no resolver context to give):
- `sceneGlobals` with `startTimeCode`, `endTimeCode` and `timeCodesPerSecond`, and `activeRenderSettingsPrim` only when the stage authors a non-empty `renderSettingsPrimPath`;
- `__usdUpAxis/upAxis`, the stage's up axis (its fallback when unauthored).

It SHALL also carry one Moho extension, which UsdImaging does not give:
`__usdMetersPerUnit/metersPerUnit`, a double, the `metersPerUnit` the
stage's session or root layer authors, present only when one authors it.
No prim other than the absolute root SHALL carry it.

It SHALL carry no other scene globals (no meters per unit or up axis under
`sceneGlobals`) and no stage metadata container. A stage metadata edit SHALL
notify nothing at the root, `__usdMetersPerUnit` included, as UsdImaging
notifies nothing for the root's info-only changes.

#### Scenario: Time range
- **WHEN** a stage authors start 1001 and end 1050
- **THEN** the root's scene globals give start 1001 and end 1050

#### Scenario: Up axis
- **WHEN** a stage authors `upAxis = "Z"`
- **THEN** the root's `__usdUpAxis/upAxis` is `Z`, and its `sceneGlobals` has no `upAxis` and no `metersPerUnit`

#### Scenario: Meters per unit authored
- **WHEN** a stage's root layer authors `metersPerUnit = 1`
- **THEN** the root's `__usdMetersPerUnit/metersPerUnit` is `1.0`, its `sceneGlobals` has no `metersPerUnit`, and no other prim has `__usdMetersPerUnit`

#### Scenario: Meters per unit unauthored
- **WHEN** no layer of the stage authors `metersPerUnit`
- **THEN** the root has no `__usdMetersPerUnit`, rather than the schema's fallback

#### Scenario: Meters per unit edited
- **WHEN** a host changes the root layer's `metersPerUnit` and applies the pending updates
- **THEN** no notice is sent, and the root's `__usdMetersPerUnit/metersPerUnit` read afterwards gives the new value

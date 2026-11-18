## ADDED Requirements

### Requirement: Display style fallbacks
`LegacyDisplayStyleOverrideSceneIndex` SHALL be
`HdsiLegacyDisplayStyleOverrideSceneIndex`. Every prim with data SHALL have
`displayStyle/refineLevel` and `displayStyle/cullStyle` beneath its own
data, answering the fallback set (`set_refine_level_fallback`,
`set_cull_style_fallback`) or nothing when none is; a prim's own opinion
SHALL win. Setting a fallback to a new value SHALL dirty every prim of the
input, the absolute root first, at `displayStyle/refineLevel` for the
refine level and at `displayStyle` for the cull style; setting the value
held SHALL notify nothing. The input's notices SHALL pass through.
`refine_level_for_complexity` SHALL be `UsdImagingGLEngine`'s
`_GetRefineLevel`.

#### Scenario: A refine level fallback
- **WHEN** a host sets the refine level fallback to 2 over the standard chain
- **THEN** every mesh reads `displayStyle/refineLevel` 2, and every prim is dirtied at `displayStyle/refineLevel`

#### Scenario: A prim's own opinion
- **WHEN** a prim has its own `displayStyle/refineLevel` 1 and the fallback is 2
- **THEN** the prim reads 1

#### Scenario: Complexity
- **WHEN** a host asks the refine level for complexity 1.0, 1.1, 1.2, 1.3 and 2.0
- **THEN** it gets 0, 1, 2, 3 and 8

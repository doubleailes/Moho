## ADDED Requirements

### Requirement: Scene globals a host sets
A scene globals index, as `HdsiSceneGlobalsSceneIndex`, SHALL pass its
input's prims, children and notices through, renames as removals and
additions, and SHALL give the absolute root a `sceneGlobals` container
overlaid over the input root's data, stronger than it. That container
SHALL name `activeRenderPassPrim`, `activeRenderSettingsPrim`,
`primaryCameraPrim`, `currentFrame`, `timeCodesPerSecond` and
`sceneStateId`, whether set or not. A field the host has set SHALL read the
value set, read live (a container obtained before a later set reads the new
value), a prim path as a one-element path array (an empty path as an empty
array); a field never set SHALL read what the input's `sceneGlobals` gives
there, or nothing. The input root's other data and its other `sceneGlobals`
fields (`startTimeCode`, `endTimeCode`) SHALL be kept. When the input has
no root data, the root's data SHALL be the `sceneGlobals` container alone.
It SHALL not be part of the standard chain.

#### Scenario: Unset fields
- **WHEN** a scene globals index is built over a stage chain whose stage authors 24 time codes per second, and nothing is set
- **THEN** its root's `sceneGlobals/currentFrame` reads nothing, `sceneGlobals/timeCodesPerSecond` reads 24 from the stage, and `sceneGlobals` still names `currentFrame`

#### Scenario: The input's root data is kept
- **WHEN** the host sets the current frame to 1001 over a stage whose root has `__usdUpAxis` and `sceneGlobals/startTimeCode`
- **THEN** the root's `sceneGlobals/currentFrame` reads 1001, and its `__usdUpAxis/upAxis` and `sceneGlobals/startTimeCode` read the stage's

#### Scenario: A set field hides the input's
- **WHEN** the stage's root gives `sceneGlobals/activeRenderSettingsPrim` `/Render/a` and the host sets the active render settings prim to `/Render/b`
- **THEN** the root's `sceneGlobals/activeRenderSettingsPrim` reads `/Render/b`

### Requirement: Scene globals setters dirty their field
Each setter of a scene globals index SHALL, when its value differs from the
one held, store it and notify the absolute root dirtied at exactly that
field's locator: `sceneGlobals/activeRenderPassPrim`,
`sceneGlobals/activeRenderSettingsPrim`, `sceneGlobals/primaryCameraPrim`,
`sceneGlobals/currentFrame`, `sceneGlobals/timeCodesPerSecond` or
`sceneGlobals/sceneStateId`. Setting a held value again SHALL notify
nothing. The first set of a field SHALL notify, even to the value the input
gives there. The current frame SHALL be compared as a time code (NaN equals
NaN); the time codes per second as a number (setting NaN again notifies
again). No setter SHALL dirty any other locator or prim.

#### Scenario: Current frame round trip
- **WHEN** the host sets the current frame to 1001, then 1001 again, then 1002
- **THEN** observers receive `/` dirtied at `sceneGlobals/currentFrame` twice (for 1001 and 1002), and the root's `sceneGlobals/currentFrame` reads 1002

#### Scenario: Each setter its own locator
- **WHEN** the host sets the primary camera prim to `/cam`
- **THEN** observers receive exactly `/` dirtied at `sceneGlobals/primaryCameraPrim`

### Requirement: Scene globals as a renderer filter
A consumer SHALL be able to add the scene globals index as a renderer filter
at phase 0, `AtStart`, where usdview's engine registers it for every
renderer, and SHALL be given the built index to drive it. The stage index's
time and the scene globals' current frame SHALL stay independent: setting
one SHALL not set the other.

#### Scenario: Before light linking
- **WHEN** a consumer passes light linking (phase 4) and the scene globals filter, in that list order
- **THEN** the terminal index is light linking over the scene globals index over the input, and the consumer holds the scene globals index

#### Scenario: Time is not coupled
- **WHEN** the host sets the stage index's time to 1001 without setting the current frame
- **THEN** the root's `sceneGlobals/currentFrame` still reads nothing

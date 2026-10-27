## ADDED Requirements

### Requirement: Prim info
Every prim of a stage index SHALL carry `__usdPrimInfo` as
`UsdImagingUsdPrimInfoSchema` defines it: the prim's `specifier` (`def`,
`over` or `class`), its USD `typeName` as authored (an empty token when
untyped), `isLoaded`, its composed `apiSchemas` (the typed schema's
built-in ones included), and its `kind` when authored, beside the native
instancing fields an instance or prototype root carries.

#### Scenario: A versioned light type and an applied schema
- **WHEN** a prim is defined as `DomeLight_1` and applies `ShapingAPI`
- **THEN** its `__usdPrimInfo/typeName` is `DomeLight_1` and its `__usdPrimInfo/apiSchemas` include `ShapingAPI`

#### Scenario: Kind
- **WHEN** a prim authors `kind = "group"`
- **THEN** its `__usdPrimInfo/kind` is `group`, and a prim that authors no kind has none

### Requirement: Time variance in the generic attribute data
Each attribute's generic entry SHALL give `mightBeTimeVarying`: whether its
value might vary over time, from its opinions (USD's
`ValueMightBeTimeVarying`), whatever the index's time.

#### Scenario: At the default time
- **WHEN** an index whose time was never set exposes a transform op authored with time samples, and another with a default value only
- **THEN** the first's `mightBeTimeVarying` is true and the second's false

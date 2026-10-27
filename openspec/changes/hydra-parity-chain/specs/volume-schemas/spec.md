## MODIFIED Requirements

### Requirement: Renderer-defined volumes
A prim that carries no UsdVol schema SHALL still expose its attributes
through the generic attribute data, so volumes defined in a renderer's own
namespace stay readable.

#### Scenario: A procedural volume
- **WHEN** a prim authors `myRenderer:volume:type = "noise"` and its noise parameters
- **THEN** a consumer reads them through the prim's generic attribute data

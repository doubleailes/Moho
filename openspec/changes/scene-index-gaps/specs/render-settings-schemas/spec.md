## MODIFIED Requirements

### Requirement: Render passes
A `RenderPass` prim SHALL have type `renderPass` and SHALL expose
`renderPass` with `passType` and `renderSource`, and SHALL expose every
collection it applies as `collections`, its schema's built-in ones
included: `renderVisibility`, and `prune`, `matte` and `cameraVisibility`
when applied.

#### Scenario: A pass and its source
- **WHEN** a render pass targets `/Render/settings` as its render source
- **THEN** its `renderPass/renderSource` is `/Render/settings`

#### Scenario: A pass's collections
- **WHEN** a render pass applies `prune`, `matte` and `cameraVisibility` collections
- **THEN** its `collections` hold `renderVisibility`, `prune`, `matte` and `cameraVisibility`, each with its complete membership expression

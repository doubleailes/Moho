# renderers Specification

## Purpose

The consumer side of a terminal scene index, in Hydra 2.0's shape: a
renderer plugin that checks support, contributes its renderer filters and
creates a renderer over the terminal index, and a renderer the host holds
and updates, as `HdRendererPlugin` and `HdRenderer`.

## Requirements

### Requirement: Renderer create args
Renderer create args SHALL be a container, as `HdRendererCreateArgsSchema`,
read through a schema that gives `gpuEnabled` as a boolean when it is
present and nothing when it is absent. Create args with no data SHALL be
valid. The schema's `drivers` SHALL NOT be offered.

#### Scenario: GPU enabled
- **WHEN** a host builds create args with `gpuEnabled` true
- **THEN** a plugin reading them through the schema gets `gpuEnabled` true

#### Scenario: Empty create args
- **WHEN** a host passes create args with no data
- **THEN** the schema gives no `gpuEnabled`, and support checks and creation still receive them

### Requirement: A renderer plugin's support check
A renderer plugin SHALL answer whether it is supported for given create
args, as `HdRendererPlugin::IsSupported`, and SHALL give a reason when it
is not.

#### Scenario: A GPU-only renderer without a GPU
- **WHEN** a plugin that needs a GPU is asked about create args with `gpuEnabled` false
- **THEN** it answers not supported, with a reason naming the missing GPU

### Requirement: Creating a renderer over a chain
Given an input scene index, the host's renderer filters, a plugin and create
args, creating a renderer SHALL check support first. When the plugin is
supported, it SHALL apply the host's and the plugin's filters to the input
in one renderer-filter order, ask the plugin for a renderer over the
result, and return that terminal index with the renderer.

#### Scenario: Host and plugin filters in one order
- **WHEN** the host passes scene globals at phase 0 `AtStart`, and the plugin gives dependency forwarding at phase 1000 `AtEnd` and light linking at phase 4 `AtStart`
- **THEN** the terminal index is dependency forwarding over light linking over scene globals over the input, and the plugin created its renderer over that terminal index

#### Scenario: Equal phase and order
- **WHEN** a host filter and a plugin filter share a phase and an insertion order
- **THEN** the host's filter is applied first, nearer the input

#### Scenario: An unsupported plugin
- **WHEN** the plugin is not supported for the create args
- **THEN** creation fails with the plugin's reason, no filter is built and no renderer is created

#### Scenario: A plugin that declines
- **WHEN** the plugin is supported but gives no renderer
- **THEN** creation fails without a renderer

### Requirement: A renderer's update
A renderer SHALL expose an update, as Hydra 2.0's `HdRenderDelegate::Update`.
The host SHALL call it before rendering. In it the renderer processes the
notices its terminal index sent since the last update. A renderer SHALL
stay on the thread that owns its scene indices.

#### Scenario: An edit reaches the renderer at its update
- **WHEN** a host edits the stage, applies the stage index's pending updates, then calls the renderer's update
- **THEN** the renderer has processed the edit's dirtied entries by the time update returns

### Requirement: The prim-managing renderer
A ready-made renderer SHALL observe its terminal index as hdPrman does. It
SHALL batch notices by prim type under a consumer-given priority, and
manage prims through a consumer-given factory. It SHALL hold every notice
until its update, which flushes the batching. Before its first update its
factory SHALL be asked for nothing but the absolute root, untyped, as
hdPrman's observer walks the still empty batching index when it is made.

#### Scenario: Only the untyped root before the first update
- **WHEN** a prim-managing renderer is created over a non-empty terminal index with a factory that accepts every prim
- **THEN** before its first update the factory has been asked for the absolute root alone, with no type, and the renderer manages the root alone; the first update makes the root again and every other prim

#### Scenario: Materials before geometry at the first update
- **WHEN** a prim-managing renderer is created over a terminal index holding a mesh and its material, with `material` ranked before `mesh`
- **THEN** with a factory that declines untyped prims it holds no managed prim before its first update, and at that update its factory is asked for the material before the mesh

#### Scenario: A dirtied entry waits for the update
- **WHEN** a mesh the renderer manages is dirtied at `mesh/doubleSided`
- **THEN** the managed mesh receives nothing until the renderer's next update, and receives `mesh/doubleSided` in it

#### Scenario: Managed prims by path
- **WHEN** the host asks a prim-managing renderer for the managed prim at a path after an update
- **THEN** it gets the prim its factory made for that path, or nothing when the factory declined it

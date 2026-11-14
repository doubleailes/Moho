# Proposal

## Why

`moho-render` was meant to let a host control any renderer of the stack
without knowing its types. In practice a host still needs the renderer's own
API to draw a viewport over the image, pick a prim under a pixel, show a
settings panel, count the renderer against a memory budget, or show an AOV
while it refines. A host adopting `moho-render` today therefore keeps a
renderer-specific adapter beside it, which is the coupling render control was
meant to remove. Everything those features need is either scene data the
render already read or a small, renderer-neutral fact the renderer knows:
this change carries them through render control.

## What Changes

- **A frame says what it shows.** Each frame names the render that made it
  (an id that grows across the renderer's renders) and the view that render
  read: the camera prim, its transform and lens, the product's framing, the
  index's time code and the rendering colour space. A host draws overlays,
  picks and colour-manages from the frame alone, never from an index that
  may have moved on since.
- **Identifier frames name their prims.** A frame of an identifier var
  (`primId`, `instanceId`) carries the table from each identifier to the
  prim the renderer attributes the hit to, so a host picks and maps prims
  from frames, as Hydra's pick task does from its id buffers, with no pick
  API on the renderer.
- **Every var refines.** A progressive renderer publishes every var it
  renders as it refines, not only the beauty: a host can show any AOV while
  the render converges, and the frames of one pass share their sample count.
- **Settings describe themselves to a person.** A setting descriptor
  carries a display name, a help text and a group besides its key, type,
  default and range, enough for a host to build a settings panel without
  renderer-specific tables.
- **The renderer reports the memory it holds.** Host and device bytes are
  standard stats, readable from the control handle on any thread and from
  the output index, so a host keeping several renderers counts them against
  its budgets.
- **The control handle reads the stats.** Stats are readable through the
  handle, not only by observing the output index on the owning thread.
- The test renderer declares an identifier AOV, reports its memory and
  describes its settings, so every new requirement is exercised.

None of these change `moho`: the parity core stays as it is. No existing
behaviour is removed; the new fields are additions to `Frame`,
`SettingDescriptor` and the control handle (**BREAKING** only for code that
builds these structs field by field, which the stack's renderers do).

Out of scope, each for a change of its own: frames that stay on the GPU
(zero-copy device images, which a GPU renderer needs before it can drop its
own frame transport), render pass collections, and several devices.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `render-control`: setting descriptors gain a display name, help and group;
  frame snapshots gain the render that made them and the view it read;
  identifier frames carry their prim table; progressive renderers publish
  every var as they refine; renderers report the memory they hold as stats
  the control handle reads; the test renderer covers all of it.

## Impact

- `crates/moho-render`: `Frame`, `SettingDescriptor`, `Control` /
  `ControlHandle` (stats), `Shared` (render id, view and id tables on
  publish, stats), `targets` (the view a product renders), the output index's
  root stats, the test renderer, tokens.
- Consumers building `Frame` or `SettingDescriptor` by field (the renderers'
  controlled adapters) add the new fields when they bump Moho.
- `docs/architecture.md` ("Render control"), and `docs/openusd-notes.md` if
  reading the camera needs an openusd API not yet recorded.
- `moho` is unchanged.

# Design

## Context

`moho-render` (see `docs/architecture.md`, "Render control") gives a host
`Arc<Frame>` snapshots through `Shared`, which render threads publish to by
epoch; `RenderTargets::resolve` reads products, vars and cameras from the
terminal index when a render starts; the output index's root holds an open
`renderer/stats` container that `Shared::set_stat` fills. A frame today is
pixels and windows only, a `SettingDescriptor` is key, type, default and
range, and stats are reachable only through the output index, on the owning
thread. See proposal.md for why hosts need more, and the spec delta for the
requirements.

The crate depends on `moho` alone and `moho` must not change for it
("Outside the parity core"). `moho::schema` has no camera schema reader, so
the camera is read by locator (`camera/<field>`, `xform/matrix`), with Hydra's
`HdCameraSchema` field names.

## Goals / Non-Goals

**Goals:**
- A host can draw, pick, colour-manage and budget from what render control
  gives, with no renderer type.
- Renderers that already publish frames change by adding fields, not by
  restructuring their render threads.

**Non-Goals:**
- A pick or prim-map API on the renderer: picking is a host operation over
  identifier frames, as in Hydra.
- Describing how the renderer framed the image when that differs from the
  scene (a renderer's fallback camera): the view is the scene's. A renderer
  that renders through something else says so by failing (`NoCamera`), as
  today.
- GPU frames, render pass collections, several devices.

## Decisions

### D1. The render id is the epoch

`Shared::begin` already starts an epoch that only grows, and only the
current epoch publishes. The frame's render id is that epoch, set by
`publish_frame` as it sets the generation, so a renderer cannot get it
wrong. Epochs skip values (`retire` also bumps them); the spec only promises
that a later render's id is greater.

*Alternative:* a separate render counter. Rejected: two counters that must
agree, for nothing a host can observe.

### D2. The view is resolved with the targets, shared by the product's frames

`RenderTargets::resolve` reads, per product, the camera prim's
`camera` container and `xform/matrix`, the product's resolution, pixel
aspect ratio and conform policy, the root's `sceneGlobals/currentFrame` and
the settings' `renderSettings/renderingColorSpace`, into a `RenderView`
held by `ProductTarget` as `Arc<RenderView>`. Every frame of the product's
vars carries a clone of that `Arc`. Values the index does not give are
`None`; nothing is defaulted, so a host can tell an authored value from a
missing one.

The view is read on the owning thread when the render starts, before the
render thread exists, which is what "the view that render read" means: an
edit after the start changes the next render's view, never a frame already
made. `RenderView` is plain data (`sdf::Path`, `f64`s, tokens), `Send +
Sync` (`docs/openusd-notes.md` already records `sdf::Path` and `Token`).

*Alternative:* the host reads the camera from the index when a frame
arrives. Rejected: the index may already show a later edit, which is the
mismatch the requirement removes. *Alternative:* the renderer fills the
view. Rejected: every renderer would re-read the same scene data, and could
read it differently.

`publish_frame` takes the view through the frame the renderer builds;
`Frame` gains `render`, `view` and `ids` fields and becomes
`#[non_exhaustive]` with a constructor
(`Frame::new(format, display_window, data_window, samples, pixels)`) and
builders (`with_view`, `with_ids`), so later additions break no renderer.

### D3. Identifier tables are per render, shared by frames

An identifier var's frame carries `Option<Arc<IdTable>>`, a sorted
`(i32, sdf::Path)` table. A renderer builds it when it knows its
identifiers (most at scene commit, before the render starts) and attaches
the same `Arc` to every frame of the var for that render, so a frame costs
a reference count, not a table. Which AOVs are identifiers is decided by
name (`primId`, `instanceId`, Hydra's), so a host can tell before a frame
arrives. The table maps to the prim the renderer attributes the hit to: for
an instance, the instance or instancer prim, as the renderer names it; the
spec only requires absolute paths of prims the terminal index holds.

*Alternative:* a table in the output index. Rejected: the output index holds
no pixels and no per-render bulk data, and the table must be read with the
frame it describes, on any thread.

### D4. Memory is two standard stats, the stats readable from the handle

`hostBytes` and `deviceBytes` are tokens in `moho_render::tokens`. `Shared`
seeds both at zero when it is made, so they exist before any render, and a
renderer sets them with `set_stat` whenever its holdings change (after a
commit, on drop of a scene). `Control` gains `stats() -> BTreeMap<Token,
Value>`; `Shared` answers it under its lock.

*Alternative:* a dedicated `held_size()` on `ControlledRenderer`. Rejected:
it would be callable only on the owning thread, and stats already carry
renderer-reported numbers to observers.

### D5. Setting descriptors gain display name, help and group

`SettingDescriptor` gains `label`, `help` and `group` strings, written into
the capabilities container as `label`, `help`, `group`. The struct becomes
`#[non_exhaustive]` with `SettingDescriptor::new(key, ty, default)` (the
label defaulting to the key, help and group empty) and builders, as `Frame`.

### D6. Progressive frames of every var

A requirement on renderers, not new API: `publish_frame` already takes any
var. The test renderer already publishes every var each sample; its tests
gain a data var to pin it.

## Risks / Trade-offs

- [The camera read by locator drifts from Moho's camera adapter] → the
  field names are Hydra's and pinned by a test reading the view of a stage
  camera with every lens field authored.
- [A renderer renders through a different camera than the scene's view]
  → out of scope (non-goal); the renderer must fail rather than render
  something the view does not describe.
- [`#[non_exhaustive]` breaks struct literals in consumers] → one bump;
  consumers move to constructors, which this change adds, and are not
  broken again by later fields.
- [Identifier tables grow with the scene] → one `Arc` per render, shared;
  built once by the renderer, never per frame.

## Migration Plan

Consumers bump Moho and replace `Frame { .. }` and `SettingDescriptor { .. }`
literals with the constructors; renderers then set the memory stats and
attach identifier tables. No data or file format changes.

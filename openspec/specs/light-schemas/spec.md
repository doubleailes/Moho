# light-schemas Specification

## Purpose
How a scene index exposes lights, their parameters, shaping and filters,
and light linking resolved from collections into categories.

## Requirements

### Requirement: Light types and parameters
`SphereLight`, `RectLight`, `DiskLight`, `CylinderLight`, `DistantLight`,
`DomeLight` and `DomeLight_1` SHALL have the types `sphereLight`,
`rectLight`, `diskLight`, `cylinderLight`, `distantLight` and `domeLight`.
Each SHALL expose `light`, an open container answering any light parameter
by name:
- the `LightAPI` inputs (`intensity`, `exposure`, `color`, `normalize`, `diffuse`, `specular`, `enableColorTemperature` and `colorTemperature`);
- the type's own inputs (`radius`, `width`, `height`, `length`, `angle`, `texture:file` and `texture:format`);
- `ShapingAPI` inputs (`shaping:focus`, `focusTint`, `cone:angle`, `cone:softness`, `ies:file`, `ies:angleScale` and `ies:normalize`);
- `filters`, the light's filter targets.

A prim with `LightAPI` applied and no light type of its own SHALL have type
`light`.

#### Scenario: A sphere light
- **WHEN** a `SphereLight` authors `inputs:intensity = 10` and `inputs:radius = 0.5`
- **THEN** its type is `sphereLight`, and its `light` gives `intensity` 10 and `radius` 0.5

#### Scenario: A dome's texture and pole
- **WHEN** a `DomeLight_1` authors `inputs:texture:file` and a pole axis
- **THEN** its `light` gives the texture with its resolved path, and its pole axis

### Requirement: Light parameters dirty the light
A change to a light's `inputs:*` or `light:*` SHALL dirty that light's
`light` data.

#### Scenario: Intensity edit
- **WHEN** a light's `inputs:intensity` changes and updates are applied
- **THEN** the light is dirtied for `light`

### Requirement: Collections
A prim that applies `CollectionAPI` instances SHALL expose `collections`,
keyed by instance name. Each SHALL give the complete membership expression,
resolved from its includes, excludes, `includeRoot`, expansion rule and
authored membership expression.

#### Scenario: A light-link collection
- **WHEN** a light's `collection:lightLink:includes` targets `/world/a` and excludes `/world/a/b`
- **THEN** its `collections/lightLink` gives an expression that matches `/world/a/c` and does not match `/world/a/b`

### Requirement: Resolved light linking
A light linking index SHALL resolve each light's `lightLink` and `shadowLink`
collections into category ids:
- a collection that includes everything has the empty id;
- the light's `light` data names its categories;
- every geometry prim's `categories` names the categories it belongs to;
- an instancer's `instanceCategories` gives them per instance.

Membership SHALL be judged at the instance or instancer's path, never at a
prototype's.

#### Scenario: A light linked to one prim
- **WHEN** a light's light-link collection includes only `/world/a`
- **THEN** `/world/a`'s categories include the light's light-link category, and `/world/b`'s do not

#### Scenario: Default linking
- **WHEN** a light authors no light-link collection
- **THEN** its light-link category is the empty id, and every prim is lit by it

### Requirement: Light linking across merged inputs
Light linking SHALL be resolvable after a merge: over a merging index, a
light linking index SHALL judge every light of every input against every
geometry prim of every input. A receiver in one input SHALL get the
categories of lights held by any other input, and the categories SHALL
equal those of one standard chain over the fully loaded stage.

#### Scenario: A light in one chunk, receivers in another
- **WHEN** a streamed scene's light, in chunk `/world/a`, links only `/world/b/lit` in chunk `/world/b`, and both chunks are inputs of a merging index with linking after the merge
- **THEN** `/world/b/lit`'s categories include the light's light-link category, `/world/b/unlit`'s do not, and both equal those of a standard chain over the fully loaded stage

#### Scenario: A chunk leaves
- **WHEN** the chunk holding the light is removed from the merging index
- **THEN** the receivers in the other chunk are notified dirtied for their categories, and no longer carry the light's category

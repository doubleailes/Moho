## ADDED Requirements

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

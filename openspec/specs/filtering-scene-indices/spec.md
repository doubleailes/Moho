# filtering-scene-indices Specification

## Purpose
Scene indices that take other scene indices as input: flattening resolves
inherited data down the hierarchy, and merging combines several inputs over
one namespace, which is how Moho streams a scene by chunks.

## Requirements

### Requirement: Flattened transforms
A flattening index SHALL give each prim a world transform: its local matrix
times its parent's world transform, or its local matrix alone when it resets
the transform stack. A prim with no transform of its own SHALL take its
parent's. Sample times over an interval SHALL be the union of the
contributing local and ancestor sample times.

#### Scenario: Nested transforms
- **WHEN** `/a` translates by (1, 0, 0) and its child `/a/b` by (0, 2, 0)
- **THEN** `/a/b`'s flattened matrix translates by (1, 2, 0)

#### Scenario: Reset
- **WHEN** `/a/b` resets the transform stack
- **THEN** its flattened matrix is its local matrix alone

#### Scenario: A moving parent
- **WHEN** only `/a`'s transform is sampled
- **THEN** `/a/b`'s flattened transform reports `/a`'s sample times over the shutter

### Requirement: Inherited visibility and purpose
Flattened visibility SHALL be the prim's own when authored, else its
parent's, else visible, so that an invisible prim makes its subtree
invisible. Flattened purpose SHALL be the prim's own when authored, else its
parent's when that is inheritable, else the prim's fallback.

#### Scenario: Invisible subtree
- **WHEN** `/a` is invisible and `/a/b` authors nothing
- **THEN** `/a/b`'s flattened visibility is invisible

#### Scenario: Purpose inherited
- **WHEN** `/a` authors purpose `proxy` and `/a/b` authors none
- **THEN** `/a/b`'s flattened purpose is `proxy`

### Requirement: Inherited constant primvars
A flattened prim SHALL also carry its ancestors' constant-interpolation
primvars that it does not define itself.

#### Scenario: A constant primvar from a parent
- **WHEN** `/a` authors a constant `primvars:tint` and its child mesh does not
- **THEN** the mesh's flattened primvars include `tint` with `/a`'s value

### Requirement: Flattening keeps dirty propagation
When a prim's flattened data is dirtied, every descendant whose flattened
data depends on it SHALL be dirtied for the same data.

#### Scenario: A parent moves
- **WHEN** `/a`'s transform changes
- **THEN** observers of the flattening index receive `/a` and every prim under it dirtied for `xform`

### Requirement: Merging inputs
A merging index SHALL combine inputs, each added at a root path, with
earlier inputs stronger. A prim present in several inputs SHALL take the
first non-empty type, and its data SHALL overlay the inputs' containers,
stronger first. Its children SHALL be the union of the inputs' children.

#### Scenario: Two inputs share a prim
- **WHEN** a chunk input and a scene input both hold `/world/elem`, with the chunk input stronger
- **THEN** reading `/world/elem` gives the chunk input's data where both define it, and the scene input's elsewhere

### Requirement: Adding and removing an input
Adding an input SHALL notify its root's ancestors and its whole subtree
added. Removing an input SHALL notify removed the prims no other input
holds, and re-added the prims another input still holds.

#### Scenario: A chunk comes and goes
- **WHEN** a chunk input rooted at `/world/elem` is added and later removed, over a scene input holding `/world/elem` without its payload contents
- **THEN** adding notifies the chunk's prims added; removing notifies the payload contents removed and `/world/elem` re-added from the scene input

### Requirement: Inputs' notices pass through
The merging index SHALL forward its inputs' notices, correcting the type of
an added prim to the merged type, and re-adding a prim one input removed
while another still holds it.

#### Scenario: A weaker input removes a shared prim
- **WHEN** the scene input removes `/world/elem` while a chunk input still holds it
- **THEN** observers receive `/world/elem` removed and then added with the chunk input's type

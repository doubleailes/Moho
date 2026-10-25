## Purpose

The contract every Moho scene index honours, in Hydra 2.0's model: prims as
types plus lazily read data, children, observers of what changed, and
hierarchical locators naming that data.

## ADDED Requirements

### Requirement: Prims and children
A scene index SHALL answer, for any absolute path, the prim at that path as
a type token and a container of data, or an undefined prim when nothing is
there. It SHALL list the paths of a prim's children. The absolute root SHALL
always be a valid query.

#### Scenario: A defined prim
- **WHEN** a scene index holding a mesh at `/world/geo` is asked for `/world/geo`
- **THEN** it returns the type `mesh` and a container of that prim's data

#### Scenario: Nothing at a path
- **WHEN** it is asked for a path no prim occupies
- **THEN** it returns an undefined prim, with no type and no data

#### Scenario: Children
- **WHEN** it is asked for the children of `/world`
- **THEN** it returns the paths of `/world`'s child prims, in a stable order

### Requirement: Data sources
A prim's data SHALL be a tree of data sources: containers (named children),
vectors (indexed elements), and sampled values (a value at a time). Values
SHALL be openusd values. Reading a data source SHALL compute only what is
read, and reading the same data source twice with no change in between SHALL
give the same value.

#### Scenario: Lazy reading
- **WHEN** a consumer reads only a mesh's topology
- **THEN** no other data of that mesh is resolved from the stage

#### Scenario: A container's names
- **WHEN** a consumer lists a container's names and reads one of them
- **THEN** every listed name gives a data source, and a name not listed gives none

### Requirement: Sampled values and sample times
A sampled value SHALL be read at an offset from the index's current time.
Asked for the sample times contributing to an interval of offsets, it SHALL
answer either that the value is uniform over the interval, or the times to
sample, including the samples bracketing the interval's edges.

#### Scenario: A uniform value
- **WHEN** a consumer asks a value with no time samples for the times contributing to the shutter interval
- **THEN** it is told the value is uniform, and reading it at offset 0 gives the value

#### Scenario: An animated value over a shutter
- **WHEN** a consumer asks an attribute sampled at times 1, 2 and 3, with the index at time 2, for the times contributing to offsets [-0.25, 0.25]
- **THEN** it receives offsets that include -1 and 1 (the bracketing samples), and reading at each gives the stage's value at that time

### Requirement: Locators and locator sets
Data SHALL be named by locators, sequences of tokens such as
`primvars/points/primvarValue`. A locator SHALL cover every locator it is a
prefix of. A locator set SHALL answer whether it intersects a locator, and
SHALL stay minimal: a locator covered by another in the set is not kept.

#### Scenario: A prefix covers its descendants
- **WHEN** a set holding `primvars` is asked whether it intersects `primvars/points/primvarValue`
- **THEN** it answers yes

#### Scenario: A set stays minimal
- **WHEN** `primvars/points` and then `primvars` are inserted into a set
- **THEN** the set holds `primvars` alone

### Requirement: Observers and notices
A scene index SHALL let observers register, and SHALL notify them in batches
of four kinds:
- added prims, with their types;
- removed prims;
- dirtied prims, each with the locator set of its changed data;
- renamed prims.

A removed path SHALL stand for its whole subtree. A dirtied entry SHALL
affect only the named prim, not its descendants. After a notice, querying
the index SHALL reflect the change.

#### Scenario: A removal covers the subtree
- **WHEN** an index notifies `/world/a` removed
- **THEN** `/world/a` and every prim under it are undefined when queried

#### Scenario: A dirty entry is local
- **WHEN** an index notifies `/world` dirtied for `xform`
- **THEN** no descendant of `/world` is implied dirtied by that entry

#### Scenario: Observers see changes after they register
- **WHEN** an observer registers after prims were added
- **THEN** it receives only the notices sent after it registered

### Requirement: Single-thread confinement
A scene index and its data sources SHALL be used from the thread that owns
the index. Values read from them SHALL be movable to other threads.

#### Scenario: Values cross threads
- **WHEN** a consumer reads a mesh's points and hands them to a worker thread
- **THEN** the worker can use them while the index stays on its own thread

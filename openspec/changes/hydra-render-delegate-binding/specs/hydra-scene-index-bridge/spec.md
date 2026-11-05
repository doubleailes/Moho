## Purpose

Reads a scene index of OpenUSD's C++ Hydra 2.0 through Moho's scene index
contract, so a consumer written against Moho reads C++ Hydra's scene
unchanged.

## ADDED Requirements

### Requirement: A C++ scene index as a Moho scene index
The bridge SHALL wrap a C++ `HdSceneIndexBase` as a Moho scene index that
answers the same prims (type and data), the same children in the same
order, and an undefined prim where the C++ index has none. The absolute
root SHALL always be a valid query.

#### Scenario: A mesh prim
- **WHEN** the wrapped C++ index holds a mesh at `/World/geo`
- **THEN** the Moho scene index answers `/World/geo` with type `mesh` and a container of its data

#### Scenario: Children in the C++ order
- **WHEN** the C++ index lists `/World`'s children as `/World/c`, `/World/a`
- **THEN** the Moho scene index lists the same paths in the same order

#### Scenario: Nothing at a path
- **WHEN** the C++ index answers an empty prim at a path
- **THEN** the Moho scene index answers an undefined prim there

### Requirement: Data sources map one to one
Each C++ data source SHALL be read as the Moho data source of the same
kind. A container SHALL become a container with the same names, a vector
a vector of the same length, a sampled data source a sampled value, and a
block (`HdBlockDataSource`) a block. A null data source SHALL be no data.
Data SHALL be read from C++ only when read through Moho.

#### Scenario: A container's names
- **WHEN** a consumer lists the names of a mesh's `primvars` container
- **THEN** it receives the C++ container's names, and each gives the data source the C++ container gives

#### Scenario: Lazy reading
- **WHEN** a consumer reads only a mesh's `mesh/topology`
- **THEN** no other data source of that prim is requested from C++

### Requirement: Values convert to openusd values
A sampled value SHALL convert each `VtValue` held by a type that
`vt::Value` represents to that `vt::Value`, element for element. This
covers scalars, half, vector, quaternion and matrix types, strings,
tokens, asset paths, path expressions, time codes and their arrays. A
value of another type SHALL read as no value.

#### Scenario: Points
- **WHEN** a consumer reads `primvars/points/primvarValue` holding a `VtVec3fArray`
- **THEN** it receives a `Vec3fVec` with the same elements in the same order

#### Scenario: An unrepresented type
- **WHEN** a sampled data source holds a type `vt::Value` cannot represent
- **THEN** reading it gives no value, and the consumer does not crash

### Requirement: Moho's path and locator conventions
A value holding a single `SdfPath` SHALL read as a one-element path array.
An empty `SdfPath` SHALL read as no data, so a container lists a name whose
path is empty but gives nothing for it. An `SdfPathVector` SHALL read as a
path array of the same length. A data source locator SHALL read as a token
array of its elements.

#### Scenario: A resolved material binding
- **WHEN** a consumer reads `materialBindings/""/path` holding `/World/mtl/red`
- **THEN** it receives a path array holding exactly `/World/mtl/red`

#### Scenario: A dependency's locator
- **WHEN** a consumer reads a `__dependencies` entry's `dependedOnDataSourceLocator` holding `primvars/points`
- **THEN** it receives the token array `primvars`, `points`

### Requirement: Sample times over a shutter
A sampled value SHALL read at a shutter offset as the C++ data source does
at that offset. Asked for the sample times contributing to an interval, it
SHALL answer uniform when the C++ data source does, and otherwise the C++
data source's times.

#### Scenario: A uniform value
- **WHEN** the C++ data source reports no contributing sample times over `[-0.25, 0.25]`
- **THEN** the Moho sampled value answers that it is uniform over that interval

#### Scenario: Motion samples
- **WHEN** the C++ data source reports sample times `-1, 0, 1` for `[-0.25, 0.25]`
- **THEN** the Moho sampled value answers `-1, 0, 1`, and reading at `-1` gives the C++ value at `-1`

### Requirement: Notices reach Moho observers
A Moho observer registered on the wrapped index SHALL receive the C++
index's added, removed, dirtied and renamed batches. Batches SHALL arrive
in the same order with the same paths, types and dirty locators. The
sender SHALL be the wrapped index, which already reflects the change. An
observer dropped on the Rust side SHALL stop receiving notices.

#### Scenario: A dirtied transform
- **WHEN** the C++ index sends `/World/geo` dirtied at `xform`
- **THEN** the Moho observer receives one dirtied entry for `/World/geo` with the locator set `xform`

#### Scenario: Time changes
- **WHEN** the host application changes the time of an animated stage
- **THEN** the observer receives the dirtied entries the C++ index sends for it, and values read afterwards are at the new time

### Requirement: Values outlive the C++ handles
A value read through the bridge SHALL be an owned openusd value. A worker
thread SHALL be able to use it after the data source that gave it is
dropped and while the C++ index changes. Data sources and the wrapped
index SHALL be used from the thread that owns the index, as any Moho scene
index.

#### Scenario: Points handed to a worker
- **WHEN** a consumer reads a mesh's points, drops the prim's container, and hands the points to a render thread
- **THEN** the render thread reads the same points after the C++ index removes the mesh

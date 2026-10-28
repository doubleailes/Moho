## ADDED Requirements

### Requirement: Extent resolving
An extent resolving index, as `UsdImagingExtentResolvingSceneIndex` with
the purposes `geometry`, `render` and `proxy`, SHALL give a prim that has
no `extent` of its own but has an `extentsHint` the union of its hint's
ranges for those purposes as its `extent` (`min`, `max`). A prim's own
`extent` SHALL be left as it is. A change to `extentsHint` SHALL also dirty
`extent`.

#### Scenario: A model with only a hint
- **WHEN** a model authors `extentsHint` with a geometry range of (0,0,0)–(1,1,1) and a render range of (-1,0,0)–(0,1,1), and no `extent`
- **THEN** its resolved `extent` is (-1,0,0)–(1,1,1)

#### Scenario: An authored extent wins
- **WHEN** a mesh authors both `extent` and `extentsHint`
- **THEN** its resolved `extent` is its authored `extent`

## REMOVED Requirements

### Requirement: Prims outside the loaded inputs
**Reason**: It served streaming by masked chunk stages (`streaming::unresolved`
and side inputs), which Hydra has no counterpart for. Moho no longer
streams; chunking is the consumer's business.
**Migration**: Read a scene from one stage through `chain::standard`. A
consumer that chunks a scene finds and loads the dependencies of each chunk
itself, from the stage.

### Requirement: Material nodes resolved after the merge
**Reason**: `MaterialNodeResolvingSceneIndex` (M10) filled networks from
other inputs after a merge of chunk stages; Hydra has no counterpart, and
a material network on a fully composed stage needs none.
**Migration**: None in Moho. The stage-side M10 data (`__materialNodes`,
`__materialNodeGraph`, off-stage connections) is removed by
`hydra-parity-materials`.

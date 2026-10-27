## REMOVED Requirements

### Requirement: Camera units and stage metadata
**Reason**: Hydra's scene globals carry no meters per unit or up axis
(`HdSceneGlobalsSchema`); camera data is in scene units, as Hydra gives it.
**Migration**: The up axis is at the root's `__usdUpAxis/upAxis`. A consumer
that needs `metersPerUnit` reads it from the stage it opened, through
`moho::openusd`.

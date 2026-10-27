## MODIFIED Requirements

### Requirement: Light types and parameters
`SphereLight`, `RectLight`, `DiskLight`, `CylinderLight`, `DistantLight`,
`DomeLight` and `DomeLight_1` SHALL have the types `sphereLight`,
`rectLight`, `diskLight`, `cylinderLight`, `distantLight` and `domeLight`.
A prim with `LightAPI` applied and no light type of its own SHALL have type
`light`. Each SHALL expose, as UsdImaging's light API adapter does:
- `light`, listing `filters` (the light's forwarded filter targets), `isLight` (true) and `materialSyncMode` (a dome light also `portals`, its forwarded portal targets, and a `DomeLight_1` first `domeOffset`, the rotation aligning it with its `poleAxis`), and answering any other name as `UsdImagingPrimAdapter::LookupLightParamAttribute` does: Hydra's light tokens (`intensity`, `exposure`, `color`, `normalize`, `diffuse`, `specular`, `enableColorTemperature`, `colorTemperature`, `radius`, `width`, `height`, `length`, `angle`, `texture:file`, `texture:format`, the `shaping:*` and `shadow:*` tokens) read their `inputs:` attribute, and any other name reads the attribute of that name;
- `material`, a network for the contexts `""`, `__all` and each context a `<context>:light:shaderId` attribute names: one node, keyed by the light prim's absolute path, with the light's shader identifier for that context as `nodeIdentifier`, its `renderContextNodeIdentifiers`, and its inputs as `parameters`; and the terminal `light` naming that node.

#### Scenario: A sphere light
- **WHEN** a `SphereLight` authors `inputs:intensity = 10` and `inputs:radius = 0.5`
- **THEN** its type is `sphereLight`; its `light` lists `filters`, `isLight` and `materialSyncMode`, and answers `intensity` 10 and `radius` 0.5; its `material/""` network has the node `/<light path>` with parameter `radius` 0.5 feeding the terminal `light`

#### Scenario: A dome's texture and pole
- **WHEN** a `DomeLight_1` authors `inputs:texture:file` and a pole axis
- **THEN** its `light` answers `texture:file` with the resolved path, and `poleAxis` with its pole axis

### Requirement: Light parameters dirty the light
A change to a light's `inputs:*` SHALL dirty that light's `light` and
`material`; a change to its `light:*` attributes SHALL dirty `light`; a
change to a dome light's `portals`, or a `DomeLight_1`'s `poleAxis`, SHALL
dirty `light`.

#### Scenario: Intensity edit
- **WHEN** a light's `inputs:intensity` changes and updates are applied
- **THEN** the light is dirtied for `light` and `material`

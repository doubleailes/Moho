## MODIFIED Requirements

### Requirement: Dependencies across prims
The flattened render settings SHALL declare, as
`UsdImagingRenderSettingsFlatteningSceneIndex` does, in `__dependencies`:
each flattened field on the settings' own `__usdRenderSettings` field it
comes from, `renderSettings/renderProducts` on each product's
`__usdRenderProduct` and each var's `__usdRenderVar`, and `__dependencies`
itself on `__usdRenderSettings/products` (so that a change to the product
list re-reads the dependencies). The flattening index SHALL NOT dirty
`renderSettings` itself: a consumer that adds dependency forwarding gets
the settings dirtied for the affected locators.

#### Scenario: A var changes
- **WHEN** a var's `sourceName` changes and updates are applied, with dependency forwarding after the chain
- **THEN** the settings that list it are dirtied for `renderSettings/renderProducts`

#### Scenario: Without forwarding
- **WHEN** the same edit is read through `chain::standard` alone
- **THEN** the var is dirtied for `__usdRenderVar/sourceName` and the settings are not dirtied

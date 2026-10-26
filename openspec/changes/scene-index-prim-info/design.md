## Context

`__usdPrimInfo` was added in S1 for the native instancing filter alone, so
it appeared only on instances and prototype roots. Hydra's
`UsdImagingDataSourceUsdPrimInfo` is on every prim, with the prim's
specifier, type name, load state, applied schemas and kind.

## Decisions

### D1. `__usdPrimInfo` as Hydra has it (M8)

Every prim carries `__usdPrimInfo` with:
- `specifier`, the token `def`, `over` or `class`;
- `typeName`, the USD type as authored, an empty token when untyped;
- `isLoaded`;
- `apiSchemas`, composed, so the typed schema's built-in ones are included,
  as `UsdPrim::GetAppliedSchemas` returns them;
- `kind`, when authored.

The instancing fields (`niPrototypePath`, `niPrototypeIdentity`,
`isNiPrototype`) stay as they were. Hydra's `variantSelections` and
`piPropagatedPrototypes` are left out until a consumer needs them. A type
or schema change arrives as a resync, so no locator maps to the new fields.

### D2. Time variance in the generic data (M9)

A sampled data source answers `sample_times` relative to the index's time,
and at the default time there is none to bracket. Whether a value might vary
over time is a property of the attribute's opinions instead:
`Attribute::might_be_time_varying`, which the time-varying set already uses.
It goes into the generic entry as `mightBeTimeVarying`, next to `authored`,
the other fact about opinions that Hydra's schemas don't carry. A property
change already dirties the entry.
*Alternative:* report sample times at the default time. Rejected: they are
offsets from a time the index does not have.

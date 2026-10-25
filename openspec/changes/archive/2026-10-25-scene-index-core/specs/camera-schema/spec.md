## Purpose

How a scene index exposes cameras: projection, aperture and lens, clipping,
shutter and exposure, with Hydra 2.0's schema names.

## ADDED Requirements

### Requirement: Camera data
A `Camera` SHALL have type `camera` and SHALL expose `camera` with:
- `projection`;
- `focalLength`, `horizontalAperture`, `verticalAperture`, `horizontalApertureOffset` and `verticalApertureOffset`, in scene units;
- `fStop` and `focusDistance`;
- `clippingRange` and `clippingPlanes`;
- `shutterOpen` and `shutterClose`, from `shutter:open` and `shutter:close`;
- `exposure`, `exposureTime`, `exposureIso`, `exposureFStop` and `exposureResponsivity`, from the `exposure:*` attributes;
- `linearExposureScale`, computed from them.

Apertures and focal length SHALL be scaled as Hydra does: tenths of a scene
unit to scene units.

#### Scenario: A 50 mm lens
- **WHEN** a camera authors focalLength 50 and horizontalAperture 36
- **THEN** its `camera/focalLength` is 5 and `camera/horizontalAperture` is 3.6

#### Scenario: Shutter
- **WHEN** a camera authors `shutter:open = -0.25` and `shutter:close = 0.25`
- **THEN** its `camera/shutterOpen` is -0.25 and `camera/shutterClose` is 0.25

### Requirement: Camera units and stage metadata
The stage's `metersPerUnit` and up axis SHALL be readable from the scene
globals, so a consumer can convert lens and clipping units.

#### Scenario: Centimetre stage
- **WHEN** the stage authors `metersPerUnit = 0.01`
- **THEN** the scene globals give 0.01

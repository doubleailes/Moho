//! UsdVol particle fields, as UsdImaging's particle field adapter
//! (`usdVolImaging/particleFieldAdapter.cpp`) and its data source
//! (`dataSourceParticleField.cpp`, `UsdImagingDataSourceParticleFieldPrim`).
//!
//! - `ParticleField` and every type derived from it
//!   (`includeDerivedPrimTypes`, so `ParticleField3DGaussianSplat`) is a
//!   gprim of type `particleField`.
//! - Its `primvars` overlay, under the gprim's, the custom primvars
//!   `positions`, `orientations`, `scales`, `opacities` and
//!   `radiance:sphericalHarmonicsCoefficients` (vertex) and
//!   `radiance:sphericalHarmonicsDegree` (constant). Each of the five pairs
//!   reads its float attribute when that holds elements at the earliest
//!   time, else its `half` twin
//!   (`UsdVolParticleField3DGaussianSplat::UsesFloat*`, openusd's
//!   `attribute_in_use`), under the float attribute's name. As every custom
//!   primvar, one gives data only when its attribute's value is authored,
//!   names `interpolation` and `role` (empty without one), `colorSpace` for
//!   a colour, and never `elementSize` (`UsdImagingDataSourceCustomPrimvars`).
//! - An edit to either attribute of a pair dirties that one primvar, and a
//!   degree edit the degree's.
//!
//! The data source's `XXX` for a `particleField` schema (kernel type,
//! projection and sorting hints) is not in UsdImaging, so not here: those
//! attributes are in the generic attribute data.
//!
//! As UsdImaging, the pair is chosen through the Gaussian splat schema
//! whatever the prim's type: the attributes come from the particle field
//! attribute APIs, not from the splat type.

use openusd::tf::Token;
use openusd::usd;
use openusd_schemas::vol::{ParticleField3DGaussianSplat, SplatData};

use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::particle_field as t;
use crate::tokens::prim_type;
use crate::tokens::primvars::{CONSTANT, PRIMVARS, VERTEX};

/// The USD type the adapter is registered for, with its derived types.
pub const PARTICLE_FIELD: &str = "ParticleField";

/// The pairs stored at two precisions: `(primvar, the pair, float
/// attribute, half attribute)`. The primvar is named as the float
/// attribute.
const PAIRS: &[(&str, SplatData, &str)] = &[
    (t::POSITIONS, SplatData::Positions, "positionsh"),
    (t::ORIENTATIONS, SplatData::Orientations, "orientationsh"),
    (t::SCALES, SplatData::Scales, "scalesh"),
    (t::OPACITIES, SplatData::Opacities, "opacitiesh"),
    (
        t::RADIANCE_SPHERICAL_HARMONICS_COEFFICIENTS,
        SplatData::RadianceCoefficients,
        "radiance:sphericalHarmonicsCoefficientsh",
    ),
];

/// Whether `prim` is a particle field (its type is `ParticleField` or
/// derives from it).
pub(crate) fn is_particle_field(prim: &usd::Prim) -> bool {
    crate::stage::is_a(prim, PARTICLE_FIELD)
}

/// The custom primvars' names, in `_GetCustomPrimvarMappings`' order: the
/// degree first, then the pairs.
pub(crate) fn custom_names() -> impl Iterator<Item = &'static str> {
    std::iter::once(t::RADIANCE_SPHERICAL_HARMONICS_DEGREE).chain(PAIRS.iter().map(|(name, ..)| *name))
}

/// The attribute the custom primvar `name` reads and its interpolation, if
/// `name` is one. A pair asks which of its attributes holds the data only
/// here, for the one primvar asked for: the question reads the float
/// attribute's whole array.
pub(crate) fn custom(prim: &usd::Prim, name: &str) -> Option<(usd::Attribute, &'static str)> {
    if name == t::RADIANCE_SPHERICAL_HARMONICS_DEGREE {
        return Some((prim.attribute(name), CONSTANT));
    }
    let (_, data, half) = PAIRS.iter().find(|(primvar, ..)| *primvar == name)?;
    let splat = ParticleField3DGaussianSplat::from_prim_unchecked(prim.clone());
    // A float attribute that cannot be read holds no elements, so the half
    // one answers, as C++ `_UsesFloatAttr` does when `Get` fails.
    let attribute = splat
        .attribute_in_use(*data)
        .unwrap_or_else(|_| prim.attribute(*half));
    Some((attribute, VERTEX))
}

/// The adapter of `ParticleField` prims and the types derived from them.
pub struct ParticleFieldAdapter;

impl Adapter for ParticleFieldAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::PARTICLE_FIELD)
    }

    /// `UsdImagingDataSourceParticleFieldPrim::Invalidate`: either
    /// attribute of a pair dirties the primvar named for the pair, and the
    /// degree its own. Property additions and removals dirty the same.
    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let mut locators = LocatorSet::new();
        for (primvar, _, half) in PAIRS {
            if name == *primvar || name == *half {
                locators.insert(Locator::new([PRIMVARS, *primvar]));
            }
        }
        if name == t::RADIANCE_SPHERICAL_HARMONICS_DEGREE {
            locators.insert(Locator::new([PRIMVARS, name]));
        }
        locators
    }
}

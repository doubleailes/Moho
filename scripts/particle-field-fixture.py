"""Writes crates/moho/tests/data/particle_field.usdc, the Gaussian splat
fixture of tests/particle_field.rs, with OpenUSD's own `.usdc` writer.

Needs `usd-core` >= 26.03 (the release that adds UsdVolParticleField3DGaussianSplat):

    python -m venv /tmp/usd && /tmp/usd/bin/pip install 'usd-core>=26.03'
    /tmp/usd/bin/python scripts/particle-field-fixture.py

Three prims, each two particles of SH degree 1 (four coefficients a particle):
`/floatOnly` authors the float attributes, `/halfOnly` only the `half` twins,
and `/both` both, with different values (the float ones win). The values are
exact in `half`, so a reader compares them exactly.
"""

import sys
from pathlib import Path

from pxr import Gf, Sdf, Usd, UsdVol, Vt

OUT = Path(__file__).resolve().parent.parent / "crates/moho/tests/data/particle_field.usdc"

POSITIONS = [(0.0, 1.0, 2.0), (-0.5, 0.25, 4.0)]
ORIENTATIONS = [(1.0, 0.0, 0.0, 0.0), (0.5, 0.5, 0.5, 0.5)]
SCALES = [(0.125, 0.25, 0.5), (1.0, 2.0, 0.0625)]
OPACITIES = [0.5, 0.75]
COEFFICIENTS = [(0.25 * i, -0.125 * i, 0.5) for i in range(8)]


def author_float(gs, offset):
    gs.CreatePositionsAttr(Vt.Vec3fArray([Gf.Vec3f(*(c + offset for c in p)) for p in POSITIONS]))
    gs.CreateOrientationsAttr(Vt.QuatfArray([Gf.Quatf(*q) for q in ORIENTATIONS]))
    gs.CreateScalesAttr(Vt.Vec3fArray([Gf.Vec3f(*s) for s in SCALES]))
    gs.CreateOpacitiesAttr(Vt.FloatArray(OPACITIES))
    gs.CreateRadianceSphericalHarmonicsCoefficientsAttr(
        Vt.Vec3fArray([Gf.Vec3f(*c) for c in COEFFICIENTS])
    )


def author_half(gs, offset):
    gs.CreatePositionshAttr(Vt.Vec3hArray([Gf.Vec3h(*(c + offset for c in p)) for p in POSITIONS]))
    gs.CreateOrientationshAttr(Vt.QuathArray([Gf.Quath(*q) for q in ORIENTATIONS]))
    gs.CreateScaleshAttr(Vt.Vec3hArray([Gf.Vec3h(*s) for s in SCALES]))
    gs.CreateOpacitieshAttr(Vt.HalfArray(OPACITIES))
    gs.CreateRadianceSphericalHarmonicsCoefficientshAttr(
        Vt.Vec3hArray([Gf.Vec3h(*c) for c in COEFFICIENTS])
    )


def main():
    OUT.parent.mkdir(parents=True, exist_ok=True)
    layer = Sdf.Layer.CreateNew(str(OUT))
    stage = Usd.Stage.Open(layer)
    for name, floats, halves in [("floatOnly", 0.0, None), ("halfOnly", None, 0.0), ("both", 0.0, 8.0)]:
        gs = UsdVol.ParticleField3DGaussianSplat.Define(stage, f"/{name}")
        gs.CreateRadianceSphericalHarmonicsDegreeAttr(1)
        if floats is not None:
            author_float(gs, floats)
        if halves is not None:
            author_half(gs, halves)
        for attr in [
            gs.GetRadianceSphericalHarmonicsCoefficientsAttr(),
            gs.GetRadianceSphericalHarmonicsCoefficientshAttr(),
        ]:
            if attr.HasAuthoredValue():
                attr.SetMetadata("elementSize", 4)
                attr.SetColorSpace("lin_rec709_scene")
    layer.Save()
    print(f"wrote {OUT}", file=sys.stderr)


if __name__ == "__main__":
    main()

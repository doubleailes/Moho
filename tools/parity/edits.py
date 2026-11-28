"""Writes a scene's edit script for the Hydra parity check (README.md).

    python edits.py SCENE > EDITS

Run with the reference build's Python (run.sh does). Each edit is chosen
from the scene's own data, among the prims a default traversal reaches; an
edit with nothing to apply to is left out. A value is written as usda text
(a token quoted). An attribute the scene does not
author is set twice, so that the second set is a plain value change.
"""

import sys

from pxr import Sdf, Usd, UsdGeom, UsdLux, UsdShade

MAX_POINTS = 10000


def fmt(value):
    """`value` as usda text."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if hasattr(value, "__len__") and not isinstance(value, str):
        return "(" + ", ".join(fmt(v) for v in value) + ")"
    return repr(float(value)) if isinstance(value, float) else str(value)


def array(values):
    return "[" + ", ".join(fmt(v) for v in values) + "]"


class Script:
    def __init__(self, stage):
        self.stage = stage
        self.lines = []

    def set(self, prim, name, type_name, text):
        attribute = prim.GetAttribute(name)
        line = f"set {prim.GetPath()}.{name} {type_name} {text}"
        self.lines.append(line)
        if not (attribute and attribute.HasAuthoredValue()):
            self.lines.append(line)


def main(scene):
    stage = Usd.Stage.Open(scene)
    script = Script(stage)
    prims = [p for p in stage.Traverse() if not p.IsInstanceProxy()]
    gprims = [p for p in prims if p.IsA(UsdGeom.Gprim) and not p.IsInPrototype()]
    xformables = [p for p in prims if p.IsA(UsdGeom.Xformable) and not p.IsInPrototype()]
    shaders = [p for p in prims if p.IsA(UsdShade.Shader)]
    materials = [p for p in prims if p.IsA(UsdShade.Material)]
    lights = [p for p in prims if p.HasAPI(UsdLux.LightAPI)]
    instancers = [p for p in prims if p.IsA(UsdGeom.PointInstancer)]
    instanceable = [p for p in prims if p.IsInstance()]

    if xformables:
        prim = next((p for p in xformables if p.GetAttribute("xformOp:translate")), xformables[0])
        attribute = prim.GetAttribute("xformOp:translate")
        type_name = str(attribute.GetTypeName()) if attribute else "double3"
        script.set(prim, "xformOp:translate", type_name, "(1, 2, 3)")
    if gprims:
        gprim = gprims[0]
        script.set(gprim, "visibility", "token", '"invisible"')
        script.set(gprim, "purpose", "token", '"proxy"')
        script.set(gprim, "primvars:displayColor", "color3f[]", "[(1, 0, 0)]")
    for shader in shaders:
        inputs = [i for i in UsdShade.Shader(shader).GetInputs()
                  if str(i.GetTypeName()) in ("float", "color3f")]
        if inputs:
            input = inputs[0]
            type_name = str(input.GetTypeName())
            text = "0.5" if type_name == "float" else "(0.5, 0.25, 0.125)"
            script.set(shader, input.GetAttr().GetName(), type_name, text)
            break
    for prim in prims:
        binding = prim.GetRelationship("material:binding")
        targets = binding.GetTargets() if binding else []
        others = [m.GetPath() for m in materials if targets and m.GetPath() != targets[0]]
        if targets and others:
            script.lines.append(f"targets {prim.GetPath()}.material:binding {others[0]}")
            break
    if lights:
        light = lights[0]
        script.set(light, "inputs:intensity", "float", "2.5")
        if gprims:
            script.lines.append(f"targets {light.GetPath()}.collection:lightLink:includes {gprims[0].GetPath()}")
            script.set(light, "collection:lightLink:includeRoot", "bool", "false")
    for instancer in instancers:
        positions = UsdGeom.PointInstancer(instancer).GetPositionsAttr().Get()
        if positions and len(positions) <= MAX_POINTS:
            moved = [tuple(p) for p in positions]
            moved[0] = (moved[0][0] + 1.0, moved[0][1], moved[0][2])
            script.set(instancer, "positions", "point3f[]", array(moved))
            break
    roots = [p for p in stage.GetPseudoRoot().GetChildren() if p.IsDefined()]
    if roots:
        script.lines.append(f"define {roots[0].GetPath()}/parity_new Mesh")
    if instanceable:
        script.lines.append(f"instanceable {instanceable[0].GetPath()} false")
    # A prim the root layer defines, which `RemovePrim` can remove.
    root_layer = stage.GetRootLayer()
    removable = [p for p in gprims if root_layer.GetPrimAtPath(p.GetPath())]
    if removable:
        script.lines.append(f"remove {removable[-1].GetPath()}")
    script.lines.append("time 2")
    print("\n".join(script.lines))


if __name__ == "__main__":
    main(sys.argv[1])

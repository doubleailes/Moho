"""Compares the dumps run.sh wrote, Hydra against Moho (README.md).

    python3 compare.py OUT_DIR [SCENE...]

Prints one Markdown row per scene: prims on each side, the static
differences no class below accounts for (before and after the edits), and
the edits whose notices are identical, out of those both sides applied,
with nothing read and with everything read; then the totals, with the
edits a side rejected and the differences each class accounts for. A
scene whose edit script or a dumper failed is reported, not compared. Writes each scene's differences to OUT_DIR/<scene>/diff.txt,
each static one with its class.

What the comparison normalizes:
- container name order and repeated names (data is keyed by locator);
- numbers, to 1e-5 relative;
- USD prototypes, named by their smallest instance;
- a propagated prototype group (`/UsdNiPropagatedPrototypes/<name>`),
  named by the prototypes it holds: its name ends in hashes of the values
  its prototypes share, which Moho computes its own way;
- a light linking category id (`group_N`), named by the light links that
  use it: each side numbers them in the order it meets the collections;
- Moho's additions to Hydra (`__usdAttributes`, `__usdRelationships`,
  `__usdMetersPerUnit`), dropped from data and from dirtied locators;
- `skelBinding`, which UsdImaging names on every prim and Moho does not
  (Moho has no skeleton data), and the absolute root's
  `system/assetResolution` (openusd has no resolver context);
- a path vector (`[ a b ]`, as openusd and C++ write `SdfPathVector`) and
  a path array (`[a, b]`, Hydra's `VtArray<SdfPath>`) holding the same
  paths;
- a name whose data source is null, and a name not listed (both are no
  data in Hydra);
- a value over 2000 characters, compared on its first 25 elements.

The classes of a static difference that remains (`docs/hydra-parity-check.md`
has the reasons):
- `mtlx`: material data in a scene whose `.mtlx` documents the reference
  build cannot open (it has no `usdMtlx`);
- `extent`: an extent or extent hint UsdImaging does not read because it is
  authored with another type than declared;
- `numbering`: a `ForInstancer<hash>` name, which hashes a prototype path
  that USD and openusd number differently when prototypes compose together;
- `rotation`: transform vectors whose rotation reads back as another
  equivalent triple;
- `placeholder`: the default visibility and transform Hydra's flattening
  serves for a native instancer it keeps a placeholder of;
- `expression`: the text of a collection's path expression, which lists an
  explicit collection's paths in hash order and quotes a predicate argument
  in C++, and does neither in openusd;
- `stale`: after the edits, an instance's index that Hydra leaves as it was
  when an earlier instance of its prototype stops being one, where its
  instancer's topology has the new index;
- `unexplained`: anything else.
"""

import collections
import math
import pathlib
import re
import sys

MOHO_ONLY = ("__usdAttributes", "__usdRelationships", "__usdMetersPerUnit")
PLACEHOLDER = ("visibility/visibility", "xform/matrix", "xform/resetXformStack")
LINKS = ("light/lightLink", "light/shadowLink", "light/lightFilterLink", "light/filterLink")
PROTOTYPE = re.compile(r"/__Prototype_\d+")
GROUP = re.compile(r"/UsdNiPropagatedPrototypes/([^/\s\],]+)")
HELD = re.compile(r"/UsdNiPropagatedPrototypes/([^/\s\],]+)/__Prototype\[([^\]]*)\]")
CATEGORY = re.compile(r"\bgroup_\d+\b")
INTEGER = re.compile(r"-?\d+")
NUMBER = re.compile(r"-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?|-?inf|nan")
LONG = 2000
ELEMENTS = 25


def dropped(path, locator):
    head = locator.split("/", 1)[0]
    if head in MOHO_ONLY or head == "skelBinding":
        return True
    return path == "/" and locator.startswith("system/assetResolution")


class Section:
    """One section of a dump: prototype names, prims and data."""

    def __init__(self):
        self.protos = {}
        self.prims = {}
        self.data = {}


class Dump:
    """One dump: its sections and each edit's notices."""

    def __init__(self, path):
        self.sections = {}
        self.edits = []
        self.errors = []
        self.complete = False
        section = None
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            word, _, rest = line.partition(" ")
            if word == "section":
                section = self.sections.setdefault(rest, Section())
            elif word == "proto":
                prototype, _, instance = rest.partition(" ")
                name = f"/__Prototype[{instance}]"
                if section is not None:
                    section.protos[prototype] = name
                elif self.edits:
                    self.edits[-1]["protos"][prototype] = name
            elif word == "prim":
                path_, _, ty = rest.partition(" ")
                section.prims[path_] = ty
            elif word == "ds":
                path_, _, tail = rest.partition(" ")
                locator, _, value = tail.partition(" ")
                if not dropped(path_, locator):
                    section.data[(path_, locator)] = value
            elif word == "edit":
                section = None
                self.edits.append({"line": rest.partition(" ")[2], "notices": [], "protos": {}})
            elif word == "editerror":
                self.edits[-1]["error"] = rest
            elif word in ("added", "removed", "renamed"):
                self.edits[-1]["notices"].append(line)
            elif word == "dirtied":
                path_, _, locators = rest.partition(" ")
                kept = sorted(set(l for l in locators.split() if not dropped(path_, l)))
                if kept:
                    self.edits[-1]["notices"].append(f"dirtied {path_} {' '.join(kept)}")
            elif word == "error":
                self.errors.append(rest)
            elif word == "end":
                self.complete = True


class Names:
    """The renaming of one side's paths and values."""

    def __init__(self):
        self.protos = {}
        self.groups = {}
        self.categories = {}

    def learn(self, section):
        """Learns a section's prototype, group and category names."""
        self.protos.update(section.protos)
        held = collections.defaultdict(set)
        for path in section.prims:
            for m in HELD.finditer(self.prototypes(path)):
                held[m.group(1)].add(m.group(2))
        for group, prototypes in held.items():
            self.groups[group] = "Group{" + ",".join(sorted(prototypes)) + "}"
        links = collections.defaultdict(set)
        for (path, locator), value in section.data.items():
            if locator in LINKS:
                for category in CATEGORY.findall(value):
                    links[category].add(f"{self.prototypes(path)}:{locator.rsplit('/', 1)[1]}")
        for category, uses in links.items():
            self.categories[category] = "Category{" + ",".join(sorted(uses)) + "}"

    def prototypes(self, text):
        return PROTOTYPE.sub(lambda m: self.protos.get(m.group(0), m.group(0)), text)

    def __call__(self, text):
        text = self.prototypes(text)
        text = GROUP.sub(lambda m: "/UsdNiPropagatedPrototypes/" + self.groups.get(m.group(1), m.group(1)), text)
        return CATEGORY.sub(lambda m: self.categories.get(m.group(0), m.group(0)), text)


def renamed(section, names):
    prims = {names(p): t for p, t in section.prims.items()}
    data = {(names(p), names(l)): names(v) for (p, l), v in section.data.items()}
    return prims, data


def elements(value):
    """The top-level elements of a bracketed list, or None."""
    if not value.startswith("[") or not value.endswith("]"):
        return None
    out, depth, start = [], 0, 1
    for i, c in enumerate(value[1:-1], 1):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == "," and depth == 0:
            out.append(value[start:i].strip())
            start = i + 1
    out.append(value[start:-1].strip())
    return out


def path_list(value):
    """A path vector's text, `[ a b ]`, as a path array's, `[a, b]`."""
    m = re.fullmatch(r"(=? ?)\[ (.*?) ?\]", value)
    if not m:
        return value
    return m.group(1) + "[" + ", ".join(m.group(2).split()) + "]"


def close(a, b):
    a, b = path_list(a), path_list(b)
    if a == b:
        return True
    a, b = a.removeprefix("= "), b.removeprefix("= ")
    if len(a) > LONG or len(b) > LONG:
        ea, eb = elements(a), elements(b)
        if ea is not None and eb is not None:
            return len(ea) == len(eb) and all(close(x, y) for x, y in zip(ea[:ELEMENTS], eb[:ELEMENTS]))
    if NUMBER.sub("#", a) != NUMBER.sub("#", b):
        return False
    return all(same_number(x, y) for x, y in zip(NUMBER.findall(a), NUMBER.findall(b)))


def same_number(x, y):
    """Whether two numbers' texts agree: integers exactly, a non-finite
    value only with itself, any other to 1e-5 relative."""
    if x == y:
        return True
    if INTEGER.fullmatch(x) and INTEGER.fullmatch(y):
        return int(x) == int(y)
    fx, fy = float(x), float(y)
    if not (math.isfinite(fx) and math.isfinite(fy)):
        return False
    return abs(fx - fy) <= 1e-5 * max(1.0, abs(fx), abs(fy))


def classify(path, locator, values, mtlx):
    if mtlx and (locator == "material" or locator.startswith("material/")):
        return "mtlx"
    if locator.startswith(("extentsHint", "extent")):
        return "extent"
    if any("ForInstancer" in text for text in (path, locator, *values)):
        return "numbering"
    if locator == "geomXformVectors/rotation":
        return "rotation"
    if locator.endswith("membershipExpression"):
        return "expression"
    if path.endswith("/UsdNiInstancer") and locator in PLACEHOLDER and not values[1]:
        return "placeholder"
    return "unexplained"


def stale(path, data, moho):
    """Whether Hydra's own topology puts the instance at `path` at the
    index Moho answers (Hydra's instance data being the stale one)."""
    instancer = data.get((path, "instance/instancer"), "").removeprefix("= ")
    locations = data.get((instancer, "instancerTopology/instanceLocations"))
    if not locations or not moho:
        return False
    paths = elements(path_list(locations).removeprefix("= ")) or []
    return path in paths and str(paths.index(path)) == moho.removeprefix("= ")


def compare_static(name, hydra, moho, mtlx, report):
    hp, hd = hydra
    mp, md = moho
    classes = collections.Counter()
    for path in sorted(set(hp) | set(mp)):
        if path not in mp or path not in hp or hp[path] != mp[path]:
            cls = "numbering" if "ForInstancer" in path else "unexplained"
            classes[cls] += 1
            report.append(f"{name} [{cls}]: prim {path} Hydra {hp.get(path)!r} Moho {mp.get(path)!r}")
    for key in sorted(set(hd) | set(md)):
        if key[0] not in hp or key[0] not in mp:
            continue
        a, b = hd.get(key), md.get(key)
        if {a, b} <= {None, "=null"}:
            continue
        if a is None or b is None or not close(a, b):
            cls = classify(key[0], key[1], (a or "", b or ""), mtlx)
            if name == "final" and key[1] == "instance/instanceIndex" and stale(key[0], hd, b):
                cls = "stale"
            classes[cls] += 1
            report.append(f"{name} [{cls}]: data {key[0]} {key[1]} Hydra {a!r} Moho {b!r}")
    return len(hp), len(mp), classes


def compare_notices(mode, hydra, moho, names_h, names_m, report):
    """The edits whose notices are identical, the edits compared, and the
    edits not exercised: rejected by a side, or missing from one."""
    identical = compared = 0
    rejected = collections.Counter()
    for n in range(max(len(hydra.edits), len(moho.edits))):
        e_h = hydra.edits[n] if n < len(hydra.edits) else None
        e_m = moho.edits[n] if n < len(moho.edits) else None
        if e_h is None or e_m is None or e_h["line"] != e_m["line"]:
            report.append(f"{mode}: edit {n} missing or out of step: Hydra "
                          f"{e_h and e_h['line']!r} Moho {e_m and e_m['line']!r}")
            rejected["missing"] += 1
            continue
        names_h.protos.update(e_h["protos"])
        names_m.protos.update(e_m["protos"])
        if "error" in e_h or "error" in e_m:
            report.append(f"{mode}: edit {e_h['line']} not exercised: Hydra "
                          f"{e_h.get('error')!r} Moho {e_m.get('error')!r}")
            rejected["both" if "error" in e_h and "error" in e_m else "one"] += 1
            continue
        compared += 1
        a = collections.Counter(map(names_h, e_h["notices"]))
        b = collections.Counter(map(names_m, e_m["notices"]))
        if a == b:
            identical += 1
            continue
        report.append(f"{mode}: edit {e_h['line']}")
        for k in sorted((a - b).elements()):
            report.append(f"  Hydra only: {k}")
        for k in sorted((b - a).elements()):
            report.append(f"  Moho only: {k}")
    return identical, compared, rejected


def names_for(dump):
    names = Names()
    for section in ("static", "final"):
        if section in dump.sections:
            names.learn(dump.sections[section])
    return names


def scene(dir):
    report = []
    if (dir / "edits.failed").exists():
        return None, ["edit generation failed"]
    runs = ("cpp.none", "cpp.read", "rs.none", "rs.read")
    failed = []
    for run in runs:
        status = dir / f"{run}.status"
        code = status.read_text().strip() if status.exists() else "?"
        if code != "0":
            failed.append(f"{run} exited {code}")
    if failed:
        return None, ["dumper failed: " + ", ".join(failed)]
    dumps = {k: Dump(dir / f"{k}.txt") for k in runs}
    broken = [k for k, d in dumps.items() if not d.complete or d.errors or "static" not in d.sections]
    if broken:
        return None, [f"incomplete: {', '.join(broken)}"] + [e for d in dumps.values() for e in d.errors]
    err = dir / "cpp.read.err"
    mtlx = err.exists() and re.search(r"Could not open asset @[^@]*\.mtlx@", err.read_text(errors="replace"))
    out = {}
    for section in ("static", "final"):
        names_h, names_m = Names(), Names()
        names_h.learn(dumps["cpp.read"].sections[section])
        names_m.learn(dumps["rs.read"].sections[section])
        hydra = renamed(dumps["cpp.read"].sections[section], names_h)
        moho = renamed(dumps["rs.read"].sections[section], names_m)
        out[section] = compare_static(section, hydra, moho, bool(mtlx), report)
    notices = {}
    for mode in ("none", "read"):
        hydra, moho = dumps[f"cpp.{mode}"], dumps[f"rs.{mode}"]
        # Each run names its own groups (the C++ hashes vary between runs).
        notices[mode] = compare_notices(mode, hydra, moho, names_for(hydra), names_for(moho), report)
    return (out, notices), report


def main(out, only):
    out = pathlib.Path(out)
    dirs = sorted(d for d in out.iterdir() if d.is_dir() and (not only or d.name in only))
    print("| Scene | Prims (Hydra / Moho) | Static: unexplained | After edits: unexplained "
          "| Notices identical, nothing read | Notices identical, all read |")
    print("|---|---|---|---|---|---|")
    totals = collections.Counter()
    explained = collections.Counter()
    explained_after = collections.Counter()
    failed = []
    for d in dirs:
        row, report = scene(d)
        (d / "diff.txt").write_text("\n".join(report) + "\n", encoding="utf-8")
        if row is None:
            failed.append(d.name)
            print(f"| {d.name} | {report[0]} | - | - | - | - |")
            continue
        sections, notices = row
        h, m, static = sections["static"]
        _, _, after = sections["final"]
        (ni, nt, nr), (ri, rt, rr) = notices["none"], notices["read"]
        explained.update(static)
        explained_after.update(after)
        totals.update(scenes=1, static=static["unexplained"], after=after["unexplained"],
                      ni=ni, nt=nt, ri=ri, rt=rt)
        totals.update({f"none {k}": v for k, v in nr.items()})
        totals.update({f"read {k}": v for k, v in rr.items()})
        print(f"| {d.name} | {h} / {m} | {static['unexplained']} | {after['unexplained']} "
              f"| {ni}/{nt} | {ri}/{rt} |")
    t = totals
    by_class = lambda counts: ", ".join(f"{k} {v}" for k, v in sorted(counts.items()) if k != "unexplained")
    not_run = ", ".join(f"{k} {v}" for k, v in sorted(t.items()) if k.startswith(("none ", "read "))) or "none"
    print(f"\nTotal: {t['scenes']} scenes compared, {len(failed)} not ({', '.join(failed) or 'none'}); "
          f"{t['static']} unexplained static differences ({t['after']} after edits); notices identical "
          f"{t['ni']}/{t['nt']} nothing read, {t['ri']}/{t['rt']} all read; edits not exercised: {not_run}. "
          f"Static differences by class: {by_class(explained)}; after the edits: {by_class(explained_after)}.")


if __name__ == "__main__":
    main(sys.argv[1], set(sys.argv[2:]))

# Hydra parity check

Compares Moho with UsdImaging and Hydra on a corpus of scenes: the prims,
their types and every data source of each, then the notices each edit of
a generated script gives. The results are in `docs/hydra-parity-check.md`.

## The two sides

- `hydradump.cpp`, built by `build.sh` against conda-forge's OpenUSD 26.08,
  builds `UsdImagingCreateSceneIndices` (draw mode off) and, as a render
  index with hdPrman's or arnold-usd's filters does, an
  `HdMergingSceneIndex` with `HdsiLightLinkingSceneIndex` and
  `HdDependencyForwardingSceneIndex` over it. The filters are built before
  the scene is added to the merge, so that they see every prim added.
- `crates/moho-parity` (`mohodump`) builds `chain::standard` with
  `RendererFilter::light_linking` and `dependency_forwarding`.

Both take `<read|none> <scene> [edits]` and print the same format:

| Line | Meaning |
|---|---|
| `section static`, `section final` | The index before the edits, and after them (`read` mode only) |
| `proto P I` | USD prototype `P`, whose smallest instance is `I` |
| `prim P T` | Prim `P` of Hydra type `T` |
| `ds P L = V` | The value at locator `L` of `P`, as C++ `TfStringify` writes it (`=null`, `=none`, `=block` otherwise) |
| `edit N LINE` | Line `N` of the edit script, applied |
| `added`, `removed`, `dirtied`, `renamed` | The notices that edit gave, dirtied locators sorted |
| `editerror E` | The edit failed |
| `end` | The dump is complete |

In `read` mode every data source is read before the edits; in `none` mode
only the prims and their types. A value over 1 MiB is cut, with ` ...`.

## The edit script

`edits.py` writes one per scene from its own data: a transform, visibility,
purpose, a `displayColor` primvar, a material parameter, a binding
retargeted, a light's intensity, a light link and then `includeRoot`, point
instancer positions, a prim defined, an instance made non-instanceable, a
prim removed, and a time change. An attribute the scene does not author is
set twice, so that the second set is a plain value change. A line is one of:

```text
set <attribute> <type> <usda value>
define <prim> <type>
remove <prim>
instanceable <prim> <true|false>
targets <relationship> <path>...
time <time>
```

Both sides read a value through a one-attribute usda layer.

## Running it

```sh
tools/parity/build.sh "$BIN"
python3 tools/parity/fixtures.py crates/moho/tests "$FIXTURES"
export FIXTURES SAMPLES=~/Workspace/samples CRUST_SAMPLES=~/Workspace/github/doubleailes/Crust/samples
tools/parity/run.sh "$BIN" "$OUT" tools/parity/scenes.txt
python3 tools/parity/compare.py "$OUT"
```

`scenes.txt` is the corpus: Moho's test fixtures (`fixtures.py` writes
every usda literal of `crates/moho/tests`), crust-render's samples, Kitchen
Set, the MaterialX teapot and lion, ALab and Moana's `isCoral` element.
`compare.py` prints a Markdown row per scene and writes each scene's
differences to `diff.txt`; its docstring lists what it normalizes.

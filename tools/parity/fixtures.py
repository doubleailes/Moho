"""Writes every usda literal of Moho's tests as a scene of the corpus.

    python fixtures.py TESTS_DIR OUT_DIR

A literal is a raw string (`r#"#usda 1.0 ..."#`) or a plain one
(`"#usda 1.0\\n..."`). A `format!` template, which holds `{{`, is left
out. Each is written as `fx_<test file>_<n>.usda`.
"""

import pathlib
import re
import sys

RAW = re.compile(r'r#"(#usda 1\.0.*?)"#', re.S)
PLAIN = re.compile(r'(?<!#)"(#usda 1\.0(?:\\.|[^"\\])*)"')


def main(tests, out):
    out = pathlib.Path(out)
    out.mkdir(parents=True, exist_ok=True)
    for path in sorted(pathlib.Path(tests).glob("*.rs")):
        text = path.read_text(encoding="utf-8")
        literals = RAW.findall(text)
        literals += [bytes(s, "utf-8").decode("unicode_escape") for s in PLAIN.findall(text)]
        n = 0
        for literal in literals:
            if "{{" in literal:
                continue
            (out / f"fx_{path.stem}_{n}.usda").write_text(literal, encoding="utf-8")
            n += 1


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])

#!/usr/bin/env bash
# Builds the C++ dumper against the reference USD build, in a pixi
# environment that holds it and a compiler.
#
#   tools/parity/build.sh OUT_DIR
#
# Writes OUT_DIR/hydradump and OUT_DIR/env (the environment's prefix, for
# run.sh). Needs pixi.
set -euo pipefail

out="${1:?usage: build.sh OUT_DIR}"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

# The reference: conda-forge's OpenUSD 26.08 for Python 3.13.
specs=(-s "openusd==26.08=py313h6963fe8_0" -s "python=3.13" -s cxx-compiler)

pixi exec "${specs[@]}" -- bash -c '
set -euo pipefail
prefix="$CONDA_PREFIX"
echo "$prefix" > "$1/env"
libs=(usdImaging hdsi hd usd usdGeom usdShade usdLux sdf pcp ar plug tf vt gf trace work arch python)
c++ -std=c++17 -O2 -DNDEBUG \
    -I"$prefix/include" -I"$prefix/include/python3.13" \
    "$2/hydradump.cpp" -o "$1/hydradump" \
    -L"$prefix/lib" -Wl,-rpath,"$prefix/lib" \
    "${libs[@]/#/-lusd_}" -ltbb -lpython3.13
' _ "$out" "$here"
echo "built $out/hydradump"

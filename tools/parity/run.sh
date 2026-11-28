#!/usr/bin/env bash
# Runs the Hydra parity check over a corpus (README.md).
#
#   tools/parity/run.sh BIN_DIR OUT_DIR SCENES
#
# BIN_DIR is build.sh's output. SCENES lists one scene per line, `name path`
# (`#` starts a comment); a path may name `$FIXTURES`, `$SAMPLES` and
# `$CRUST_SAMPLES`, which the caller exports. For each scene, OUT_DIR/<name>/ gets its edit
# script and both dumps in both modes ({cpp,rs}.{none,read}.txt, with each
# dumper's stderr beside it). Scenes run in parallel (JOBS, default 8), each
# dumper under a timeout (TIMEOUT seconds, default 3600) and a memory cap
# (MEMORY, default 60G).
set -euo pipefail

bin="$(cd "${1:?usage: run.sh BIN_DIR OUT_DIR SCENES}" && pwd)"
out="${2:?usage: run.sh BIN_DIR OUT_DIR SCENES}"
scenes="${3:?usage: run.sh BIN_DIR OUT_DIR SCENES}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

cargo build --release --quiet -p moho-parity --manifest-path "$repo/Cargo.toml"
target="$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
export MOHODUMP="$target/release/moho-parity"
export HYDRADUMP="$bin/hydradump"
export PREFIX="$(cat "$bin/env")"
export HERE="$here" OUT="$out"
export TIMEOUT="${TIMEOUT:-3600}" MEMORY="${MEMORY:-60G}"

one() {
    local name="$1" scene dir="$OUT/$1"
    scene="$(eval echo "$2")"
    mkdir -p "$dir"
    "$PREFIX/bin/python" "$HERE/edits.py" "$scene" > "$dir/edits.txt" 2> "$dir/edits.err" || true
    local cap=(systemd-run --user --scope -q -p "MemoryMax=$MEMORY" -p MemorySwapMax=0 timeout "$TIMEOUT")
    for mode in none read; do
        "${cap[@]}" "$HYDRADUMP" "$mode" "$scene" "$dir/edits.txt" > "$dir/cpp.$mode.txt" 2> "$dir/cpp.$mode.err" || true
        "${cap[@]}" "$MOHODUMP" "$mode" "$scene" "$dir/edits.txt" > "$dir/rs.$mode.txt" 2> "$dir/rs.$mode.err" || true
    done
    echo "$name"
}
export -f one

grep -v '^\s*\(#\|$\)' "$scenes" | xargs -P "${JOBS:-8}" -L 1 bash -c 'one "$0" "$1"'

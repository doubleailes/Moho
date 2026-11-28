#!/usr/bin/env bash
# Runs the Hydra parity check over a corpus (README.md).
#
#   tools/parity/run.sh BIN_DIR OUT_DIR SCENES
#
# BIN_DIR is build.sh's output. SCENES lists one scene per line: a name
# (letters, digits, `_`, `-` and `.`), then the path, the rest of the line
# (`#` starts a comment). A path may begin with `$FIXTURES`, `$SAMPLES` or
# `$CRUST_SAMPLES`, which the caller exports; nothing else in it is
# expanded. For each scene, OUT_DIR/<name>/ gets its edit script and both
# dumps in both modes ({cpp,rs}.{none,read}.txt), each with the dumper's
# stderr (.err) and exit status (.status) beside it; when the edit script
# cannot be generated, `edits.failed` and no dump. Scenes run in parallel
# (JOBS, default 8), each dumper under a timeout (TIMEOUT seconds, default
# 3600) and a memory cap (MEMORY, default 60G).
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

# The path of a scene line, its one documented variable expanded.
expand() {
    local path="$1" var
    for var in FIXTURES SAMPLES CRUST_SAMPLES; do
        if [[ "$path" == "\$$var/"* ]]; then
            printf '%s/%s' "${!var:?$var is not set}" "${path#"\$$var/"}"
            return
        fi
    done
    if [[ "$path" == *'$'* ]]; then
        echo "unsupported variable in $path" >&2
        return 1
    fi
    printf '%s' "$path"
}
export -f expand

one() {
    local name="$1" scene dir status
    if [[ ! "$name" =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]*$ ]]; then
        echo "bad scene name: $name" >&2
        return 1
    fi
    scene="$(expand "$2")"
    dir="$OUT/$name"
    rm -rf "$dir"
    mkdir -p "$dir"
    if ! "$PREFIX/bin/python" "$HERE/edits.py" "$scene" > "$dir/edits.txt" 2> "$dir/edits.err"; then
        touch "$dir/edits.failed"
        echo "$name: edit generation failed"
        return 0
    fi
    local cap=(systemd-run --user --scope -q -p "MemoryMax=$MEMORY" -p MemorySwapMax=0 timeout "$TIMEOUT")
    for mode in none read; do
        for side in cpp rs; do
            local dump="$HYDRADUMP"
            [[ $side == rs ]] && dump="$MOHODUMP"
            status=0
            "${cap[@]}" "$dump" "$mode" "$scene" "$dir/edits.txt" \
                > "$dir/$side.$mode.txt" 2> "$dir/$side.$mode.err" || status=$?
            echo "$status" > "$dir/$side.$mode.status"
        done
    done
    echo "$name"
}
export -f one

# Each scene line as two NUL-terminated fields, its name and its path.
grep -v '^\s*\(#\|$\)' "$scenes" \
    | sed -E 's/^\s*([^[:space:]]+)\s+(.*[^[:space:]])\s*$/\1\x00\2\x00/' \
    | tr -d '\n' \
    | xargs -0 -n 2 -P "${JOBS:-8}" bash -c 'one "$0" "$1"'

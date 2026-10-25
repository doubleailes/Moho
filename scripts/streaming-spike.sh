#!/usr/bin/env bash
# Runs the S0 streaming spike's strategies on one scene, each in its own
# process (a process's peak memory cannot be reset), and prints a comparison.
#
#   scripts/streaming-spike.sh [--skip-c] [--with-d] [--out DIR] <scene>
#
# --skip-c  skip the full-stage reference (C), for Moana: Crust's recorded
#           75.74 GiB composition stands as its baseline.
# --with-d  also run (D), (B) read through Moho. Its digest hashes index data,
#           not raw reads, so it is reported but not compared with the
#           reference (crates/moho-spike/tests/strategy.rs compares it with an
#           index over the fully loaded stage).
# --out DIR keep each strategy's JSON lines and a meta.txt (machine, openusd
#           rev, date, scene) in DIR.
#
# Needs cargo and jq. Builds the release binary first. Exits non-zero when a
# digest differs from the reference or a strategy reports errors.
set -euo pipefail

usage() { echo "usage: $0 [--skip-c] [--with-d] [--out DIR] <scene>" >&2; exit 2; }

skip_c=0
with_d=0
out=""
scene=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-c) skip_c=1; shift ;;
        --with-d) with_d=1; shift ;;
        --out) [[ $# -ge 2 ]] || usage; out="$2"; shift 2 ;;
        -*) usage ;;
        *) [[ -z "$scene" ]] || usage; scene="$1"; shift ;;
    esac
done
[[ -n "$scene" ]] || usage
command -v jq >/dev/null || { echo "$0: jq is required" >&2; exit 1; }

repo="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release --quiet -p moho-spike --manifest-path "$repo/Cargo.toml"
target="$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" | jq -r .target_directory)"
bin="$target/release/streaming-spike"

strategies=(a b c)
[[ $skip_c -eq 1 ]] && strategies=(a b)
# (D) runs first so that the reference stays the last strategy.
[[ $with_d -eq 1 ]] && strategies=(d "${strategies[@]}")

if [[ -n "$out" ]]; then
    mkdir -p "$out"
    dir="$out"
    rev="$(sed -n 's/^openusd = .*rev = "\([0-9a-f]*\)".*/\1/p' "$repo/Cargo.toml")"
    [[ -n "$rev" ]] || { echo "$0: no openusd rev found in $repo/Cargo.toml" >&2; exit 1; }
    {
        echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "host: $(hostname)"
        echo "kernel: $(uname -srm)"
        echo "cpu: $(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ *//') ($(nproc) threads)"
        echo "memory: $(grep MemTotal /proc/meminfo 2>/dev/null | awk '{printf "%.1f GiB", $2/1048576}')"
        echo "openusd rev: $rev"
        echo "moho rev: $(git -C "$repo" rev-parse --short HEAD 2>/dev/null || echo unknown)"
        echo "scene: $scene"
        echo "strategies: ${strategies[*]}"
    } > "$dir/meta.txt"
else
    dir="$(mktemp -d)"
    trap 'rm -rf "$dir"' EXIT
fi

for s in "${strategies[@]}"; do
    echo "running strategy $s..." >&2
    "$bin" --strategy "$s" "$scene" > "$dir/$s.jsonl"
done

# The reference digest: (C) when it ran, else (B), Crust's import.
ref="$(jq -r 'select(.phase=="total") | .digest' "$dir/${strategies[-1]}.jsonl")"

gib() { jq -rn --arg k "$1" '$k | if test("^[0-9]+$") then (tonumber / 1048576 * 100 | round / 100 | tostring) else . end'; }

failed=0
printf '%-8s %6s %10s %10s %10s %10s %12s %18s %s\n' \
    strategy chunks open_s total_s peak_GiB final_GiB max_chunk_GiB digest match
for s in "${strategies[@]}"; do
    f="$dir/$s.jsonl"
    chunks="$(jq -r 'select(.phase=="total") | .chunks' "$f")"
    open_s="$(jq -r 'select(.phase=="open") | .wall_s' "$f")"
    total_s="$(jq -r 'select(.phase=="total") | .wall_s' "$f")"
    peak="$(gib "$(jq -r 'select(.phase=="total") | .hwm_kib' "$f")")"
    final="$(gib "$(jq -r 'select(.phase=="total") | .rss_kib' "$f")")"
    max_chunk="$(gib "$(jq -rs '[.[] | select(.phase=="chunk") | .rss_kib | numbers] | max // "unavailable"' "$f")")"
    digest="$(jq -r 'select(.phase=="total") | .digest' "$f")"
    errors="$(jq -r 'select(.phase=="total") | .errors + .mismatches' "$f")"
    match="yes"
    if [[ "$s" == "d" ]]; then
        match="n/a (index digest)"
    elif [[ "$digest" != "$ref" ]]; then
        match="NO"; failed=1
    fi
    [[ "$errors" == "0" ]] || { match="$match (errors: $errors)"; failed=1; }
    printf '%-8s %6s %10.2f %10.2f %10s %10s %12s %18s %s\n' \
        "$s" "$chunks" "$open_s" "$total_s" "$peak" "$final" "$max_chunk" "$digest" "$match"
done

# A digest that differs from the reference, or any read error or mismatch,
# fails the run once the whole table is printed.
if [[ $failed -ne 0 ]]; then
    echo "$0: strategies disagree or reported errors" >&2
    exit 1
fi

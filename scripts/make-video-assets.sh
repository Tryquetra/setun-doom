#!/usr/bin/env bash
# Rebuilds the README video previews from recordings/final.
#
# Requires the five MP4 files produced by scripts/record-levels.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FINAL="${RECORDINGS:-$ROOT/recordings/final}"
OUT="$ROOT/assets/videos"

mkdir -p "$OUT"

thumbnail() {
    local name="$1" seek="$2"
    ffmpeg -y -hide_banner -loglevel error -ss "$seek" -i "$FINAL/$name.mp4" \
        -frames:v 1 -vf "scale=1280:-2" -q:v 3 "$OUT/$name.jpg"
}

thumbnail level0-fidelity 30
thumbnail level1-fast 30
thumbnail level1-veracity 30
thumbnail level2-fidelity 30
thumbnail level3-dos-doom 50

PALETTE="$(mktemp --suffix=.png)"
trap 'rm -f "$PALETTE"' EXIT

ffmpeg -y -hide_banner -loglevel error -ss 12 -t 6 -i "$FINAL/level1-fast.mp4" \
    -vf "fps=12,scale=640:-1:flags=neighbor,palettegen=max_colors=128" "$PALETTE"
ffmpeg -y -hide_banner -loglevel error -ss 12 -t 6 -i "$FINAL/level1-fast.mp4" \
    -i "$PALETTE" \
    -lavfi "fps=12,scale=640:-1:flags=neighbor[x];[x][1:v]paletteuse=dither=bayer" \
    "$OUT/level1-fast.gif"

ls -lh "$OUT"

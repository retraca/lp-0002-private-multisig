#!/usr/bin/env bash
# Render an asciinema cast of the LP-0002 demo into an MP4.
#
# The raw cast keeps real timing (real proofs take minutes); we compress idle
# gaps to a few seconds at render time so the video is watchable while still
# showing every line of real output, including the proof-generation banners.
#
# Usage: ./scripts/render_video.sh <input.cast> <output.mp4>

set -euo pipefail

CAST="${1:-/tmp/lp0002.cast}"
OUT="${2:-/tmp/lp0002-demo.mp4}"
GIF="${OUT%.mp4}.gif"

# Compress idle to 3s max, slightly faster playback, readable font.
agg --idle-time-limit 3 --speed 1.4 --font-size 20 --theme monokai \
    --fps-cap 15 "$CAST" "$GIF"

# GIF -> MP4 (H.264, yuv420p for universal playback). Pad to even dimensions.
ffmpeg -y -i "$GIF" \
    -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p" \
    -movflags +faststart "$OUT"

echo "wrote $OUT"
ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUT" \
    | awk '{printf "duration: %d:%02d\n", $1/60, $1%60}'

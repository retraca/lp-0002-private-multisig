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
# Large font for a crisp, modern, full-HD look (native ~1580x1120).
agg --idle-time-limit 25 --speed 1.0 --font-size 32 --font-family "Menlo" \
    --line-height 1.4 --theme monokai --fps-cap 12 "$CAST" "$GIF"

# GIF -> 1080p MP4: scale to fit, center on a matching dark background so it
# reads like a full-screen terminal recording.
ffmpeg -y -i "$GIF" \
    -vf "scale=-2:1040,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x272822,format=yuv420p" \
    -r 24 -movflags +faststart "$OUT"

echo "wrote $OUT"
ffprobe -v error -show_entries format=duration -of csv=p=0 "$OUT" \
    | awk '{printf "duration: %d:%02d\n", $1/60, $1%60}'

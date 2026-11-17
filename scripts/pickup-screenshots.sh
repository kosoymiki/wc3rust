#!/usr/bin/env bash
# Pick up fresh manual screenshots from the operator's Screenshots folder.
# Usage: scripts/pickup-screenshots.sh [src_dir] [out_dir]
set -uo pipefail
SRC="${1:-/storage/emulated/0/Pictures/Screenshots}"
OUT="${2:-$(cd "$(dirname "$0")/.." && pwd)/analysis/agent-hud/device-shots}"
mkdir -p "$OUT"
MARK="$OUT/.last_pickup"
[ -f "$MARK" ] || date +%s > "$MARK"
while true; do
  now=$(date +%s)
  find "$SRC" -maxdepth 1 -type f \( -iname '*.jpg' -o -iname '*.png' \) ! -name '.pending-*' -newermt "@$(cat "$MARK")" -print0 2>/dev/null |
    while IFS= read -r -d '' f; do
      base=$(basename "$f")
      if cp -p "$f" "$OUT/$base" 2>/dev/null; then
        echo "$(date +%H:%M:%S) picked $base" >> "$OUT/pickup.log"
      fi
    done
  date +%s > "$MARK"
  sleep 5
done

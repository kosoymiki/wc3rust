#!/usr/bin/env bash
# Clean, per-binary r2 artifacts (full function list, entry decompile, exports).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/analysis/r2/artifacts"
mkdir -p "$OUT"
for pair in \
  "War3.exe:Files100_War3.exe" \
  "Game.dll:Files100_Game.dll" \
  "Storm.dll:Files100_Storm.dll" ; do
  name="${pair%%:*}"; file="${pair##*:}"
  path="$ROOT/analysis/game_files/$file"
  [ -f "$path" ] || { echo "MISSING $path" >> "$OUT/progress.log"; continue; }
  echo "START $name" >> "$OUT/progress.log"
  ulimit -v 4500000
  timeout 900 r2 -2 -q -c 'e scr.color=0; e anal.timeout=600; aaa; afl' "$path" > "$OUT/$name.afl.txt" 2> "$OUT/$name.afl.err"
  timeout 300 r2 -2 -q -c 'e scr.color=0; aaa; s entry0; pdf' "$path" > "$OUT/$name.entry.pdf.txt" 2> "$OUT/$name.entry.pdf.err"
  timeout 300 r2 -2 -q -c 'e scr.color=0; aaa; s entry0; pdc' "$path" > "$OUT/$name.entry.pdc.txt" 2> "$OUT/$name.entry.pdc.err"
  rabin2 -Ej "$path" > "$OUT/$name.exports.json" 2>/dev/null
  rabin2 -ij "$path" > "$OUT/$name.imports.json" 2>/dev/null
  echo "DONE $name $(wc -l < "$OUT/$name.afl.txt")" >> "$OUT/progress.log"
done
echo R2_ARTIFACTS_DONE >> "$OUT/progress.log"

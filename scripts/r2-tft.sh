#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/analysis/r2/tft"; mkdir -p "$OUT"; : > "$OUT/progress.log"
for pair in \
  "Game.dll:Files100_Game.dll" \
  "Storm.dll:Files100_Storm.dll" \
  "FrozenThrone.exe:Files100_Frozen Throne.exe" \
  "WarcraftIII.exe:Files100_Warcraft III.exe" ; do
  name="${pair%%:*}"; file="${pair##*:}"; path="$ROOT/analysis/tft_files_$file"
  [ -f "$path" ] || { echo "MISSING $path" >> "$OUT/progress.log"; continue; }
  echo "START $name" >> "$OUT/progress.log"
  rabin2 -I "$path" > "$OUT/$name.info.txt" 2>/dev/null
  rabin2 -i "$path" > "$OUT/$name.imports.txt" 2>/dev/null
  rabin2 -E "$path" > "$OUT/$name.exports.txt" 2>/dev/null
  rabin2 -S "$path" > "$OUT/$name.sections.txt" 2>/dev/null
  rabin2 -z "$path" > "$OUT/$name.strings.txt" 2>/dev/null
  ( ulimit -v 4500000; timeout 900 r2 -2 -q -c 'e scr.color=0; e anal.timeout=600; aaa; afl' "$path" > "$OUT/$name.afl.txt" 2> "$OUT/$name.afl.err" )
  ( ulimit -v 4500000; timeout 300 r2 -2 -q -c 'e scr.color=0; aaa; s entry0; pdc' "$path" > "$OUT/$name.entry.pdc.txt" 2> "$OUT/$name.entry.pdc.err" )
  echo "DONE $name $(wc -l < "$OUT/$name.afl.txt")" >> "$OUT/progress.log"
done
echo R2_TFT_DONE >> "$OUT/progress.log"

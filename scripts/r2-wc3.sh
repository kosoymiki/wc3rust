#!/usr/bin/env bash
# Full radare2 sweep over the locally extracted WC3 binaries.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/analysis/r2"
mkdir -p "$OUT"
BIN="$ROOT/analysis/game_files"
SUMMARY="$OUT/summary.txt"
: > "$SUMMARY"
for f in \
  Files100_War3.exe \
  Files100_Game.dll \
  Files100_Storm.dll \
  Files100_Warcraft\ III.exe \
  Files100_WorldEdit.exe \
  Files100_World\ Editor.exe \
  Files100_BNUpdate.exe \
  Files100_ijl15.dll \
  Files100_Mss32.dll \
  Files100_redist_miles_Mp3dec.asi \
  Files100_redist_miles_Mssdolby.m3d \
  Files100_redist_miles_Msseax2.m3d \
  Files100_redist_miles_Mssfast.m3d \
  Files100_redist_miles_Reverb3.flt \
  SetupDat_blizzard.ax \
  SetupDat_War3Unin.exe ; do
  path="$BIN/$f"
  [ -f "$path" ] || { echo "MISSING $f" >> "$SUMMARY"; continue; }
  tag=$(echo "$f" | tr ' .' '__')
  log="$OUT/$tag.log"
  echo "===== $f =====" >> "$SUMMARY"
  sha256sum "$path" | awk '{print "sha256 "$1}' >> "$SUMMARY"
  rabin2 -I "$path" 2>/dev/null | grep -E '^(arch|bits|bintype|class|endian|machine|os|lang|baddr|binsz|compiled)' >> "$SUMMARY"
  rabin2 -i "$path" 2>/dev/null | tail -n +3 >> "$OUT/$tag-imports.txt"
  rabin2 -e "$path" 2>/dev/null | tail -n +3 >> "$OUT/$tag-exports.txt"
  rabin2 -S "$path" 2>/dev/null | tail -n +3 >> "$OUT/$tag-sections.txt"
  rabin2 -z "$path" 2>/dev/null | tail -n +3 > "$OUT/$tag-strings.txt"
  {
    echo "## r2 analysis $f"
    ulimit -v 4500000
    timeout 600 r2 -2 -q -c 'e anal.timeout=300; aaa; ?e FUNCS; aflc; ?e ENTRY; s entry0; pdf; ?e IMPORTS; ii; ?e EXPORTS; iE; ?e STRINGS; izz~?; ?e PDC_ENTRY; pdc' "$path"
  } > "$log" 2>&1
  funcs=$(grep -A1 '^FUNCS$' "$log" | tail -1 | tr -d ' ')
  imports=$(grep -A1 '^IMPORTS$' "$log" | tail -1 | wc -l)
  exports=$(grep -A1 '^EXPORTS$' "$log" | tail -1 | wc -l)
  echo "funcs=${funcs:-?} imports_lines=${imports} exports_lines=${exports} log=$log" >> "$SUMMARY"
done
echo "R2_SWEEP_DONE" >> "$SUMMARY"

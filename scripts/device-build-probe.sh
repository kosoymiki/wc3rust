#!/usr/bin/env bash
# Device build-pipeline probe: select the army, open BUILD, place the building,
# and print the exact log evidence for every step.
#
# Why a script: the operator reports «здания не строятся». The sim is healthy
# (examples/build_probe.rs: can_place 98.6% around the start), so the failure is
# in the tap chain. Coordinates come from the game's own telemetry
# (`GAME_HUD_RECTS slots=[i@x,y]`) and labels from `GAME_CARD labels=[i:LABEL]`,
# never from guessed pixels.
#
# Usage: H2_DEVICE_SLOT=1 scripts/device-build-probe.sh [--keep]
set -uo pipefail

if [[ "${H2_DEVICE_SLOT:-0}" != "1" ]]; then
  echo "DEVICE_SLOT: not granted (set H2_DEVICE_SLOT=1 explicitly)"
  exit 3
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PKG=org.wc3rust.p0
OUT="$ROOT/analysis"
LOG="$OUT/device-build-probe.log"
mkdir -p "$OUT"

tap() { su -c "input tap $1 $2" >/dev/null 2>&1; }
dump_log() { su -c "logcat -d -v brief" > "$LOG" 2>/dev/null; }
last_hud() { grep "GAME_HUD_RECTS" "$LOG" | tail -1; }
last_card() { grep "GAME_CARD" "$LOG" | tail -1; }

su -c "dumpsys window" 2>/dev/null | grep -q "mCurrentFocus=.*$PKG" || {
  echo "PROBE_FAIL: app is not foreground"; exit 1; }

dump_log
HUD="$(last_hud)"
CARD="$(last_card)"
if [[ -z "$HUD" ]]; then echo "PROBE_FAIL: no GAME_HUD_RECTS line"; exit 1; fi
echo "PROBE hud: $HUD"
echo "PROBE card: ${CARD:-<none>}"

# x,y of a card slot index from `slots=[0@x,y 1@x,y ...]`.
slot_xy() {
  python3 - "$1" "$2" <<'PY'
import re, sys
line, idx = sys.argv[1], sys.argv[2]
for m in re.finditer(r"(\d+)@(-?\d+),(-?\d+)", line):
    if m.group(1) == idx:
        print(m.group(2), m.group(3)); break
PY
}
# First slot index whose label matches the shell-glob pattern.
label_slot() {
  python3 - "$1" "$2" <<'PY'
import re, sys
line, pat = sys.argv[1], sys.argv[2]
for m in re.finditer(r"(\d+):([^ \]]+)", line):
    if pat.lower() in m.group(2).lower():
        print(m.group(1)); break
PY
}

# A selected worker's card *is* the build list (0:ДОМ ВОЖДЕЙ … 8:СТОРОЖЕВАЯ
# БАШНЯ), so only tap ARMY when no build entry is present yet.
BUILD_PATTERNS="ДОМ ЛОГОВО КАЗАРМ ЛЕСОПИЛ АЛТАРЬ ЗВЕРИН ОБИТЕЛ ТОТЕМ БАШН BUILD HALL BARRACKS MILL"
has_build=0
for pattern in $BUILD_PATTERNS; do
  if [[ -n "$(label_slot "$CARD" "$pattern")" ]]; then has_build=1; break; fi
done
if [[ "$has_build" == 0 ]]; then
  ARMY_IDX="$(label_slot "$CARD" ARMY)"
  [[ -z "$ARMY_IDX" ]] && ARMY_IDX=4
  read -r AX AY <<<"$(slot_xy "$HUD" "$ARMY_IDX")"
  [[ -n "${AX:-}" ]] || { echo "PROBE_FAIL: ARMY slot $ARMY_IDX missing in HUD line"; exit 1; }
  echo "PROBE tap ARMY slot=$ARMY_IDX at $AX,$AY"
  tap "$AX" "$AY"; sleep 2; dump_log
  grep -q "BUTTON ARMY" "$LOG" || echo "PROBE_WARN: no BUTTON ARMY in log"
  grep -oE "selected=[0-9]+" "$LOG" | tail -1
fi

# Card of the new selection: find BUILD / build menu entry.
CARD="$(last_card)"
BUILD_IDX=""
for pattern in $BUILD_PATTERNS; do
  BUILD_IDX="$(label_slot "$CARD" "$pattern")"
  [[ -n "$BUILD_IDX" ]] && break
done
if [[ -n "$BUILD_IDX" ]]; then
  read -r BX BY <<<"$(slot_xy "$HUD" "$BUILD_IDX")"
  echo "PROBE tap BUILD slot=$BUILD_IDX at $BX,BY"
  tap "$BX" "$BY"; sleep 2; dump_log
  CARD="$(last_card)"
else
  echo "PROBE: no BUILD label on the card; labels: ${CARD:-<none>}"
fi

# The build submenu lists structures: pick the first construction entry.
HALL_IDX="$(label_slot "$CARD" HALL)"
[[ -z "$HALL_IDX" ]] && HALL_IDX=$(label_slot "$CARD" БОЛЬШ)
[[ -z "$HALL_IDX" ]] && HALL_IDX=$(label_slot "$CARD" FARM)
if [[ -n "$HALL_IDX" ]]; then
  read -r HX HY <<<"$(slot_xy "$HUD" "$HALL_IDX")"
  echo "PROBE tap building slot=$HALL_IDX at $HX,HY"
  tap "$HX" "$HY"; sleep 1; dump_log
fi

# Placement: a point in the open field, well away from every HUD rect.
SIZE=$(grep -oE "GAME_SURFACE [0-9]+x[0-9]+" "$LOG" | tail -1 | awk '{print $2}')
if [[ -z "${SIZE:-}" ]]; then
  SIZE=$(su -c "screencap -p /data/local/tmp/probe-size.png && python3 - <<'PY'
from PIL import Image
im = Image.open('/data/local/tmp/probe-size.png')
print(f"{im.size[0]}x{im.size[1]}")
PY" 2>/dev/null | tail -1)
fi
W=${SIZE%x*}; H=${SIZE#*x}
[[ -n "${W:-}" && -n "${H:-}" ]] || { echo "PROBE_FAIL: surface size unknown"; exit 1; }
GX=$(( W * 55 / 100 )); GY=$(( H * 35 / 100 ))
echo "PROBE tap ground at $GX,$GY"
tap "$GX" "$GY"; sleep 2; dump_log

sleep 6
dump_log

echo "--- evidence ---"
grep -E "PLACE_TAP|PLACE_WORLD|PLACE_SITE|PLACE_BUILT|PLACE_BLOCKED|DENY BUILD|GHOST def" "$LOG" | tail -14
echo "--- construction ---"
grep -oE "BUILD_\(PROGRESS|SITE|DONE\)[^\"]{0,80}|construct[a-z_]*=[0-9.]+|progress=[0-9.]+" "$LOG" | tail -6

su -c "screencap -p /data/local/tmp/build-probe.png" >/dev/null 2>&1
su -c "cp /data/local/tmp/build-probe.png '$OUT/device-build-probe.png'" >/dev/null 2>&1
su -c "chown u0_a394:u0_a394 '$OUT/device-build-probe.png'" >/dev/null 2>&1

if grep -qE "PLACE_SITE|PLACE_BUILT|BUILDING " "$LOG"; then
  echo "PROBE_PASS placement=$(grep -oE "PLACE_SITE [a-z0-9]+|PLACE_BUILT [a-z0-9]+" "$LOG" | tail -1)"
else
  echo "PROBE_FAIL: no placement evidence"
  exit 1
fi

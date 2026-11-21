#!/usr/bin/env bash

# Device-slot guard: never launch the shared-device app without an explicit tech-lead grant.
if [[ "${H2_DEVICE_SLOT:-0}" != "1" ]]; then echo "DEVICE_SLOT: not granted (set H2_DEVICE_SLOT=1 explicitly)"; exit 3; fi
# End-to-end device self-test: launch, screenshot, analyse, tap the real HUD,
# verify orders/training/FPS and fail on any visual or gameplay regression.
#
# Usage: scripts/game-selftest.sh [--no-install] [--keep]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
APK="$ROOT/build/android-p0/release/wc3rust-p0.apk"
PKG="org.wc3rust.p0"
ACT="$PKG/org.libsdl.app.SDLActivity"
OUT="$ROOT/analysis"
LOG="$OUT/device-selftest.log"
mkdir -p "$OUT"

INSTALL=1
KEEP=0
ALLOW_VENDOR=0
RUN_STARTED=$(date +%s)
for arg in "$@"; do
  case "$arg" in
    --no-install) INSTALL=0 ;;
    --keep) KEEP=1 ;;
    --allow-vendor) ALLOW_VENDOR=1 ;;
  esac
done

fail() { echo "SELFTEST_FAIL: $*"; exit 1; }
note() { echo "SELFTEST: $*"; }

if [[ "$INSTALL" == 1 ]]; then
  [[ -f "$APK" ]] || fail "APK missing: $APK"
  su -c "pm install -r '$APK'" >/dev/null || fail "pm install"
fi

# K-01: bind the run to bytes, not to a filename. The device must execute the
# same base.apk this script names; a stale install fails here, loudly.
APK_ID="$(python3 -m scripts.qa.device apk-sha --built "$APK")" \
  || fail "installed package does not match built APK: $APK_ID"
BUILT_SHA="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["built_sha256"])' "$APK_ID")"
INSTALLED_SHA="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["installed_sha256"])' "$APK_ID")"
note "APK sha=$BUILT_SHA installed=$INSTALLED_SHA"

su -c "am force-stop $PKG" >/dev/null 2>&1
su -c "logcat -c" >/dev/null 2>&1
su -c "input keyevent KEYCODE_WAKEUP" >/dev/null 2>&1
su -c "am start -W -n $ACT" >/dev/null 2>&1 || fail "am start"
# Launch exactly once; never re-launch in the focus loop (shared-device safety).
for _ in $(seq 1 6); do
  if su -c "dumpsys window" 2>/dev/null | grep -q "mCurrentFocus=.*$PKG"; then break; fi
  sleep 2
done
su -c "dumpsys window" 2>/dev/null | grep -q "mCurrentFocus=.*$PKG" || fail "activity is not foreground"

# Wait for the first rendered frame (bounded, real evidence in the log).
for _ in $(seq 1 40); do
  su -c "logcat -d -v brief" 2>/dev/null > "$LOG"
  grep -q "GAME_FIRST_FRAME" "$LOG" && break
  sleep 1
done
grep -q "GAME_FIRST_FRAME" "$LOG" || { su -c "logcat -d -v brief" > "$LOG" 2>/dev/null; fail "no first frame"; }
grep -q "GAME_FATAL" "$LOG" && fail "native fatal in log"
grep -q "GAME_READY source=user-mpq" "$LOG" || fail "user MPQ was not loaded"
if ! grep -q "GAME_VULKAN turnip=1" "$LOG"; then
  [[ "$ALLOW_VENDOR" == 1 ]] || fail "Turnip ICD was not selected"
fi
if ! grep -qE 'P0_GPU_ADAPTER name="(Turnip|Mesa)|P0_GPU_ADAPTER .*driver="Mesa' "$LOG"; then
  [[ "$ALLOW_VENDOR" == 1 ]] || fail "the renderer is not running on Turnip: $(grep P0_GPU_ADAPTER "$LOG" | tail -1)"
fi

shoot() {
  local name="$1" dest="$OUT/$name.png" mtime
  # K-12: never let an old analysis/device-*.png stand in for this run. The
  # destination is removed, each transfer step is checked, the PNG magic is
  # verified and the mtime must be inside this run's window.
  rm -f "$dest"
  su -c "screencap -p /data/local/tmp/$name.png" >/dev/null 2>&1 || fail "screencap $name"
  su -c "cat /data/local/tmp/$name.png" > "$dest" 2>/dev/null || fail "copy screenshot $name"
  [[ -s "$dest" ]] || fail "screenshot $name is empty"
  python3 -c 'import sys; sys.exit(0 if open(sys.argv[1], "rb").read(8) == b"\x89PNG\r\n\x1a\n" else 1)' "$dest" \
    || fail "screenshot $name is not a PNG"
  mtime=$(stat -c %Y "$dest")
  [[ "$mtime" -ge "$RUN_STARTED" ]] || fail "screenshot $name is stale (mtime $mtime < run start $RUN_STARTED)"
  python3 scripts/visual_check.py "$dest" || fail "visual check $name"
}

shoot device-1-start
W=$(python3 -c "from PIL import Image; print(Image.open('$OUT/device-1-start.png').size[0])")
H=$(python3 -c "from PIL import Image; print(Image.open('$OUT/device-1-start.png').size[1])")
note "surface ${W}x${H}"

# Tap targets come from the game's own telemetry, never from duplicated
# geometry: `GAME_HUD_RECTS` publishes every card slot centre and `GAME_CARD`
# publishes the label of each slot. A HUD reflow can no longer silently move the
# buttons under the harness (the old 6-column math did exactly that).
tap() { su -c "input tap $1 $2" >/dev/null 2>&1; }
hud_line() { grep "GAME_HUD_RECTS" "$LOG" | tail -1; }
card_line() { grep "GAME_CARD" "$LOG" | tail -1; }
slot_xy() {
  python3 - "$1" "$2" <<'PY'
import re, sys
line, idx = sys.argv[1], sys.argv[2]
for m in re.finditer(r"(\d+)@(-?\d+),(-?\d+)", line):
    if m.group(1) == idx:
        print(m.group(2), m.group(3)); break
PY
}
label_slot() {
  python3 - "$1" "$2" <<'PY'
import re, sys
line, pat = sys.argv[1], sys.argv[2].lower()
for m in re.finditer(r"(\d+):([^ \]]+)", line):
    if pat in m.group(2).lower():
        print(m.group(1)); break
PY
}
tap_slot() {
  local label="$1" idx x y
  idx=$(label_slot "$(card_line)" "$label")
  [[ -n "$idx" ]] || fail "no card slot labelled '$label' (card: $(card_line))"
  read -r x y <<<"$(slot_xy "$(hud_line)" "$idx")"
  [[ -n "${x:-}" ]] || fail "slot $idx ('$label') has no coordinates in $(hud_line)"
  note "tap '$label' slot=$idx at $x,$y"
  tap "$x" "$y"
}

tap_slot ARMY
sleep 2
su -c "logcat -d -v brief" > "$LOG" 2>/dev/null
grep -q "BUTTON ARMY" "$LOG" || fail "ARMY button did not fire"
grep -q "selected=4" "$LOG" || fail "ARMY did not select the starting army"
shoot device-2-army

# Move the selected army to open ground.
GX=$(( W * 62 / 100 )); GY=$(( H * 46 / 100 ))
tap "$GX" "$GY"
sleep 2
su -c "logcat -d -v brief" > "$LOG" 2>/dev/null
grep -q "ORDER MOVE units=4" "$LOG" || fail "ground tap did not issue a move order"
shoot device-3-moving

# Return to the hall, select it, train a Footman.
tap_slot BASE
sleep 1
tap $(( W / 2 )) $(( H / 2 ))
sleep 1
tap_slot FOOTMAN
sleep 2
su -c "logcat -d -v brief" > "$LOG" 2>/dev/null
grep -q "TRAIN Footman" "$LOG" || fail "hall training was not queued"
note "waiting 22 s for the training queue"
sleep 22
su -c "logcat -d -v brief" > "$LOG" 2>/dev/null
grep -q -E "TRAINED Footman|trained=1|army=5" "$LOG" || fail "trained Footman did not arrive"
shoot device-4-trained

# FPS and tick progression from the status lines.
fps=$(grep "GAME_STATE" "$LOG" | tail -1 | sed -n 's/.*fps=\([0-9]*\).*/\1/p')
tick1=$(grep "GAME_STATE" "$LOG" | tail -2 | head -1 | sed -n 's/.*tick=\([0-9]*\).*/\1/p')
tick2=$(grep "GAME_STATE" "$LOG" | tail -1 | sed -n 's/.*tick=\([0-9]*\).*/\1/p')
note "fps=${fps:-?} tick=${tick1:-?}->${tick2:-?}"
[[ -n "$fps" && "$fps" -ge 20 ]] || fail "FPS below 20 (${fps:-missing})"
[[ -n "$tick1" && -n "$tick2" && "$tick2" -gt "$tick1" ]] || fail "simulation tick did not advance"
grep -q "GAME_FATAL" "$LOG" && fail "fatal after gameplay"

if [[ "$KEEP" == 0 ]]; then
  su -c "am force-stop $PKG" >/dev/null 2>&1
fi
echo "SELFTEST_APK sha=$BUILT_SHA installed=$INSTALLED_SHA"
echo "SELFTEST_PASS fps=$fps screenshots=$OUT/device-{1..4}-*.png"

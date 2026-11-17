#!/usr/bin/env bash
# Device lifecycle stress for AUD-026 (pause/resume/terminate x N).
#
# Runs the already-installed APK; it never installs or replaces anything on the
# device. Each cycle backgrounds the activity (pause -> WILL_ENTER_BACKGROUND ->
# synchronous checkpoint), resumes it, and every fifth cycle force-stops the
# process before relaunching so the next launch must restore the checkpoint.
#
# The device log is then checked by scripts/check-lifecycle-stress-log.sh, which
# is the same parser exercised offline by scripts/test-lifecycle-log-check.sh.
#
# Usage: scripts/device-lifecycle-stress.sh [--cycles N] [--serial S]
#            [--package PKG] [--wait SEC] [--out FILE]
# Requires: adb on PATH, one device/emulator, the APK already installed.
set -euo pipefail

CYCLES=10
SERIAL=""
PACKAGE="org.wc3rust.p0"
WAIT=4
OUT="${TMPDIR:-/tmp}/wc3rust-lifecycle-stress.log"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cycles) CYCLES="$2"; shift 2 ;;
    --serial) SERIAL="$2"; shift 2 ;;
    --package) PACKAGE="$2"; shift 2 ;;
    --wait) WAIT="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

command -v adb >/dev/null || { echo "adb not found on PATH" >&2; exit 2; }
ADB=(adb)
[[ -n "$SERIAL" ]] && ADB+=( -s "$SERIAL" )
COMPONENT="$PACKAGE/org.libsdl.app.SDLActivity"

"${ADB[@]}" get-state >/dev/null || { echo "no adb device" >&2; exit 2; }
"${ADB[@]}" logcat -c
"${ADB[@]}" shell am force-stop "$PACKAGE"
sleep 1

terminations=0
for ((cycle = 1; cycle <= CYCLES; cycle++)); do
  echo "== cycle $cycle/$CYCLES: launch"
  "${ADB[@]}" shell am start -W -n "$COMPONENT" >/dev/null
  sleep "$WAIT"
  echo "== cycle $cycle/$CYCLES: pause (HOME)"
  "${ADB[@]}" shell input keyevent KEYCODE_HOME
  sleep 1
  echo "== cycle $cycle/$CYCLES: resume"
  "${ADB[@]}" shell am start -W -n "$COMPONENT" >/dev/null
  sleep "$WAIT"
  if (( cycle % 5 == 0 )); then
    terminations=$((terminations + 1))
    echo "== cycle $cycle/$CYCLES: terminate (force-stop) and relaunch"
    "${ADB[@]}" shell am force-stop "$PACKAGE"
    sleep 1
    "${ADB[@]}" shell am start -W -n "$COMPONENT" >/dev/null
    sleep "$WAIT"
  fi
done

echo "== collecting logs -> $OUT"
{
  "${ADB[@]}" logcat -d -v brief -s wc3rust:I
  echo "== crash buffer"
  "${ADB[@]}" logcat -d -b crash || true
} > "$OUT"

exec "$(dirname "${BASH_SOURCE[0]}")/check-lifecycle-stress-log.sh" \
  --cycles "$CYCLES" --terminations "$terminations" "$OUT"

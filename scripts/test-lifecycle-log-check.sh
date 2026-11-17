#!/usr/bin/env bash
# Offline self-test for scripts/check-lifecycle-stress-log.sh (AUD-026).
#
# Builds synthetic device logs so the log check itself is proved without a
# phone: a healthy 3-cycle run must PASS, and a crash (or a missing checkpoint)
# must FAIL. The device-side driver is scripts/device-lifecycle-stress.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK="$ROOT/scripts/check-lifecycle-stress-log.sh"
TMP="${TMPDIR:-/tmp}/wc3rust-lifecycle-check.$$"
mkdir -p "$TMP"
trap 'rm -rf "$TMP"' EXIT

cycles=3
terminations=1
ok="$TMP/ok.log"
{
  echo "I/wc3rust: GAME_LIFECYCLE watch=1 main_thread=true block_on_pause=1 low_latency_audio=1 recreate_activity=0"
  for cycle in $(seq 1 "$cycles"); do
    echo "I/wc3rust: GAME_CHECKPOINT path=/data/user/0/org.wc3rust.p0/files/wc3rust-resume.sav digest=000000000000000$cycle units=4 tick=$((cycle * 100)) map=map phase=playing"
    echo "I/wc3rust: GAME_LIFECYCLE background phase=Background focused=false"
    echo "I/wc3rust: GAME_LIFECYCLE foreground phase=Foreground focused=true"
  done
  echo "I/wc3rust: GAME_RESUME path=/data/user/0/org.wc3rust.p0/files/wc3rust-resume.sav units=4 tick=300 digest=0000000000000003 map=map phase=playing"
} > "$ok"

"$CHECK" --cycles "$cycles" --terminations "$terminations" "$ok" > "$TMP/ok.out" 2>&1
grep -q "LIFECYCLE_STRESS PASS" "$TMP/ok.out"

crash="$TMP/crash.log"
cp "$ok" "$crash"
echo "FATAL EXCEPTION: main" >> "$crash"
if "$CHECK" --cycles "$cycles" --terminations "$terminations" "$crash" > "$TMP/crash.out" 2>&1; then
  echo "checker accepted a crashed log" >&2
  exit 1
fi
grep -q "FAIL: crashes=" "$TMP/crash.out"

missing="$TMP/missing.log"
grep -v "GAME_CHECKPOINT path=" "$ok" > "$missing"
if "$CHECK" --cycles "$cycles" --terminations "$terminations" "$missing" > "$TMP/missing.out" 2>&1; then
  echo "checker accepted a log without checkpoints" >&2
  exit 1
fi
grep -q "FAIL: checkpoints=" "$TMP/missing.out"

echo "LIFECYCLE_LOG_CHECK_SELFTEST PASS (ok=pass crash=fail missing-checkpoint=fail)"

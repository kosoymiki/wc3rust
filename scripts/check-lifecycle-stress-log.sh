#!/usr/bin/env bash
# Verify the log produced by scripts/device-lifecycle-stress.sh (AUD-026).
#
# The check is deliberately mechanical: every pause must have produced a
# checkpoint log line, every force-stop cycle must have produced a resume line,
# the watcher must have reported that it runs on the SDL main thread, and no
# crash/fatal/checkpoint-failure marker may appear.
#
# Usage: scripts/check-lifecycle-stress-log.sh --cycles N [--terminations M] LOG
set -euo pipefail

CYCLES=1
TERMINATIONS=0
LOG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --cycles) CYCLES="$2"; shift 2 ;;
    --terminations) TERMINATIONS="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) LOG="$1"; shift ;;
  esac
done
[[ -f "$LOG" ]] || { echo "log not found: $LOG" >&2; exit 2; }

count() { grep -c -- "$1" "$LOG" 2>/dev/null || true; }

watch=$(count "GAME_LIFECYCLE watch=1 main_thread=true")
background=$(count "GAME_LIFECYCLE background")
foreground=$(count "GAME_LIFECYCLE foreground")
checkpoints=$(count "GAME_CHECKPOINT path=")
resumes=$(count "GAME_RESUME path=")
fatal=$(count "GAME_FATAL")
crashes=$(grep -Ec "FATAL EXCEPTION|signal 11|signal 6|Abort message|SIGSEGV" "$LOG" 2>/dev/null || true)
checkpoint_failed=$(count "GAME_CHECKPOINT_FAILED")
resume_failed=$(count "GAME_RESUME_FAILED")

echo "lifecycle stress summary ($LOG)"
printf '  cycles=%s terminations=%s\n' "$CYCLES" "$TERMINATIONS"
printf '  watcher_main_thread=%s\n' "$watch"
printf '  background=%s foreground=%s\n' "$background" "$foreground"
printf '  checkpoints=%s resumes=%s\n' "$checkpoints" "$resumes"
printf '  fatal=%s crashes=%s checkpoint_failed=%s resume_failed=%s\n' \
  "$fatal" "$crashes" "$checkpoint_failed" "$resume_failed"

fail=0
check_min() {
  local label="$1" actual="$2" expected="$3"
  if (( actual < expected )); then
    echo "FAIL: $label=$actual < $expected" >&2
    fail=1
  fi
}
check_zero() {
  local label="$1" actual="$2"
  if (( actual != 0 )); then
    echo "FAIL: $label=$actual (expected 0)" >&2
    fail=1
  fi
}

check_min "watcher_main_thread" "$watch" 1
check_min "background" "$background" "$CYCLES"
check_min "foreground" "$foreground" "$CYCLES"
# A pause saves, then the resume advances the tick. A cycle may legitimately
# coalesce two pauses, so require at least one save per two cycles.
check_min "checkpoints" "$checkpoints" $(( (CYCLES + 1) / 2 ))
check_min "resumes" "$resumes" "$TERMINATIONS"
check_zero "fatal" "$fatal"
check_zero "crashes" "$crashes"
check_zero "checkpoint_failed" "$checkpoint_failed"
check_zero "resume_failed" "$resume_failed"

if (( fail != 0 )); then
  echo "LIFECYCLE_STRESS FAIL" >&2
  exit 1
fi
echo "LIFECYCLE_STRESS PASS"

#!/data/data/com.termux/files/usr/bin/bash
# agent_test_guard — kill test/build processes that were not started by the
# tech lead.
#
# Rule (2026-11-22): subagents never run tests or test builds. The text rule was
# ignored, so this watchdog enforces it: every 5 s it scans /proc for
# `cargo test|build`, `cargo check --tests`, `h2-cargo test|build` and kills the
# process unless its environment carries H2_TECHLEAD=1. The tech lead exports
# that variable for its own detached verification runs.
#
# Usage: setsid scripts/agent_test_guard.sh </dev/null >/dev/null 2>&1 &
set -u
LOG=/data/data/com.termux/files/usr/tmp/agent_test_guard.log
: >> "$LOG"
echo "GUARD_START $$ $(date -u)" >> "$LOG"

is_authorized() {
  local pid="$1"
  tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep -qx 'H2_TECHLEAD=1'
}

while true; do
  for pid in $(ls /proc 2>/dev/null | grep -E '^[0-9]+$'); do
    [ "$pid" = "$$" ] && continue
    cmd=$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null) || continue
    case "$cmd" in
      *"cargo test"*|*"cargo build"*|*"cargo check"*"--tests"*|*"h2-cargo test"*|*"h2-cargo build"*|*"run_mechanic_tests.sh"*)
        if ! is_authorized "$pid"; then
          echo "GUARD_KILL pid=$pid cmd=${cmd:0:160} $(date -u +%H:%M:%S)" >> "$LOG"
          kill -TERM "$pid" 2>/dev/null
          sleep 0.2
          kill -KILL "$pid" 2>/dev/null
        fi
        ;;
    esac
  done
  sleep 5
done

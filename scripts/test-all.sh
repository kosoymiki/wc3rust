#!/usr/bin/env bash
# Host test suite for Herofolends-RS.
#
# One crate at a time under scripts/safe-test.sh (address-space capped, single
# crate, detached-friendly). Does not run `cargo test --workspace`.
#
# Usage: scripts/test-all.sh [--device]
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
OUT="$ROOT/analysis"
mkdir -p "$OUT"
SUMMARY="$OUT/test-all.log"
: > "$SUMMARY"
failures=0
run() {
  local label="$1"; shift
  echo "== $label" | tee -a "$SUMMARY"
  "$@" >/dev/null 2>&1
  local rc=$?
  if [[ $rc -ne 0 ]]; then
    echo "FAIL $label rc=$rc" | tee -a "$SUMMARY"
    failures=$((failures + 1))
  else
    echo "PASS $label" | tee -a "$SUMMARY"
  fi
}

for crate in wc3-core wc3-mpq wc3-formats wc3-assetdb wc3-path wc3-sim wc3-jass wc3-replay wc3-input wc3-audio wc3-anim wc3-render wc3-game wc3-adrenotools; do
  run "unit:$crate" ./scripts/safe-test.sh "$crate"
done
run "assets:formats" "${SHELL:-/bin/bash}" -c 'ulimit -v 4500000; cargo test -p wc3-game --test assets -- --test-threads=1'
run "gameplay:wiring" "${SHELL:-/bin/bash}" -c 'ulimit -v 4500000; cargo test -p wc3-game --test gameplay -- --test-threads=1'
run "visual:real-map" "${SHELL:-/bin/bash}" -c 'ulimit -v 4500000; cargo test -p wc3-game --test visual -- --test-threads=1'

if [[ "${1:-}" == "--device" ]]; then
  run "device:selftest" ./scripts/game-selftest.sh
fi

if [[ $failures -ne 0 ]]; then
  echo "TEST_ALL_FAIL failures=$failures (see $SUMMARY and analysis/test-*.log)"
  exit 1
fi
echo "TEST_ALL_PASS"

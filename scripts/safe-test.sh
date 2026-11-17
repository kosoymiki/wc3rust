#!/usr/bin/env bash
# Memory-capped, single-crate test runner.
#
# The interactive opencode server shares the phone with cargo/rustc. An uncapped
# test compile/run has already triggered the kernel OOM killer twice (see
# analysis/test logs / dmesg). This runner:
#   * runs exactly one crate,
#   * caps address space with ulimit -v (4.5 GB),
#   * disables debug info for the test profile,
#   * serialises tests and cargo jobs,
#   * writes a log with a SAFE_TEST_DONE marker.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
crate="${1:?usage: safe-test.sh <crate> [-- test args]}"
shift || true
log="$ROOT/analysis/test-$crate.log"
mkdir -p "$ROOT/analysis"

export CARGO_BUILD_JOBS=1
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_DEV_DEBUG=0
export SDL3_PREFIX="$ROOT/third_party/sdl3-min"

(
  ulimit -v 4500000
  cargo test -p "$crate" --lib -- --test-threads=1 "$@"
) >"$log" 2>&1
rc=$?
echo "SAFE_TEST_DONE crate=$crate rc=$rc" >>"$log"
exit "$rc"

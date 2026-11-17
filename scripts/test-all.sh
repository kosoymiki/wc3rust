#!/usr/bin/env bash
# Workspace-wide quality gate (AUD-030).
#
# What this proves, and what it deliberately does NOT do:
#   1. The member list is derived from `cargo metadata --no-deps` every run.
#      There is no hand-written crate subset: adding a workspace member adds it
#      to the gate automatically.
#   2. `cargo check --workspace` runs for the host target.
#   3. Every installed Android target that has a linker gets a
#      `cargo check -p wc3rust-android --target <triple>`. A target without
#      a linker is recorded as SKIP with the missing tool, never as PASS
#      (host here is already aarch64-linux-android, so the shipping ABI is still
#      checked by phase 2).
#   4. Every member with tests is tested through the single-slot runner
#      (scripts/h2-cargo: flock + memory floor + bounded jobs), one member at a
#      time, each under a wall-clock timeout. EXCLUDE below is the only way a
#      member is skipped and every entry names the reason.
#   5. Output is never hidden: each job writes its own log, and on failure the
#      tail of that log is printed.
#   6. The summary is machine-readable (`analysis/test-all.jsonl`), one JSON
#      object per job plus a final summary object.
#
# Usage:
#   scripts/test-all.sh [options]
#     --check-only          only `cargo check --workspace`
#     --no-check            skip the workspace check
#     --no-tests            skip the per-member test phase
#     --no-targets          skip the Android target checks
#     --only m1,m2          restrict the test phase to these members
#     --timeout SECONDS     per-job timeout (default 1800)
#     --list                print members/exclusions as JSON and exit
#     --self-test           prove the gate fails on a deliberately failing test
#                           placed in a crate the old subset never ran
#
# Exit status: 0 only when every executed job passed; 1 when a job failed;
# 2 on bad usage/metadata; 3 when the shared filesystem cannot hold the next
# link step (`TEST_ALL_ABORT reason=disk`, checked before every job and after a
# failed job's log shows ENOSPC) — an aborted gate is not a member failure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CARGO="${H2_CARGO:-$ROOT/scripts/h2-cargo}"
LOG_DIR="$ROOT/analysis/test-all"
SUMMARY_JSONL="$ROOT/analysis/test-all.jsonl"
TIMEOUT_S="${H2_TEST_ALL_TIMEOUT:-1800}"
ULIMIT_KB="${H2_TEST_ULIMIT_KB:-4500000}"
# Fail fast instead of spraying misleading member FAILs when the shared target
# filesystem is out of space: a full disk corrupts cargo freshness state and
# makes every later job look like a code failure (observed 2026-11-17).
MIN_FREE_KB="${H2_TEST_ALL_MIN_FREE_KB:-1500000}"
ANDROID_APP="wc3rust-android"

# Per-member address-space caps. The default (4.5 GB, the historical
# `safe-test.sh` value) is enough for one test binary, but `wc3-game --lib` runs
# 452 tests in one process and accumulates asset state; it aborted at 4.5 GB
# with "memory allocation of 16 MiB bytes failed" (2026-11-17). Raise only for
# the members that need it, so a runaway elsewhere still hits the default.
declare -A MEMBER_ULIMIT_KB=(
  [wc3-game]="${H2_TEST_ULIMIT_WC3_GAME_KB:-7000000}"
  [wc3-render]="${H2_TEST_ULIMIT_WC3_RENDER_KB:-6000000}"
)

# Per-member wall-clock budget. `wc3-game --lib` alone runs 452 tests and takes
# ~55 min serial on this device (measured 2026-11-17: 3283 s), so the default
# 30 min would time out a *passing* suite and turn a real result into a timeout.
declare -A MEMBER_TIMEOUT_S=(
  [wc3-game]="${H2_TEST_TIMEOUT_WC3_GAME_S:-4200}"
  [wc3-render]="${H2_TEST_TIMEOUT_WC3_RENDER_S:-2700}"
)

# Documented, evidence-backed exclusions. Keep this list short: each entry must
# name the target-specific reason why the member cannot run in this gate.
declare -A EXCLUDE=(
  # Hook crates are `cdylib` Android shims loaded into a foreign process
  # (Adreno tools / the game's linker). They have no `tests/` directory and no
  # `#[cfg(test)]` module (checked 2026-11-17), so there is nothing to run;
  # compile coverage comes from `cargo check --workspace` (phase 1), which
  # builds every crate in this list.
  [wc3-adrenotools-main-hook]="cdylib Android hook shim: no tests, compiled by phase 1"
  [wc3-adrenotools-fopen-hook]="cdylib Android hook shim: no tests, compiled by phase 1"
  [wc3-adrenotools-gsl-hook]="cdylib Android hook shim: no tests, compiled by phase 1"
)

DO_CHECK=1
DO_TESTS=1
DO_TARGETS=1
DO_SELF_TEST=0
ONLY=""
LIST=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check-only) DO_TESTS=0; DO_TARGETS=0 ;;
    --no-check) DO_CHECK=0 ;;
    --no-tests) DO_TESTS=0 ;;
    --no-targets) DO_TARGETS=0 ;;
    --only) shift; ONLY="${1:-}" ;;
    --only=*) ONLY="${1#--only=}" ;;
    --timeout) shift; TIMEOUT_S="${1:-$TIMEOUT_S}" ;;
    --timeout=*) TIMEOUT_S="${1#--timeout=}" ;;
    --list) LIST=1 ;;
    --self-test) DO_SELF_TEST=1 ;;
    -h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "test-all: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done

mkdir -p "$LOG_DIR"
: > "$SUMMARY_JSONL"
FAILED=0
PASSED=0
SKIPPED=0

json() { printf '%s\n' "$1" >> "$SUMMARY_JSONL"; }

# ---------------------------------------------------------------- members

metadata_members() {
  cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" \
    | jq -r '.packages[] | [.name, (.manifest_path | sub("/Cargo.toml$"; ""))] | @tsv'
}

MEMBER_TSV="$(metadata_members)"
if [[ -z "$MEMBER_TSV" ]]; then
  echo "test-all: cargo metadata returned no workspace members" >&2
  exit 2
fi

members() { cut -f1 <<<"$MEMBER_TSV"; }
member_dir() { awk -F'\t' -v m="$1" '$1 == m { print $2 }' <<<"$MEMBER_TSV"; }

if [[ $LIST -eq 1 ]]; then
  jq -n --arg members "$(members)" --arg excluded "$(printf '%s\n' "${!EXCLUDE[@]}")" \
    '{members: ($members | split("\n") | map(select(length > 0))),
      excluded: ($excluded | split("\n") | map(select(length > 0)))}'
  exit 0
fi

# Only the test phase honours --only; phases 1 and 3 always cover the workspace
# because the point of the gate is that a green run means "everything".
selected() {
  [[ -z "$ONLY" ]] && return 0
  [[ ",$ONLY," == *",$1,"* ]]
}

# ---------------------------------------------------------------- runner

free_kb() { df -Pk "$ROOT" | awk 'NR == 2 { print $4 }'; }

# Abort the whole gate (not a member failure) when the disk cannot hold the
# next link step. Exit 3 is distinct from "tests failed" (1) and "bad usage" (2).
require_disk() {
  local free
  free=$(free_kb)
  if [[ -n "$free" && "$free" -lt "$MIN_FREE_KB" ]]; then
    echo "TEST_ALL_ABORT reason=disk free_kb=$free need_kb=$MIN_FREE_KB"
    json "$(printf '{"phase":"abort","job":"disk","rc":null,"verdict":"abort","free_kb":%d,"need_kb":%d}' "$free" "$MIN_FREE_KB")"
    exit 3
  fi
}

# run_job <phase> <name> <command...>
# One job = one log + one JSONL record + one pass/fail/skip verdict.
run_job() {
  local phase="$1" name="$2"; shift 2
  local log="$LOG_DIR/${phase}-${name//\//_}.log"
  local started elapsed rc verdict
  started=$(date +%s)
  if [[ "$1" == "SKIP" ]]; then
    verdict="skip"
    SKIPPED=$((SKIPPED + 1))
    echo "SKIP $phase:$name — $2"
    json "$(printf '{"phase":"%s","job":"%s","rc":null,"verdict":"skip","reason":"%s"}' "$phase" "$name" "${2//\"/\\\"}")"
    return 0
  fi
  require_disk
  local job_ulimit="${MEMBER_ULIMIT_KB[$name]:-$ULIMIT_KB}"
  local job_timeout="${MEMBER_TIMEOUT_S[$name]:-$TIMEOUT_S}"
  echo "== $phase:$name (ulimit=${job_ulimit}KB timeout=${job_timeout}s)"
  (
    # Address-space cap: one wc3-game test binary peaks at ~3.7 GB, and the
    # interactive server needs the rest of the phone. Tests run with one thread
    # (see the test phase) so two heavy lib tests cannot stack inside the cap.
    ulimit -v "$job_ulimit" 2>/dev/null || true
    export CARGO_PROFILE_TEST_DEBUG=0
    export SDL3_PREFIX="$ROOT/third_party/sdl3-min"
    timeout -k 10 "$job_timeout" "$@"
  ) >"$log" 2>&1
  rc=$?
  elapsed=$(( $(date +%s) - started ))
  if [[ $rc -eq 0 ]]; then
    verdict="pass"; PASSED=$((PASSED + 1))
    echo "PASS $phase:$name ${elapsed}s"
  elif [[ $rc -eq 124 || $rc -eq 137 ]]; then
    verdict="fail"; FAILED=$((FAILED + 1))
    echo "FAIL $phase:$name timeout after ${job_timeout}s (log: ${log#$ROOT/})"
    tail -n 40 "$log" | sed 's/^/  | /'
  else
    if grep -q "No space left on device" "$log" 2>/dev/null; then
      echo "TEST_ALL_ABORT reason=disk phase=$phase job=$name (log: ${log#$ROOT/})"
      tail -n 20 "$log" | sed 's/^/  | /'
      json "$(printf '{"phase":"abort","job":"%s","rc":%d,"verdict":"abort","reason":"disk","log":"%s"}' "$name" "$rc" "${log#$ROOT/}")"
      exit 3
    fi
    verdict="fail"; FAILED=$((FAILED + 1))
    echo "FAIL $phase:$name rc=$rc ${elapsed}s (log: ${log#$ROOT/})"
    tail -n 40 "$log" | sed 's/^/  | /'
  fi
  json "$(printf '{"phase":"%s","job":"%s","rc":%d,"verdict":"%s","seconds":%d,"log":"%s"}' \
    "$phase" "$name" "$rc" "$verdict" "$elapsed" "${log#$ROOT/}")"
  [[ $rc -eq 0 ]]
}

# ---------------------------------------------------------------- self-test

if [[ $DO_SELF_TEST -eq 1 ]]; then
  # A green gate is only trustworthy if it catches a failure in a crate the old
  # subset never ran. `wc3-bink` is exactly such a crate (absent from the old
  # scripts/test-all.sh loop), so the probe goes there.
  probe_member="${H2_SELF_TEST_MEMBER:-wc3-bink}"
  probe_dir="$(member_dir "$probe_member")"
  if [[ -z "$probe_dir" ]]; then
    echo "SELF_TEST_FAIL: $probe_member is not a workspace member" >&2
    exit 1
  fi
  probe="$probe_dir/tests/zz_aud030_self_test.rs"
  mkdir -p "$(dirname "$probe")"
  cat > "$probe" <<'EOF'
// Injected by `scripts/test-all.sh --self-test` (AUD-030). It must make the
// gate exit non-zero; the gate removes this file afterwards.
#[test]
fn aud030_self_test_must_fail() {
    panic!("AUD-030 self-test: the gate must report this failure");
}
EOF
  cleanup() { rm -f "$probe"; }
  trap cleanup EXIT
  set +e
  "$0" --only "$probe_member" --no-check --no-targets --timeout 600 \
    >"$LOG_DIR/self-test.log" 2>&1
  rc=$?
  rm -f "$probe"
  trap - EXIT
  if [[ $rc -eq 0 ]]; then
    echo "SELF_TEST_FAIL: the gate reported success with a failing test in $probe_member"
    exit 1
  fi
  echo "SELF_TEST_PASS: failing test in $probe_member made the gate exit $rc"
  echo "  self-test gate log: ${LOG_DIR#$ROOT/}/self-test.log"
  exit 0
fi

# ---------------------------------------------------------------- phase 1

if [[ $DO_CHECK -eq 1 ]]; then
  run_job check workspace "$CARGO" check --workspace
fi

# ---------------------------------------------------------------- phase 3

if [[ $DO_TARGETS -eq 1 ]]; then
  host="$(rustc -vV | awk '/^host:/ { print $2 }')"
  sysroot="$(rustc --print sysroot)"
  for libdir in "$sysroot"/lib/rustlib/*-linux-android*; do
    [[ -d "$libdir" ]] || continue
    triple="$(basename "$libdir")"
    [[ "$triple" == "$host" ]] && continue
    linker=""
    candidates=("$triple-clang")
    case "$triple" in
      aarch64-linux-android) candidates+=("aarch64-linux-android21-clang") ;;
      armv7-linux-androideabi) candidates+=("armv7a-linux-androideabi21-clang" "armv7a-linux-androideabi-clang") ;;
      i686-linux-android) candidates+=("i686-linux-android21-clang") ;;
      x86_64-linux-android) candidates+=("x86_64-linux-android21-clang") ;;
    esac
    for candidate in "${candidates[@]}"; do
      if command -v "$candidate" >/dev/null 2>&1; then linker="$candidate"; break; fi
    done
    if [[ -z "$linker" && -n "${ANDROID_LINKER:-}" ]]; then
      linker="$ANDROID_LINKER"
    fi
    if [[ -z "$linker" ]]; then
      run_job target "$triple" SKIP "no NDK/clang linker for $triple on this host (host $host is checked by phase 1)"
      continue
    fi
    CARGO_TARGET_UNDERSCORE="$(tr 'a-z-' 'A-Z_' <<<"$triple")_LINKER"
    export "$CARGO_TARGET_UNDERSCORE"="$linker"
    run_job target "$triple" "$CARGO" check -p "$ANDROID_APP" --target "$triple"
  done
fi

# ---------------------------------------------------------------- phase 2

if [[ $DO_TESTS -eq 1 ]]; then
  while IFS= read -r member; do
    selected "$member" || continue
    if [[ -n "${EXCLUDE[$member]:-}" ]]; then
      SKIPPED=$((SKIPPED + 1))
      echo "SKIP test:$member — ${EXCLUDE[$member]}"
      json "$(printf '{"phase":"test","job":"%s","rc":null,"verdict":"skip","reason":"%s"}' "$member" "${EXCLUDE[$member]}")"
      continue
    fi
    run_job test "$member" "$CARGO" test -p "$member" -- --test-threads=1
  done < <(members)

  # The shared negative corpus needs the optional movie decoder to reach the
  # AVI parser; the default feature run above covers everything else.
  if selected "wc3-game"; then
    run_job test "wc3-game#input_limits" \
      "$CARGO" test -p wc3-game --test input_limits --features movie_decoder
  fi
fi

# ---------------------------------------------------------------- summary

json "$(printf '{"phase":"summary","members":%d,"passed":%d,"failed":%d,"skipped":%d,"timeout_s":%s,"rc":%d}' \
  "$(members | wc -l | tr -d ' ')" "$PASSED" "$FAILED" "$SKIPPED" "$TIMEOUT_S" \
  "$([[ $FAILED -eq 0 ]] && echo 0 || echo 1)")"

if [[ $FAILED -ne 0 ]]; then
  echo "TEST_ALL_FAIL failed=$FAILED passed=$PASSED skipped=$SKIPPED (see ${LOG_DIR#$ROOT/}/ and ${SUMMARY_JSONL#$ROOT/})"
  exit 1
fi
echo "TEST_ALL_PASS passed=$PASSED skipped=$SKIPPED members=$(members | wc -l | tr -d ' ') (summary: ${SUMMARY_JSONL#$ROOT/})"

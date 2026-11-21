#!/usr/bin/env bash
# Orc01 acceptance smoke (GOAL 2026-11-18, lane 510).
#
# Loads the first Reign-of-Chaos orc chapter headless through the real runtime
# (`wc3-game --example campaign_report` boots the mission, the JASS script and
# the cutscene director) and ticks it. It is a hard gate: the workspace gate
# `scripts/test-all.sh` runs this script and fails when it fails.
#
# Asserted, all from the run's own output (nothing is assumed):
#   * the process exits 0 and does not panic;
#   * `CAMPAIGN_LOAD ... units>=N quests>=N` — the mission spawned its roster
#     and registered its quests;
#   * `CAMPAIGN_MISSION_STATE ... script_booted=true` — war3map.j ran;
#   * `CAMPAIGN_CINE frames=<ticks>` — the full tick budget executed;
#   * `CAMPAIGN_CAMERA keyframes>=4` — the Opening cutscene rig exists
#     (packet 503 acceptance: >=4 monotone keyframes).
#
# Prints the canonical line the tech-lead acceptance asks for:
#   ORC01 units=N quests=N ticks=3000 cam=K fades=M subs=S
#
# Usage: tests/gates/orc01-smoke.sh
# Env:   H2_ORC01_DATA_ROOT  install root holding Maps/Campaign/Orc01.w3m
#        ORC01_TICKS         tick budget (default 3000)
#        ORC01_TIMEOUT_S     wall clock for the cargo run (default 1500)
#        ORC01_MIN_UNITS / ORC01_MIN_QUESTS / ORC01_MIN_CAM / ORC01_MIN_SUBS
#                            regression floors (defaults below)
#
# Exit: 0 when every assertion holds, 1 on the first violated assertion.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TICKS="${ORC01_TICKS:-3000}"
TIMEOUT_S="${ORC01_TIMEOUT_S:-2100}"
DATA_ROOT="${H2_ORC01_DATA_ROOT:-$ROOT/analysis/war3_full3}"
MAP="$DATA_ROOT/Maps/Campaign/Orc01.w3m"
MIN_UNITS="${ORC01_MIN_UNITS:-1}"
MIN_QUESTS="${ORC01_MIN_QUESTS:-1}"
MIN_CAM="${ORC01_MIN_CAM:-4}"
MIN_SUBS="${ORC01_MIN_SUBS:-1}"
LOG="${ORC01_LOG:-$ROOT/analysis/test-all/orc01-run.log}"
mkdir -p "$(dirname "$LOG")"

fail() {
  local reason="$1"
  # The campaign evidence lines are printed before the failure so a red gate is
  # diagnosable from its own output (tail alone may be inside the quest block).
  if [[ -f "$LOG" ]]; then
    grep -E '^CAMPAIGN_(LOAD|MISSION_STATE|CINE|CAMERA) ' "$LOG" | tail -4 | sed 's/^/  | /'
  fi
  echo "  | last 60 lines of $LOG:"
  tail -n 60 "$LOG" 2>/dev/null | sed 's/^/  | /'
  echo "ORC01_SMOKE_FAIL reason=$reason data_root=$DATA_ROOT ticks=$TICKS log=$LOG"
  exit 1
}

[[ -f "$MAP" ]] || { : > "$LOG"; fail "missing-map $MAP"; }
# Cheap preflight before touching the cargo slot: a truncated or zero-byte map
# must fail here (the loader would reject it anyway), so the gate's failure
# self-test is deterministic on a busy device and does not depend on a build.
magic="$(head -c 4 "$MAP" 2>/dev/null | tr -d '\0')"
bytes="$(stat -c %s "$MAP" 2>/dev/null || echo 0)"
if [[ "$magic" != "HM3W" || "$bytes" -lt 1024 ]]; then
  : > "$LOG"
  fail "map-header-invalid magic=${magic:-none} bytes=$bytes"
fi

# The runtime resolves the campaign from these two variables; both must point at
# the same install so a partially populated copy cannot satisfy one lookup and
# silently miss the other.
export WC3RUST_DATA="$DATA_ROOT"
export WC3RUST_CAMPAIGN="$DATA_ROOT/Maps/Campaign"

started=$(date +%s)
timeout -k 10 "$TIMEOUT_S" \
  "$ROOT/scripts/h2-cargo" run -p wc3-game --example campaign_report --offline -- \
  --mission Orc01 --frames "$TICKS" >"$LOG" 2>&1
rc=$?
elapsed=$(( $(date +%s) - started ))

if [[ $rc -eq 75 ]]; then
  echo "ORC01_SMOKE_ABORT reason=cargo-slot elapsed=${elapsed}s log=$LOG"
  exit 3
fi
if [[ $rc -eq 137 ]]; then
  echo "ORC01_SMOKE_ABORT reason=killed elapsed=${elapsed}s log=$LOG"
  exit 3
fi
[[ $rc -eq 0 ]] || fail "cargo-run-rc=$rc elapsed=${elapsed}s"
if grep -q "panicked at" "$LOG"; then fail "panic"; fi
if grep -q "campaign maps not found" "$LOG"; then fail "campaign-dir-not-found"; fi

load="$(grep -m1 '^CAMPAIGN_LOAD ' "$LOG")"
[[ -n "$load" ]] || fail "no-CAMPAIGN_LOAD-line"

num() { sed -n "s/.*[[:space:]]$1=\([0-9][0-9]*\).*/\1/p" <<<"$2" | head -1; }

units="$(num units "$load")"
quests="$(num quests "$load")"
source_kind="$(sed -n 's/.*[[:space:]]source=\([a-z]*\).*/\1/p' <<<"$load" | head -1)"
[[ -n "$units" && -n "$quests" ]] || fail "CAMPAIGN_LOAD-missing-units-or-quests"
(( units >= MIN_UNITS )) || fail "units=$units < $MIN_UNITS"
(( quests >= MIN_QUESTS )) || fail "quests=$quests < $MIN_QUESTS"

grep -q '^CAMPAIGN_MISSION_STATE .*script_booted=true' "$LOG" || fail "script-not-booted"

cine="$(grep -m1 '^CAMPAIGN_CINE ' "$LOG")"
[[ -n "$cine" ]] || fail "no-CAMPAIGN_CINE-line"
frames="$(num frames "$cine")"
fades="$(num fades "$cine")"
subs="$(num transmissions "$cine")"
[[ "${frames:-}" == "$TICKS" ]] || fail "frames=${frames:-none} != ticks=$TICKS"
(( ${fades:-0} >= 0 )) || fail "fades-unparsable"
(( ${subs:-0} >= MIN_SUBS )) || fail "transmissions=${subs:-none} < $MIN_SUBS"

camera="$(grep -m1 '^CAMPAIGN_CAMERA ' "$LOG")"
[[ -n "$camera" ]] || fail "no-CAMPAIGN_CAMERA-line"
cam="$(num keyframes "$camera")"
[[ -n "$cam" ]] || fail "keyframes-unparsable"
(( cam >= MIN_CAM )) || fail "keyframes=$cam < $MIN_CAM (opening cutscene rig)"

echo "ORC01 units=$units quests=$quests ticks=$frames cam=$cam fades=$fades subs=$subs source=$source_kind elapsed=${elapsed}s"
echo "ORC01_SMOKE_PASS log=$LOG"

#!/data/data/com.termux/files/usr/bin/bash
# Robustness lane (ledger 419) verification sequence.
# One cargo at a time through scripts/h2-cargo; every step's rc is recorded.
#
# K-08: the old script hardcoded the checkout path and ended without `exit`,
# so a red step was invisible to the caller. The path is derived from this
# file and the exit status is the worst step's status.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1
LOG="${TMPDIR:-$PREFIX/tmp}/robustness-verify.log"
mkdir -p "$(dirname "$LOG")"
: > "$LOG"
say() { echo "$*" | tee -a "$LOG"; }

worst=0
record() {
    local rc=$1
    [ "$rc" -ne 0 ] && [ "$worst" -eq 0 ] && worst=$rc
    return 0
}

say "=== STEP 1 formats+mpq+data tests  $(date -Iseconds)"
scripts/h2-cargo test -p wc3-formats -p wc3-mpq -p wc3-data --offline >> "$LOG" 2>&1
rc1=$?
record "$rc1"
say "STEP1_RC=$rc1"

say "=== STEP 2 wc3-game check  $(date -Iseconds)"
scripts/h2-cargo check -p wc3-game --offline >> "$LOG" 2>&1
rc2=$?
record "$rc2"
say "STEP2_RC=$rc2"

say "=== STEP 3 wc3-game malformed_map test  $(date -Iseconds)"
scripts/h2-cargo test -p wc3-game --offline --test malformed_map >> "$LOG" 2>&1
rc3=$?
record "$rc3"
say "STEP3_RC=$rc3"

say "=== DONE $(date -Iseconds) rc1=$rc1 rc2=$rc2 rc3=$rc3 overall=$worst"
exit "$worst"

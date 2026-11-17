#!/data/data/com.termux/files/usr/bin/bash
# Robustness lane (ledger 419) verification sequence.
# One cargo at a time through scripts/h2-cargo; every step's rc is recorded.
set -u
cd /data/data/com.termux/files/home/src/herofolends-rs || exit 1
LOG=/data/data/com.termux/files/usr/tmp/opencode/robustness-verify.log
: > "$LOG"
say() { echo "$*" | tee -a "$LOG"; }

say "=== STEP 1 formats+mpq+data tests  $(date -Iseconds)"
scripts/h2-cargo test -p wc3-formats -p wc3-mpq -p wc3-data --offline >> "$LOG" 2>&1
rc1=$?
say "STEP1_RC=$rc1"

say "=== STEP 2 wc3-game check  $(date -Iseconds)"
scripts/h2-cargo check -p wc3-game --offline >> "$LOG" 2>&1
rc2=$?
say "STEP2_RC=$rc2"

say "=== STEP 3 wc3-game malformed_map test  $(date -Iseconds)"
scripts/h2-cargo test -p wc3-game --offline --test malformed_map >> "$LOG" 2>&1
rc3=$?
say "STEP3_RC=$rc3"

say "=== DONE $(date -Iseconds) rc1=$rc1 rc2=$rc2 rc3=$rc3"

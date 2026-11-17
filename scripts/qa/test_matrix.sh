#!/data/data/com.termux/files/usr/bin/bash
# Full-suite test matrix: one row per crate with counts, rc and the exact
# failing test names. Writes analysis/tech-lead/test_matrix.json.
#
#   bash scripts/qa/test_matrix.sh [--json PATH]
#
# Every row is a real `cargo test` invocation; a red target keeps its failing
# test names so nobody has to guess which assertion broke.

set -u
cd "$(dirname "$0")/../.." || exit 2
OUT=analysis/tech-lead/test_matrix.json
while [ $# -gt 0 ]; do
    case "$1" in
        --json) shift; OUT="${1:-$OUT}" ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

ROWS="$(mktemp "${TMPDIR:-/data/data/com.termux/files/usr/tmp}/test-matrix.XXXXXX")"
LOG_DIR="$(mktemp -d "${TMPDIR:-/data/data/com.termux/files/usr/tmp}/test-matrix-run-XXXXXX")"
trap 'rm -f "$ROWS"' EXIT
: > "$ROWS"

run_suite() { # run_suite <label> <crate> <args...>
    local label="$1" crate="$2"; shift 2
    local log="/data/data/com.termux/files/usr/tmp/suite-${label}.log"
    local command="cargo test -p $crate --offline $*"
    local started rc passed failed
    started=$(date +%s)
    CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 timeout 1800 \
        cargo test -p "$crate" --offline "$@" > "$log" 2>&1
    rc=$?
    passed=$(grep -oE '[0-9]+ passed' "$log" | awk '{s+=$1} END {print s+0}')
    failed=$(grep -oE '[0-9]+ failed' "$log" | awk '{s+=$1} END {print s+0}')
    local failing
    failing=$(grep -oE '^failures:$' -A1000 "$log" >/dev/null 2>&1; grep -E '^\s+[a-z0-9_:]+$' "$log" | awk '!seen[$0]++' | head -20 | tr '\n' ' ')
    python3 - "$ROWS" "$label" "$crate" "$command" "$rc" "$passed" "$failed" "$log" "$(( $(date +%s) - started ))" "$failing" <<'PY'
import json, sys
path, label, crate, command, rc, passed, failed, log, seconds, failing = sys.argv[1:11]
row = {
    "suite": label,
    "crate": crate,
    "command": command,
    "rc": int(rc),
    "tests": int(passed) + int(failed),
    "passed": int(passed),
    "failed": int(failed),
    "failing_tests": failing.split(),
    "log": log,
    "seconds": int(seconds),
}
open(path, "a").write(json.dumps(row) + "\n")
PY
    printf '%-24s rc=%-3s passed=%-4s failed=%-3s\n' "$label" "$rc" "$passed" "$failed"
    return "$rc"
}

status=0
run_suite sim-lib        wc3-sim  --lib                        || status=1
run_suite game-lib       wc3-game --lib                        || status=1
run_suite data           wc3-data                              || status=1
run_suite input          wc3-input                             || status=1
run_suite re-lib         wc3-re   --lib                        || status=1

python3 - "$ROWS" "$OUT" <<'PY'
import json, sys, time
rows_path, out_path = sys.argv[1:3]
rows = [json.loads(line) for line in open(rows_path) if line.strip()]
payload = {
    "schema": "wc3-test-matrix-v1",
    "generated_unix": int(time.time()),
    "rows": rows,
    "summary": {
        "suites": len(rows),
        "tests": sum(r["tests"] for r in rows),
        "passed": sum(r["passed"] for r in rows),
        "failed": sum(r["failed"] for r in rows),
        "red_suites": [r["suite"] for r in rows if r["rc"] != 0],
    },
}
json.dump(payload, open(out_path, "w"), indent=2)
open(out_path, "a").write("\n")
print(json.dumps(payload["summary"]))
for row in rows:
    if row["rc"] != 0:
        print(f"RED {row['suite']} rc={row['rc']} failing={row['failing_tests']} log={row['log']}")
PY
exit "$status"

#!/data/data/com.termux/files/usr/bin/bash
# Final measurement sequence. Waits for the workspace to compile (other agents
# edit shared crates), then runs the two configurations and writes JSON + logs.
set -u
cd "$(dirname "$0")/.." || exit 1
export CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0
ulimit -v 4000000 2>/dev/null || true
OUT=analysis/agent-perf/bench-results
mkdir -p "$OUT"
LOG="$OUT/final-run.log"
: > "$LOG"

attempt=0
while [ "$attempt" -lt 25 ]; do
    attempt=$((attempt + 1))
    if cargo build -p herofolends-bench --bin herofolends-gpu-bench > "$OUT/final-build.log" 2>&1; then
        echo "FINAL build ok attempt=$attempt" >> "$LOG"
        break
    fi
    echo "FINAL build attempt=$attempt failed: $(grep -m1 '^error' "$OUT/final-build.log" || true)" >> "$LOG"
    sleep 45
done
if [ "$attempt" -ge 25 ]; then
    echo "FINAL gave up" >> "$LOG"
    exit 2
fi

PIN=""
if command -v taskset >/dev/null 2>&1 && taskset -c 4-7 true 2>/dev/null; then
    PIN="taskset -c 4-7"
fi
export H2_BENCH_LOADAVG="$(su -c 'cat /proc/loadavg' 2>/dev/null || true)"
echo "FINAL loadavg=$H2_BENCH_LOADAVG" >> "$LOG"

BIN=target/debug/herofolends-gpu-bench
for skin in gpu cpu; do
    echo "FINAL run skin=$skin start=$(date +%H:%M:%S) loadavg=$(su -c 'cat /proc/loadavg' 2>/dev/null)" >> "$LOG"
    $PIN "$BIN" --frames 60 --units 120 --width 2414 --height 1080 --skin "$skin" \
        --json "$OUT/skin-$skin-2414x1080.json" > "$OUT/skin-$skin-2414x1080.log" 2>&1
    echo "FINAL run skin=$skin rc=$?" >> "$LOG"
done
python3 - <<'PY' >> "$LOG" 2>&1
import json
for skin in ("gpu", "cpu"):
    path = f"analysis/agent-perf/bench-results/skin-{skin}-2414x1080.json"
    try:
        d = json.load(open(path))
        print("JSON OK", path, d["baseline"]["cpu_ms"]["min"], d["optimized"]["cpu_ms"]["min"])
    except Exception as e:
        print("JSON FAIL", path, e)
PY
echo "FINAL done" >> "$LOG"

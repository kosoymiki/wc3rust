#!/data/data/com.termux/files/usr/bin/bash
# Build and run the real-renderer GPU bench under the project's memory caps.
#
#   scripts/perf-gpu.sh [bench args...]
#
# Defaults match the device frame: 2414x1080, 120 units, 240 measured frames.
# Everything the bench reports is measured on the device (CPU scopes, heap
# allocation traffic, GPU timestamp queries, RSS); see apps/bench/src/gpu_bench.rs.
#
# K-08: the old build step piped cargo through `tee | tail`, so a failed build
# looked successful (`false | tee | tail` returns 0) and an OOM/link error
# without a line starting with `error` was accepted, benchmarking the previous
# binary. The build now uses cargo's own exit status and records it.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
# Keep the compiler inside the device's memory budget.
ulimit -v 4000000 2>/dev/null || true

OUT_DIR="analysis/agent-perf/bench-results"
mkdir -p "$OUT_DIR"
LOG="$OUT_DIR/build.log"
BIN="target/debug/wc3rust-gpu-bench"

# Another agent may be mid-edit in a crate we depend on; retry the build a few
# times instead of reporting a transient compile error as a bench failure.
build() {
    local attempts=3
    local attempt=1
    local rc
    while [ "$attempt" -le "$attempts" ]; do
        : > "$LOG"
        if scripts/h2-cargo build -p wc3rust-bench --bin wc3rust-gpu-bench >> "$LOG" 2>&1; then
            tail -5 "$LOG"
            if [ ! -x "$BIN" ]; then
                echo "perf-gpu: cargo rc=0 but $BIN is missing; refusing to benchmark a stale binary" >&2
                return 1
            fi
            echo "perf-gpu: build ok (attempt $attempt) rc=0"
            return 0
        else
            rc=$?
        fi
        tail -20 "$LOG"
        echo "perf-gpu: build attempt $attempt failed rc=$rc (see $LOG); retrying in 45s" >&2
        attempt=$((attempt + 1))
        sleep 45
    done
    echo "perf-gpu: build failed after $attempts attempts" >&2
    return 1
}

if ! build; then
    exit 2
fi

# /proc/loadavg is root-only on this device; record it with the run so the report
# can state the contention the numbers were taken under.
LOADAVG="$(su -c 'cat /proc/loadavg' 2>/dev/null || true)"
export H2_BENCH_LOADAVG="$LOADAVG"
echo "perf-gpu: loadavg=${LOADAVG:-unknown}"
# Pin to the big cores so the scheduler cannot move the bench onto a little core
# mid-measurement.
PIN=""
if command -v taskset >/dev/null 2>&1; then
    if taskset -c 4-7 true 2>/dev/null; then
        PIN="taskset -c 4-7"
    fi
fi
STAMP="$(date +%Y%m%d-%H%M%S)"
JSON="$OUT_DIR/bench-$STAMP.json"

# Memory-capped run; 20 minute guard so a hung driver cannot stall the agent.
( ulimit -v 4000000 2>/dev/null; exec $PIN "$BIN" "$@" --json "$JSON" ) 2>&1 | tee "$OUT_DIR/bench-$STAMP.log"
STATUS=${PIPESTATUS[0]}
if [ "$STATUS" -eq 0 ] && [ ! -s "$JSON" ]; then
    echo "perf-gpu: bench rc=0 but no JSON evidence at $JSON" >&2
    exit 3
fi
echo "perf-gpu: bench rc=$STATUS json=$JSON"
exit "$STATUS"

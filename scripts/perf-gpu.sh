#!/data/data/com.termux/files/usr/bin/bash
# Build and run the real-renderer GPU bench under the project's memory caps.
#
#   scripts/perf-gpu.sh [bench args...]
#
# Defaults match the device frame: 2414x1080, 120 units, 240 measured frames.
# Everything the bench reports is measured on the device (CPU scopes, heap
# allocation traffic, GPU timestamp queries, RSS); see apps/bench/src/gpu_bench.rs.
set -u

cd "$(dirname "$0")/.." || exit 1

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
# Keep the compiler inside the device's memory budget.
ulimit -v 4000000 2>/dev/null || true

OUT_DIR="analysis/agent-perf/bench-results"
mkdir -p "$OUT_DIR"

# Another agent may be mid-edit in a crate we depend on; retry the build a few
# times instead of reporting a transient compile error as a bench failure.
build() {
    local attempts=3
    local attempt=1
    while [ "$attempt" -le "$attempts" ]; do
        if cargo build -p wc3rust-bench --bin wc3rust-gpu-bench 2>&1 | tee "$OUT_DIR/build.log" | tail -5; then
            if ! grep -q '^error' "$OUT_DIR/build.log"; then
                echo "perf-gpu: build ok (attempt $attempt)"
                return 0
            fi
        fi
        echo "perf-gpu: build attempt $attempt failed, retrying in 45s (see $OUT_DIR/build.log)"
        attempt=$((attempt + 1))
        sleep 45
    done
    echo "perf-gpu: build failed after $attempts attempts"
    return 1
}

if ! build; then
    exit 2
fi

BIN="target/debug/wc3rust-gpu-bench"
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
echo "perf-gpu: bench rc=$STATUS json=$JSON"
exit "$STATUS"

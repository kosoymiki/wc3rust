#!/data/data/com.termux/files/usr/bin/bash
# Release-build the bench (the APK profile) and run the 131-unit frame.
#
# The debug build spends ~14 ms of CPU in `prepare`; the shipped game is a
# release build with thin LTO, so the parity-perf numbers must come from here.
# Other agents edit shared crates, so the build retries until the workspace is
# compilable again.
set -u
cd "$(dirname "$0")/.." || exit 1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_DEV_DEBUG=0
OUT=analysis/agent-perf/bench-results
mkdir -p "$OUT"
LOG="$OUT/release-build.log"
: > "$LOG"

attempt=0
while [ "$attempt" -lt 20 ]; do
    attempt=$((attempt + 1))
    if cargo build --release -p herofolends-bench --bin herofolends-gpu-bench >> "$LOG" 2>&1; then
        echo "RELEASE build ok attempt=$attempt" | tee -a "$LOG"
        break
    fi
    echo "RELEASE build attempt=$attempt failed: $(grep -m1 '^error' "$LOG" || true)" | tee -a "$LOG"
    sleep 40
done
if [ "$attempt" -ge 20 ]; then
    echo "RELEASE build giving up" | tee -a "$LOG"
    exit 2
fi

BIN=target/release/herofolends-gpu-bench
PIN=""
if command -v taskset >/dev/null 2>&1 && taskset -c 4-7 true 2>/dev/null; then
    PIN="taskset -c 4-7"
fi
export H2_BENCH_LOADAVG="$(su -c 'cat /proc/loadavg' 2>/dev/null || true)"

for spec in "131 2414 1080" "131 1600 720"; do
    set -- $spec
    units=$1; width=$2; height=$3
    stamp="r2rel-${units}u-${width}x${height}"
    echo "RELEASE run $stamp loadavg=$(su -c 'cat /proc/loadavg' 2>/dev/null)" | tee -a "$LOG"
    $PIN "$BIN" --frames 60 --units "$units" --width "$width" --height "$height" \
        --json "$OUT/$stamp.json" > "$OUT/$stamp.log" 2>&1
    echo "RELEASE run $stamp rc=$?" | tee -a "$LOG"
    grep -E 'BENCH (cpu|gpu variant=full|throughput|facade_timing|raw_timing|parity |counter_parity)' \
        "$OUT/$stamp.log" | tee -a "$LOG"
done

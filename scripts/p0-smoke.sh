#!/usr/bin/env bash
# P0 smoke: SDL3 init/version + wgpu Vulkan triangle on the device Vulkan/Turnip.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export SDL3_PREFIX="${SDL3_PREFIX:-$ROOT/third_party/sdl3-min}"
if [[ ! -f "$SDL3_PREFIX/lib/libSDL3.a" ]]; then
  echo "missing $SDL3_PREFIX/lib/libSDL3.a" >&2
  echo "build SDL3 first: see docs/PLAN.md / analysis/p0-evidence.md" >&2
  exit 2
fi

cargo test --workspace
cargo build --release -p p0-smoke
mkdir -p analysis
./target/release/p0-smoke | tee analysis/p0-smoke.out

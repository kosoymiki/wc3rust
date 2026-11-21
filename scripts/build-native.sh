#!/usr/bin/env bash
# Build libmain.so (the Android cdylib) as a reproducible release artifact.
#
# Why this script exists:
#   * `build-apk.sh` packages a prebuilt target/release/libmain.so. Nothing in
#     the tree recorded which sources that .so came from, so a failed/partial
#     `cargo build` could ship an older library (K-02).
#   * The Rust compiler embeds the absolute checkout and registry paths in
#     panic locations/diagnostics, so two checkouts of the same commit produced
#     different .so bytes (K-03). RUSTFLAGS remaps every device path to a
#     stable /build/... prefix.
#   * The stamp written here is what `build-apk.sh` consumes: the packaged .so
#     must hash-match the exact input inventory, not an mtime guess.
#
# Usage: scripts/build-native.sh [--release|--debug]   (default: release)
# Env:   H2_TARGET            target triple (default aarch64-linux-android)
#        SDL3_ANDROID_LIB    forwarded to the app build.rs
#
# Cargo runs only through scripts/h2-cargo (single-slot, memory-gated).
set -euo pipefail

BUILD_TYPE=release
for arg in "$@"; do
  case "$arg" in
    --release) BUILD_TYPE=release ;;
    --debug) BUILD_TYPE=debug ;;
    -h|--help) sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $arg (expected --release or --debug)" >&2; exit 2 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PY=python3
TARGET="${H2_TARGET:-aarch64-linux-android}"
LIB="$ROOT/target/$BUILD_TYPE/libmain.so"
STAMP="$ROOT/target/$BUILD_TYPE/.libmain-inputs.json"
PROFILE_DIR="$BUILD_TYPE"
[[ "$BUILD_TYPE" == release ]] && PROFILE_DIR=release

# Reproducible path mapping (K-03) lives in .cargo/config.toml so every cargo
# invocation in this repository - not only this script - compiles with the same
# RUSTFLAGS. Alternating flag sets would invalidate the shared target/ cache on
# every switch. Here we only verify the config is present and effective; the
# artifact check below proves no device path survived into libmain.so.
CARGO_CONFIG="$ROOT/.cargo/config.toml"
if [[ ! -f "$CARGO_CONFIG" ]] || ! grep -q -- '--remap-path-prefix' "$CARGO_CONFIG"; then
  echo "build-native: $CARGO_CONFIG is missing the --remap-path-prefix rules (K-03)" >&2
  exit 2
fi
REMAPPED_ROOT="/build/wc3rust"
# Release diagnostics are stripped: the debug sections are not part of a
# shipped APK and only add bytes/nondeterminism (debuginfo is path-heavy even
# after remapping).
if [[ "$BUILD_TYPE" == release ]]; then
  export CARGO_PROFILE_RELEASE_STRIP="${CARGO_PROFILE_RELEASE_STRIP:-debuginfo}"
fi

echo "== build-native ($BUILD_TYPE) target=$TARGET"
echo "== rustflags source: .cargo/config.toml ($(grep -c -- '--remap-path-prefix' "$CARGO_CONFIG") remap rules)"

# The shared cargo slot is a single FIFO (scripts/h2-cargo). Sampling the input
# digest before queueing is wrong: observed waits of 3-15 min let other lanes
# edit sources, so the attempt was stale before the compile even started. The
# digest is taken when the compile actually starts, observed in the h2-cargo log.
H2_LOG_PATH="${H2_CARGO_LOG_PATH:-${PREFIX:-/data/data/com.termux/files/usr}/tmp/h2-cargo.log}"

# The inventory must be identical before and after the compile; otherwise a
# lane edited a source mid-build and the .so is a mix of two trees.
attempts=3
for attempt in $(seq 1 "$attempts"); do
  offset=0
  [[ -f "$H2_LOG_PATH" ]] && offset=$(stat -c %s "$H2_LOG_PATH" 2>/dev/null || echo 0)
  scripts/h2-cargo build -p wc3rust-android --"$BUILD_TYPE" &
  cargo_pid=$!
  before=""
  while kill -0 "$cargo_pid" 2>/dev/null; do
    if [[ -z "$before" && -f "$H2_LOG_PATH" ]]; then
      if tail -c +$((offset + 1)) "$H2_LOG_PATH" 2>/dev/null \
          | grep -qE "^h2-cargo begin .* :: build -p wc3rust-android --$BUILD_TYPE$"; then
        before="$($PY scripts/build_inputs.py native)"
        echo "== inputs at compile start: $before (attempt $attempt/$attempts)"
      fi
    fi
    sleep 1
  done
  wait "$cargo_pid"
  rc=$?
  if [[ $rc -ne 0 ]]; then
    echo "build-native: cargo attempt $attempt failed (rc=$rc); retrying in 20s" >&2
    sleep 20
    continue
  fi
  if [[ -z "$before" ]]; then
    echo "build-native: the compile start was not observed in $H2_LOG_PATH; refusing to certify this artifact" >&2
    sleep 20
    continue
  fi
    after="$($PY scripts/build_inputs.py native)"
    if [[ "$before" != "$after" ]]; then
      echo "build-native: sources changed during the build ($before -> $after); retrying" >&2
      sleep 15
      continue
    fi
    [[ -f "$LIB" ]] || { echo "build-native: cargo succeeded but $LIB is missing" >&2; exit 3; }
    # Prove the path remap landed in the artifact, not just in the config:
    # registry panic locations must now read /build/... and no device diagnostic
    # path (*.rs) may survive. Non-diagnostic strings produced at compile time
    # by env!("CARGO_MANIFEST_DIR") are not remapped by rustc; they are
    # functional host-fallback paths in crate code (assets.rs/hud.rs) and are
    # reported, not fatal.
    remapped="$(grep -a -c -E "/build/(cargo-registry|home/\.cargo)/" "$LIB" || true)"
    if [[ "${remapped:-0}" -eq 0 ]]; then
      echo "build-native: no /build/... registry paths in libmain.so; path remap did not land" >&2
      exit 5
    fi
    leaked=""
    for pattern in "$ROOT" "${HOME}/.cargo" "${HOME}/src"; do
      hit="$(grep -a -o -E "$pattern/[^\"' ]*\\.rs" "$LIB" | head -1 || true)"
      if [[ -n "$hit" ]]; then leaked="$hit"; break; fi
    done
    if [[ -n "$leaked" ]]; then
      echo "build-native: libmain.so still embeds a device diagnostic path: $leaked" >&2
      exit 5
    fi
    fallback="$(grep -a -c -F "$ROOT" "$LIB" || true)"
    if [[ "${fallback:-0}" -gt 0 ]]; then
      echo "build-native: NOTE ${fallback} non-diagnostic CARGO_MANIFEST_DIR fallback strings remain (crate-source owned):"
      strings "$LIB" | grep -F "$ROOT" | head -2 || true
    fi
    "$PY" - "$STAMP" "$before" "$TARGET" "$CARGO_CONFIG" "$LIB" <<'PY'
import hashlib, json, subprocess, sys, time
from pathlib import Path

stamp, inputs, target, cargo_config, lib = sys.argv[1:6]
lib_path = Path(lib)
digest = hashlib.sha256()
with lib_path.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1 << 20), b""):
        digest.update(chunk)
config_digest = hashlib.sha256(Path(cargo_config).read_bytes()).hexdigest()
rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
head = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
payload = {
    "schema": "wc3rust-libmain-inputs-v2",
    "target": target,
    "inputs_sha256": inputs,
    "rustflags_source": ".cargo/config.toml",
    "rustflags_sha256": config_digest,
    "remap_config": True,
    "rustc": rustc,
    "lib_sha256": digest.hexdigest(),
    "lib_bytes": lib_path.stat().st_size,
    "head": head,
    "built_at_unix": int(time.time()),
}
Path(stamp).write_text(json.dumps(payload, indent=2) + "\n")
print(f"build-native: stamp {stamp}")
print(f"LIB_SHA256 {digest.hexdigest()} {lib_path.stat().st_size} {lib_path}")
PY
    echo "== build-native ok: $LIB"
    exit 0
done

echo "build-native: cargo build failed after $attempts attempts" >&2
exit 4

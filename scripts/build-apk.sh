#!/usr/bin/env bash
# Build the wc3rust Android P0 APK.
#
# Native:  libSDL3.so (Android/arm64, built by CMake) + libmain.so (Rust cdylib,
#          SDL_main -> wgpu Vulkan surface) + libiconv/libc++_shared (SDL deps).
# Java:    SDL3's SDLActivity layer, compiled with javac + d8.
# Package: aapt2 -> deterministic zip -> zipalign -> apksigner.
#
# Build types (AUD-014): this project is packaged by aapt2, not Gradle, so the
# build type selects aapt2's `--debug-mode`, the packaging equivalent of the
# Gradle `debug` build type. The main manifest must never hardcode
# `android:debuggable`: `release` links without the flag (attribute absent =
# false) and `debug` links with it. The compiled APK is asserted after linking
# and the release security summary re-reads the APK (AUD-029).
#
# Release identity (K-01/K-02/K-03/K-16):
#   * libmain.so must carry a stamp from scripts/build-native.sh and its input
#     digest must match the sources present now (contents, not mtimes);
#   * SDL3/loader/keystore/Termux libs are verified against scripts/build-deps.json;
#   * the ZIP is rewritten with a fixed timestamp (SOURCE_DATE_EPOCH = HEAD
#     commit time), so two builds of the same tree are byte-identical;
#   * build/android-p0/release/build-manifest.json records HEAD, the dirty
#     source digest, the input digest and the APK sha256. scripts/qa/final_report.py
#     refuses a release whose manifest, tree and installed package disagree.
#
# Usage: scripts/build-apk.sh [--release|--debug]
# Env:   ANDROID_JAR  path to android.jar (auto-detected for Termux builds)
set -euo pipefail

BUILD_TYPE=release
for arg in "$@"; do
  case "$arg" in
    --release) BUILD_TYPE=release ;;
    --debug) BUILD_TYPE=debug ;;
    -h|--help)
      sed -n '2,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (expected --release or --debug)" >&2
      exit 2
      ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PY=python3
fail() { echo "BUILD_APK_FAIL: $*" >&2; exit 2; }

ANDROID_JAR="${ANDROID_JAR:-}"
if [[ -z "$ANDROID_JAR" ]]; then
  for candidate in \
    "$PREFIX/tmp/opencode/android-sdk/android-36/android.jar" \
    "$PREFIX/tmp/opencode/android-sdk/android.jar" \
    "$HOME/src/xray-mgr-android/android-35/android.jar"; do
    if [[ -f "$candidate" ]]; then ANDROID_JAR="$candidate"; break; fi
  done
fi
if [[ -z "${ANDROID_JAR:-}" || ! -f "$ANDROID_JAR" ]]; then
  fail "android.jar not found; set ANDROID_JAR"
fi

APP="$ROOT/apps/android/app/src/main"
SDL_LIB="$ROOT/third_party/sdl3-android-build/libSDL3.so"
MAIN_LIB="$ROOT/target/release/libmain.so"
STAMP="$ROOT/target/release/.libmain-inputs.json"
BUILD="$ROOT/build/android-p0/$BUILD_TYPE"
KEYSTORE="$ROOT/apps/android/debug.keystore"

# The debug flag belongs to the build type, never to the source manifest.
if grep -q 'android:debuggable' "$APP/AndroidManifest.xml"; then
  fail "main AndroidManifest.xml must not set android:debuggable; use --debug/--release"
fi

for tool in aapt2 javac d8 zipalign apksigner zip sha256sum python3 git; do
  command -v "$tool" >/dev/null || fail "missing tool: $tool"
done
[[ -f "$SDL_LIB" ]] || fail "missing $SDL_LIB (build SDL3 for Android first)"
[[ -f "$MAIN_LIB" ]] || fail "missing $MAIN_LIB (run scripts/build-native.sh)"
[[ -f "$KEYSTORE" ]] || fail "missing $KEYSTORE (pinned in scripts/build-deps.json; restore the audited debug keystore)"

echo "== verify prebuilt inputs (scripts/build-deps.json)"
"$PY" scripts/build_deps.py verify || fail "prebuilt input mismatch (K-16); see scripts/build-deps.json"

# ---------------------------------------------------------------- libmain stamp
# The stamp binds the packaged .so to the exact input inventory. Without it the
# old guard could only compare mtimes (copyable, and tripped by examples/).
echo "== verify libmain.so input stamp"
native_digest="$("$PY" scripts/build_inputs.py native)"
if [[ -f "$STAMP" ]]; then
  "$PY" - "$STAMP" "$native_digest" "$MAIN_LIB" <<'PY' || fail "libmain.so was built from different inputs; run scripts/build-native.sh"
import hashlib, json, sys
from pathlib import Path
stamp_path, current, lib = sys.argv[1:4]
stamp = json.loads(Path(stamp_path).read_text())
errors = []
if stamp.get("inputs_sha256") != current:
    errors.append(f"inputs stamp={stamp.get('inputs_sha256')} current={current}")
if not stamp.get("remap_config") or stamp.get("rustflags_source") != ".cargo/config.toml":
    errors.append("stamp was not produced with the .cargo/config.toml path remap (K-03): rerun scripts/build-native.sh")
elif stamp.get("rustflags_sha256") != hashlib.sha256(Path(".cargo/config.toml").read_bytes()).hexdigest():
    errors.append(".cargo/config.toml changed after libmain.so was built: rerun scripts/build-native.sh")
digest = hashlib.sha256()
with open(lib, "rb") as handle:
    for chunk in iter(lambda: handle.read(1 << 20), b""):
        digest.update(chunk)
if stamp.get("lib_sha256") != digest.hexdigest():
    errors.append(f"libmain sha={digest.hexdigest()} stamp={stamp.get('lib_sha256')}")
if errors:
    for error in errors:
        print(f"STAMP_MISMATCH {error}", file=sys.stderr)
    raise SystemExit(1)
print(f"STAMP_OK inputs={current[:12]} lib={stamp['lib_sha256'][:12]} rustc={stamp.get('rustc')}")
PY
else
  echo "STAMP_ABSENT: falling back to a precise mtime check (run scripts/build-native.sh to record provenance)"
  newer="$("$PY" scripts/build_inputs.py native --newer-than "$MAIN_LIB" || true)"
  if [[ -n "$newer" && "$newer" != "[]" ]]; then
    fail "libmain.so is older than its build inputs: $newer"
  fi
fi

rm -rf "$BUILD"
mkdir -p "$BUILD/classes" "$BUILD/dex" "$BUILD/apk/lib/arm64-v8a" "$BUILD/apk-assets"

# ---------------------------------------------------------------- release identity
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
"$PY" scripts/build_inputs.py source-state --json > "$BUILD/source-state.json"
"$PY" scripts/build_inputs.py packaging --json > "$BUILD/packaging-inputs.json"
"$PY" scripts/build_inputs.py native --json > "$BUILD/native-inputs.json"
packaging_digest="$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["digest"])' "$BUILD/packaging-inputs.json")"
"$PY" - "$BUILD/apk-assets/build-identity.json" "$BUILD/source-state.json" "$native_digest" "$packaging_digest" "$BUILD_TYPE" "$SOURCE_DATE_EPOCH" <<'PY'
import json, sys
from pathlib import Path
out, state_path, native, packaging, build_type, epoch = sys.argv[1:7]
state = json.loads(Path(state_path).read_text())
payload = {
    "schema": "wc3rust-build-identity-v1",
    "head": state["head"],
    "source_dirty_digest": state["source_dirty_digest"],
    "dirty_count": state["dirty_count"],
    "native_inputs_sha256": native,
    "packaging_inputs_sha256": packaging,
    "build_type": build_type,
    "source_date_epoch": int(epoch),
}
Path(out).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY

echo "== aapt2 compile/link ($BUILD_TYPE)"
aapt2 compile --dir "$APP/res" -o "$BUILD/res.zip"
LINK_ARGS=(
  -o "$BUILD/base.apk"
  -I "$ANDROID_JAR"
  --manifest "$APP/AndroidManifest.xml"
  -A "$BUILD/apk-assets"
  -R "$BUILD/res.zip"
  --auto-add-overlay
  --min-sdk-version 24
  --target-sdk-version 35
)
if [[ "$BUILD_TYPE" == debug ]]; then
  LINK_ARGS+=(--debug-mode)
fi
aapt2 link "${LINK_ARGS[@]}"

# Read the flag back from the linked artifact (AUD-014): the merged manifest is
# the only source of truth, not the XML on disk.
MANIFEST_TREE="$(aapt2 dump xmltree --file AndroidManifest.xml "$BUILD/base.apk")"
if [[ "$BUILD_TYPE" == debug ]]; then
  grep -q ':debuggable([^)]*)=true' <<<"$MANIFEST_TREE" \
    || fail "debug build lost android:debuggable in the merged manifest"
else
  if grep -q ':debuggable(' <<<"$MANIFEST_TREE"; then
    fail "release build must not contain android:debuggable"
  fi
fi
BADGING_FULL="$(aapt2 dump badging "$BUILD/base.apk")"
BADGING="${BADGING_FULL%%$'\n'*}"
echo "MANIFEST $BADGING"

echo "== javac"
mapfile -t JAVA_SOURCES < <(find "$APP/java" -name '*.java')
javac -source 8 -target 8 -Xlint:-options -classpath "$ANDROID_JAR" \
  -d "$BUILD/classes" "${JAVA_SOURCES[@]}"

echo "== d8"
mapfile -t CLASS_FILES < <(find "$BUILD/classes" -name '*.class')
d8 --min-api 24 --lib "$ANDROID_JAR" --output "$BUILD/dex" "${CLASS_FILES[@]}"

echo "== stage libs"
cp "$MAIN_LIB" "$BUILD/apk/lib/arm64-v8a/libmain.so"
cp "$SDL_LIB" "$BUILD/apk/lib/arm64-v8a/libSDL3.so"
cp -L "$PREFIX/lib/libiconv.so" "$BUILD/apk/lib/arm64-v8a/libiconv.so"
cp -L "$PREFIX/lib/libc++_shared.so" "$BUILD/apk/lib/arm64-v8a/libc++_shared.so"
# Khronos loader that honours VK_ICD_FILENAMES, so a user Turnip ICD is used
# instead of the vendor driver. Falls back cleanly if the app ships without it.
# The Vulkan shim loads the stable Turnip ICD and exposes the Android surface
# extensions. It must live in the app's native lib dir: wgpu/ash loads
# "libvulkan.so" independently of SDL and otherwise picks the system loader.
if [[ -f "$ROOT/target/release/libturnip_vendor.so" ]]; then
  cp "$ROOT/target/release/libturnip_vendor.so" "$BUILD/apk/lib/arm64-v8a/libvulkan.so"
else
  cp "$ROOT/third_party/vulkan-loader/libvulkan.so" "$BUILD/apk/lib/arm64-v8a/libvulkan.so"
fi
# Turnip ICD dependencies (bionic builds from Termux). The loader finds the
# ICD in the app files dir and resolves these from the app's native lib dir.
for lib in libdrm.so libzstd.so.1 libz.so.1; do
  if [[ -f "$PREFIX/lib/$lib" ]]; then
    cp -L "$PREFIX/lib/$lib" "$BUILD/apk/lib/arm64-v8a/$lib"
  fi
done

# Deterministic archive (K-03): fixed entry order/timestamps, fixed permissions.
echo "== package (SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH)"
PACK_JSON="$("$PY" scripts/apk_pack.py \
  --base "$BUILD/base.apk" \
  --dex "$BUILD/dex/classes.dex" \
  --lib-dir "$BUILD/apk/lib" \
  --out "$BUILD/unsigned.apk" \
  --epoch "$SOURCE_DATE_EPOCH")"
echo "PACK $PACK_JSON"

echo "== zipalign/sign"
zipalign -f 4 "$BUILD/unsigned.apk" "$BUILD/aligned.apk"
apksigner sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --out "$BUILD/wc3rust-p0.apk" "$BUILD/aligned.apk"
apksigner verify --verbose "$BUILD/wc3rust-p0.apk" > "$BUILD/verify.txt"
head -6 "$BUILD/verify.txt"
apksigner verify --print-certs "$BUILD/wc3rust-p0.apk" > "$BUILD/certs.txt"
CERT_LINE="$(grep -m1 'SHA-256 digest' "$BUILD/certs.txt" || true)"
CERT_SHA="$(sed -n 's/.*SHA-256 digest: //p' <<<"$CERT_LINE" | tr -d ' ')"
APK_SHA="$(sha256sum "$BUILD/wc3rust-p0.apk" | awk '{print $1}')"
APK_BYTES="$(stat -c %s "$BUILD/wc3rust-p0.apk")"
LIB_SHA="$(sha256sum "$MAIN_LIB" | awk '{print $1}')"
SQL_SHA="$(sha256sum "$SDL_LIB" | awk '{print $1}')"
VK_SHA="$(sha256sum "$BUILD/apk/lib/arm64-v8a/libvulkan.so" | awk '{print $1}')"
DEPS_SHA="$(sha256sum scripts/build-deps.json | awk '{print $1}')"

# The manifest is the release evidence K-01 requires: source identity + input
# digests + the exact artifact bytes. final_report.py compares it with the tree
# and with whatever package the device reports installed.
"$PY" - "$BUILD/build-manifest.json" "$BUILD/source-state.json" "$native_digest" \
  "$packaging_digest" "$APK_SHA" "$APK_BYTES" "$LIB_SHA" "$SQL_SHA" "$VK_SHA" \
  "$CERT_SHA" "$BUILD_TYPE" "$SOURCE_DATE_EPOCH" "$DEPS_SHA" "$BADGING" "$PACK_JSON" \
  "$BUILD/apk/lib/arm64-v8a" <<'PY'
import hashlib, json, sys, time
from pathlib import Path

(out, state_path, native, packaging, apk_sha, apk_bytes, lib_sha, sdl_sha,
 vk_sha, cert_sha, build_type, epoch, deps_sha, badging, pack_json, lib_dir) = sys.argv[1:18]
state = json.loads(Path(state_path).read_text())
pack = json.loads(pack_json)
libs = {}
for path in sorted(Path(lib_dir).iterdir()):
    if not path.is_file():
        continue
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    libs[path.name] = {"sha256": digest.hexdigest(), "bytes": path.stat().st_size}
payload = {
    "schema": "wc3rust-build-manifest-v2",
    "generated_at_unix": int(time.time()),
    "build_type": build_type,
    "source": {
        "head": state["head"],
        "dirty_count": state["dirty_count"],
        "tracked_dirty_count": state["tracked_dirty_count"],
        "untracked_input_count": state["untracked_input_count"],
        "source_dirty_digest": state["source_dirty_digest"],
        "digest_scope": "tracked modifications/deletions + untracked files that are build inputs",
        "dirty_paths": state["dirty_paths"],
        "untracked_input_paths": state["untracked_input_paths"],
    },
    "inputs": {
        "native_sha256": native,
        "packaging_sha256": packaging,
        "libmain_sha256": lib_sha,
        "libmain_stamp": "target/release/.libmain-inputs.json",
    },
    "artifact": {
        "path": "build/android-p0/%s/wc3rust-p0.apk" % build_type,
        "sha256": apk_sha,
        "bytes": int(apk_bytes),
        "libs": libs,
        "libs_note": "K-16: every untracked .so packaged into the APK, by content",
        "signer_cert_sha256": cert_sha,
        "manifest_badging": badging,
        "install": None,
    },
    "packaging": {
        "source_date_epoch": int(epoch),
        "zip_entries": pack.get("entries"),
        "unsigned_sha256": pack.get("sha256"),
        "build_deps_sha256": deps_sha,
    },
}
Path(out).write_text(json.dumps(payload, indent=2) + "\n")
print("MANIFEST " + out)
PY

echo "APK: $BUILD/wc3rust-p0.apk"
ls -lh "$BUILD/wc3rust-p0.apk"
echo "RELEASE_HEAD $(git rev-parse HEAD)"
"$PY" - "$BUILD/build-manifest.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
print(
    "RELEASE_DIRTY count=%d tracked=%d untracked_inputs=%d source_dirty_digest=%s"
    % (
        m["source"]["dirty_count"],
        m["source"]["tracked_dirty_count"],
        m["source"]["untracked_input_count"],
        m["source"]["source_dirty_digest"],
    )
)
PY
echo "RELEASE_APK sha256=$APK_SHA bytes=$APK_BYTES"
echo "RELEASE_INSTALLED sha256=not-verified-by-build (device evidence is verified by scripts/qa/final_report.py)"

# AUD-029: the release security summary is generated from the signed APK and
# printed to stdout (CI log) as well as written under dist/.
echo "== release security summary ($BUILD_TYPE)"
"$PY" "$ROOT/scripts/release_security_summary.py" \
  --apk "$BUILD/wc3rust-p0.apk" \
  --build-type "$BUILD_TYPE" \
  --dist "$ROOT/dist"

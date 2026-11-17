#!/usr/bin/env bash
# Build the wc3rust Android P0 APK.
#
# Native:  libSDL3.so (Android/arm64, built by CMake) + libmain.so (Rust cdylib,
#          SDL_main -> wgpu Vulkan surface) + libiconv/libc++_shared (SDL deps).
# Java:    SDL3's SDLActivity layer, compiled with javac + d8.
# Package: aapt2 -> zip -> zipalign -> apksigner.
#
# Build types (AUD-014): this project is packaged by aapt2, not Gradle, so the
# build type selects aapt2's `--debug-mode`, the packaging equivalent of the
# Gradle `debug` build type. The main manifest must never hardcode
# `android:debuggable`: `release` links without the flag (attribute absent =
# false) and `debug` links with it. The compiled APK is asserted after linking
# and the release security summary re-reads the APK (AUD-029).
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
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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

ANDROID_JAR="${ANDROID_JAR:-}"
if [[ -z "$ANDROID_JAR" ]]; then
  for candidate in \
    "$PREFIX/tmp/opencode/android-sdk/android-36/android.jar" \
    "$PREFIX/tmp/opencode/android-sdk/android.jar"; do
    if [[ -f "$candidate" ]]; then ANDROID_JAR="$candidate"; break; fi
  done
fi
if [[ -z "${ANDROID_JAR:-}" || ! -f "$ANDROID_JAR" ]]; then
  echo "android.jar not found; set ANDROID_JAR" >&2
  exit 2
fi

APP="$ROOT/apps/android/app/src/main"
SDL_LIB="$ROOT/third_party/sdl3-android-build/libSDL3.so"
MAIN_LIB="$ROOT/target/release/libmain.so"
BUILD="$ROOT/build/android-p0/$BUILD_TYPE"
KEYSTORE="$ROOT/apps/android/debug.keystore"

# The debug flag belongs to the build type, never to the source manifest.
if grep -q 'android:debuggable' "$APP/AndroidManifest.xml"; then
  echo "main AndroidManifest.xml must not set android:debuggable; use --debug/--release" >&2
  exit 2
fi

for tool in aapt2 javac d8 zipalign apksigner zip keytool; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 2; }
done
[[ -f "$SDL_LIB" ]] || { echo "missing $SDL_LIB (build SDL3 for Android first)" >&2; exit 2; }
[[ -f "$MAIN_LIB" ]] || { echo "missing $MAIN_LIB (cargo build -p wc3rust-android --release)" >&2; exit 2; }
# Never package a stale native library: a failed `cargo build` used to leave the
# previous libmain.so in place and the APK shipped old code.
stale_rs="$(find crates apps -name '*.rs' -newer "$MAIN_LIB" -print -quit 2>/dev/null || true)"
stale_toml="$(find . -maxdepth 3 -name 'Cargo.toml' -newer "$MAIN_LIB" -print -quit 2>/dev/null || true)"
if [[ -n "$stale_rs" || -n "$stale_toml" ]]; then
  echo "libmain.so is older than the Rust sources ($stale_rs $stale_toml); rebuild with cargo build -p wc3rust-android --release" >&2
  exit 2
fi

rm -rf "$BUILD"
mkdir -p "$BUILD/classes" "$BUILD/dex" "$BUILD/apk/lib/arm64-v8a"

echo "== aapt2 compile/link ($BUILD_TYPE)"
aapt2 compile --dir "$APP/res" -o "$BUILD/res.zip"
LINK_ARGS=(
  -o "$BUILD/base.apk"
  -I "$ANDROID_JAR"
  --manifest "$APP/AndroidManifest.xml"
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
    || { echo "debug build lost android:debuggable in the merged manifest" >&2; exit 2; }
else
  if grep -q ':debuggable(' <<<"$MANIFEST_TREE"; then
    echo "release build must not contain android:debuggable" >&2
    exit 2
  fi
fi

echo "== javac"
mapfile -t JAVA_SOURCES < <(find "$APP/java" -name '*.java')
javac -source 8 -target 8 -Xlint:-options -classpath "$ANDROID_JAR" \
  -d "$BUILD/classes" "${JAVA_SOURCES[@]}"

echo "== d8"
mapfile -t CLASS_FILES < <(find "$BUILD/classes" -name '*.class')
d8 --min-api 24 --lib "$ANDROID_JAR" --output "$BUILD/dex" "${CLASS_FILES[@]}"

echo "== package"
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
elif [[ -f "$ROOT/third_party/vulkan-loader/libvulkan.so" ]]; then
  cp "$ROOT/third_party/vulkan-loader/libvulkan.so" "$BUILD/apk/lib/arm64-v8a/libvulkan.so"
fi
# Turnip ICD dependencies (bionic builds from Termux). The loader finds the
# ICD in the app files dir and resolves these from the app's native lib dir.
for lib in libdrm.so libzstd.so.1 libz.so.1; do
  if [[ -f "$PREFIX/lib/$lib" ]]; then
    cp -L "$PREFIX/lib/$lib" "$BUILD/apk/lib/arm64-v8a/$lib"
  fi
done
cp "$BUILD/base.apk" "$BUILD/unsigned.apk"
cp "$BUILD/dex/classes.dex" "$BUILD/apk/classes.dex"
( cd "$BUILD/apk" && zip -q -r "$BUILD/unsigned.apk" classes.dex lib )

echo "== zipalign/sign"
zipalign -f 4 "$BUILD/unsigned.apk" "$BUILD/aligned.apk"
if [[ ! -f "$KEYSTORE" ]]; then
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android \
    -alias androiddebugkey -dname "CN=Android Debug,O=Android,C=US" \
    -keyalg RSA -keysize 2048 -validity 10000 >/dev/null 2>&1
fi
apksigner sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --out "$BUILD/wc3rust-p0.apk" "$BUILD/aligned.apk"
apksigner verify --verbose "$BUILD/wc3rust-p0.apk" | head -6

echo "APK: $BUILD/wc3rust-p0.apk"
ls -lh "$BUILD/wc3rust-p0.apk"

# AUD-029: the release security summary is generated from the signed APK and
# printed to stdout (CI log) as well as written under dist/.
echo "== release security summary ($BUILD_TYPE)"
python3 "$ROOT/scripts/release_security_summary.py" \
  --apk "$BUILD/wc3rust-p0.apk" \
  --build-type "$BUILD_TYPE" \
  --dist "$ROOT/dist"

#!/usr/bin/env bash
# Install the aebuilder-built Mesa Turnip ICD and the wc3 Vulkan loader shim
# into the app's private files. Run as the Termux user (uses su).
#
# The shim (crates/wc3-vulkan-loader) opens the AOSP `libvulkan.so` through the
# Rust libadrenotools port and redirects its ICD open to Turnip. The AOSP loader
# provides VK_KHR_surface/VK_KHR_android_surface; Turnip provides the device.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="org.herofolends.p0"
APP_DIR="/data/data/$PKG/files"
TURNIP_DIR="$APP_DIR/turnip"
MESA="${MESA_SOURCE:-$HOME/aeBuilDer cpp/mesa-source}"
DRIVER="$MESA/build-aebuilder/src/freedreno/vulkan/libvulkan_freedreno.so"
SHIM="$ROOT/target/release/libvulkan.so"
TMP="$PREFIX/tmp/turnip-install-$$"

[[ -f "$DRIVER" ]] || { echo "missing Turnip driver: $DRIVER" >&2; exit 2; }
[[ -f "$SHIM" ]] || { echo "missing shim; build: cargo build --release -p wc3-vulkan-loader" >&2; exit 2; }
for hook in libmain_hook.so libfile_redirect_hook.so libgsl_alloc_hook.so; do
  [[ -f "$ROOT/target/release/$hook" ]] || { echo "missing $hook; build the three hook crates" >&2; exit 2; }
done

appuid=$(su -c "stat -c %u /data/data/$PKG")
mkdir -p "$TMP"
cp "$DRIVER" "$TMP/libvulkan_freedreno.so"
patchelf --set-rpath '$ORIGIN' "$TMP/libvulkan_freedreno.so"
cp "$SHIM" "$TMP/libvulkan.so"
cp "$ROOT/target/release/libmain_hook.so" "$ROOT/target/release/libfile_redirect_hook.so" \
   "$ROOT/target/release/libgsl_alloc_hook.so" "$TMP/"

# Turnip's direct dependencies: system libs that are not in the app namespace,
# plus the Termux-built bionic ones.
declare -a SYSTEM_LIBS=(
  /system/lib64/libhardware.so
  /system/lib64/libcutils.so
  /system/lib64/libvndksupport.so
  /system/lib64/libbase.so
  /system/lib64/libc++.so
  /apex/com.android.runtime/lib64/bionic/libdl_android.so
  /system/lib64/ld-android.so
)
for lib in "${SYSTEM_LIBS[@]}"; do
  name=$(basename "$lib")
  su -c "cp '$lib' '$TMP/$name'; chown 10394:10394 '$TMP/$name'"
  patchelf --set-rpath '$ORIGIN' "$TMP/$name" || true
done
for lib in libzstd.so.1 libdrm.so libz.so.1 libc++_shared.so; do
  [[ -f "$PREFIX/lib/$lib" ]] && cp -L "$PREFIX/lib/$lib" "$TMP/$lib"
done
cat > "$TMP/freedreno_icd.aarch64.json" <<EOF
{
    "ICD": {
        "api_version": "1.4.363",
        "library_arch": "64",
        "library_path": "$TURNIP_DIR/libvulkan_freedreno.so"
    },
    "file_format_version": "1.0.1"
}
EOF

su -c "mkdir -p '$TURNIP_DIR'"
su -c "cp '$TMP'/* '$TURNIP_DIR/'"
su -c "chown -R $appuid:$appuid '$TURNIP_DIR'; chmod 755 '$TURNIP_DIR'; chmod 644 '$TURNIP_DIR'/*"
echo "installed Turnip + shim for $PKG:"
su -c "ls -la '$TURNIP_DIR'"
echo "start the app and verify: adb logcat | grep -E 'GAME_VULKAN|P0_GPU_ADAPTER'"

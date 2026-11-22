#!/data/data/com.termux/files/usr/bin/bash
# Deliver the HUD pack to the installed app.
#
# The game loads its skin from <app files>/hud_pack (see
# crates/wc3-game/src/hudskin/pack.rs). If that directory is missing the skin
# falls back to the FDF art: numeral-baked control-group tiles and the attack
# cursor leak into the console (device 2026-11-22). This copies the final pack
# with the app's uid so the process can read it.
#
# Usage: scripts/install-hud-pack.sh [--release]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="${H2_ANDROID_PACKAGE:-org.wc3rust.p0}"
SRC="$ROOT/analysis/hudskin/final/wc3rust-hud-final"
DST="/data/data/$PKG/files/hud_pack"

[ -f "$SRC/manifest.json" ] || { echo "pack missing: $SRC/manifest.json" >&2; exit 2; }

UID_APP=$(su -c "stat -c %u /data/data/$PKG/files" 2>/dev/null || true)
[ -n "${UID_APP:-}" ] || { echo "cannot resolve $PKG data uid (is it installed?)" >&2; exit 2; }

su -c "rm -rf '$DST'" 2>/dev/null || true
su -c "mkdir -p '$DST'"
su -c "cp -r '$SRC/.' '$DST/'"
su -c "chown -R $UID_APP:$UID_APP '$DST'"
su -c "chmod -R u+rwX,go-rwx '$DST'"

if su -c "test -r '$DST/manifest.json'"; then
  echo "HUD_PACK_INSTALLED pkg=$PKG uid=$UID_APP dir=$DST"
else
  echo "HUD_PACK_NOT_READABLE dir=$DST" >&2
  exit 1
fi

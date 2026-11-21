#!/data/data/com.termux/files/usr/bin/bash
# Collect device evidence for the audio/rendering/state acceptance rows.
#
# Device access is gated: the script refuses to touch the device unless
# H2_DEVICE_SLOT=1 is exported (the tech-lead grants the slot). Without the
# slot it exits rc=3 and writes NOTHING, so a missing grant can never look like
# a successful device run.
#
# With the slot it:
#   1. installs build/android-p0/release/wc3rust-p0.apk (pm install -r),
#   2. launches org.wc3rust.p0 and waits for GAME_FIRST_FRAME,
#   3. collects GAME_SCENARIO / GAME_READY / GAME_STATE / GAME_AUDIO_STATS,
#   4. records the newest screenshot path if one exists in
#      /storage/emulated/0/Pictures/Screenshots,
#   5. stops the app and returns the device to Termux,
#   6. writes analysis/tech-lead/device_audio.log and device_run.json.
#
# Usage: H2_DEVICE_SLOT=1 bash scripts/qa/device_evidence.sh [--apk PATH] [--seconds N]

set -u
cd "$(dirname "$0")/../.." || exit 2

APK=build/android-p0/release/wc3rust-p0.apk
SECONDS_TO_COLLECT=12
while [ $# -gt 0 ]; do
    case "$1" in
        --apk) shift; APK="${1:-$APK}" ;;
        --seconds) shift; SECONDS_TO_COLLECT="${1:-$SECONDS_TO_COLLECT}" ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

OUT_DIR=analysis/tech-lead
AUDIO_LOG="$OUT_DIR/device_audio.log"
RUN_JSON="$OUT_DIR/device_run.json"
PKG=org.wc3rust.p0
COMPONENT="$PKG/org.libsdl.app.SDLActivity"

if [ "${H2_DEVICE_SLOT:-}" != "1" ]; then
    echo "DEVICE_SLOT_ABSENT: H2_DEVICE_SLOT != 1 (no device access, nothing written)" >&2
    exit 3
fi
if [ ! -f "$APK" ]; then
    echo "APK_MISSING $APK" >&2
    exit 2
fi

su_() { su -c "$1"; }
sha=$(sha256sum "$APK" | awk '{print $1}')
started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
start_epoch=$(date +%s)

install_out=$(su_ "pm install -r $(pwd)/$APK" 2>&1)
install_rc=$?
if [ "$install_rc" != 0 ] || ! printf '%s' "$install_out" | grep -q Success; then
    python3 - "$RUN_JSON" "$sha" "$started" "$install_out" <<'PY'
import json, sys, time
path, sha, started, out = sys.argv[1:5]
json.dump({"schema": "wc3-device-run-v1", "status": "FAIL", "phase": "install",
           "apk_sha256": sha, "started_at": started, "generated_unix": int(time.time()),
           "error": out[-500:]}, open(path, "w"), indent=2)
PY
    echo "DEVICE_INSTALL_FAILED" >&2
    exit 1
fi

# K-01: the installed base.apk must be byte-identical to the artifact.
installed_sha=$(su_ "sha256sum $(su_ "pm path $PKG" | sed -n 's/^package://p' | head -1)" | awk '{print $1}')
if [ "$installed_sha" != "$sha" ]; then
    python3 - "$RUN_JSON" "$sha" "$installed_sha" "$started" <<'PY'
import json, sys, time
path, sha, installed, started = sys.argv[1:5]
json.dump({"schema": "wc3-device-run-v1", "status": "FAIL", "phase": "install_identity",
           "apk_sha256": sha, "installed_sha256": installed, "started_at": started,
           "generated_unix": int(time.time()),
           "error": "installed base.apk differs from the built artifact"},
          open(path, "w"), indent=2)
PY
    echo "DEVICE_INSTALL_IDENTITY_FAILED installed=$installed_sha built=$sha" >&2
    exit 1
fi

launch_out=$(su_ "am start -W -n $COMPONENT" 2>&1)
launch_rc=$?

first_frame=""
deadline=$(( $(date +%s) + 40 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
    first_frame=$(su_ "logcat -d -s wc3rust" 2>/dev/null | grep -m1 'GAME_FIRST_FRAME' || true)
    [ -n "$first_frame" ] && break
    sleep 1
done

sleep "$SECONDS_TO_COLLECT"
log_dump=$(su_ "logcat -d -s wc3rust" 2>/dev/null || true)
printf '%s\n' "$log_dump" | grep -E 'GAME_SCENARIO|GAME_READY|GAME_STATE|GAME_AUDIO_STATS|GAME_FIRST_FRAME|GAME_UI_FDF' > "$AUDIO_LOG"

screenshot=""
# K-12: only a screenshot captured after this run started may be attached.
# `ls -t | head -1` picked the newest image of any app, including an old run.
newest=$(find /storage/emulated/0/Pictures/Screenshots -maxdepth 1 -type f \
    \( -name '*.png' -o -name '*.jpg' \) -newermt "@$start_epoch" 2>/dev/null \
    | sort | tail -1 || true)
if [ -n "$newest" ] && [ -f "$newest" ]; then
    screenshot="$newest"
fi

su_ "am force-stop $PKG" >/dev/null 2>&1 || true
su_ "am start -W -n com.termux/com.termux.app.TermuxActivity" >/dev/null 2>&1 || true
focus=$(su_ "dumpsys window" 2>/dev/null | grep mCurrentFocus | head -1 || true)

python3 - "$RUN_JSON" "$AUDIO_LOG" "$sha" "$started" "$first_frame" "$screenshot" "$focus" \
        "$install_rc" "$launch_rc" "$APK" "$installed_sha" <<'PY'
import json, sys, time
path, audio_log, sha, started, first_frame, screenshot, focus, install_rc, launch_rc, apk, installed = sys.argv[1:12]
lines = [line for line in open(audio_log, encoding="utf-8", errors="replace").read().splitlines() if line.strip()]
stats = [line for line in lines if "GAME_AUDIO_STATS" in line]
underruns = []
for line in stats:
    for token in line.split():
        if token.startswith("underruns="):
            try: underruns.append(int(token.split("=", 1)[1]))
            except ValueError: underruns.append(-1)
ok = bool(first_frame) and bool(stats) and all(value == 0 for value in underruns)
payload = {
    "schema": "wc3-device-run-v1",
    "status": "PASS" if ok else "FAIL",
    "generated_unix": int(time.time()),
    "started_at": started,
    "apk": apk,
    "apk_sha256": sha,
    "installed_sha256": installed,
    "install_identity_match": installed == sha,
    "package": "org.wc3rust.p0",
    "install_rc": int(install_rc),
    "launch_rc": int(launch_rc),
    "first_frame": first_frame or None,
    "audio_stats_lines": len(stats),
    "underruns": underruns,
    "screenshot": screenshot or None,
    "screenshot_fresh": bool(screenshot),
    "focus_after": focus,
    "log": audio_log,
    "lines_collected": len(lines),
}
json.dump(payload, open(path, "w"), indent=2)
print("DEVICE_EVIDENCE_" + ("PASS" if ok else "FAIL"))
sys.exit(0 if ok else 1)
PY

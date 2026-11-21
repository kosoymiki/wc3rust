#!/usr/bin/env bash
# Release APK gate (AUD-014/AUD-029, lane 510).
#
# The release APK is the artifact acceptance is judged on, so its security flags
# are a gate, not a log line. Every value below is read back from the *signed
# APK* (compiled manifest + signing block + zip entries) by
# `scripts/release_security_summary.py`; the source manifest is checked only to
# prove it never hardcodes `android:debuggable`.
#
# Asserted:
#   * the source AndroidManifest.xml has no `android:debuggable` (the flag
#     belongs to the build type, `--release`/`--debug` of scripts/build-apk.sh);
#   * the compiled manifest AND badging both report debuggable=false;
#   * apksigner verifies the signature;
#   * the APK ships exactly the arm64-v8a ABI and declares its permissions;
#   * the summary is deterministic: two runs over the same APK are byte-equal;
#   * the summary's sha256 equals the file's sha256 (the report is not stale).
#
# Usage: tests/gates/apk-release-gate.sh [path/to/wc3rust-p0.apk]
# Env:   H2_APK  override the APK path
#
# Exit: 0 when every assertion holds, 1 otherwise. A missing APK is a failure
# with the exact rebuild command: a gate that skips the artifact proves nothing.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

APK="${1:-${H2_APK:-$ROOT/build/android-p0/release/wc3rust-p0.apk}}"
MANIFEST="$ROOT/apps/android/app/src/main/AndroidManifest.xml"
SUMMARY_PY="$ROOT/scripts/release_security_summary.py"
TMP="$(mktemp -d "${TMPDIR:-/data/data/com.termux/files/usr/tmp}/wc3rust-apk-gate.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "APK_GATE_FAIL reason=$1 apk=$APK"
  [[ -n "${2:-}" ]] && echo "  | $2"
  exit 1
}

[[ -f "$APK" ]] || fail missing-apk "rebuild with: scripts/build-apk.sh --release (after scripts/h2-cargo build -p wc3rust-android --release)"
[[ -f "$MANIFEST" ]] || fail missing-source-manifest "$MANIFEST"
[[ -f "$SUMMARY_PY" ]] || fail missing-summary-script "$SUMMARY_PY"
command -v python3 >/dev/null || fail missing-python3

# 1. The source manifest must not hardcode the flag: build-apk.sh refuses to
#    build if it does, and this gate refuses a tree that would trip it.
if grep -q 'android:debuggable' "$MANIFEST"; then
  fail source-manifest-hardcodes-debuggable "$MANIFEST"
fi

# 2. Read the APK back twice; a report that differs between runs is not a gate.
mkdir -p "$TMP/a" "$TMP/b"
python3 "$SUMMARY_PY" --apk "$APK" --build-type release --dist "$TMP/a" >"$TMP/a/stdout.txt" 2>"$TMP/a/stderr.txt"
rc_a=$?
python3 "$SUMMARY_PY" --apk "$APK" --build-type release --dist "$TMP/b" >"$TMP/b/stdout.txt" 2>"$TMP/b/stderr.txt"
rc_b=$?
[[ $rc_a -eq 0 ]] || fail "summary-rc-$rc_a" "$(tail -3 "$TMP/a/stderr.txt")"
[[ $rc_b -eq 0 ]] || fail "summary-rc-$rc_b" "$(tail -3 "$TMP/b/stderr.txt")"
if ! diff -q "$TMP/a/security-summary-release.json" "$TMP/b/security-summary-release.json" >/dev/null; then
  fail non-deterministic-summary "two runs over the same APK produced different JSON"
fi

# 3. Assert the security facts on the generated report.
out="$(python3 - "$TMP/a/security-summary-release.json" "$APK" <<'PY'
import hashlib, json, sys

report_path, apk_path = sys.argv[1], sys.argv[2]
s = json.load(open(report_path, encoding="utf-8"))
problems = []
if s["debuggable"]["manifest"] is not False:
    problems.append(f"compiled manifest debuggable={s['debuggable']['manifest']}")
if s["debuggable"]["badging"] is not False:
    problems.append(f"badging debuggable={s['debuggable']['badging']}")
if s["signing"].get("verified") is not True:
    problems.append("apksigner did not verify the APK")
if "arm64-v8a" not in s.get("abi_filters", []):
    problems.append(f"abi_filters={s.get('abi_filters')} (expected arm64-v8a)")
if not isinstance(s.get("permissions"), list):
    problems.append("permissions missing from the report")
sha = hashlib.sha256(open(apk_path, "rb").read()).hexdigest()
if s.get("apk_sha256") != sha:
    problems.append(f"report sha256 {s.get('apk_sha256')} != file {sha}")
if not s.get("gate", {}).get("passed"):
    problems.append(f"summary gate failures={s.get('gate', {}).get('failures')}")
if problems:
    print("PROBLEMS " + " ; ".join(problems))
    sys.exit(1)
schemes = sorted(k for k, v in s["signing"].get("schemes", {}).items() if v)
perms = ",".join(s["permissions"]) or "none"
print(f"schemes={'+'.join(schemes)} abi={','.join(s['abi_filters'])} permissions={perms} sha256={sha} "
      f"warnings={len(s['gate'].get('warnings', []))}")
PY
)" || fail summary-assertion "$out"

echo "APK_GATE debuggable=manifest:false,badging:false signing=verified,$out"

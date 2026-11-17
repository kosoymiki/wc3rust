# Device cycle — ROUND7 status

Timestamp: **2026-11-13T16:41:16Z**
Current APK sha256: `70c15e91e4f5f8d4d33cb510f371c7a4929aaee5b67b8714dc32ce06a1cc245e`

Status: **BLOCKED (external: secure keyguard)**

- Reason: secure keyguard active (deviceLocked=1, trustState=UNTRUSTED); app cannot be foregrounded for evidence
- dumpsys: `deviceLocked=1`, `mDreamingLockscreen=true`, focus `NotificationShade`, wakefulness Awake
- cycle launch -> FAIL: launch failed after 3 attempts, focus is 'NotificationShade'
- input keyevent KEYCODE_BACK / KEYCODE_HOME -> focus unchanged NotificationShade
- input keyevent KEYCODE_WAKEUP + wm dismiss-keyguard -> mWakefulness=Awake, focus unchanged
- input swipe up (landscape 1386,1150 -> 1386,350, 260ms) -> focus unchanged
- cmd statusbar collapse -> focus unchanged

Last valid cycle: `analysis/agent-qa/parity/device-cycle/cycle.json` (APK `4c8b86177a99cc4147bd0c47a14b9a3ea6c7b5c0a4f51ba4a6d92a8697a4880d`) — ROUND6 evidence, not valid for the 19:38 build.

Next step: Operator unlocks the device (PIN/biometric). Then run exactly: python3 scripts/qa/device_cycle.py --apk build/android-p0/herofolends-p0.apk --out analysis/agent-qa/parity/device-cycle

Boundary: operator credential required; root does not bypass device authentication.

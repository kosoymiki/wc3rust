# THICK GOAL — Agent G: QA / device-evidence layer (ledger 591)

## Mission
Final QA gate for the whole art push: device (or honest headless) evidence at shipped 2414x1080
for every closed bug, magenta=0 regression matrix, and a merged QA report. You are the ONLY
agent allowed to use device slot H2_DEVICE_SLOT=1 during this push.

## Evidence snapshot
- Agents A (_ledger 585_), B (586) closed bugs #14/#7/#10 with named tests; B produced headless mocks because no device slot was attached.
- `analysis/agent-qa/operator-bugs/REPORT.md`: 18 rows, fixed 8 / partial 10. Need post-push rescan of #7/#10/#14.
- `scripts/visual_check.py`, `scripts/qa/**` exist; H2_DEVICE_SLOT=1 required for real device.

## Goal + done-criteria
1. Check whether a device slot is attached (`H2_DEVICE_SLOT`, `adb devices`). If attached: rebuild APK via `scripts/build-apk.sh`, install, run scenarios 09-army-panel / 05-ghost / 06-place and capture real 2414x1080 screenshots into `analysis/agent-qa/art-push-20261120/`. If NOT attached: state that as an honest boundary and verify the headless mocks A/B produced (pixel dims, magenta=0, labels "HEADLESS MOCK").
2. Magenta regression: scan all produced PNGs for magenta/purple pixel clusters; matrix PASS/FAIL per image. `magenta=0` required on world/army/ghost.
3. QA report `analysis/agent-qa/art-push-20261120/QA_REPORT.md` with per-bug verdict (device-verified | headless-only | not-verified), exact evidence paths, rc, and honest limits.

## Invariants / READ_ONLY
- OWNER: `analysis/agent-qa/art-push-20261120/**`, `scripts/qa/**`, `scripts/visual_check.py`.
- NEVER edit: any `crates/**` source, other agents' analysis dirs, `Cargo.toml`, `scripts/build-apk.sh` (read/execute only), other scripts outside OWNER.
- HONEST ART STANDARD: a headless mock is NEVER a device screenshot; label every artifact. No fabricated evidence.

## Verification
Per-image magenta counts + dims + verdict table; report with rc; checkpoint 591 each phase, close 591 with h2_ids_json. Reply H2_PERSISTED + SESSION_CLOSED with ids.

# Device status — ROUND8/P0 (NOT RUN)

Timestamp: **2026-11-13T17:57:48Z**
Current APK: `0f19ebbbf3fdbfdbb94ee7b62981078bcca21b26671423ba9c0007c39b4dfd78` (installed 2026-11-13 20:49:48)

Status: **NOT_RUN — operator is using the phone.** The tech-lead directed an immediate stop of device
launches (no `am start`, no `monkey`, no APK installs) until an explicit grant. No launch was retried after
the directive; `org.herofolends.p0` is force-stopped (rc=0) and the foreground app is `bin.mt.plus.canary`.

## Attempt history (evidence, not claims)

| # | Time | Result | Artifact |
|---|---|---|---|
| 1 | 20:41:54 | FAIL harness: fixed-fraction build points all PLACE_BLOCKED after the minimap camera jump; screenshots 01-12 present | `analysis/agent-qa/parity/p0-device/cycle-attempt1.json` |
| 2 | 20:49:55 | PARTIAL: launch, minimap, command card, build (PLACE_BUILT htow cost=450 gold=624, BUTTON BUILD), getevent PASS; control-group recall, tooltip and train unproven (harness) | `analysis/agent-qa/parity/p0-device/cycle-attempt2.json` |
| 3 | 20:52:28 | FAIL harness: UnboundLocalError button_logs (fixed); build blocked at all offsets in that session | `analysis/agent-qa/parity/p0-device/cycle-attempt3.json` |
| 4 | 20:55:22 | ABORTED: screenshot 1080x2414 does not match display 2414x1080 -> app not foreground (operator force-stopped org.herofolends.p0, rc=0); no further launch attempted | `analysis/agent-qa/parity/p0-device/cycle.json` |

## What the device evidence already proves (attempt 2, APK `0f19ebbb…`/`e45bb941…`)

- launch: `GAME_FIRST_FRAME surface=2414x1080`, `GAME_READY`; console height/target captured with screenshot;
- minimap tap moved the world view (`world_diff` 28.7-51.4, `camera_moved=true`);
- command card: `BUTTON BASE` logged; control groups: save+recall executed (`selected 6 -> 6`);
- build: `BUTTON BUILD` + `PLACE_BUILT htow cost=450` (attempt 2);
- getevent: injected `input motionevent` reaches `/dev/input/event5` (35-1354 lines per capture).

## Not yet proven on device

- control-group recall after a *different* selection (BASE selects the hall, count 1 -> recall 6);
- tooltip band appearance under press-and-hold (tooltip rect diff vs control diff);
- train from the new hall (card slot 3) on this build.

These stay **unproven**, not PASS. The harness fixes are in `scripts/qa/device_p0_cycle.py`; they need one
granted cycle on an idle phone.

Next step: Wait for an explicit tech-lead device grant on an idle phone, then run exactly: python3 scripts/qa/device_p0_cycle.py --apk build/android-p0/herofolends-p0.apk --out analysis/agent-qa/parity/p0-device

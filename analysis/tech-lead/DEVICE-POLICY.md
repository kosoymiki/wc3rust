# DEVICE POLICY — wc3rust (2026-11-18, operator order)

## Hard rule

**No agent touches the phone.** Not one command:

- `am start` / `am force-stop` / `monkey` — the app is never opened by an agent;
- `input tap|swipe|text|keyevent` — no synthetic input;
- `screencap` / `screenrecord` — no screenshots;
- `pm install|uninstall` — no installs;
- `logcat -c` — no log clearing (it destroys evidence of the operator's own runs).

The device belongs to the operator. He opens the app, he taps, he takes screenshots.
Agents read `logcat -d` **only** when the tech-lead explicitly asks for a specific query,
and never mutate device state.

## Who may run device steps

Only the tech-lead (this session), and only when the operator says the phone is free.
Even then: no `logcat -c`, no force-stop of a run the operator started.

## Why this rule exists (observed)

- `analysis/tech-lead/device_battle_walk.py` (lane F) and `device_menu_walk.py` launch the
  app with `am start`, tap with `input tap` and clear the log (`logcat -c`). Running them
  while the operator used the phone produced: focus fights with Telegram, "package churn"
  (pid 30970 → 31159), a walk that tapped into the wrong page because the layout came from
  a stale frame, and a wiped log that hid the operator's own reproduction.
- An agent that "verifies on the device" is not verifying the product: it is driving the
  operator's phone. The product verification an agent may do: `h2-cargo check` rc=0, a
  headless example (`world_frame`, `hud_mock`, `campaign_report`), a PNG, a log line.
- A device PASS/FAIL is produced by the tech-lead on request, with the operator's eyes on
  the screen, and is reported with the exact APK SHA.

## Enforcement

- Lanes that need device evidence ask the tech-lead in the report; the tech-lead schedules
  it with the operator.
- Any agent found running `am`/`input`/`screencap`/`pm` on this device has its result
  discarded and the lane re-scoped.
- The device scripts stay in the repo for the operator's own use; they are not agent tools.

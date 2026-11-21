# ROUND 8 — touch layer device evidence (control groups, box-select, hotkey gap)

Layer: touch (`r2-touch`, ledger 356). Date: 2026-11-13 19:44–19:51 MSK. Device: RMX3709, Android 16 (SDK 36), aarch64, KernelSU root (`su -c 'id'` → `uid=0`).
Packet: ROUND 8 THICK GOAL — device verification of control groups, hotkeys, box-select; PARITY_TOUCH update; product/device evidence.

## Product under test

- APK: `build/android-p0/herofolends-p0.apk`, built **19:35:09**, sha256 `70c15e91e4f5f8d4…` (contains `libmain.so` built 19:33:19, i.e. the ROUND 6/7 10-slot row).
- `su -c 'pm install -r build/android-p0/herofolends-p0.apk'` → `Success`.
- `su -c 'am start -W -n org.herofolends.p0/org.libsdl.app.SDLActivity'` → `TotalTime: 445`.
- logcat: `GAME_FULLSCREEN entered=true flag=true flags=0x10002621`, `GAME_SURFACE 2414x1080`, `GAME_SAFE_AREA left=210 top=120 right=90 bottom=96`, `GAME_FIRST_FRAME surface=2414x1080 fullscreen=true units=16 phase=Playing`.
- focus: `mCurrentFocus=Window{… u0 org.herofolends.p0/org.libsdl.app.SDLActivity}`.

## Layout on device (computed from `mobile_hud.rs` layout, scale 1.0 at 2414x1080)

- `UiScale::compute(1080).factor = 1.0`; `scale = min(2414,1080)/1080 = 1.0`.
- console = `Rect(210, 792, 2114, 288)` (`CONSOLE_HEIGHT=288`).
- Group row (`game.rs:1166`): `y = 792+12 = 804`, `size = 40`, `gap = 6`, `total = 454`, `x0 = 210 + (2114−454)/2 = 1040`; slot *i* centre = **(1060 + 46·i, 824)**, labels 1..9,0.
- Command card: `cell=64`, `gap=4`, card = `Rect(2032, 856, 268, 200)`; ARMY = card slot 1 centre **(2132, 888)**.

## Device observations (observed; screenshots + logcat)

| # | Action (adb via `su -c`) | Observation |
|---|---|---|
| 1 | `input tap 2132 888` (ARMY chip) | `GAME_STATE … selected=0` → **`selected=6`** (19:47:42); `army2.png` shows 6 workers in the selection panel |
| 2 | `input motionevent DOWN 1152 824`; `sleep 0.8`; `UP` (slot 3 hold) | toast **`GROUP 3 SAVED (6 UNITS)`** in `group3-saved.png`; slot 3 highlighted, count `6` under the numeral |
| 3 | `DOWN 900 500`; `sleep 0.55`; `MOVE 1500 760` | `box-marquee.png` shows the semi-transparent **marquee rectangle** spanning exactly the drag (≥0.30 s hold ⇒ `DragMode::Box`) |
| 4 | `UP 1500 760` (box release) | `GAME_STATE … selected=6` → **`selected=1`** (19:48:26); `box-released.png` |
| 5 | `input tap 1152 824` (slot 3 tap) | `GAME_STATE … selected=1` → **`selected=6`** (19:48:46); `group3-recall2.png` |

Conclusion: assign → recall round-trip and the physical box-select path are **device-verified** on the current product (control groups 1..9,0 row; hold=assign, tap=recall; box drag ≥300 ms draws the marquee and changes the selection on release).

Screenshot sha256 (`analysis/agent-touch/device-round8/`):
`base.png 95ca61b4…`, `base2.png 3d0f8952…`, `army2.png 548df80f…`, `group3-saved.png 5ae82985…`, `box-marquee.png 42fc823c…`, `box-released.png a406d35d…`, `group3-recall2.png 8c6d1df8…`, `hotkey-negative.png 42d10914…`. Earlier frames from a relaunch window (`base.png`, `army-tap.png`, `group3-hold.png`, `group3-recall.png`) are kept as chronology, not as pass evidence.

## Hotkeys — gap found and fixed in the platform layer

Observed gap on the current product: `apps/android-native/src/lib.rs` had **no SDL keyboard handling at all** (no `SDL_EVENT_KEY_*` branch, and `Game::key_char` had no production caller — only tests). Negative device test on the pre-fix APK:

```
su -c 'input keycombination KEYCODE_CTRL_LEFT KEYCODE_2'   # Ctrl+2
su -c 'input keyevent KEYCODE_2'                            # 2
logcat -d -s wc3rust:V | grep -c GAME_HOTKEY            # -> 0
GAME_STATE … selected=6                                      # unchanged
```

Fix (owner path `apps/android-native/src/lib.rs`, `apps/android-native/Cargo.toml`):

- constants `SDL_EVENT_KEY_DOWN = 0x300`, `SDL_KMOD_CTRL = 0x00c0`, `SDL_SCANCODE_ESCAPE = 41`;
- `SdlEvent::key_scancode()` (offset 24), `key_mod()` (offset 32, u16), `key_repeat()` (offset 37);
- `wc3_input::scancode_char` (input layer, unit-tested): `4..=29 → a..z`, `30..=39 → 1..9,0` (physical scancodes, layout-neutral, Shift never turns `1` into `!`);
- event arm: non-repeat key → `GAME_HOTKEY key=… ctrl=…` log + `game.key_char(ch, ctrl)`; Escape → `game.hotkey(wc3_input::Hotkey::Escape, ctrl)`;
- `wc3-input` path dependency added to the app crate.

Compile evidence (dev): `cargo check -p herofolends-android --offline` → **rc 0** (log shows `Checking wc3-game` + `Checking herofolends-android`), 19:41:48 (`$PREFIX/tmp/opencode/r8-check.log`). Mapping test: `cargo test -p wc3-input --offline` → **rc 0, 23 passed / 0 failed** (adds `scancode_chars_match_the_sdl_keyboard`, 19:55:08).

Rebuild + device verification (completed):

- The RE agent's in-flight `wc3-re` edits settled at 20:00:13; `cargo check -p wc3-re` → **rc 0** (`r8-build-retry.log`: `TREE_STABLE iter=16`, `RE_CHECK_RC_1=0`).
- `cargo build -p herofolends-android --release --offline` → **rc 0** (20:05:27), `target/release/libmain.so` 8.87 MB (contains the key handler).
- `scripts/build-apk.sh` → first run rc 1 (`javac: Could not reserve enough space for 2250752KB object heap` under the 4.5 GB virtual-memory limit); ONE fix: re-run without `ulimit -v` and with `JAVA_TOOL_OPTIONS=-Xmx768m` → **rc 0**, APK `build/android-p0/herofolends-p0.apk` built 20:06:38, 6.16 MB, sha256 `2d6b8f5537857b29…`, `apksigner verify` v2/v3 true.
- `su -c 'pm install -r …'` → **Success**; `am start -W` → Complete; `GAME_FIRST_FRAME` logged.

Hotkey device evidence (new APK, logcat `analysis/agent-touch/device-round8/hotkeys-logcat.txt`):

| Injection | Logcat (observed) | Effect |
|---|---|---|
| `input keycombination KEYCODE_CTRL_LEFT KEYCODE_2` (Ctrl+2) | `20:07:10.621 GAME_HOTKEY key=2 ctrl=true` | toast **`GROUP 2 SAVED (6 UNITS)`**, row slot 2 shows count `6` — `hotkey-assign-ctrl2.png` (sha256 `6771f989…`) |
| `input keyevent KEYCODE_2` (2) | `20:08:43.233 GAME_HOTKEY key=2 ctrl=false` | recall dispatch logged (game had restarted, group empty in that process) |
| `input keyevent KEYCODE_B` (B) | `20:08:45.862 GAME_HOTKEY key=b ctrl=false` | `GAME_STATE … selected=1` at `20:08:47.090` (town hall selected) |

Pre-fix negative control (old APK): the same Ctrl+2/2 injections produced **0** `GAME_HOTKEY` lines and no selection change.

Limit (stated, not hidden): a *non-empty* hotkey recall in one uninterrupted window was not captured — between 20:07 and 20:10 the device focus was taken by `DocumentsUI PickActivity`, `bin.mt.plus` and Chrome (shared device), so those later injections went to other windows. The recall semantics are covered by `cargo test -p wc3-game --lib game::tests::` (`hotkeys_drive_the_card_and_control_groups`, rc 0) and by the touch recall that *was* device-verified. Ambiguous frames are kept under `device-round8/interim/`, not used as pass evidence.

Final layer gates on the current tree (20:10–20:12): `cargo test -p wc3-game --lib game::tests:: --offline -- --test-threads=1` → **rc 0, 31 passed / 1 ignored**; `cargo test -p wc3-input --offline` → **rc 0, 23 passed** (`$PREFIX/tmp/opencode/r8-gametests.log`).

## Cross-layer findings from the device run (owner: GPU / platform)

1. **wgpu surface abort (intermittent):** `/data/tombstones/tombstone_17` — 2026-11-13 19:44:36, pid 482, `SDLThread`, `signal 6 (SIGABRT)`, abort message:
   `wgpu error: Validation Error / Caused by: In Surface::configure / Surface does not support the adapter's queue family`.
   Owner: GPU/wgpu layer (surface/adapter selection). Next step: guard `Surface::configure` against adapter/surface queue-family incompatibility (or retry with the fallback adapter) before creating the swapchain; the app relaunch masked it during this run.
2. **Shared-device contention:** at 19:45:30 a `Monkey` harness relaunched the app (`Monkey: args: [-p, org.herofolends.p0, -c, android.intent.category.LAUNCHER, 1]`) and at 19:49:07 `ActivityTaskManager: Force removing ActivityRecord … app died, no saved state` (no tombstone). Another agent is cycling the device; evidence above was captured in stable windows. Coordinate device slots before long taps/holds.

## Reproduce (root)

```bash
su -c 'pm install -r build/android-p0/herofolends-p0.apk'
su -c 'am start -W -n org.herofolends.p0/org.libsdl.app.SDLActivity'
su -c 'input tap 2132 888'                                        # ARMY
su -c 'input motionevent DOWN 1152 824'; sleep 0.8; su -c 'input motionevent UP 1152 824'
su -c 'input motionevent DOWN 900 500'; sleep 0.55; su -c 'input motionevent MOVE 1500 760'
su -c 'screencap -p /sdcard/box.png'
su -c 'input motionevent UP 1500 760'
su -c 'logcat -d -s wc3rust:V | grep GAME_STATE | tail -1'
```

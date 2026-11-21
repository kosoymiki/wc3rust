# PACKET C — device evidence for the current build (owner: device-qa)

## Mission
Produce verifiable device evidence for the build installed right now (built 11:18, `present=Mailbox`):
main menu, in-game HUD, world. Measure the HUD against the pack contract numerically and report
pass/fail per rule. You do NOT change source code.

## Why
The final HUD frame was never captured after the last fix set (console full-bleed, symmetric
content gutters, inventory fit/hide, wider toast/tooltip, anisotropy 16). Without a device frame the
delivery is unproven.

## Evidence snapshot (observed)
- Package `org.herofolends.p0`, activity `org.libsdl.app.SDLActivity`.
- Display: `wm size` override 1080x2414 portrait; the game renders landscape 2414x1080.
- Headless gates already green: `analysis/hudskin/pack-2414x1080.png` + `pack-layout-2414x1080.json`,
  `tools/hud_pack_check.py` PASS, `tools/hud_pack_layout` (example `hud_layout`) PASS.
- Known good device frame (pre-fix): `analysis/hudskin/device-playable.png`.
- The operator uses this phone for other apps (ChatGPT/Telegram take focus). `am start` from a
  Termux `su` shell sometimes leaves the game behind; a HOME→start sequence usually works.

## Invariants
- Never fight the operator for the screen: at most 3 launch attempts, then report `blocked` with the
  exact focus line (`dumpsys window | grep mCurrentFocus`) and stop.
- Never modify app data: do not delete `files/*.w3x`, `*.sav`, `hud_pack`.
- All claims carry a file path + size + the command that produced it.

## OWNER_PATHS
- analysis/device-qa-20261117/** (new dir: PNGs + REPORT.md)

## READ_ONLY
everything else (all source, all existing analysis files).

## Phase plan
1. Cold start: `su -c 'am force-stop org.herofolends.p0'`, HOME, `am start`, verify focus, screencap menu.
2. Enter PLAY (tap the PLAY row; if the menu needs it, use `input tap` with coordinates derived from
   the captured frame, not guessed) and capture 3 in-game frames 5 s apart.
3. Measure on the in-game frame at native 2414x1080 with a Python script under your OWNER dir:
   console top y vs `0.2933*1080 = 317`, console left/right vs 0 and 2414, minimap square side
   (expect 238), card cell (expect 90), card grid right edge inside the frame, toast/tooltip text
   inside their plates, presence of the gold 1-px line, absence of empty inventory plates when only
   workers are selected.
4. Compare the device rects with `analysis/hudskin/pack-layout-2414x1080.json` and report the delta
   per element (device vs headless).
5. Write `analysis/device-qa-20261117/REPORT.md` with the table + the PNG list.

## Acceptance artifacts
- >= 3 PNGs (menu, in-game x2) at native resolution under `analysis/device-qa-20261117/`.
- `REPORT.md` with the measured rect table, deltas, verdict per rule (PASS/FAIL) and exact commands.

## Hints
- Screenshot: `su -c 'screencap -p /data/data/com.termux/files/home/.../frame.png'`.
- The app logs useful state: `GAME_HUD_RECTS`, `GAME_FIRST_FRAME`, `GAME_STATE` (tick/fps).
- If `GAME_STATE ... tick=0 fps=0` persists, the present path regressed: capture it as evidence and
  say so (do not "fix" source).

## Report format
Goal | Actions | Evidence (paths, sizes, rect table) | Result | Risks/Next | H2_PERSISTED ids.

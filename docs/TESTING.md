# wc3rust device QA

Deterministic visual-regression and gameplay gate for the Android build
(`org.wc3rust.p0`, 2414x1080 landscape, Termux + KernelSU root).

Everything here is device-driven: the real APK is built, installed, launched,
screenshotted, tapped, and read back through `logcat`. No mocks, no hand-made
screenshots, no "looks fine" verdicts.

## Quick start

```bash
# Full gate: build + install + launch + visual + gameplay
scripts/qa-all.sh

# Visual only (fast; useful while iterating on HUD/terrain)
scripts/qa-all.sh --fast --no-build

# Test the installed APK without rebuilding
scripts/qa-all.sh --no-build

# Also render/grade the whole asset corpus (2048 MDX, 691 icons, 111 tiles)
scripts/qa-all.sh --gallery

# Wait up to 4 minutes for a VICTORY/DEFEAT overlay
scripts/qa-all.sh --long 240
```

Exit code is `0` only when every enabled hard gate passes. Evidence lands in local,
gitignored paths (`analysis/**`) on the operator's machine:

| Path | Content |
|---|---|
| `analysis/agent-qa/latest/gameplay.json` | step-by-step log evidence |
| `analysis/agent-qa/latest/gameplay/*.png` | screenshot + annotated + JSON per state |
| `analysis/agent-qa/gallery/gates.json` | corpus gates |
| `analysis/agent-qa/gallery/sheets/` | contact sheets |

## Requirements

- Root: `su -c id` must print `uid=0(root)` (KernelSU). The runner refuses to
  fake permissions; every device command is quoted and bounded.
- `python3` with `Pillow` and `numpy` (`pkg install python-pillow python-numpy`).
- `cargo` for building the APK and the gallery renderer.
- A connected display: the runner waits for `GAME_FIRST_FRAME` and verifies the
  screenshot is the same size as the current display, so a stale portrait buffer
  cannot be annotated as a landscape run.

## Device runner (`scripts/qa/device.py`)

Direct `su -c` shell (adb is not attached to this device). Commands:

```bash
python3 -m scripts.qa.device check          # root, package, display, APK
python3 -m scripts.qa.device info --json    # display/rotation/density/insets/focus
python3 -m scripts.qa.device launch --json  # force-stop, launch, wait focus+frame
python3 -m scripts.qa.device screenshot out.png
python3 -m scripts.qa.device tap X Y
python3 -m scripts.qa.device swipe X1 Y1 X2 Y2 --ms 300
python3 -m scripts.qa.device pinch CX CY START END
python3 -m scripts.qa.device logs --grep GAME_STATE --lines 200
```

Coordinates are screenshot pixels in the current rotation.

- **Launch**: `am force-stop`, `logcat -c`, wake, dismiss keyguard, `am start
  -W`, wait for `mCurrentFocus`, wait for `GAME_FIRST_FRAME` (bounded 55 s),
  reject `GAME_FATAL`, wait for landscape, verify focus, read insets.
- **Status/nav bars**: insets are parsed from `dumpsys window`
  (`InsetsSource type=statusBars/navigationBars/displayCutout`). The game also
  logs its SDL safe area in `GAME_FIRST_FRAME safe=SafeInsets { … }`; that is
  the rectangle the HUD is actually laid out inside (currently
  `210,120,90,96` on this device, larger than the WM insets). The analyzer uses
  the SDL safe area for HUD geometry and the WM insets for system-bar overlap.
- **Foreign overlays**: call banners, notification shade, dialogs. The runner
  collapses the shade, brings the game forward, and retries the capture while a
  neutral-dark large rectangle is detected. If it persists, the analyzer masks
  it and marks terrain/text checks `WARN` rather than blaming the game.
- **Pinch**: no adb/uiautomator required. The runner reads the InputReader
  `RawToDisplay Transform` matrix for `touchpanel` from `dumpsys input`
  (rotation included), inverts it, and emits a protocol-B multi-touch gesture
  through `sendevent` in one root shell. Two slots, 10 steps, explicit
  `SYN_REPORT`, tracking-id release. Use it for camera pinch checks; the game
  does not currently bind pinch to gameplay.

## Visual analyzer (`scripts/qa/analyze.py`)

Run manually:

```bash
python3 -m scripts.qa.analyze SHOT.png \
  --insets 120,120,0,48 --safe 210,120,90,96 --density 480 --fps 60 \
  --json-out out.json --annotated out.png
```

`--insets` is the system-bar rectangle, `--safe` the game's SDL safe area.
The annotated PNG draws red system bars, a magenta terrain ROI/grid, an orange
command-cluster box, yellow detected button borders, cyan text boxes (height +
contrast in the label), green unit clusters, blue joystick, and boxes around
black/white/pink areas.

### Hard gates

| Gate | FAIL when | Why |
|---|---|---|
| `system_bar_overlap` | any HUD rect intersects a visible status/cutout/nav bar | the original screenshot had the clock over the resource row and the battery over `BASE` |
| `touch_targets` | any command/chip rect is under 48 dp in either axis | Android/Material minimum; current 132 px primary = 44 dp at 480 dpi |
| `button_overlap` | two command rects intersect | the ring radius (110 px) is smaller than the button size (132 px), so adjacent ring buttons overlap by 22 px |
| `button_clipping` | a button leaves the safe area or the screen | `STOP` sat under the gesture bar |
| `command_cluster` | cluster height > 40% of screen, or <8 dp margin to the safe edge | catches scattered/scraped command cards |
| `tiny_text` | a HUD text band measures under 24 px (8 dp at 480 dpi) | tiny labels were unreadable on the 2412-wide screen |
| `text_contrast` | a HUD text band is below 3:1 | WCAG large-text minimum; disabled labels are ~1.1:1 |
| `text_overlap` | two detected HUD text lines overlap >15% | catches the old `ARMY 4` / `ENEMY HUMAN04` collision |
| `terrain_seams` | a periodic grid has ratio ≥1.8 **and** absolute edge amplitude ≥8 grey levels | ratio alone fires on smooth terrain; amplitude proves a hard tile edge |
| `terrain_flat_patches` | the largest connected low-variance patch is ≥6% of the terrain ROI | the broken render had 12–37% flat, untextured rectangles |
| `black_holes` | black >18% or largest component >3% of the world | texture holes |
| `white_holes` | white >6% or largest >2% | the giant white plane |
| `pink_alpha` | any magenta cluster >0.1% | alpha/missing-texture checker |
| `units_visible` | no blue/red unit tint clusters in the world | units absent |
| `joystick_contrast` | disc contrast <1.6:1 or knob not visibly brighter | low-contrast joystick was invisible on grass |
| `minimap_readability` | variance <8, <24 colours, or <20 blue/red pixels | empty minimap |
| `selection_readability` | no readable selection text band ≥3:1 | empty info panel |
| `command_icons` | any of the 8 command buttons has no icon texels | icons were missing in the first modern-HUD build |
| `fps` | `GAME_STATE fps=` below 20 | unplayable |

Warnings (do not fail the build but are reported): `chrome_detection` (the new
translucent borders are hard to segment), `layout_drift` (pixels disagree with
the `mobile_hud.rs` mirror — update `scripts/qa/layout.py`),
`foreign_overlay`, `text_present` (a band has no glyphs, e.g. disabled labels).

### Layout mirror

`scripts/qa/layout.py` is a line-by-line port of
`crates/wc3-game/src/mobile_hud.rs:624` (`MobileHud::layout`) plus
`style.rs:61` (`Tokens`). The old `HudLayout` is no longer the drawing path.
If the HUD changes, the button-detection drift warning plus this file are the
only maintenance points.

### Calibration evidence

All four historical defect captures fail `terrain_flat_patches` (12–37% flat
patches); the fixed terrain render (2026-11-12 16:05) is at 3.6% and passes the
seam gate with amplitude 2.9. Historical screenshots:
`analysis/user-shot-1..4.png`, `analysis/g1-small.png`, `analysis/fix3-small.png`.

## Gameplay self-test (`scripts/qa/gameplay.py`)

Launches the game, then proves each state from the game's own log lines:

1. **select** – taps a blue unit cluster found in the screenshot, waits for
   `GAME_STATE selected>=1`. Falls back to the `ARMY` chip and records a defect
   if the chip tap is swallowed (system-bar overlap).
2. **move** – taps open ground, requires a new `ORDER MOVE units=N`.
3. **train** – `BASE` chip (or hall sprite), tap the `FOOTMAN` context card,
   require `TRAIN Footman`, then `TRAINED Footman` within 32 s.
4. **build** – select a peasant (try small blue clusters), tap `BUILD`, place on
   ground, require gold drop ≥300 or unit count +1.
5. **attack-move** – select army, tap `ATK-MOVE`, tap the enemy direction,
   require `ORDER ATTACK-MOVE`.
6. **win/lose** (`--long N`) – keep pushing, capture `phase_won.png` /
   `phase_lost.png` on `GAME_VICTORY`/`GAME_DEFEAT`.

`--resume` additionally runs the HOME→foreground probe. It is opt-in because it
currently reproduces a real abort (`wgpu error: Validation Error In
Surface::configure — Surface does not support the adapter's queue family`);
the `app_crash` step records PID before/after and the abort tail.

Each step stores a screenshot, annotated PNG and analyzer JSON. Partial results
are written after every screenshot, so an interrupted run still leaves evidence.

## Corpus galleries (`crates/wc3-game/tests/asset_gallery.rs`)

The gallery renders every model with the game's own wgpu pipeline and every BLP
with its decoder, not a file listing:

```bash
GALLERY_ROOT=analysis/war3_full GALLERY_OUT=analysis/agent-qa/gallery \
  cargo test --release -p wc3-game --test asset_gallery -- --ignored --nocapture
python3 -m scripts.qa.gallery            # gates + contact sheets
```

`qa-all.sh` without `--gallery` grades an existing gallery if present and fails
if it is absent; `--no-gallery` explicitly skips the phase (device-only gate).

Exact counts (observed):
`analysis/war3_full` contains **2048 MDX** — 1021 Doodads, 411 Units,
230 Abilities, 131 Buildings, 99 Objects, 97 UI, 45 Environment,
14 SharedModels — plus **691 CommandButtons BLP** and **111 TerrainArt BLP**
(110 unique stems because two tilesets share a filename). The gates assert the
exact counts, zero parse/render failures, zero missing textures, zero
corpse/effect geosets visible in `Stand` (sampled at the sequence start), zero
backward-walking models (root walk translation against model +X), and no
black/white/pink/empty thumbnails. Contact sheets are written to
`analysis/agent-qa/gallery/sheets/`.

`GALLERY_LIMIT=N` and `GALLERY_ONLY=models,icons,terrain` are smoke-run knobs.
The gallery test is `#[ignore]` so it never slows the normal unit suite.

## Workspace gate (`scripts/test-all.sh`)

The device procedure above proves the shipped APK; `scripts/test-all.sh` proves the
tree it was built from. One run executes, in order:

| Phase | Command | Pass condition |
|---|---|---|
| workspace check | `cargo check --workspace` | rc=0 for every member from `cargo metadata` |
| main menu | `tests/gates/menu-layout-gate.sh` | >= 6 tappable entries whose rects contain their tap points |
| Orc01 chapter | `tests/gates/orc01-smoke.sh` | boots `Maps\Campaign\Orc01.w3m` headless, 3000 ticks, roster/quests/JASS/cutscene match |
| per-member tests | `cargo test -p <member>` | rc=0, tests single-threaded behind the `h2-cargo` slot |
| Android targets | `cargo check -p wc3rust-android --target <triple>` | rc=0, or SKIP with the missing linker named |
| port parity | `scripts/port-parity-all.sh` | every unit under `analysis/port-parity/` replays N/N and passes `gate` |

Every job writes its own log under `analysis/test-all/` and one JSON line to
`analysis/test-all.jsonl`; a failure prints the tail of that job's log. Exit status
is 0 only when every executed job passed, 1 on a job failure, 2 on bad
usage/metadata, 3 when the filesystem cannot hold the next link step
(`TEST_ALL_ABORT reason=disk`) — an aborted gate is not a member failure.

Narrow debugging runs: `--only <member>`, `--no-check`, `--no-tests`,
`--no-targets`, `--no-orc01`, `--no-menu`, `--no-parity`, and `PORT_PARITY=0`. A
skipped phase is recorded as SKIP with its reason in the JSONL, never as a PASS;
`--check-only`, `--no-tests` and `--only` turn the port-parity loop off by
construction because a single member or a compile-only run cannot prove the layer
matrix. `--self-test` proves the gate can fail: it plants a failing test in a crate
the old hand-written subset never ran and a deliberately broken Orc01 run, and
requires both to turn the gate red.

The gate is single-slot by construction: `scripts/h2-cargo` serialises cargo
through one flock and caps jobs at 2, so a full run queues behind any other lane's
build. Expect minutes, not seconds, on this device; a run that prints nothing for a
long time is waiting for the cargo slot, not hung.

## Reliability playbook

- **Game backgrounded by the operator/notification** – the runner detects
  focus loss, collapses the shade, re-starts the activity without force-stop,
  and retries the screenshot. A cold `am force-stop` is only used by `launch`.
- **Black screen after background/resume** – captured as a real defect: the
  surface is not redrawn after the app is resumed; the analyzer reports
  `black_holes` + `units_visible`, and `gameplay` screenshots show it. The
  `--resume` probe records the wgpu abort; without `--resume` the gameplay test
  focuses on the action path.
- **Shader compile failure** – `terrain_render.rs` embeds WGSL; a reserved
  keyword (`let patch = ...` at line 628) makes `Renderer::new` panic with
  `Shader 'terrain-shader' parsing error`. The gallery test catches it before a
  single thumbnail is written. Any `cargo test`/APK build after that edit fails
  at runtime, not at compile time.
- **Logcat is not UTF-8** – `common.su` captures bytes and decodes with
  `errors="replace"`; a stray Android log line can never abort a QA run.
- **Rotation not settled** – screenshots are rejected unless their size equals
  the current `DisplayFrames`; launch waits for landscape.
- **Threshold changes** require new evidence in
  `analysis/agent-qa/REPORT.md`; never lower a threshold just to pass.

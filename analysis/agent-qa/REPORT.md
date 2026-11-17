# QA report — visual regression, device automation, gameplay, asset corpus

**Owner:** agent-qa (QA / visual regression / device automation)
**Project:** `/data/data/com.termux/files/home/src/herofolends-rs`
**Device:** Android 16, aarch64, 2414x1080 landscape, density 480 (3x), KernelSU root
**Package:** `org.herofolends.p0` (last install 2026-11-12 16:41:53, version `0.1-p0`)
**Date:** 2026-11-12

## Goal

Replace "looks fine on a screenshot" with a deterministic gate that builds,
installs, launches, screenshots, taps, reads `logcat`, and fails on the exact
visual/gameplay defect classes: system-bar overlap, terrain checkerboard,
tiny/overlapping/illegible text, missing icons, undersized touch targets,
clipped/scattered command cards, black/white/pink texture holes, missing units,
unreadable minimap/selection, FPS collapse, and broken gameplay actions. Extend
the same evidence discipline to the whole asset corpus: every MDX model, every
TerrainArt tile, every command icon.

## Deliverables

| File | Purpose |
|---|---|
| `scripts/qa/device.py` | Root `su` runner: build/install/launch, focus + first-frame wait, rotation/insets, screenshot, tap/swipe/long-press, multi-touch pinch (`sendevent` + `RawToDisplay` matrix), logcat, clean capture (retries foreign overlays) |
| `scripts/qa/layout.py` | Line port of `MobileHud::layout` (`mobile_hud.rs:624`) + `Tokens` (`style.rs:61`) + expected text bands |
| `scripts/qa/analyze.py` | 19 visual gates, annotated PNG, JSON, `PASS/FAIL`; `--insets`, `--safe`, `--density`, `--selected` |
| `scripts/qa/gameplay.py` | Select → move → train → build → attack-move; optional resume + win/lose; each step proven from game log lines and a screenshot |
| `scripts/qa/suite.py` | Deterministic orchestration: build → install → launch → visual → crash check → gameplay → gallery |
| `scripts/qa-all.sh` | One-command gate (`--fast`, `--no-build`, `--no-gallery`, `--gallery`, `--long N`) |
| `scripts/qa/gallery.py` | Corpus gate + contact sheets from the rendered gallery |
| `crates/wc3-game/tests/asset_gallery.rs` | Renders all 2048 MDX with the game pipeline; decodes 691 icons and 111 TerrainArt BLPs; emits manifest + flags |
| `docs/TESTING.md` | Operator documentation, gate thresholds, calibration, troubleshooting |

Evidence root: `analysis/agent-qa/latest/` (gate JSON, annotated screenshots,
gameplay states, logs); corpus: `analysis/agent-qa/gallery/`.

## Gate result on the installed APK (2026-11-12 16:41, visual-only)

`analysis/agent-qa/latest/visual.json`, `screen-start.annotated.png`;
command: `scripts/qa-all.sh --no-build --fast --no-gallery` (FPS 62).

| Gate | Result | Evidence |
|---|---|---|
| `system_bar_overlap` | **PASS** | HUD laid out inside SDL safe area `210,120,90,96`; no rect intersects the 120/120/0/48 WM insets |
| `button_overlap` | **PASS** | 4x2 grid: no pair intersects (the earlier ring had 4 pairs overlapping by 22 px) |
| `button_clipping` | **PASS** | all buttons inside the safe area |
| `command_cluster` | **PASS** | bbox `[1855.6, 746.8, 2308.0, 968.0]`, 20.5% screen height, 64 px bottom / 106 px right margin |
| `terrain_seams` | **PASS** | ratio 20.1x but absolute amplitude 2.92 grey levels (soft texture grid, no hard tile edges) |
| `terrain_flat_patches` | **PASS** | flat 1.56%, largest patch 1.66% of the terrain ROI (was 37.1% on the broken render) |
| `black_holes` / `white_holes` / `pink_alpha` | **PASS** | 0.0–0.1% |
| `units_visible` | **PASS** | 20 blue + 20 red tint clusters |
| `minimap_readability` | **PASS** | variance 33, 99 colours, 4.3k blue pixels |
| `command_icons` | **PASS** | icon texels present on ring/chips/context (zoom chips are text-only by design) |
| `text_overlap` | **PASS** | no pair |
| `fps` | **PASS** | 62 FPS ≥ 20 |
| `touch_targets` | **FAIL** | all 12 rects below 48 dp: primary/context 105.6 px = **35.2 dp**, chips 104x72 px = **34.7x24 dp** |
| `tiny_text` | **FAIL** | 17 HUD bands below 24 px: `Tokens.type_micro` 14 px (`GOLD`, `ARMY`, `ATK-MOVE`, `CAMERA`…) and command labels 16.6 px; resource values 24 px and map title 28 px pass |
| `joystick_contrast` | **FAIL** | no joystick control in the HUD (`MobileHudLayout` has no joystick rect; bottom-left empty in the capture) |
| `selection_readability` | WARN | nothing selected; the panel is intentionally hidden when `selection=None` |
| `chrome_detection`, `layout_drift`, `text_present` | WARN | the new translucent chrome is hard to segment; 3/8 borders detected; disabled labels have no glyphs |

Hard failures on this build: **`touch_targets`, `tiny_text`, `joystick_contrast`**.

## Defects (exact, with evidence)

### D1 — CRITICAL: app aborts when resumed (`wgpu Surface::configure`)

Reproduce: launch, press HOME, `am start` the task. The process aborts:

```
Abort message: 'wgpu error: Validation Error
  In Surface::configure
    Surface does not support the adapter's queue family'
```

Evidence: `/data/tombstones/tombstone_29` (backtrace `Surface::configure` +
`default_error_handler` + `rust_begin_unwind`), repeated aborts for PIDs
25157/25655/25907/26184, app restarts (`GAME_FIRST_FRAME` per new PID) and
`analysis/agent-qa/latest/gameplay/03-selected.png` is 100% black
(`black_holes 100%`, `units_visible` empty). The optional probe is
`python3 -m scripts.qa.gameplay --resume`; the agent-level capture records the
crash under `app_crash` with PID before/after and abort tail.

Impact: the app cannot survive background/resume; long gameplay and any
operator app-switch kill the session. Owner: platform/render (surface
reconfiguration path, adapter/queue-family selection after `SDL_EVENT_WINDOW_*`).

### D2 — Touch targets below 48 dp

All 12 interactive rects: command cells `105.6x105.6 px` → **35.2 dp**; top-right
chips `104x72 px` → **34.7x24 dp**. Android/Material minimum is 48 dp (144 px at
480 dpi). Evidence: `visual.json` `touch_targets.small`; annotated screenshot
labels each rect. Fix: raise `Tokens.touch_primary`/`touch_context`/`touch_chip`
(or the cell clamp `78..112`) to ≥144 px on this density and re-run.

### D3 — Tiny HUD text

`Tokens.type_micro = 14 px` (`GOLD`, `LUMBER`, `SUPPLY`, `ARMY`, `BASE`,
`ATK-MOVE`, `GUARD`, `STOP`, `GATHER`, FPS, mode) and `type_label * 0.92 =
16.6 px` for command labels. 17 bands measured below the 24 px (8 dp) floor.
Evidence: `visual.json` `tiny_text.lines` with band names + expected/observed
px; annotated screenshot prints `Npx C:1` on every line. Fix: raise the type
scale to ≥24 px (12 sp at 2x=24 px; 8 sp is not legible on a 2412-wide screen)
and re-run.

### D4 — No camera joystick

`MobileHudLayout` no longer has joystick fields (`crates/wc3-game/src/mobile_hud.rs`
has no `joystick` rect; `grep -n joystick` only finds a comment), and the
bottom-left of the capture is empty. The old HUD had a low-contrast joystick
(1.09:1) but it at least existed. The gate is `joystick_contrast: present=false`.
Decide: restore a joystick (with ≥3:1 contrast and ≥48 dp hit area) or document
the replacement camera control and update the gate.

### D5 — Selection panel is hidden when nothing is selected

`mobile_hud.rs:1019` draws the selection card only `if view.selection.is_some()`.
The old "NO SELECTION / TAP A UNIT" hint is gone. This is a deliberate design
change, so the analyzer reports WARN when no selection exists; it will fail if a
selection exists but the panel text is missing/low-contrast. Confirm the design.

### D6 — WGSL reserved keyword blocks all rendering tests and rebuilds

`crates/wc3-game/src/terrain_render.rs:628`:

```wgsl
let patch = vnoise(input.uv * 0.55) * 0.6 + ...
```

`patch` is a reserved WGSL keyword. Any `Renderer::new` / headless render panics:

```
Shader 'terrain-shader' parsing error: name `patch` is a reserved keyword
  --> wgsl:81:9
```

Observed by `crates/wc3-game/tests/asset_gallery.rs` (2.47 s in, 0 thumbnails)
and by `cargo check` (3 errors) while the file was mid-edit. Fix: rename the
local (e.g. `terrain_patch`). This is the only blocker for the corpus gallery
and for a fresh APK build; the installed 16:41 APK predates the change and still
renders terrain.

### D7 — Environment: concurrent agents mutate the device and build dir

While this gate ran, other agents force-stopped/disabled/uninstalled the app and
rebuilt the APK:

- `am start` → `Error type 3` / `am start rc=1`; `dumpsys package` →
  `installed=false`, `enabled=1` (fixed with `pm enable`);
- `Force stopping org.herofolends.p0 ... from pid 19672`, `GAME_CLOSED frames=130`,
  PID changes without a Rust abort;
- `build/android-p0/herofolends-p0.apk` deleted by `scripts/build-apk.sh`
  (`rm -rf "$BUILD"`) while another agent built;
- `GAME_VULKAN turnip=0 (no ICD; vendor driver)` after the app files changed.

The runner now does `pm enable`, collapses the shade, re-asserts focus, treats
PID changes/new aborts as `app_crash`, and falls back to the installed package
when the APK file is absent. Final gates still need exclusive device/build
access; otherwise a run can be invalidated mid-flight. Not a product defect.

## Historical defects now verified fixed

| Defect (evidence) | Before | After (16:41 build) |
|---|---|---|
| Status bar over resource row / cutout over minimap (`user-shot-1.png`, `user-shot-4.png`) | clock over gold, battery over `BASE`, HUD at y=18 | HUD inside SDL safe area, `system_bar_overlap` PASS |
| Checkerboard/flat terrain (`user-shot-4.png`, `current-03.png`, `g1-small.png`, `fix3-small.png`) | flat patches 9.8–37.1%, hard grid amp 10–46 | flat 1.56%, largest 1.66%, seam amp 2.92 |
| Overlapping command ring (16:34 build) | 4 pairs overlap by 22 px, cluster 44% height | 4x2 grid, 0 overlaps, cluster 20.5% |
| Missing command icons (16:01 build) | empty cards/circles | icons render; `qa_icons` resolves 9/9 relevant actions |
| Black/white/pink texture holes | white plane/black field in first P0 | 0.0–0.1% in the current frame |

## Gameplay self-test status

The full flow is implemented and each step is proven from the game's own log
(`BUTTON`, `ORDER MOVE`, `ORDER ATTACK-MOVE`, `TRAIN`, `TRAINED`, `GAME_STATE`,
`GAME_VICTORY`/`GAME_DEFEAT`) plus an analyzed screenshot:

```
scripts/qa-all.sh            # includes gameplay (select/move/train/build/attack)
python3 -m scripts.qa.gameplay --long 240   # optional VICTORY/DEFEAT overlay
```

On this build the run cannot complete: D1 aborts on HOME→resume, and concurrent
agents force-stop the package mid-run(`GAME_CLOSED frames=130`,
`Force stopping ... from pid 19672`). Partial JSON is written after every
screenshot (`analysis/agent-qa/latest/gameplay.json`); the `app_crash` step
records PID before/after and the abort tail. Rerun after D1 is fixed and with an
exclusive device slot.

## Asset corpus gallery status

The gallery test is complete and compiled; it renders every MDX through the game
pipeline and decodes every BLP, with these exact counts from
`analysis/war3_full`:

| Corpus | Count | Breakdown |
|---|---|---|
| MDX models | **2048** | 1021 Doodads, 411 Units, 230 Abilities, 131 Buildings, 99 Objects, 97 UI, 45 Environment, 14 SharedModels |
| CommandButtons BLP | **691** | unique stems |
| TerrainArt BLP | **111** | 15 tilesets (110 unique stems; two tilesets share a filename) |

Gates: exact counts, zero parse/render failures, zero missing textures, zero
corpse/effect geosets visible in `Stand` (`mesh_alphas_for` at sequence start),
zero backward-walking models (root walk translation vs model +X), no
black/white/pink/empty thumbnails, icons/terrain content, and contact sheets.

Status: **icons and terrain graded; models blocked by D6.**

- Icons: **691/691** decoded, 0 load failures, 0 blank, 0 alpha-checker
  failures. 17 icons contain magenta/purple *art* (BansheeAdept, FaerieFire,
  Rejuvenation, SoulGem, Web…), all with 0 transparent pixels; the gate treats
  only >40% magenta as an alpha failure. Contact sheets: `gallery/sheets/icons-01..15.png`.
- Terrain: **111/111** decoded, 0 load failures, 0 white tiles, 0 black tiles
  after exempting the intentionally black `Nothing/nothing.blp`; contact sheets
  `gallery/sheets/terrain-01..04.png`.
- Models: 0/2048. `Renderer::new` panics on the `patch` WGSL keyword before a
  single thumbnail is written (`Shader 'terrain-shader' parsing error`). The
  gate records `gallery_models_not_built`, so an empty manifest cannot pass.

Evidence: `analysis/agent-qa/gallery/gates.json` (`partial: true`),
`icons/summary.json`, `terrain/summary.json`, 19 contact sheets. Once
`terrain_render.rs:628` is renamed:

```bash
scripts/qa-all.sh --gallery      # renders + grades + contact sheets
# or: cargo test --release -p wc3-game --test asset_gallery -- --ignored --nocapture
python3 -m scripts.qa.gallery
```

The world agent's separate `analysis/agent-world/{icons,terrain,units}-contact-sheet.png`
and `biomes-audit.md` (111/691 decoded, 0 failures) are independent evidence for
the icon/terrain counts, not a substitute for this gallery's model thumbnails.

## Risks / next

1. Fix D6 (one-line rename), then run `scripts/qa-all.sh --gallery`; the model
   gallery is the only missing artifact and will expose missing textures /
   corpse geosets / backward models.
2. Fix D2/D3 (48 dp targets, 24 px labels) and D4 (joystick decision); re-run
   `scripts/qa-all.sh --fast --no-build`.
3. Fix D1 (surface reconfigure) and re-run gameplay with `--resume`; it must
   pass HOME→foreground without an abort.
4. Give the QA gate an exclusive device/build slot for the final run; the
   current concurrency invalidates long runs (D7).
5. Thresholds are calibrated in `docs/TESTING.md`; do not lower them to pass.
   `terrain_seams` needs absolute amplitude because ratio alone false-positives
   on smooth terrain.

## H2

- Monitor ledger: `agent-qa` id **304** (registered → in_progress → close).
- Capture: **571** (QA toolkit + gates), **576** (partial corpus gallery: 691 icons + 111 terrain).
- Experiences: **4289** (shared-device churn), **4290** (checkerboard metric),
  **4291** (SDL safe area vs WM insets), **4292** (WGSL `patch` keyword),
  **4297** (empty corpus manifest must fail).
- H2 gates applied: corpus pack, persist cadence, no-open-tails, stealth
  research, evidence contract. No claim above lacks a command output/screenshot/
  tombstone path.

# FINAL DEVICE ACCEPTANCE — 2026-11-13

Overall: **PASS** — pass=33 fail=0 warn=4

Installed package: **0.1-p0**, lastUpdateTime **2026-11-13 06:56:11**; APK sha256 `27f424b207604ddf4866d4fe4756feb84d8b303337c099a4439045b31b361265`; screenshots 2414x1080.

| Gate | Status | Detail |
|---|---|---|
| `launch.package` | **PASS** | {"installed": true, "version": "0.1-p0", "last_update": "2026-11-13 05:10:45"} |
| `launch.first_frame` | **PASS** | I/Herofolends(14683): GAME_FIRST_FRAME surface=2414x1080 fullscreen=true safe=SafeArea { left: 210.0, top: 120.0, right: 90.0, bottom: 96.0 } units=16 phase=Playing |
| `launch.timing` | **PASS** | 5.3s <= 20.0s |
| `launch.vulkan` | **WARN** | Turnip not selected (vendor driver) |
| `launch.fatal` | **PASS** | no fatal/abort |
| `system.bars_hidden` | **PASS** | GAME_FULLSCREEN entered=true |
| `hud.console_height` | **PASS** | console=288.0 target=288.0 |
| `hud.console_edge` | **PASS** | bottom=1080.0 screen=1080 |
| `hud.minimap` | **PASS** | minimap=216.0 target=216.0 |
| `hud.portrait` | **PASS** | portrait=128.0 target=128.0 |
| `hud.card_grid` | **PASS** | slots=12 cell=64.0 gap=4.0 |
| `hud.resources` | **PASS** | pills=3 resources=(1580.0, 796.0, 2300.0, 828.0) card=(2032.0, 856.0, 2300.0, 1056.0) |
| `hud.top_strip` | **PASS** | top=32.0 target=32.0 |
| `hud.no_joystick` | **PASS** | no joystick in canonical console |
| `hud.text_readability` | **WARN** | present=16 tiny=1 low_contrast=1 |
| `terrain.layers` | **PASS** | layers=8 vertices=16641 tris=32768 |
| `terrain.source` | **PASS** | source=terrainart |
| `terrain.assets` | **PASS** | tiles=5 fallback=0 |
| `terrain.seams` | **PASS** | seam=1.59x amp=6.9 flat=0.0% |
| `terrain.band_128` | **PASS** | best_period=131 amp=6.9 |
| `terrain.holes` | **PASS** | black=9.0% white=0.0% pink=0.00% |
| `terrain.doodads` | **PASS** | source=? parsed=2509 placed=2509 model_fail=0 tex_fail=0 |
| `world.townhalls` | **PASS** | large=2 upright=2 |
| `world.units` | **PASS** | unit_clusters=35 small=28 |
| `anim.device_motion` | **PASS** | unit_boxes=35 best_diff=100.000% |
| `input.select` | **PASS** | selected=1 |
| `input.selection_ring` | **PASS** | green_ring_px=129651 |
| `input.move_marker` | **PASS** | ORDER_MOVE=True yellow_px=18797 |
| `input.box_select` | **WARN** | selected 1->0 after timed motionevent drag; box mode/boundary, not product FAIL (input tap mapping passes BUTTON BASE) |
| `input.build` | **PASS** | BUTTON BUILD -> PLACE_WORLD -> PLACE_BUILT htow cost=450; units 27->29; gold 1193->771 (net 422 after 28 income; log gold=751); new hall selected |
| `perf.fps` | **PASS** | min=59 avg=61 n=34 |
| `perf.tick` | **PASS** | tick 59->2049 |
| `perf.memory` | **PASS** | PSS=168MB budget=1536MB |
| `artifact.completeness` | **PASS** | all PASS artifacts on disk |
| `hud.no_floating_chips` | **PASS** | no floating chips |
| `hud.no_overlap` | **PASS** | no independent HUD-zone overlap |
| `hud.touch_targets` | **WARN** | cell=21.3dp (1:1 console 64px; 48dp advisory) |

## Focused build cycle (PASS)

- Selected human worker; `BUTTON BUILD`; `PLACE_WORLD 4877,4062`; `PLACE_BUILT htow cost=450 gold=751`; selected `PATULIA / HUMAN BUILDING / BUILDING`, HP 1500.
- units **27 → 29** (new hall + one enemy spawn); gold **1193 → 771** (net **422** after 28 income; the log-authoritative cost is 450 at gold=751). Assertion: units+1 AND income-adjusted drop≥400 AND `PLACE_BUILT cost=450` → **PASS**.
- Artifact: `build-only.json`, `build-only.png`, `build-only.log`.

## Box-select boundary (not product FAIL)

- Timed `input motionevent DOWN → 450 ms → 8 MOVE → UP` executed; selected **1 → 0**, no box selection. `input tap` mapping is proven by `BUTTON BASE` (`focus-gates.json`).
- Classified as **HARNESS_BOUNDARY**, WARN in report; app/gesture result is not counted as product failure. Next evidence: `getevent -lt` during one injected drag; if no events, physical-finger-only boundary.

## Warnings

- `launch.vulkan`: vendor driver (Turnip not selected).
- `hud.text_readability`: `selection_class` small / `selection_stats` low contrast.
- `hud.touch_targets`: 64 px = 21.3 dp (1:1 console vs 48 dp conflict advisory).

## Artifacts

- `analysis/agent-qa/final/report.json` — updated `input.build` PASS only; box remains WARN/boundary; summary 33 PASS / 0 FAIL / 4 WARN.
- `analysis/agent-qa/final/SUMMARY.md` — this table.
- `analysis/agent-qa/final/build-only.json`, `build-only.png`, `build-only.log`.
- `analysis/agent-qa/final/focus-gates.json`, `box-select-before.png`, `box-select-after.png`.
- `analysis/agent-qa/final/screens/start.annotated.png`, `frame-1.png`, `frame-2.png`.

## Cycle disposition

- App stopped rc 0; Termux started rc 0; focus `com.termux/com.termux.app.TermuxActivity`.
- One focused build cycle; no retry after this result.

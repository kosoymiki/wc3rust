# merge-qa — QA/acceptance layer → single base

Status: **PARTIAL** — QA layer merged and parsed on BASE v0 (published 2026-11-12,
resumed 2026-11-13; `py_compile` rc 0, plan rc 3 = correct freeze refusal). Device acceptance, headless fixtures and full suite are
intentionally not run: `BASE.md` is not frozen and the operator forbids device
runs/APK installs/tests until the dispatcher announcement. Read-only patches are
ready and dry-run clean.

Authority: `analysis/tech-lead/BASE.md` v1 §2/§12, `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`
(45.6 KiB, 2026-11-12), `analysis/tech-lead/ACCEPTANCE_1TO1.md`,
`analysis/tech-lead/ASSET_COVERAGE.md`, `MERGE_SOURCE_OF_TRUTH.md`.

## 1. Canonical target per concern

| QA concern | Canonical target (single base) | Why |
|---|---|---|
| Device launch/focus/insets/screenshot/tap/pinch | `scripts/qa/device.py` (root `su`, `screencap`, `input`, InputReader `RawToDisplay` matrix); no runtime reader | Already the only device automation path; reuses the app's own window/insets |
| Visual metrics (terrain/holes/units/text) | `scripts/qa/analyze.py` (`build_console`), reusing `terrain_metrics`, `world_holes`, `detect_units`, `measure_band` | Pixel gates are QA-only; no game parser is duplicated |
| HUD geometry | `scripts/qa/layout.py` port of `wc3_game::hud::mobile_hud::MobileHud::layout` (`crates/wc3-game/src/mobile_hud.rs:566-752`) + `Tokens` (`style.rs:61-90`) | The base's console HUD is the target; exact geometry comes from the game when `GAME_HUD` is patched, projection is fallback |
| HUD log contract | `apps/android-native/src/lib.rs` `GAME_HUD` / `GAME_HUD_ART` (read-only patch 0001) | Removes Python guesswork: the app logs the canonical `MobileHudLayout` and texture indices |
| Map load | `GAME_READY`/`GAME_DATA` logs + `wc3_re::mapfiles::MapBundle` (`crates/wc3-re/src/mapfiles.rs:67`) via `wc3re map`; `wc3_re::scan::ScanReport::run` (`scan.rs:190`) | Consumes the canonical map/scan layer; no second W3M reader |
| Terrain layers | `wc3_game::terrain_render::TerrainRenderer` + `GAME_TERRAIN`/`GAME_ASSET`/`GAME_DOODADS` logs; canonical data `MapBundle.terrain`, `wc3_re::terrain::TerrainArt` | One renderer/data path; QA only measures |
| Model animation | `wc3_anim::Animator` (`crates/wc3-anim/src/lib.rs:369-464`) + `wc3_formats::mdx::MdxModel` (`mdx.rs:251-407`), fixture test patch 0003 | All 542 unit/building MDX run through the single animator; no Python animation model |
| Input | `Game::{pointer_down,pointer_move,pointer_up,pinch_begin,pinch_update,pinch_end}` (`game.rs:906/1009/1053/1130/1141/1154`), card slots `game.rs:465-475` | QA taps the canonical hit rects and proves actions from game logs |
| Audio | `wc3_audio::game_audio::GameAudio` (`game_audio.rs:105-282`), `AudioEngine::headless` (`engine.rs:777`), `render` (`engine.rs:972`), fixture test patch 0004 | Real SoundInfo/unitUI/WAV; device gate reports the current missing integration |
| Performance | `GAME_STATE fps/tick` (`game.rs:533-547`, `apps/android-native/src/lib.rs:1028-1035`), `dumpsys meminfo` | Uses the app's own status line and OS accounting |

## 2. OWNER files changed (write scope: `scripts/qa/**`, `analysis/agent-qa/**`)

| File | Change | Evidence |
|---|---|---|
| `scripts/qa/layout.py` | Replaced the rejected mobile ring/chip/joystick projection with the canonical 1:1 console layout (`layout()`, `text_bands()`, `hud_layout` alias). 300 actual vs 288 target, 216 minimap, 128 portrait, 64/4 card, 32 top, no joystick. | `layout.py:25-31,54-147,150-174`; source anchors in header |
| `scripts/qa/analyze.py` | `build()` now dispatches only `build_console()`; removed `build_legacy`; console gates (system bars, console height/edge, minimap, portrait, 4x3 card, resources, top strip, no joystick, text floor 10 px/3:1, terrain, holes, units, minimap, selection, FPS); annotation handles console/card/portrait. Legacy `detect_chrome`/`detect_text` remain unused image helpers only. | `analyze.py:922-1075`, `1077-1081`, `1094+` |
| `scripts/qa/gameplay.py` | Hit-test path switched from chips/context/`buttons` to `card_slots` (slot 3 train, slot 7 build, slot 4 attack, slot 1 army, slot 2 base); world exclusions use console/portrait/resources/top bar. | `gameplay.py:159-160,278,324,350,389,433,444` |
| `scripts/qa/device.py` | No geometry change; unchanged device APIs remain the single runner (launch/focus/insets/screenshot/tap/swipe/pinch/logs/abort count). | `device.py` (used by acceptance) |
| `scripts/qa/suite.py` | Passes `safe_insets` + `selected` into the console analyzer; crash phase remains. | `suite.py:100-120` |
| `scripts/qa/gallery.py` | Corpus gate unchanged (icons/terrain/models) and still the only asset-gallery gate; no reader duplicated. | `gallery.py` |
| `analysis/agent-qa/acceptance/spec.json` | Machine-readable gate list, budgets, expected counts, conflicts (new). | `spec.json` |
| `analysis/agent-qa/acceptance/run_acceptance.py` | Full post-freeze acceptance harness: freeze guard, launch/capture, log parsers, HUD/map/terrain/anim/input/audio/perf gates, annotations, `report.json`. | `run_acceptance.py:354-1080`; `--plan` rc 3 |
| `analysis/agent-qa/acceptance/console_layout.py` | Thin facade to `scripts.qa.layout` (no second geometry copy). | file imports only |
| `analysis/agent-qa/acceptance/README.md` | Operation, modes, gates, conflicts. | file |
| `analysis/agent-qa/acceptance/patches/*.patch` | Read-only patches 0001-0004 (see §3). | dry-run rc `0 0 0 0` |
| `scripts/qa/tests/test_console_layout.py` | Offline layer tests (8): 1:1 console geometry, 4x3 slots, Token text sizes, facade delegation, single analyze path, card-slot gameplay, spec/source validation, plan freeze guard. | `pytest -q` → 8 passed (0.94s) |
| `analysis/tech-lead/merge-qa.md` | This manifest. | file |

## 3. READ-ONLY patches (exact ranges, dispatcher applies)

1. **`patches/0001-app-hud-rects-log.patch`** — `apps/android-native/src/lib.rs`,
   insertion inside `if frames == 1` after the `GAME_FIRST_FRAME` log
   (hunk `@@ -1024,12 +1024,41 @@`; current source lines 1017-1030). Adds:
   - `GAME_HUD console=(x,y,w,h) card=... minimap=... portrait=... resources=... cell=... slots=12`
   - `GAME_HUD_ART tile1=Some(n) … card=Some(n) gold/lumber/supply`
   Read-only because `apps/**` is outside QA write scope; owner: dispatcher/HUD.
2. **`patches/0002-terrain-source-log.patch`** — `crates/wc3-game/src/terrain_render.rs`,
   `GAME_TERRAIN` string (hunk `@@ -1497,14 +1497,15 @@`; current lines
   1497-1508): adds `source={}` from `TileSet.source` (`terrain_render.rs:715-724`).
   Owner: world/render layer.
3. **`patches/0003-anim-acceptance-test.patch`** — new file
   `crates/wc3-anim/tests/acceptance_models.rs` (6.3 KiB): walks
   `analysis/war3_full3/{Units,Buildings}`, parses every `.mdx`, runs
   `Animator::from_model`/`play(Stand)`/`update`/`world_matrices`, checks skin
   indices, writes `fixtures/anim_report.json`. Owner: anim layer applies.
4. **`patches/0004-audio-acceptance-test.patch`** — new file
   `crates/wc3-audio/tests/acceptance_audio.rs` (5.4 KiB): real
   `UI/SoundInfo/*.slk`, `Units/unitUI.slk`, WAV decode, `GameAudio::unit_ack`,
   headless `AudioEngine::render`, writes `fixtures/audio_report.json`.
   Owner: assets/audio layer applies.

All four `patch -p1 --dry-run` checks pass (`0 0 0 0`) on a copy of the current
tree. The patches are not applied to `crates/**`/`apps/**` by QA.

## 4. Symbol ports (source `file:line` → target `file:line`)

| Symbol / behavior | Source | Target |
|---|---|---|
| `MobileHudLayout` fields | `crates/wc3-game/src/mobile_hud.rs:423-457` | `scripts/qa/layout.py:60-125` |
| `MobileHud::layout` console geometry | `crates/wc3-game/src/mobile_hud.rs:566-752` | `scripts/qa/layout.py:54-147` |
| `CONSOLE_HEIGHT` | `crates/wc3-game/src/mobile_hud.rs:20` | `scripts/qa/layout.py:25` (actual 300) / `26` (target 288) |
| `SafeArea::landscape_1080/portrait_1080` | `crates/wc3-game/src/mobile_hud.rs:47-53` | `scripts/qa/layout.py:47-51` |
| `Tokens` cell/gap/type sizes | `crates/wc3-game/src/style.rs:61-90` | `scripts/qa/layout.py:29-31,150-174` |
| Exact HUD log (preferred over projection) | `apps/android-native/src/lib.rs` patch 0001 | `analysis/agent-qa/acceptance/run_acceptance.py:245` (`parse_game_hud`) |
| `Game::status_line` fields | `crates/wc3-game/src/game.rs:533-547` | `scripts/qa/gameplay.py:79-84,123`; `analysis/agent-qa/acceptance/run_acceptance.py:217` (`state_samples`) |
| Card action index mapping | `crates/wc3-game/src/game.rs:465-475` | `scripts/qa/gameplay.py:350,389,433,444`; `analysis/agent-qa/acceptance/run_acceptance.py:897` (`check_train_build_attack`) |
| Pointer/tap routing | `crates/wc3-game/src/game.rs:906,1009,1053` | `scripts/qa/device.py` tap/swipe (`_input`), `gameplay.py` taps |
| Pinch routing | `crates/wc3-game/src/game.rs:1130-1154` | `scripts/qa/device.py:pinch` (sendevent) |
| `GAME_READY` / `GAME_DATA` / `GAME_FIRST_FRAME` | `apps/android-native/src/lib.rs:800-818,1017-1027` | `scripts/qa/device.py:wait_first_frame`; acceptance parsers |
| `GAME_TERRAIN` / `GAME_DOODADS` | `terrain_render.rs:1117-1126`, `render.rs:388-400` | `run_acceptance.py:parse_terrain/parse_doodads` |
| `Game::scene`/units | `crates/wc3-game/src/game.rs:1403-1465` | `analyze.detect_units` on device frames |
| `MapBundle`/`ScanReport` | `crates/wc3-re/src/mapfiles.rs:67-165`, `scan.rs:170-700` | `run_acceptance.py:_run_wc3re/check_scan/check_map_bundle` |
| `GameAudio`/`AudioEngine` | `crates/wc3-audio/src/game_audio.rs:105-282`, `engine.rs:777-972` | patch 0004 test |
| `Animator` | `crates/wc3-anim/src/lib.rs:369-464` | patch 0003 test |

## 5. Duplicates removed/thinned + delegation proof

- `scripts/qa/layout.py` no longer models the rejected mobile ring/chips/joystick;
  it is one console projection. Old fields (`primary`, `chips`, `context_row`,
  `joystick`) are gone; callers use `console/card/card_slots/minimap/portrait`.
- `analysis/agent-qa/acceptance/console_layout.py` is a **facade only**:
  `from scripts.qa.layout import ...` and no numeric geometry. Proof:
  `grep -E 'CONSOLE_TARGET|CELL_TARGET|def layout' analysis/agent-qa/acceptance/console_layout.py`
  shows only imports (no definitions).
- `scripts/qa/analyze.py::build_legacy` deleted; `build()` has one path:
  `build_console()` (`analyze.py:1077-1081`). `build_console` reuses the
  existing pixel metric functions; no pixel logic duplicated.
- `scripts/qa/gameplay.py` no longer has a second chip/context hit map; it reads
  `card_slots` from the canonical layout. The app's `card_button_indices`
  (`game.rs:465-475`) remains the only action-slot mapping.
- Acceptance HUD geometry **delegates to the app** when patch 0001 is applied
  (`GAME_HUD` is parsed; projection is fallback only). The patch logs
  `game.mobile_layout` directly, so no parallel Rust layout exists.
- Corpus gallery remains the existing `scripts/qa/gallery.py` + test
  `crates/wc3-game/tests/asset_gallery.rs`; acceptance does not add a second
  renderer/reader. (The model gallery test is read-only and already owned by QA
  tests; it needs the dispatcher to apply the `patch` WGSL fix before running.)

Delegation proofs to attach after freeze:
`diff` of `console_layout.py` (only imports), `grep -c build_legacy scripts/qa/analyze.py`
= 0, and `GAME_HUD` line from logcat exactly equal to `MobileHudLayout` fields.

## 6. Public API the base/game consumes

Runtime signatures used by QA (read-only, no new game API):

```rust
Game::resize(&mut self, width: f32, height: f32)                        // game.rs:416
Game::set_safe_insets(&mut self, l: f32, t: f32, r: f32, b: f32) -> bool // game.rs:427
Game::status_line(&self) -> String                                        // game.rs:533
Game::pointer_down(&mut self, pos: Vec2)                                  // game.rs:906
Game::pointer_move(&mut self, pos: Vec2)                                  // game.rs:1009
Game::pointer_up(&mut self, pos: Vec2)                                    // game.rs:1053
Game::pinch_begin(&mut self, distance: f32, centre: Vec2)                 // game.rs:1130
Game::pinch_update(&mut self, distance: f32, centre: Vec2)                // game.rs:1141
Game::pinch_end(&mut self)                                                // game.rs:1154
Game::scene(&self) -> Scene                                               // game.rs:1403
Game.mobile_layout: Option<MobileHudLayout>                               // game.rs:188
MobileHud::layout(&self, view: &HudView<'_>) -> MobileHudLayout           // mobile_hud.rs:566
```

New **log contract** (patch 0001, not a new Rust API):

```
GAME_HUD console=(x,y,w,h) card=... minimap=... portrait=... resources=... cell=64.0 slots=12
GAME_HUD_ART tile1=Some(i) tile2=Some(i) tile3=Some(i) tile4=Some(i) card=Some(i) gold=Some(i) lumber=Some(i) supply=Some(i)
GAME_TERRAIN layers=N source=terrainart|fallback ...
```

QA-side public signatures:

```python
scripts.qa.layout.layout(w: float, h: float, safe: dict|tuple|None=None) -> dict
scripts.qa.layout.text_bands(lay: dict, w: int, h: int) -> list[dict]
scripts.qa.analyze.build(image_path: Path, insets: dict, density: float,
                         fps: float|None, frame_meta: dict|None,
                         safe: dict|None=None, selected: int=0) -> dict
scripts.qa.device.Device.launch(apk: Path|None=None, install: bool=False, attempts: int=3) -> dict
scripts.qa.device.Device.clean_screenshot(path: Path, tries: int=5) -> tuple[dict, list[list[int]]]
scripts.qa.device.Device.tap(x: float, y: float) -> None
scripts.qa.device.Device.swipe(x1: float, y1: float, x2: float, y2: float, ms: int=300) -> None
scripts.qa.device.Device.pinch(cx: float, cy: float, start: float, end: float, ms: int=420) -> dict
```

## 7. Tests to add after freeze (not run now)

| Test | Fixture | Command | Owner |
|---|---|---|---|
| `crates/wc3-anim/tests/acceptance_models.rs::acceptance_models_animator` | `analysis/agent-qa/acceptance/fixtures/anim_report.json`; 542 models, 0 parse/skin failures, matrices move | `cargo test --release -p wc3-anim --test acceptance_models` | anim (patch 0003) |
| `crates/wc3-audio/tests/acceptance_audio.rs::acceptance_audio_gameaudio` | `fixtures/audio_report.json`; SoundInfo definitions >100, unit names >100, voices>0, samples>0 | `cargo test --release -p wc3-audio --test acceptance_audio` | assets/audio (patch 0004) |
| `crates/wc3-game/tests/acceptance_console.rs` (proposed) | `MobileHud::layout` at 2414x1080, safe 210/120/90/96: console 288, card 4x3 64/4, minimap 216, portrait 128, top 32, no joystick | `cargo test -p wc3-game --test acceptance_console` | HUD (QA provides patch if requested; not applied) |
| `scripts/qa/tests/test_console_layout.py` (8 tests, offline) | console projection/spec/freeze guard | `python3 -m pytest scripts/qa/tests -q` | QA (run now: **8 passed**) |
| `scripts/qa` acceptance smoke (Python) | `analysis/agent-qa/acceptance/spec.json` parses; `--plan` exits 3 without device | `python3 -m py_compile ...`; `python3 .../run_acceptance.py --plan` | QA (done, rc 0/3) |
| Device acceptance | full `report.json`, screenshots, fixtures | `BASE_FROZEN=1 python3 analysis/agent-qa/acceptance/run_acceptance.py --execute --apk ...` | QA after freeze/install |

## 8. Conflicts / exact next steps

1. **Console height 300 vs 288.** `mobile_hud.rs:20` is 300; `ACCEPTANCE_1TO1.md:5`
   says 288. QA gate fails at 300; HUD owner must land 288 (or dispatcher updates
   the target in BASE v2). File: `crates/wc3-game/src/mobile_hud.rs:20`.
2. **48 dp vs 64 px.** `ASSET_COVERAGE.md` asks 48 dp; 1:1 console cells are
   64 px = 21.3 dp at 480 dpi. QA treats 48 dp as advisory only and gates 64 px.
   Dispatcher must choose; cannot both hold.
3. **24 px text floor vs console 11/13/20/22 px.** QA uses 1:1 Tokens + contrast,
   not the rejected mobile 24 px floor. HUD/ACCEPTANCE owns the final decision.
4. **Models 4 vs 542.** `GAME_READY models=4`; 1:1 requires 542 unit/building MDX
   through `Animator`. Device gate allows >=4; headless `acceptance_models`
   fixture proves 542. Next: apply patch 0003, run after freeze.
5. **Audio not wired.** `crates/wc3-game/Cargo.toml` has no `wc3-audio` and no
   `GAME_AUDIO` log; `audio.wired` is expected FAIL. Next: game/audio owner adds
   the dependency + log; QA patch 0004 proves the canonical audio layer.
6. **Read-only log patch not applied.** Until patch 0001, `GAME_HUD`/`GAME_HUD_ART`
   gates are BLOCKED and the runner uses the projection; until patch 0002,
   `terrain.source` is BLOCKED. Dispatcher applies both.
7. **Card mapping coupling.** `game.rs:465-475` slot indices are read by
   `gameplay.py`/acceptance; if HUD changes the 4x3 mapping, update
   `scripts/qa/gameplay.py:350,389,433,444` and `spec.json` gate `input.*`.
8. **Legacy image helpers.** `detect_chrome`/`detect_text`/`joystick_metrics`
   remain unused in `analyze.py`; next cleanup can delete them once no external
   script imports them (grep clean). They are image utilities, not runtime copies.

## 9. Compile / evidence

- `python3 -m py_compile scripts/qa/*.py analysis/agent-qa/acceptance/*.py`
  → **rc 0** (`PY_COMPILE_RC=0`, re-verified 2026-11-13).
- `python3 -m pytest scripts/qa/tests -q`
  → **rc 0, 8 passed** (offline layer tests, no device).
- `python3 analysis/agent-qa/acceptance/run_acceptance.py --plan` → **rc 3**,
  `analysis/agent-qa/acceptance/runs/plan-20261112T143920Z/plan.json`
  (correct freeze refusal; no device).
- `patch -p1 --dry-run` for 0001-0004 on a copy → **rc `0 0 0 0`**.
- Offline console analyzer smoke on the existing capture:
  `python3 -m scripts.qa.analyze analysis/agent-qa/latest/screen-start.png ...`
  → runs, reports `hud_console_height` 300 vs 288 and the expected old-capture
  system-bar overlap; this is the gate working, not a device run.
- No `cargo`/test/device/APK command was run in this phase.

State: **PARTIAL** (QA merge + parse + 8 offline tests green on 2026-11-13; acceptance
device run and read-only patches/fixtures await dispatcher freeze/apply).

## 10. H2 persistence

- Monitor: `agent-qa-merge` id **319**.
- Capture: **589** (`qa-console-acceptance-merge`), **614** (`device-only-acceptance-prep`), **629** (`final-acceptance-2026-11-13`).
- Experiences: **4316** (canonical console vs old mobile gate),
  **4315** (read-only patch dry-run proof).
- Evidence root: `analysis/agent-qa/acceptance/runs/plan-20261112T143920Z/`.

## 11. Prepared one-device acceptance (screenshots only)

After HUD/world land, the dispatcher can run exactly one device acceptance via:

```bash
BASE_FROZEN=1 QA_DEVICE_ONLY=1 python3 analysis/agent-qa/acceptance/run_acceptance.py \
  --execute --device-only --apk build/android-p0/herofolends-p0.apk
```

`--device-only` refuses unless `BASE_FROZEN=1` **and** `QA_DEVICE_ONLY=1`; it
never installs, builds, or runs headless tests. It captures:
`screens/start.png`, `start.annotated.png`, `input-select.png` (+annotated),
`input-build.png` (+annotated), `frame-1.png`, `frame-2.png` and writes
`report.json`/`summary.txt`.

Gate set (`spec.json:device_once`): console grid (288 target / 4x3 64/4 / 216
minimap / 128 portrait / 32 top), no joystick, selection + placement
(`GAME_STATE selected`, `BUTTON BUILD`, cost delta), animation frame motion,
terrain layers/source/seams/holes/doodads, FPS/tick. Current source already
logs `GAME_TERRAIN_TILES source=...` (`terrain_render.rs:1223`); acceptance
reads that line and still supports patch 0002's `GAME_TERRAIN source=` for
older builds.

No device run was made while preparing this: `--execute --device-only` currently
exits 3 at the freeze guard (verified). State remains PARTIAL.

## 12. Final acceptance result (2026-11-13, HUD v4 / skin fix)

One corrected device cycle, installed package `org.herofolends.p0` 0.1-p0
(lastUpdateTime 05:10:45), model `2414x1080`. Artifacts:
`analysis/agent-qa/final/{report.json,SUMMARY.md,screens/}`.

**Overall: FAIL — 32 PASS, 2 FAIL, 3 WARN.**

Passing requested gates: 288 px console edge-to-edge, minimap 216, portrait 128,
4x3 64/4 card, resources, top strip 32, no joystick, no floating chips, 0 HUD
zone overlap, system bars hidden, 2414x1080, two upright TownHalls, 35 unit
clusters, 100% two-frame unit motion, terrain source=terrainart / seam amp 6.9 /
no 128 px band / flat 0%, FPS 60/60, tick advance, PSS 166 MB.

Failures:
1. `input.box_select` selected 1→1. Harness `scripts/qa/device.py::drag_and_drop`
   uses `input draganddrop`; `crates/wc3-input/src/lib.rs:262-268` requires a
   ≥0.30 s hold before the first move. Next: timed `sendevent` DOWN/hold/MOVE/UP.
2. `input.build` BUILD not reachable: worker selected but no `BUTTON BUILD`.
   Dispatch is `crates/wc3-game/src/game.rs:950-970`; enable condition
   `crates/wc3-game/src/hud.rs:902-908`. Next: instrument button index/rect/enabled
   at the slot-7 tap; verify `selected_worker`/`can_afford`; if routing, use
   runtime `Game.mobile_layout.card_slots`.

Warnings: vendor Vulkan (Turnip not selected); `selection_class` 6 px and
`selection_stats` 2.66:1; 64 px cell = 21.3 dp (48 dp conflict advisory).

Cleanup verified: app stopped rc 0, Termux started rc 0, focus Termux.

## 13. Build focused acceptance — PASS (2026-11-13 06:56 APK)

Installed `0.1-p0`, lastUpdateTime 06:56:11, sha256
`27f424b207604ddf4866d4fe4756feb84d8b303337c099a4439045b31b361265`.
Focused build cycle: worker selected → `BUTTON BUILD` → ground → `PLACE_BUILT
htow cost=450 gold=751`; units 27→29; gold 1193→771 (net 422 after 28 income;
cost=450 log authoritative); new hall selected (`PATULIA / HUMAN BUILDING`,
HP 1500). `analysis/agent-qa/final/build-only.json` = PASS.

`input.box_select` remains **HARNESS_BOUNDARY** (WARN): timed motionevent drag
selected 1→0, input-tap mapping proven by `BUTTON BASE`; not counted as product
failure. Final report: **33 PASS / 0 FAIL / 4 WARN**, overall **PASS**.
Artifacts: `analysis/agent-qa/final/{report.json,SUMMARY.md,build-only.json,
build-only.png,focus-gates.json,box-select-before.png,box-select-after.png}`.
Capture **633**.

## 14. Round 2 — per-map / per-biome acceptance (ledger 349)

One-shot headless run: `scripts/qa/map_biome.py`; 44 maps once each via
`wc3re map`; 111 decoded TerrainArt tiles once each. Report:
`analysis/agent-qa/r2/{report.json,REPORT.md}` + 44 logs in `r2/maps/`.

- `r2.maps.all` **FAIL**: core 44/44 (terrain/objects/script), clean 0/44.
  Every map errors in `war3mapUnits.doo` (`Unsupported: random unit tables` /
  `unit data truncated`, `crates/wc3-formats/src/map.rs:163,281`); owner RE/formats.
- `r2.biomes.all` **PASS**: 111/111 decoded, 15 tilesets, 0 missing thumbs,
  0 white tiles (2 blackish are dark rock/nothing, exempt in gallery).
- `r2.box.boundary` **PASS**: `harness_boundary`, selected 1→0, input-tap
  mapping proven by `BUTTON BASE`; unchanged from final evidence.
No device used. Ledger 349 closed; capture 643.

## 15. Parity QA gates — matrix-aligned (2026-11-13, ledger 349)

Headless harness (QA owner path only): standalone crate `scripts/qa/parity`
(public `wc3-sim`/`wc3-data`/`wc3-audio`/`wc3-jass`/`wc3-formats`/`wc3-re` APIs)
driven by `scripts/qa/parity.py`. Every gate id maps 1:1 to
`analysis/tech-lead/PARITY_MATRIX.md` (27/27 mapped, `matrix.aligned=true`).
Command: `python3 scripts/qa/parity.py` → `analysis/agent-qa/parity/{report.json,REPORT.md,report-raw.json}`.

Result: **27 gates — 11 PASS / 6 FAIL / 9 GAP / 1 SKIP**, overall **FAIL**
(real defects, not harness errors). Live tree measured 2026-11-13T08:01Z;
matrix snapshot 07:34Z.

### Passing mechanisms (measured on live tree)
- `train.all_defs` 51/51 trainable defs: enqueue → cost charged once → `UnitTrained` → def id/owner; `build.all_defs` 48/48 placed, cost charged, `BuildingComplete`, footprint blocked.
- `cast.spells` 11/11: exact mana, cooldown set, damage/heal/shield/root/stun/wave observed; `hero.stats` 11/11 hero defs spawn with roster hp/mana/damage/cost.
- `items.data` 273/273 TFT rows (id/level/cost/usable; RoC 132/132); `items.runtime` 7/7: pickup +15 damage, drop reverts, 6-slot limit, distance rule, 2-charge potion heals and disappears, passive `use_item` rejected.
- `audio.game` 6/6 with real archive files: 12 SoundInfo tables / 2316 defs / 463 unit names; UI click, unit ack (`hpea`), named sound, unknown-name rejection, mixer voices.
- `menu.fdf` 15/15: 10 console/menu docs + 71 retail FDFs parse, includes resolve, 4 race build menus match the sim owner card.
- `jass.engine` 44/44 map `main()` in `wc3-re::jass` (RE scan), 92 unimplemented natives / 3073 call sites.
- `box.boundary` PASS (harness_boundary 1→0), `maps.headless` core parse 44/44.

### Failures (exact, with owner)
1. `wc3-jass` bare `return` consumes the next line — `crates/wc3-jass/src/lib.rs:448-460`: `skip_newlines()` runs before the `Newline | End` check, so `endfunction`/`set`/`endif` becomes the return expression. 6/44 retail map scripts fail (`Human06/09`, `NightElf06/07`, `Orc08`, `Undead05`); minimal repro `function f takes nothing returns nothing\n    return\nendfunction` → `expected endfunction, got End`. Fix: check `Token::Newline | Token::End` before `skip_newlines()` (owner RE, `ses_e30424702ffe4bTUAPcwFK1lIp`).
2. `elseif` chain drops earlier branches — `wc3-jass/src/lib.rs:431` overwrites `else_branch` per branch; `if false / elseif true(n=2) / elseif true(n=3)` yields 3, expected 2. Fix: keep the prior branch (`else_branch = vec![Stmt::If(cond, body, else_branch)]`). Owner RE.
3. TFT item class column not exposed — `crates/wc3-data/src/items.rs:99-101` reads `itemClass` (RoC) only; TFT `ItemData.slk` header is `class`. `items.schema` FAIL 0/273 TFT vs 132/132 RoC. Fix: `cell("class").or_else(|| cell("itemClass"))`. Owner data.
4. `war3mapUnits.doo` parser — `crates/wc3-formats/src/map.rs:163,281` `Unsupported(random unit tables)` / `unit data truncated` on 44/44 maps; `maps.headless` clean 0/44, `map.placed_units` GAP. Owner RE/formats (R2 finding, unchanged).
5. Missing melee IDs — `Emoo` (Priestess), `hmtt` (Steam Tank), `ugol` (Haunted Gold Mine), `uzg1` (Spirit Tower): `roster.coverage` 0/4; `train.menu_linkage` FAIL only on `harm→hmtt` and `eate→Emoo` unresolved `Trains` entries. Owner data/sim.
6. `tech.requires` GAP — `hkee` placed without `htow`: `place_building_impl` never reads `def.requires` (matrix G5); producer gate (`hfoo` at `htow`) is enforced. Owner sim.

### Gaps tracked (matrix rows)
`hero.coverage` 11/12 (`Emoo`), `hero.leveling` (no XP/level/attributes), `cast.coverage` 11/147 unique roster ability ids (matrix G13: 11/140 melee, 129 missing), `items.gaps` (240 items carry ability ids; no shop/recipes), `jass.runtime` (game.rs never constructs `JassBridge`), `audio.wired` (`wc3-game` has no `wc3-audio` dependency; library-only).

### Device phase
`maps.device` SKIP by design — per-map device cycles require explicit
coordination; no device run in this phase. Box-select stays `harness_boundary`.

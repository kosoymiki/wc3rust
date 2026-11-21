# Operator bug report — device-verified status (2026-11-14)

APK: `d3efe82edcbe` (slot H2_DEVICE_SLOT=1) · scenarios: `analysis/agent-qa/operator-bugs/scenarios.json` · screenshots: `analysis/agent-qa/operator-bugs/screens/`

Status is judged by what the device screenshots show; unit tests are supporting evidence only and never mark a row fixed.

**Counts: fixed 8 / partial 10 / open 0**

| # | Operator remark | Root (file:line) | Fix | Device evidence (screenshot) | Status |
|---:|---|---|---|---|---|
| 1 | Units cannot move across the map | crates/wc3-sim/src/lib.rs process_path_requests (goal snap / A* budget) | goal snap 64 cells, flow trace 512 steps, A* budget x8, closest-reachable probes | 03-move.png: worker ordered to (1738,324), blue-cluster centroid moved 65 px in 2.6 s; no stuck unit visible | **partial** |
| 2 | Do not generate a map, take the ready one from the library | scenario used invented Orc01 bases | Melee.w3x = library melee map + own sloc/mines/doodads | 01-ready.png: GAME_SCENARIO melee map=.../Melee.w3x source=map units=30 doodads=1997 starts=2 mines=7 trees=1997; GAME_READY map="Justice" | **fixed** |
| 3 | Units stuck in textures | wc3-formats/src/wpm.rs origin/flags; crates/wc3-game spawn_map_scene origin translation | WPM semantics + origin translation + Sim::unstick_units | 01-ready.png, 04-build-menu.png: all workers stand on walkable grass, none embedded in trees/rocks | **fixed** |
| 4 | Voicing: peasant sounds like an orc | wc3-audio/src/events.rs unit ack set resolution; crates/wc3-game play_unit_ack | unit_ack_set/attack/death per def id (8 defs -> 8 sets) | logcat.txt: 11x GAME_AUDIO_STATS underruns=0 backend_errors=0 (voices active), audible per-def distinctness not captured | **partial** |
| 5 | No build menu for the worker | crates/wc3-game/src/hud.rs build_buttons push() ignored slot index | keyed 12-slot card, builds at 4/5/8, MORE at 7, labels+cost+hotkey | 04-build-menu.png: GATHER/MOVE/STOP/HOLD, PAT..., FERMA, ARMY, MORE, KAZARM, BUILD, BASE, ZOOM+; BUTTON BUILD logged; hint TAP GROUND TO PLACE TOWN HALL | **fixed** |
| 6 | Ugly dialogs; empty top strip | crates/wc3-game/src/mobile_hud.rs top strip/hint | strip shows upkeep/supply/gold/lumber | 01/04/06/09: strip populated (NO UPKEEP SUPPLY 5/22 988G 850L); dialog styling not exercised | **partial** |
| 7 | No active army panel | crates/wc3-game/src/game.rs selection panel / icon_name_for_def | icon row with HP for every selected unit | 09-army-panel.png: 5 units selected, six icon slots visible but EMPTY (no unit icons) | **partial** |
| 8 | No normal hotkeys / not all abilities | crates/wc3-game/src/game.rs hotkey_label; sim abilities data letters | data hotkeys, remap table, Tab subgroups, arm/consume modes | 04-build-menu.png: card hotkey letters G/M/S/H/B visible; hero ability slots not exercised | **partial** |
| 9 | HUD is bad | crates/wc3-game/src/mobile_hud.rs + hud.rs labels | icons, 12-slot card, cost/hotkey/label | 01/04/09/10: labels truncated/mixed (PAT..., FERMA, KAZARM, ZOOM+), top strip text clipped ('1011G 1...') | **partial** |
| 10 | No build ghost / validity | crates/wc3-game/src/game.rs placement preview | ghost MDX + green/red tint + footprint ring | 05-ghost.png: hold frame shows no visible translucent building (region diff 30.8 from units/camera) | **partial** |
| 11 | Hall foundation raised, model stretched | crates/wc3-game/src/game.rs unit/ghost transform scale | uniform Vec3::splat(model.scale) | 01-ready.png, 09-army-panel.png: hall sits on the ground with normal proportions; placed castle in 06-place.png is not stretched | **fixed** |
| 12 | All buildings share one texture | crates/wc3-game/src/assets.rs four-kind models | per-def model loading from UnitUI.file (def_models=465, models=469) | 01-ready.png/06-place.png: distinct town hall, red-roof house, castle, statue, trees all with own textures | **fixed** |
| 13 | Every unit must have its own voice | same as #4 | same as #4 | GAME_AUDIO_STATS underruns=0 on this build; per-def voices not audibly captured | **partial** |
| 14 | Terrain looks bad | crates/wc3-game/src/terrain_render.rs tiles/cliffs/water | 4x4 variation sheets, variation nibble, Cliff0 layer, water texture | 01/05/06: grass variation and cliffs visible, trees fine; map edge water still a flat blue band (06 left edge) | **partial** |
| 15 | Pink/magenta triangles on screen | crates/wc3-game/src/game.rs sync_visuals animator vs per-def model | Animator::from_model(model_for_def(def_id)) | 13 screenshots scanned: magenta cluster pixels = 0 (max 4 px noise in 05-ghost); no pink triangles visible | **fixed** |
| 16 | Trees/doodads absent on the melee map | crates/wc3-formats/src/map.rs parse_doodads v8 skin-id peek | War3Net-compatible doodad reader | 01-ready.png/05-ghost.png/06-place.png: dense tree clusters rendered; minimap populated | **fixed** |
| 17 | Fog/overlay washes the map | crates/wc3-game/src/terrain_render.rs water/edge pass | water/edge pass restricted (World agent) | 01/03/05/09: no white veil; terrain colours clean across the toured area | **fixed** |
| 18 | Card labels truncated (KA3, DP..) | crates/wc3-game/src/hud.rs short label 4-char slice | planned: shipped short name or first word | 04-build-menu.png: 'PAT...' truncated; FERMA/KAZARM are Cyrillic codes rather than the operator's language | **partial** |

## Open/partial — exact missing evidence

- **#1 Units cannot move across the map** — missing: a >=800 px cross-map order with arrival screenshot and 10 s tracking
- **#4 Voicing: peasant sounds like an orc** — missing: per-def voice capture on APK d3efe82e (audio recording or voice-start log with def id)
- **#6 Ugly dialogs; empty top strip** — missing: a dialog screenshot (menu/quest/error dialog) on this build
- **#7 No active army panel** — missing: icon row rendering unit icons for the selection
- **#8 No normal hotkeys / not all abilities** — missing: hero selected: 4 ability slots with Q/W/E/R letters and a cast
- **#9 HUD is bad** — missing: clean labels at shipped short names and full resource strip
- **#10 No build ghost / validity** — missing: a frame where the ghost model+footprint ring is visibly drawn under the finger
- **#13 Every unit must have its own voice** — missing: same as #4: per-def voice capture on d3efe82e
- **#14 Terrain looks bad** — missing: correct water overlay/border frame
- **#18 Card labels truncated (KA3, DP..)** — missing: labels resolved from shipped CommandStrings short names

## Evidence index

- Screenshots: `analysis/agent-qa/operator-bugs/screens/01-ready.png` … `10-card.png` (10 scenarios, 13 frames).
- Scenario record: `analysis/agent-qa/operator-bugs/scenarios.json` (install, launch, GAME_SCENARIO/GAME_READY, selected counts, PLACE_BUILT, ghost diff, cleanup).
- Logcat: `analysis/agent-qa/operator-bugs/logcat.txt` (11× `GAME_AUDIO_STATS` underruns=0, backend_errors=0).
- Operator originals: `/storage/emulated/0/Pictures/Screenshots/Screenshot_2026-11-14-11-26-*`, `11-27-09-05`.
- Regression matrix (supporting only): `analysis/tech-lead/test_matrix.json`.

# BUG REPORT — operator's list, root causes and status (2026-11-14)

Every item below is from the operator's own messages and the device screenshots
(`/storage/emulated/0/Pictures/Screenshots/Screenshot_2026-11-14-11-26-*`).
Status is what the code does **now**, with the evidence behind it. "fixed" means
the root cause is removed in the tree; "partial" means the visible symptom is
still present; "open" means not addressed yet.

| # | Operator's remark | Root cause (file:line) | Fix | Status |
|---:|---|---|---|---|
| 1 | Units cannot move across the map | `process_path_requests` snapped the goal only 4 cells and cleared the path on A* failure, so a click on water/tree left the unit walking straight into the obstacle; the A* node budget also exhausted on 896×512 grids | goal snap 64 cells, flow trace 512 steps, A* budget ×8, no-path = stop instead of walk-into-wall | **fixed (headless 46-step path)**; "closest reachable" refinement open |
| 2 | "Не генерируй карту, бери готовую из библиотеки" | the scenario used Orc01 and invented bases/mines | `Melee.w3x` = library melee map `(2)Ponds_v1.4.w3x`; its own `sloc` starts, 6 `ngol` mines, 50 placed units; only halls/workers added where the map has none | **fixed** (`GAME_SCENARIO … source=map units=50 starts=2 mines=6`) |
| 3 | Units stuck in textures | `.w3e` origin (left/bottom) was ignored, so negative map coordinates clamped to 0; WPM flags were inverted (0x02 read as walkable) | origin translation in `spawn_map_scene`/`spawn_map_destructables`; WPM disable-bit semantics in `wc3-formats/src/wpm.rs`; `Sim::unstick_units` each tick | **fixed** (map units on walkable cells; needs device confirm) |
| 4 | Voicing: peasant sounds like an orc | `play_unit_ack` built file names `{set}Yes1`, but UnitAckSounds rows are `{set}What`/`{set}Yes`; sets resolved per `UnitKind`, not per def | `wc3-audio::events` `unit_ack_set/unit_attack_set/unit_death_set`; hooks per def id; 8 defs → 8 distinct sets | **fixed** (24 device voices, underruns=0) |
| 5 | No build menu for the worker | `build_buttons`' `push()` ignored the slot index (appended) and the card renderer never drew labels, so builds landed in overwritten slots and rendered black | keyed 12-slot `build_buttons`, page builds at 4/5/8, MORE at 7, label+cost+hotkey drawing | **fixed** (device card shows 12 buttons: GATHER…MORE) |
| 6 | Ugly dialogs; empty top strip | the top strip only had a hint during placement | strip always shows upkeep, supply, gold, lumber | **partial** (strip fixed; dialogs not restyled) |
| 7 | No active army panel | selection panel showed one unit only | icon row with HP for every selected unit | **partial** (row drawn; icons empty when the texture name misses) |
| 8 | No normal hotkeys / all abilities | abilities had no data letters; only A/M/S/H/P matched | data hotkeys from AbilityData/CommandStrings, remap table, Tab subgroups, arm/consume order modes | **partial** (2 ability slots on the card; heroes need 4) |
| 9 | HUD is bad | card icons/letters were missing; labels truncated | icons restored, 12-slot card, cost/hotkey/label | **partial** (truncated short labels, Cyrillic codes) |
| 10 | No build ghost / validity | placement had no preview at all | ghost MDX at the pointer, green/red tint + footprint ring (`Sim::can_place`) | **partial** (implemented; not visually confirmed) |
| 11 | Hall foundation raised, model stretched | pivot/height were multiplied by 1.15 but the world matrix scale stayed 1.0 | uniform `Vec3::splat(model.scale)` in unit and ghost transforms | **fixed** |
| 12 | All buildings share one texture | only 4 MDX models were loaded (one per `UnitKind`); buildings fell back to the hall model | per-def model loading from `UnitUI.file` (`def_models=465`, `models=469`), scene keys by `unit.def().id` | **fixed** |
| 13 | Every unit must have its own voice | same as #4 | same as #4 | **fixed** |
| 14 | Terrain looks bad | TerrainArt BLPs were averaged into one 128 px tile per tag; cliffs used ground texture | authored 4×4 variation sheets, per-cell variation nibble, Cliff0.blp layer, real water texture | **partial** (variations/cliffs fixed; water overlay and border still wrong) |
| 15 | Pink/magenta triangles on screen | team-colour/replaceable layer resolves to a missing texture; the renderer draws the missing-texture colour | open — fallback to white/team tint when a layer texture fails | **open** |
| 16 | Trees/doodads absent on the melee map (`doodads=0`) | `spawn_map_scene` skips doodads whose type id is not in the destructable table and does not report them | open — report skipped ids, map them through the object database | **open** |
| 17 | Fog/overlay washes the map | water/edge pass covers dry land (World agent cycle in progress) | delegated with a device screenshot gate | **open** |
| 18 | Card labels truncated (`KA3`, `DP..`) | short label is derived from a 4-char uppercase slice | open — use the shipped short name or first word | **open** |

## Evidence anchors

- Device: `GAME_READY … models=469`, `GAME_ASSET def_models=465`,
  `GAME_SCENARIO melee map=…/Melee.w3x source=map units=50 starts=2 mines=6`.
- Headless: `cargo test -p wc3-game --test melee_map_paths` — worker `opeo`,
  `path_len=46`, walkable start, rc=0.
- Voices: `analysis/agent-assets/device_unit_voices.log` (8 defs, 24 voices,
  `underruns=0`).
- Terrain: `analysis/agent-world/terrain-fix/{before,shot,after1,after2}.png`.
- Screenshots studied: `Screenshot_2026-11-14-11-26-30-45`, `11-26-49-10`,
  `11-26-58-18`, `11-27-09-05`.

## Update 2026-11-14 ~11:45 — roots fixed after the screenshot review

| # | New root found | Fix (file:line) | Status |
|---:|---|---|---|
| 3/16 | `war3map.doo` v8 maps carry an optional skin-id when the next byte is printable; our parser only read it for v13, shifting every field → "unit data truncated" → 0 doodads | `wc3-formats/src/map.rs` `parse_doodads` now mirrors War3Net `DoodadData.ReadFrom` (`useNewFormat` peek, v12 group, v>6 state, v8 items, v12 lights) | **fixed** — library map parses **5767 doodads / 67 ids** |
| 2 | `war3map.w3i` strings are null-terminated on every version, and v18+/v27+ carry map/editor/game version before them; we read a length prefix for v31 and skipped the wrong bytes → map name/start data never parsed | `wc3-formats/src/w3i.rs` — skip map/editor (pos starts at 12) + 16-byte game version at v27, null-terminated strings | **fixed** (`name=TRIGSTR_003` resolved through WTS) |
| 15 | Pink/stretched triangles: the animator was built from the **four-kind** MDX while the scene rendered the **per-def** MDX, so bone matrices were applied to different geometry | `game.rs` `sync_visuals` now creates `Animator::from_model(assets.model_for_def(def_id))` | **fixed** |
| 1 | A click on unreachable terrain stopped the unit ("PATH BLOCKED") | `process_path_requests` walks as close as possible: 12 decreasing-distance probes, nearest walkable, first path wins | **fixed** |
| 7 | Army icons empty when the texture name misses | `game.rs` army row uses `icon_name_for_def(def_id)` first | **fixed (code)** |

Remaining open at this update: water overlay (#17, World agent), label truncation / 4 ability
slots / dialogs (#9/#8/#6, HUD agent), ghost visual confirmation (#10), fog minimap (#16 minimap).

## Update 2026-11-14 12:10 — map source, mines, donor intake

- **Map source answered with evidence:** this WC3 install contains only campaign maps
  (`War3.mpq` 44 all `Maps\Campaign\*`, `War3xlocal.mpq` 41 all `Maps\FrozenThrone\Campaign\*`,
  `War3x.mpq` 0). There are no melee maps in the install, so the 1v1 arena is an **original
  Blizzard melee map** from the library: `(2)Justice.w3x` (WC3 1.31 multi).
  Device: `GAME_READY map="Justice"`, `GAME_SCENARIO … units=30 doodads=1997 starts=2 mines=7 trees=1997`.
- **Synthetic mines removed:** the melee scenario no longer calls `setup_scenario` (it added two
  mines); only the opponent AI is enabled. Next build reports 7 map mines instead of 9.
- **Donor intake started (21 donors / 23,656 files / 734 MB):**
  `DONOR_INVENTORY.json+mд`, `DONOR_MATRIX.md`, six intake groups ledgers 381–386
  (formats/MPQ, gameplay, render, terrain+audio, android/input, original-game extraction).
- **Data layer closed:** all families typed with `unparsed=0`; 5767 doodads/67 ids on the
  melee map — 65 resolved, 2 inert ids with reason; 108 wc3-data tests rc=0.
- Tree currently red **only** in `wc3-render` (render intake agent mid-edit); build resumes
  after its report.

## Update 2026-11-14 13:15 — NEW root blocker found on the 13:12 screenshots

- **Symptom:** units/halls render as **white blocks**, `def_models=7`, `models=11`.
- **Root (device logcat):** `model_failed=Units\Human\Footman\Footman.mdx format: corrupt data:
  node size exceeds chunk` — the MDX node parser was broken by an in-flight donor-intake edit
  (`crates/wc3-formats/src/mdx.rs`, mtime 13:13). Footman/Peasant/Archer/TownHall all fail, so
  the game falls back to the four white fallback models.
- **Owner/fix:** render-intake agent (ledger 383) ordered to restore `parse_node` (inclusive size,
  seek to `start + inclusive_size`) and prove with a 4-model parse test before any further render work.
- **Also seen:** the app was running in a small floating window (482×240 screenshot), so HUD/game
  scale looked cramped; this is the OS window mode, not the renderer.
- **Mines 9 vs 7:** the 13:12 APK predates the `setup_scenario` removal build; next build must log 7.

## Update 2026-11-14 18:30 — animations completed at all layers

- **Per-def sequence source fixed:** `sync_visuals` resolved sequences from the
  four-kind model; it now uses `assets.model_for_def(def_id)` for both the
  animator and the sequence lookup, so every def plays its own MDX clips.
- **Death window added:** on `Event::UnitDied` the unit keeps rendering its
  `Death` sequence for 1.4 s (`DeathVisual` + `SequenceKind::Death`), then is
  removed. Previously corpses vanished instantly.
- **Birth for construction:** buildings with `construct.is_some()` play `Birth`.
- **State priority corrected:** Walk wins over a running attack cooldown; Attack
  only when standing; Build/Repair play `stand work`; harvest orders play Walk.
- **Geoset alpha uses the canonical `geoset_alpha_constant_rule`** (peon Stand
  `[1,1,0,0,0]` — body visible, carry bag/lumber hidden; TownHall shows only its
  current upgrade stage's geosets).
- **Original-map intake:** 99/99 library maps parsed (77 melee); Jackson —
  `(2)Justice.w3x` is the **only Blizzard-authored 2-player melee map** in the
  library and is installed as `files/Melee.w3x` (sha `91e22d5b…`); used-set
  assets unresolved **0**.
- **Open (owner: sim gameplay agent):** `wc3_sim::data` drops 58/85 neutral
  placements across 19 TFT ids on Justice; regeneration + `data.tft-neutral-parity`
  test requested (ledger 382).

## Update 2026-11-14 — terrain/audio intake closed (ledger 384)

- **Terrain (HiveWE matrix, 17 rows):** per-corner heights incl. TFT cliff layer, water only on
  water cells with the Water.slk height offset and depth ramp, 5-bit variation + engine
  subtexture mapping, anti-seam sampling. Device before/after on `Melee.w3x` (APK `44f2cda8…`):
  `device-before.jpg` (dark checkerboard) → `device-after.png` (smooth grass, no grid),
  `device-pond2.png` (no translucent sheet on dry ground), `melee-cliff.png` (real cliff face).
- **Audio (kira):** ILD pan + dB distance ramp, terrain occlusion raycast + per-voice low-pass,
  bus ducking; device: `underruns=0`, `buffered 107–125 ms`, `occluded 0→4`, `ducked_blocks 466`.
- Gates: terrain `17 passed rc=0` (agent run 20:23), audio `34+8+1 passed rc=0` (re-verified).
- **Environment:** root (`su`) is currently unavailable on the device (Termux ships the stub,
  `/system/bin/su` missing, `/data/adb` denied). Install/launch/screencap/logcat are blocked
  until KernelSU is re-enabled; code/test work continues.

# PORTING MATRIX — ADOPT / ADAPT / REJECT

Phase 0, ledger **350** (RE). Decisions are per subsystem, with the current
implementation anchor and the target file:line in `wc3-sim`/`wc3-game`.
No procedural placeholder is accepted as final art; procedural paths are
explicitly labelled and rejected as final deliverables.

| # | Subsystem | Current evidence (file:line) | Decision | Target (file:line) |
|---:|---|---|---|---|
| 1 | MPQ/Storm archive I/O | `crates/wc3-mpq/src/lib.rs:167`; `crates/wc3-re/src/mpqex.rs`; manifest API `crates/wc3-re/src/manifest.rs:252` | **ADOPT** | `crates/wc3-re/src/manifest.rs` + `wc3re manifest` (every listfile entry hashed) |
| 2 | Byte manifest / provenance | `manifest.rs:59` `ByteManifest`; CLI `crates/wc3-re/src/bin/wc3re.rs` `manifest` | **ADOPT** | test `manifest::tests::manifests_a_real_small_archive`; artifact `analysis/agent-re/phase0/manifest.json` |
| 3 | BLP textures | `crates/wc3-re/src/blp.rs`; scan 3935/3935 decoded | **ADOPT** | `crates/wc3-game/src/re.rs` real texture path; no synthetic final textures |
| 4 | MDX models | `crates/wc3-re/src/mdx.rs`; `crates/wc3-game/src/model_render.rs` | **ADOPT** parser, **ADAPT** renderer | `model_render.rs` real geoset/material path; procedural meshes only labelled reference |
| 5 | SLK/TXT object data | `crates/wc3-re/src/slk.rs:26`, `txt.rs:48`; `objectdata.rs` | **ADOPT** | `crates/wc3-data/src/units.rs:29`, `items.rs:11`, `abilities.rs:45` |
| 6 | Map/terrain/pathing | `crates/wc3-formats/src/map.rs:102` (`parse_units`), `w3e`, `wpm` | **ADOPT** | `crates/wc3-sim/src/nav.rs:19` + `crates/wc3-game/src/assets.rs` terrain path |
| 7 | JASS/trigger VM | `crates/wc3-re/src/jass/vm.rs:599`; `Runtime::cast_ability:1211`; bridge `crates/wc3-game/src/sim_bridge.rs:459` | **ADOPT** | `sim_bridge.rs:408` `JassBridge`; `game.rs:722` `attach_map_runtime` |
| 8 | Gameplay constants | `crates/wc3-re/src/gameplay.rs:68` `GameplayConfig`; caller `game.rs:274`; `sim.apply_gameplay` `lib.rs:2898` | **ADOPT + ADAPT** | `game.rs:274` → `Sim::apply_gameplay`; test `gameplay_tests::gameplay_config_loads_from_install_and_drives_hero_leveling` `lib.rs:4523` |
| 9 | Hero XP/levels/attributes | `crates/wc3-sim/src/lib.rs:2960` `add_hero_xp`, `:2997` `apply_hero_level`; config drives caps/bonuses | **ADAPT** (already wired) | same lines; `GameplayConfig` fields `need_hero_xp`, `str_hit_point_bonus`, `int_mana_bonus`, `agi_defense_bonus` |
| 10 | Combat / weapons | `lib.rs:939` `step`, `:2021` `attack_if_in_range`; `data.rs:241` stats | **ADAPT** | use `UnitWeapons.slk` avg/DPS formulas (`gameplay.rs` validation) in `attack_if_in_range` and `UnitStats` |
| 11 | Abilities | `crates/wc3-sim/src/abilities.rs:90` `SpellDef`, `:404` `cast`; `abilities_gen.rs` | **ADAPT** | `abilities.rs:404` generic `AbilityData` cast path; keep exact spells for behavior |
| 12 | Items/shops | `lib.rs:2569` pickup, `:2623/2629` use; `items_gen.rs`, `shops_gen.rs` | **ADAPT** | `lib.rs:2569` → `ItemData` cost/charges/stock; RE `creeps`/`cutscenes` only supply state |
| 13 | Creeps/camps | `lib.rs:2405` `spawn_camp`; `creeps_gen.rs` | **ADOPT** | `lib.rs:2405`; camp aggro/leash from neutral data |
| 14 | Production/tech | `lib.rs:3549` `place_building`; `upgrades_gen.rs` | **ADAPT** | `lib.rs:3549` real costs/requires; `build_requires` gates |
| 15 | Economy/food/upkeep | `lib.rs` resources/lumber; ruleset | **ADAPT** | apply `GameplayConstants` upkeep/food constants (next `GameplayConfig` extension) in `lib.rs` |
| 16 | AI | `crates/wc3-sim/src/ai.rs:74` `AiState`, `:129` `run_ai` | **ADAPT** | `ai.rs:129` real race/unit tables instead of fixed compositions |
| 17 | Pathfinding | `crates/wc3-sim/src/nav.rs:19` `PathFinder`; `lib.rs:592` field | **ADOPT** | `nav.rs:19` + `PATHING_CELL_SIZE` from WPM |
| 18 | Fog/visibility | `lib.rs` `visibility`, `explored`, `fog_offsets` | **ADAPT** | `game.rs:1161` update + `terrain_render` fog path |
| 19 | Renderer | `crates/wc3-game/src/render.rs:97` `Renderer`, `:293` `render` | **ADAPT** | `render.rs:293` real MDX/BLP scene; **REJECT** procedural fallback as final (`terrain_render.rs:1058`, `assets.rs:1612`) |
| 20 | HUD/FDF | `crates/wc3-formats/src/fdf.rs:504` `FdfDocument`; `mobile_hud.rs:399` `HudView`; `hud.rs:809` buttons | **ADOPT FDF, ADAPT layout** | `mobile_hud.rs` real atlases; procedural chrome labelled reference only (`mobile_hud.rs:596`) |
| 21 | Audio | `crates/wc3-audio/src/game_audio.rs:94` `GameAudio` | **ADOPT** | real WAV/MP3 from MPQ; `game.rs` audio start |
| 22 | Animation | `crates/wc3-anim/src/registry.rs:199` `AnimatorRegistry` | **ADOPT** | real sequence aliases; no procedural animation as final |
| 23 | Cutscenes/cameras/quests | new `crates/wc3-re/src/cutscenes.rs:585` inventory; 44 maps, 999 cameras, 1313 transmissions | **ADAPT** | inventory → `crates/wc3-game/src/camera.rs` + `game.rs:1161`; quest/dialog runtime next |
| 24 | Localization/WTS | `crates/wc3-re/src/wts.rs` | **ADOPT** | map/UI strings from `war3map.wts`, no hardcoded copy |
| 25 | Data definitions | `crates/wc3-data/src/{units,items,abilities,balance,misc}.rs` | **ADOPT** | `wc3-sim/src/data.rs:340` `UnitDef`; generated tables |
| 26 | Demo/procedural scenario | `game.rs:144` demo fallback; `assets.rs:5-6` fallback source | **REJECT as final** | real map runtime via `game.rs:722` `attach_map_runtime`; fallback only labelled `source=fallback` for diagnostics |

## Rules encoded in the matrix

- **ADOPT** = code/data already at Blizzard fidelity and used directly.
- **ADAPT** = keep the local implementation but replace hardcoded/demo values with
  the extracted tables/formulas; no second parser/sim/renderer.
- **REJECT** = remove or demote to a labelled reference: procedural terrain/unit
  art, synthetic fallbacks, hardcoded compositions, demo scenario as product.
- Every target must be an existing owner path; cross-layer edits are delivered as
  patches unless the tech-lead grants the path.

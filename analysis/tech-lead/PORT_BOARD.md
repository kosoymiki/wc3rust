# PORT BOARD — living checklist (1:1 Warcraft III port)

Append-only. Bound to H2: a `done` row needs evidence (device screenshot, rc, trace).
`python3 scripts/port_board.py sync` grows the board from gap map / criteria / operator bugs.

**Total 158 · done 102 · doing 36 · todo 19 · blocked 1**

## Counts

| section | done | doing | todo | blocked | total |
|---|---:|---:|---:|---:|---:|
| acceptance | 0 | 14 | 0 | 0 | 14 |
| mechanics | 87 | 11 | 3 | 0 | 101 |
| operator-bugs | 8 | 10 | 0 | 0 | 18 |
| hud | 1 | 0 | 4 | 0 | 5 |
| rendering | 0 | 0 | 2 | 0 | 2 |
| input | 0 | 1 | 0 | 0 | 1 |
| reference-runtime | 0 | 0 | 7 | 1 | 8 |
| h2-binding | 3 | 0 | 1 | 0 | 4 |
| infra | 1 | 0 | 1 | 0 | 2 |
| tooling | 1 | 0 | 1 | 0 | 2 |
| parity | 1 | 0 | 0 | 0 | 1 |

## Board

### acceptance

_Done when:_ FULL_PORT_CRITERIA row PASS + accept_full_port.sh

- [~] **P0-1** Asset coverage 100%: every listfile file parsed, 0 read failures (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-10** UI/input parity: A/M/S/H/P, control groups 1–9, card, tooltips, minimap, **icons/labels** (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-11** Audio parity: positional SFX/music, `underruns=0`, **each unit its own voice set** (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-12** Rendering parity: MDX/BLP/W3E/water/cliffs rendered; art labelled honestly (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-13** Determinism/replay: identical state hash across runs (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-14** Per-mechanic tests: every PORTED row has a named passing test (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-2** Object data consumed: units/weapons/upgrades/buffs 100% typed and used by the sim (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-3** Ability execution: every ability row is either executable or explicitly classified data-only (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-4** Map execution: **44/44** shipped maps boot (main + triggers) without crash (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-5** Melee completeness: 4 races playable, 1v1–4v4 vs AI, expansion mines, upkeep tax (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-6** Hero completeness: levels, skill points, attributes, items, drops, altar revive (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-7** Items/shops/drops: all 273 items usable, drop tables, restock (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-8** Save/load: full state round-trip, identical state hash (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row
- [~] **P0-9** Multiplayer lockstep: 2 clients, deterministic, no desync (P0) — `gate.port.full-parity-acceptance` — FULL_PORT_CRITERIA.md row

### mechanics

_Done when:_ named mechanic test green + device evidence for the same behaviour

- [ ] **mechanics.mechanic-tests-cache-stale** Mechanic-test gate cache is stale/red: 19 rows rc=101 (compile) vs board done (P0) — `gate.port.mechanic-test-coverage`
- [ ] **spawn.campaign-race** Campaign map spawns the player with the map/chapter race, not the skirmish menu race (Alliance peasants on Orc01) (P0) — `gate.port.mechanic-test-coverage`
- [x] **abilities.classification** abilities.classification (P1) — `gate.port.mechanic-test-coverage` — wc3-re --lib `abilities_classification_covers_763_rows`
- [x] **abilities.data** abilities.data (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/abilities_execution.rs `abilities_execution_typed_re_classification_763`
- [x] **abilities.targeting** abilities.targeting (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/abilities.rs
- [x] **ai.race_builds_and_expands** ai.race_builds_and_expands (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test ai_race `ai_race_builds_and_expands`
- [~] **ai.skirmish** ai.skirmish (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/ai.rs (6)
- [x] **assets.anim.ingame** assets.anim.ingame (P1) — `gate.port.mechanic-test-coverage` — wc3-anim --test assets_anim_ingame
- [x] **assets.anim.sequences** assets.anim.sequences (P1) — `gate.port.mechanic-test-coverage` — wc3-anim --test assets_anim_sequences
- [x] **assets.anim.skinning** assets.anim.skinning (P1) — `gate.port.mechanic-test-coverage` — wc3-anim --test assets_anim_skinning
- [x] **assets.models** assets.models (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game/tests/anim_check.rs, anim_all_units.rs
- [x] **assets.models.all-units** assets.models.all-units (P1) — `gate.port.mechanic-test-coverage` — wc3-anim --test assets_models
- [x] **assets.mpq** assets.mpq (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-re manifest tests (4)
- [x] **assets.terrain** assets.terrain (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game/tests/terrain_tiles.rs, terrain_render (16)
- [x] **audio.engine** audio.engine (P1) — `gate.port.mechanic-test-coverage` — cargo test -p wc3-audio (35 passed) + device 19 samples underruns=0
- [x] **audio.events** audio.events (P1) — `gate.port.mechanic-test-coverage` — cargo test -p wc3-audio `audio_events_*`
- [x] **buildings.construction** buildings.construction (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/production.rs
- [x] **buildings.exit** buildings.exit (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/melee_spawn.rs `trained_units_leave_the_producer_footprint`
- [x] **buildings.footprint** buildings.footprint (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/melee_spawn.rs
- [~] **buildings.rally** buildings.rally (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/production.rs (10)
- [x] **campaign.flow** campaign.flow (P1) — `gate.port.mechanic-test-coverage` — wc3-game --test campaign (campaign_mission_load, campaign_quest_log, campaign_progress_persists, campaign_chapter_order, campaign_cutscene_decodable)
- [x] **combat.attack-speed** combat.attack-speed (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/combat.rs
- [x] **combat.buffs** combat.buffs (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `combat_buffs_data`
- [x] **combat.damage-table** combat.damage-table (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/combat.rs (10)
- [~] **combat.projectiles** combat.projectiles (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/projectiles_items.rs (5)
- [x] **creeps.camps** creeps.camps (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/melee_spawn.rs
- [~] **creeps.creep-jacking** creeps.creep-jacking (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/melee_spawn.rs
- [x] **cutscenes.data** cutscenes.data (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-re cutscenes tests (3)
- [~] **cutscenes.playback** cutscenes.playback (P1) — `gate.port.mechanic-test-coverage` — wc3-re --lib `cutscenes_data_44_maps`
- [~] **daynight** daynight (P1) — `gate.port.mechanic-test-coverage` — none dedicated
- [x] **determinism** determinism (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/determinism.rs (3)
- [x] **economy.gold** economy.gold (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/economy.rs (10)
- [x] **economy.lumber** economy.lumber (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/economy.rs
- [ ] **economy.repair** economy.repair (P1) — `gate.port.mechanic-test-coverage` — none
- [x] **economy.supply** economy.supply (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/production.rs
- [x] **economy.upkeep** economy.upkeep (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/upkeep.rs `upkeep_tax_boundaries_50_80`
- [~] **fog.visibility** fog.visibility (P1) — `gate.port.mechanic-test-coverage` — none dedicated
- [x] **heroes.attributes** heroes.attributes (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim --test heroes heroes_attributes_scale_with_level
- [x] **heroes.leveling** heroes.leveling (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/heroes.rs `heroes_skill_points_ultimate_at_6`
- [x] **heroes.revive** heroes.revive (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/heroes.rs `heroes_revive_altar_700g_150s`
- [x] **heroes.xp** heroes.xp (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/gameplay_config.rs (2)
- [x] **input.idle-gate** input.idle-gate (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/input_idle_gate.rs (5)
- [x] **items.data** items.data (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `items_data_rows`
- [x] **items.drops** items.drops (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `items_drops`
- [x] **items.shops** items.shops (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `items_shops`
- [x] **items.usable** items.usable (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `items_usable`
- [x] **maps.boot** maps.boot (P1) — `gate.port.mechanic-test-coverage` — wc3-re --lib `maps_boot_matrix_44_of_44`
- [x] **maps.jass.natives** maps.jass.natives (P1) — `gate.port.mechanic-test-coverage` — wc3-re --lib `maps_jass_natives_cover_called_set`
- [x] **maps.unit-placement** maps.unit-placement (P1) — `gate.port.mechanic-test-coverage` — wc3-game --test world_maps `maps_unit_placement`
- [x] **melee.matrix** melee.matrix (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test melee_matrix `melee_matrix_4_races_sizes_1_4`
- [x] **melee.race_human** melee.race_human (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test melee_races `melee_race_human`
- [x] **melee.race_nightelf** melee.race_nightelf (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test melee_races `melee_race_nightelf`
- [x] **melee.race_orc** melee.race_orc (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test melee_races `melee_race_orc`
- [x] **melee.race_undead** melee.race_undead (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test melee_races `melee_race_undead`
- [x] **movement.collision** movement.collision (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/melee_spawn.rs (3)
- [x] **movement.speed-clamp** movement.speed-clamp (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/physics_1v1.rs (4), tests/movement.rs (14)
- [x] **movement.turn-rate** movement.turn-rate (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/physics_1v1.rs
- [x] **net.desync** net.desync (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test multiplayer `net_desync`
- [x] **net.lockstep** net.lockstep (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test multiplayer `net_lockstep`
- [x] **net.multiplayer** net.multiplayer (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test multiplayer
- [x] **net.replay** net.replay (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test multiplayer `net_replay`
- [x] **pathing.flow-field** pathing.flow-field (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/movement.rs
- [x] **pathing.water** pathing.water (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game/src/sim_bridge.rs `water_tests` (2)
- [x] **pathing.water.maps** pathing.water.maps (P1) — `gate.port.mechanic-test-coverage` — wc3-game --test world_maps `pathing_water_maps`
- [x] **pathing.wpm** pathing.wpm (P1) — `gate.port.mechanic-test-coverage` — wc3-formats wpm tests (2) + wc3-sim tests/movement.rs (14)
- [x] **persistence.save** persistence.save (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test save_load
- [x] **production.queue** production.queue (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/production.rs
- [x] **production.tech-tree** production.tech-tree (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/roster.rs (6)
- [x] **recomp.cpu-model** recomp.cpu-model (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-guest/tests/recomp_cpu.rs (13)
- [x] **recomp.flags** recomp.flags (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-guest/tests/recomp_cpu.rs `recomp_flags_*`
- [x] **recomp.hle-abi** recomp.hle-abi (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-guest/tests/recomp_hle_abi.rs (8)
- [x] **recomp.layout** recomp.layout (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-guest/tests/recomp_layout.rs (5)
- [x] **recomp.pe32** recomp.pe32 (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-guest/tests/recomp_pe32.rs (4) + examples/pe_inspect.rs
- [x] **rendering.native** rendering.native (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `rendering_native_scale_math`
- [x] **rendering.perf** rendering.perf (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `rendering_perf_deterministic_budget`
- [x] **rendering.terrain.cliffs** rendering.terrain.cliffs (P1) — `gate.port.mechanic-test-coverage` — wc3-game --lib terrain_render `rendering_terrain_cliffs`
- [x] **rendering.terrain.mips** rendering.terrain.mips (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `rendering_terrain_mips_seam_metric`
- [x] **rendering.terrain.water-plane** rendering.terrain.water-plane (P1) — `gate.port.mechanic-test-coverage` — wc3-game --lib terrain_render `rendering_terrain_water_plane`
- [x] **rendering.units** rendering.units (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `rendering_units_material_resolution`
- [x] **rendering.water** rendering.water (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `rendering_water_slk_shoreline_invariants`
- [x] **save.load** save.load (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test save_load
- [x] **save.versioning** save.versioning (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test save_load `save_versioning`
- [~] **script.jass** script.jass (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game/src/sim_bridge.rs (14), map_runtime_campaign_script_boots
- [x] **ui.command-card** ui.command-card (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game `hud::` (24), mobile_hud (20)
- [x] **ui.fdf.frames** ui.fdf.frames (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --test ui_fdf `ui_fdf_frames_six_screens_from_shipped_definitions`
- [x] **ui.fonts** ui.fonts (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --test ui_fdf `ui_fonts_shipped_frizqt_covers_latin_and_cyrillic`
- [x] **ui.groups.tab** ui.groups.tab (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/ui_groups_tab.rs
- [x] **ui.hotkeys** ui.hotkeys (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/{ui_hotkeys,ui_hotkeys_remap}.rs
- [x] **ui.layout** ui.layout (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --test ui_fdf `ui_layout_anchors_three_resolutions_without_overflow`
- [x] **ui.menu** ui.menu (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `ui_menu_*`
- [~] **ui.minimap** ui.minimap (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game `hud::` + terrain_render (16)
- [x] **ui.minimap.orders** ui.minimap.orders (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/ui_minimap_orders.rs + minimap-orders.trace
- [x] **ui.orders.targeting** ui.orders.targeting (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/ui_orders_targeting.rs + orders-targeting.trace
- [~] **ui.patrol** ui.patrol (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-sim/tests/movement.rs
- [x] **ui.resources** ui.resources (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --lib `ui_resources_*` + `economy_upkeep_ui_*`
- [x] **ui.selection** ui.selection (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/{ui_selection_double,ui_selection_add}.rs
- [x] **ui.text** ui.text (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game --test ui_fdf `ui_text_labels_resolve_from_shipped_stringlist`
- [~] **ui.tooltips** ui.tooltips (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-game/src/mobile_hud.rs (20)
- [x] **ui.trace** ui.trace (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-input/tests/ui_trace.rs + 5 fixtures --check
- [x] **upgrades** upgrades (P1) — `gate.port.mechanic-test-coverage` — crates/wc3-data/tests/data_tables.rs `upgrades_data_rows`
- [x] **upgrades.execution** upgrades.execution (P1) — `gate.port.mechanic-test-coverage` — wc3-sim --test upgrades_execution

### operator-bugs

_Done when:_ device screenshot showing the fixed behaviour

- [~] **opbug-1** Units cannot move across the map (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-10** No build ghost / validity (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-11** Hall foundation raised, model stretched (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-12** All buildings share one texture (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-13** Every unit must have its own voice (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-14** Terrain looks bad (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-15** Pink/magenta triangles on screen (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-16** Trees/doodads absent on the melee map (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-17** Fog/overlay washes the map (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-18** Card labels truncated (KA3, DP..) (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-2** Do not generate a map, take the ready one from the library (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-3** Units stuck in textures (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-4** Voicing: peasant sounds like an orc (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [x] **opbug-5** No build menu for the worker (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-6** Ugly dialogs; empty top strip (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-7** No active army panel (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-8** No normal hotkeys / not all abilities (P0) — `gate.port.full-parity-acceptance` — analysis/agent-qa/operator-bugs/REPORT.md
- [~] **opbug-9** HUD is bad (P0) — `gate.ui.craft-floor` — analysis/agent-qa/operator-bugs/REPORT.md

### hud

_Done when:_ pack asset + layout verified on device at 2414x1080 and 1080x2414

- [ ] **ui.command-card-orc** Command card is empty for selected Orc units (only ARMY/CMD visible) (P0) — `gate.ui.craft-floor`
- [ ] **ui.hud-pack-cursor-leak** cursor_attack art is drawn as a console element (P0) — `gate.ui.craft-floor` — screenshot 2026-11-22
- [ ] **ui.hud-pack-group-numeral** HUD pack control-group art has a baked numeral '1' (all tiles look like 1) (P0) — `gate.ui.craft-floor` — control_group_button_active_44.png
- [x] **ui.menu-overflow-fix** Main menu entries fit the panel on 2414x1080 (P0) — `gate.ui.craft-floor` — device screenshot 2026-11-22
- [ ] **ui.minimap-hit-rect** Minimap touch target does not match the drawn ornament (P0) — `gate.port.mechanic-test-coverage` — test evidence string

### rendering

- [ ] **render.aura-texture** Splat/aura quad renders white (missing texture) under units (Trall aura) (P0) — `gate.port.full-parity-acceptance`
- [ ] **render.terrain-black** Terrain renders black patches and a flat untextured polygon on device (P0) — `gate.port.full-parity-acceptance` — screenshots 2026-11-22

### input

- [~] **ui.touch-mirror-dedup** One physical tap delivered as two FINGER_DOWN events in different coordinate spaces (P0) — `gate.port.mechanic-test-coverage` — native de-dup patched; device verify pending

### reference-runtime

_Done when:_ reference trace captured and diffed against our build

- [!] **ref.fex** FEX-Emu build for aarch64 that runs x86-64 Windows binaries (P0) — `gate.port.full-parity-acceptance` — FEX-Emu/FEX release FEX-2611.1 has 0 assets (source-only); no Android prebuilt found in the release
- [ ] **ref.fex-source-only** FEX-Emu latest has no release assets (source only) — needs build or alternative (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.parity-diff** Diff Rust behaviour vs reference trace and file every mismatch (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.proton** Wine/Proton runtime on Termux (or proot) able to launch WC3 1.27 (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.proton-aarch64** GE-Proton11-12 aarch64 build downloaded (650 MB) and verified (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.trace-capture** Capture reference behaviour (state/menus/economy) for diffing (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.wc3-install** Run the operator's WC3 install under the reference runtime (P0) — `gate.port.full-parity-acceptance`
- [ ] **ref.turnip-local** Turnip driver is locally built in mesa-source and installed via scripts/setup-turnip.sh (P1) — `gate.port.full-parity-acceptance`

### h2-binding

_Done when:_ gate + capture/experience/ledger ids recorded

- [x] **h2.background-tests-only** Tests and long commands are background-only (setsid + log rc) (P0) — `gate.ops.background-tests-only` — docs/TESTING.md 'Long commands are background-only' + test-all.sh header #10; lane630 runs port-parity-all detached (setsid pid 23736, log $PREFIX/tmp/lane630-parity-all.log; rc read from log)
- [x] **h2.board-maintenance** Board is synced every generation (scripts/port_board.py sync) (P0) — `gate.port.mechanic-test-coverage` — test-all.sh phase 0: gate:board rc=0 (port_board.py check) + gate:board_tests rc=0 (pytest 10 passed); H2_SUMMARY_JSONL=$PREFIX/tmp/board-gate2.jsonl PASS passed=2; docs/TESTING.md 'Living port board gate' workflow sync/sections/done/check
- [x] **h2.evidence-per-done** Every done item carries device/CLI evidence (P0) — `gate.port.mechanic-test-coverage` — pytest scripts/tests 10 passed: done without --evidence -> rc=2 REFUSED and board bytes unchanged (test_done_without_evidence_is_refused); check fails done-without-evidence + blocked-without-reason (test_check_fails_on_*)
- [ ] **h2.mechanic-gate-per-item** Every mechanics item names its mechanic test id (P0) — `gate.port.mechanic-test-coverage`

### infra

- [x] **app.hud-pack-delivery** HUD pack is delivered to the app and loads (script install-hud-pack.sh) (P0) — `gate.ui.craft-floor` — commit fa021ff + HUD_PACK logs
- [ ] **app.startup-exit** Process sometimes exits during init after GAME_AUDIO without FATAL/tombstone (P0) — `gate.port.mechanic-test-coverage` — logcat 2026-11-22

### tooling

- [ ] **tooling.hud-generator-fix** HUD asset generator (wc3rust-hud-origin): emit numeral-free control-group tiles and exclude cursor art from console roles (P0) — `gate.ui.craft-floor`
- [x] **tooling.relax-dev-loop** Relax JIT integrated as an opencode2 dev-loop tool (P1) — `gate.relax.dev-loop-only` — plugin + build + run verified

### parity

- [x] **parity.real-archive-oracle** Retail War3.mpq byte oracle wired into the gate (P0) — `gate.h2.real-game-oracle` — 10523/10523 matched


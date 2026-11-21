# Melee scene regression after the TFT install — ledger 417

Date: 2026-11-15. Repo: `/data/data/com.termux/files/home/src/herofolends-rs`
(root `su -c 'id'` → `uid=0(root) gid=0(root) context=u:r:ksu:s0`).
Owner paths written: `crates/wc3-game/src/{game.rs,sim_bridge.rs}`,
`crates/wc3-game/tests/melee_map_paths.rs`, `analysis/agent-android/**`.

## 1. Goal and invariants

`Game::new` on `(2)Justice.w3x` must load the map's own content — 88
`war3mapUnits.doo` units, 7 gold mines, 1997 destructables — plus the per-seat
w3i opening (2 halls, 5 workers each), on the TFT install (`War3.mpq` +
`War3x.mpq` + `War3xlocal.mpq`) **and** on a RoC-only install (`War3.mpq`).
`justice_scene_counts_match_the_map_placements` stays green; the three
red tests in `melee_map_paths.rs` pass.

Invariants kept: one simulation (`wc3_sim::Sim`), map data as the source of
truth for placements, roster ids from `wc3_sim::data` (no table literal in
`game.rs`), no synthetic entity when the map ships its own.

## 2. Measured before the fix

Device and headless at the same commit (packet measurement, re-confirmed
headless here) printed the same short scene:

```
JUSTICE melee=true player=0 enemy=1 units=42 mines=6 destructables=1997 \
  base=Vec2(14288, 2000) enemy_base=Vec2(2192, 2000)
GAME_SCENARIO melee-ready map=Justice units=42 mines=6 hall_base=Vec2(14288, 2000) \
  enemy_base=Vec2(2192, 2000) camera=1500
```

The raw scene path was already correct under the same install:

```
JUSTICE raw units=99 doodads=2902 mines=7 -> counts units=88 doodads=1997 \
  starts=2 mines=7 camps=22                 (justice_scene_counts_match_the_map_placements PASS)
```

## 3. Hypothesis under test — KILLED

> "With the TFT archives mounted, some neutral/creep ids resolve differently in
> the object DB (`Units\UnitData.slk` from War3x), so `wc3_sim::data::def_of`
> rejects placements, or `Game::new`'s melee branch filters neutrals."

Killed by two independent measurements, not by argument:

1. **The raw path is the same object-database environment.** `spawn_map_scene`
   runs in the test process with the same install mounted and resolves 88/7/1997
   (`GAME_SCENE_CENSUS`, §4). A TFT override that dropped ids would drop them
   there too.
2. **`def_of` never reads the install.** `crates/wc3-sim/src/data.rs:808`:
   `def_of(id)` = `ROSTER.iter().chain(CREEP_DEFS).chain(SHOP_DEFS).chain(EXTRA_DEFS)`
   — compile-time tables generated from the SLKs at build time
   (`analysis/agent-sim/gen_roster.py`, `roster_gen.rs`). The operator's object
   DB feeds `wc3_data::GameDatabase` (stats/icons/menus), not the sim roster.
   Mounting War3x cannot change `def_of`.

## 4. Instrumentation (same generation, device + test)

`wc3-game/src/sim_bridge.rs`:

* `MapSpawns::unit_census` / `doodad_census` (`BTreeMap<String, SceneIdCount>`,
  `SceneIdCount { placed, unknown_def, spawn_failed }`) — every raw record is
  classified.
* `GAME_SCENE_CENSUS` one-line summary, `GAME_SCENE_UNIT id=… placed/unknown_def/
  spawn_failed` per id, `GAME_SCENE_DOODAD` for ids with no data; all through
  `crate::dev_log!` (Android log sink on device, stdout in tests).
* `scene_skips(&MapSpawns)` — compact `id:reason=count` for the game log.

`wc3-game/src/game.rs`: `GAME_SCENE_MAP` (scene source + counts + skipped ids),
`GAME_SCENE_SEATS` (w3i seats/halls/workers), `GAME_SCENE_KIT` (synthetic
fallback breakdown), plus the existing `GAME_SCENARIO`/`GAME_RACE` game-log
lines the device QA gate greps.

Justice census (observed, TFT install, `GAME_SCENE_CENSUS`):

```
unit_records=99 sloc=2 mines=7 unit_placed=88 unit_unknown=2 unit_spawn_failed=0
unit_ids=29 doodad_records=2902 doodad_placed=1997 doodad_no_data=905
doodad_spawn_failed=0 doodad_ids=80
```

| id | placed | unknown_def | spawn_failed |
|---|---:|---:|---:|
| nban | 4 | 0 | 0 |
| nbrg | 2 | 0 | 0 |
| nfoh | 2 | 0 | 0 |
| nfsh | 3 | 0 | 0 |
| nfsp | 2 | 0 | 0 |
| ngad | 2 | 0 | 0 |
| nggr | 2 | 0 | 0 |
| ngme | 2 | 0 | 0 |
| ngnb | 4 | 0 | 0 |
| ngrk | 4 | 0 | 0 |
| ngst | 3 | 0 | 0 |
| nhdc | 14 | 0 | 0 |
| nhfp | 8 | 0 | 0 |
| nhhr | 3 | 0 | 0 |
| nkob | 3 | 0 | 0 |
| nkog | 6 | 0 | 0 |
| nkol | 2 | 0 | 0 |
| nkot | 4 | 0 | 0 |
| nmoo | 1 | 0 | 0 |
| nmr8 | 2 | 0 | 0 |
| nmrl | 2 | 0 | 0 |
| nomg | 2 | 0 | 0 |
| **nrac** | **0** | **2** | 0 |
| nrdr | 2 | 0 | 0 |
| nrog | 2 | 0 | 0 |
| nshe | 2 | 0 | 0 |
| nsqt | 2 | 0 | 0 |
| ntav | 1 | 0 | 0 |
| nwzg | 2 | 0 | 0 |
| **Σ** | **88** | **2** | **0** |

99 records = 2 `sloc` + 7 `ngol` + 88 def-covered units + 2 `nrac`. Nothing is
rejected for a data reason except `nrac` (§9). `unit_spawn_failed=0` on every id:
no placement was refused by the sim (footprint/supply/owner). Creep-camp
grouping (`camps=22`) is metadata over neutral units — it never drops a unit, so
`unit_placed` (88) equals `counts.units` (the raw-path test asserts the same
number). The campaign-arena branch (§6) is the only path that skips map units,
and it logs `source=campaign-arena` with the tree count instead of pretending to
have loaded them.

Device vs headless: the same strings go through `wc3_game::dev_log` (Android
`__android_log_print` sink set by the app) and through `Game::log`
(`GAME_SCENARIO …` / `GAME_*` lines the QA gate drains). Headless output is
observed above; the device strings are produced by the identical code path — the
APK itself is outside this layer's write scope (`apps/android-native/**` is
read-only here), so the on-device rerun is a rebuild/install step for the app
owner, and the previously measured device line (`units=42 mines=6`) is exactly
the old `GAME_SCENE_KIT` sum proven in §5.

## 5. Root cause (two defects, both measured)

**(a) The melee scene did not load the map.** `game.rs::spawn_melee_scenario`
had been rewritten (ledger 402) to call `sim_bridge::spawn_map_destructables`
(trees only) + `setup_melee_1v1`, i.e. the map's `war3mapUnits.doo` units, its
gold mines and its camps were dropped and a synthetic arena was generated.
Logged breakdown of the old path (`GAME_SCENE_KIT`, measured):

```
GAME_SCENE_KIT source=setup_melee_1v1 halls=2 workers=5 enemy_workers=5 \
  start_mines=2 expansion_mines=4 camps=6 creeps=10 sim_units=42 sim_mines=6
```

42 units = 2 halls + 10 workers + 6 camps × 5 creeps; 6 mines = 2 start +
4 expansion (`wc3-sim/src/lib.rs:4277` `setup_melee_layout`). That is exactly the
device number — no data layer involved. This also contradicted the delivered
contract in `analysis/agent-android/MELEE_LAYER_398.md` §2 ("units=88 … mines=7 …
`Game::new`: units=100").

**(b) The sim never ticked in headless sessions.** `Game::new` sets
`boot_menu = melee`; the first `update` opens the main menu unless the host
injected input (`game.rs:1972-1987`, `open_main_menu` → `Screen::MainMenu`), and
`update` returns before stepping the sim for every non-`Playing` screen. Tests
that issue orders through `Game::issue` (which sets `input_seen`) worked; the
two failing tests that poke `sim.issue` directly or issue nothing stayed at
`tick=0` (measured: `ROTATE … tick=0`, `start=… end=… travelled=0`).

## 6. Fix

`crates/wc3-game/src/game.rs`

* `spawn_melee_scenario` now loads a multiplayer melee map with
  `sim_bridge::spawn_map_scene` (terrain pathing, map units, mines, camps,
  destructables, w3i per-seat halls/workers) and points the camera/bases/mines
  at the real hall/mine positions. The enemy AI is `enable_ai_for_race(enemy,
  w3i race)`.
* `map_path_is_campaign(&Assets)` extracted (used by the melee detection and the
  scene): a legacy campaign stem forced into the arena (`Human04`, `Orc01`)
  keeps the synthetic `setup_melee_1v1` opening and only adopts the map's trees,
  because its script owns the real placements (Human04 places three halls,
  Orc01 has no mines in its doo). Maps with no usable w3i seat block (e.g.
  `(2)Ponds_v1.4`, selectable race) take the same fallback, and the fallback
  drops its generated mines when the map already placed its own.
* New host entry point `pub fn enter_battle(&mut self)` — clears `boot_menu`,
  sets `input_seen`, `screen = Screen::Playing`. Documented as the headless
  entry point; the interactive app still boots the glue flow.

`crates/wc3-game/src/sim_bridge.rs` — census + `scene_skips` (§4), and
`spawn_map_destructables` retained for the campaign arena.

`crates/wc3-game/tests/melee_map_paths.rs`

* `cross_map_move_on_justice` enters the battle and issues the move through
  `Game::issue(Command::Move)` (the operator path) instead of poking the sim.
* `rotation_keeps_selection_card_and_match` enters the battle before rotating.
* `justice_melee_start_units_follow_the_w3i_race_of_each_seat` also asserts the
  scene total (100) and prints the `GAME_*` scene log.
* New: `justice_scene_is_identical_on_a_roc_only_install`,
  `justice_nrac_is_absent_from_war3_and_war3x_unit_data`,
  `glue_screen_freezes_the_sim_until_the_host_enters_the_battle` (§8, §9, §11.a).

## 7. After the fix (measured)

```
GAME_SCENE_MAP source=spawn_map_scene map=…(2)Justice.w3x units=88 mines=7 \
  destructables=1997 starts=2 camps=22 melee_races=Some((Orc, Human)) \
  skipped=nrac:unknown_def=2
GAME_SCENE_SEATS source=w3i player=Orc enemy=Human player_hall=Some(89) \
  enemy_hall=Some(95) player_workers=5 enemy_workers=5 sim_units=100 sim_mines=7
GAME_SCENARIO melee map=…Justice.w3x source=map starts=2 units=88 mines=7 \
  trees=1997 camps=22 skipped=nrac:unknown_def=2
GAME_RACE player=Orc enemy=Human player_hall=Some(UnitId(89)) \
  enemy_hall=Some(UnitId(95)) workers=5 enemy_workers=5 mines=7 source=w3i
GAME_SCENARIO melee-ready map=Justice units=100 mines=7 \
  hall_base=Vec2(14288, 2000) enemy_base=Vec2(2192, 2000) camera=1500
JUSTICE melee=true player=0 enemy=1 units=100 mines=7 destructables=1997 \
  base=Vec2(14288, 2000) enemy_base=Vec2(2192, 2000)
JUSTICE races player=Orc enemy=Human
JUSTICE workers player=["opeo","opeo","opeo","opeo","opeo"] enemy=["hpea","hpea","hpea","hpea","hpea"]
start=Vec2(14576, 2000) end=Vec2(8512.6, 7651.9) target=Vec2(8192, 6144) \
  best=29 travelled=9884
ROTATE 1080x2414 selection=[UnitId(90)] cards=12 phase=Playing tick=45 units=100
ROC objects=true units=100 mines=7 destructables=1997 melee=true
```

`units=100` = 88 map units + 2 halls + 10 workers. The `melee-ready` line is the
same string the device log sink prints, so device and headless now agree.

## 8. RoC-only install

`/data/data/com.termux/files/usr/tmp/opencode/wc3_roc/` contains a single
`War3.mpq` (symlink to the ISO extract). `Game::new(Assets::load(Some(roc),
justice))` → `ROC objects=true units=100 mines=7 destructables=1997 melee=true`
(`justice_scene_is_identical_on_a_roc_only_install`, PASS). The scene is
install-independent by construction (§3.2) and now measured on the files.

## 9. Unreachable id record — `nrac` (2 records)

`justice_nrac_is_absent_from_war3_and_war3x_unit_data` reads the tables from each
archive layer (`wc3_re::storm::StormFs::open_wc3_install` over
`analysis/agent-assets/wc3-install`, which links the same three archives the
device install has):

```
NRAC layer=War3x.mpq  …/War3x.mpq  file=Units\UnitData.slk    bytes=298132 hfoo=true nrac=false
NRAC layer=War3x.mpq  …/War3x.mpq  file=Units\UnitBalance.slk bytes=561950 hfoo=true nrac=false
NRAC layer=War3.mpq   …/War3.mpq   file=Units\UnitData.slk    bytes=197885 hfoo=true nrac=false
NRAC layer=War3.mpq   …/War3.mpq   file=Units\UnitBalance.slk bytes=356460 hfoo=true nrac=false
```

(`War3xlocal.mpq` carries no `Units\*.slk`; the positive control `hfoo` resolves
in every layer read, so the scan cannot pass by finding nothing.) Verdict:
`nrac` has **no row in War3 or War3x** — it is recorded as unreachable (a
marker-only id), not given invented stats; the two records stay dropped as
`unknown_def=2`.

## 10. Gates (2026-11-15)

| gate | command | result |
|---|---|---|
| melee paths | `CARGO_BUILD_JOBS=2 cargo test -p wc3-game --test melee_map_paths --offline -- --nocapture` | **rc=0, 10 passed** (was 4 passed / 3 failed) |
| playable loop | `cargo test -p wc3-game --test playable_loop --offline` | rc=0, 1 passed (383.9 s) |
| sim crate | `cargo test -p wc3-sim --offline` | rc=0 |
| menu probe | `cargo test -p wc3-game --test menu_probe --offline` | rc=0, 2 passed |
| touch playability | `cargo test -p wc3-game --test touch_playability --offline` | rc=0, 2 passed |
| targeted lib | `cargo test -p wc3-game --lib --offline -- --exact game::tests::map_units_spawn game::tests::map_runtime_human04_is_melee` | rc=0, 2 passed |
| playability_real_map | `cargo test -p wc3-game --test playability_real_map --offline` | 3 failed — see §11.a (2 are the glue-screen harness defect, 1 is pre-existing Orc01) |
| lib (full) | `cargo test -p wc3-game --lib --offline` | **198 passed / 5 failed / 32 ignored, rc=101** — the 5 are the asset/render expectations in §11.b, both scene-related lib tests are green |

## 11. Findings outside this layer's write scope (reported, not touched)

### 11.a `playability_real_map.rs` — 2 glue-screen harness defects + 1 pre-existing

`cargo test -p wc3-game --test playability_real_map --offline` → 3 failed.
`crates/wc3-game/tests/playability_real_map.rs` is read-only in this packet.

| test | measured | cause |
|---|---|---|
| `worker_harvests_gold_on_the_real_map` | `worker never paid out gold: 1000 -> 1000 (mine Vec2(14592, 1280))` | the scene is correct (census 88/7, mine found); the test issues through `game.sim.issue` and never sets `input_seen`, so the first `update` opens the glue screen and the sim stays at `tick=0` (defect 5b). Fix: call `game.enter_battle()` after `Game::new` (or use `game.issue`). |
| `worker_builds_a_farm_and_the_building_appears` | `worker card exposes a building entry` | same gate: `game.update(0.0)` returns from `tick_screen` before `refresh_buttons`, so `buttons` is empty. Same one-line fix. |
| `units_walk_across_the_real_map` | `a long move must produce a path (start Vec2(26512,13200))` | **pre-existing** (ledger 398 §7.1): Orc01 start 3 `(26688,12672)` is `walkable=false`, so the farthest-pair base lands in a blocked pocket. Re-measured now: `target/debug/donorintake maps verify analysis/war3_full3/Maps/Campaign/Orc01.w3m` → `start 3 … walkable false`, `gold_mines 0`, `30 units`. Unchanged by this layer (the campaign arena reproduces the old `GAME_SCENE_KIT sim_units=42 sim_mines=6` line exactly). |

Both harness failures are pinned by the new test
`glue_screen_freezes_the_sim_until_the_host_enters_the_battle` in
`melee_map_paths.rs`, which asserts `Screen::MainMenu`, `tick` unchanged after
120 updates, and `tick` advancing after `enter_battle()`.

### 11.b `cargo test -p wc3-game --lib` — asset/render expectations vs the TFT install

| test | message | likely layer |
|---|---|---|
| `assets::geoaset_tests::spell_numbers_come_from_the_sheet` | `Cost1 left 25 right 125` | object DB (TFT ability data) |
| `assets::geoaset_tests::typed_object_data_feeds_game_assets` | `cost from UnitBalance left Some(135) right Some(160)` | object DB (TFT unit prices) |
| `assets::geoaset_tests::menus_are_driven_by_the_typed_database` | `peasant build list … left 11 right 10` | object DB (TFT build list gained a row) |
| `assets::geoaset_tests::geoaset_skinning_weights_are_normalised` | `Archer MDX has at most 4 influences` | TFT MDX model |
| `render::tests::rendering_units_material_resolution` | `tint must change team pixels, got 0` | TFT model/team-colour layers |

They are install-data expectations (RoC-era numbers vs the TFT object DB), not
melee-scene regressions: none of them constructs `Game`; they call
`Assets::load`/render directly. `analysis/tech-lead/merge-data.md` §11.4 records
`typed_object_data_feeds_game_assets` as passing before the TFT install landed.
Owner: asset/data layer (`crates/wc3-game/src/assets.rs`, `render.rs`) — outside
this packet's write scope. The two lib failures the melee change did cause
(`game::tests::map_units_spawn`, `game::tests::map_runtime_human04_is_melee`) are
fixed by the campaign-arena branch and verified green (§10).

## 12. Risks / next

* The campaign-stem arena keeps `setup_melee_1v1`; if those demo stems are ever
  retired, the fallback can go with them.
* On a real melee map the **w3i seats own the races**, so the glue screen's
  race picker no longer changes the delivered kits (it still drives the
  synthetic fallback). That is the ledger-398 contract and what the tests pin;
  if the picker is meant to seat the player, it must map to a chosen `sloc`
  rather than to a race literal.
* `Game::enter_battle` is the only new public API; the app keeps the glue flow
  (`boot_menu`), so no device behaviour change beyond the scene contents.
* The 5 asset/render failures need the data layer to re-pin expectations to the
  TFT object DB (or stage the RoC layer) — not a scene bug.
* `(2)Ponds_v1.4.w3x` has no usable w3i race block (selectable race), so it runs
  the fallback on top of its map units (`sim_units=151`, `sim_mines=6`, no map
  mines) and `melee_worker_walks_and_builds_on_the_ready_map` passes with a real
  46-waypoint path. If Ponds should seat per-slot races too, `map_player_slots`
  must accept a selectable race instead of returning `None`.

## 13. Device-harness contract: `GAME_HUD_RECTS` (tech-lead addendum)

`game.rs::rebuild_layout` now publishes the touch geometry once per layout
rebuild (never per frame), right after the layout is stored:

```
GAME_HUD_RECTS scale=1.00 card=x,y,w,h tile=NN.N slots=[0@x,y 1@x,y …] army=x,y minimap=x,y,w,h
```

Fields come straight from `MobileHudLayout` (`scale`, `card`, `cell`,
`card_slots[i].centre()`, `army.centre()`, `minimap`), so
`analysis/tech-lead/device_pass.sh` can tap the exact slot centres for the
placement-ghost frame (#10), the army panel (#7) and the minimap-to-water frame
(#14) instead of guessing pixels (two mis-taps sent `PLACE_TAP` to coordinates
the harness never chose). Source of the contract: `analysis/h2/PATCH_hud_rects.md`;
it goes through `dev_log`, so it reaches logcat on device and stdout in tests.

## 14. H2 persistence

* Ledger 417 (`wc3rust/melee-scene-tft`): `completed`
  (`h2_ids_json={"capture":888,"experience":4542}`).
* `harness2_capture` 888 — task/evidence/files record.
* `harness2_experience` 4542 — cue "melee scene comes up short after an
  expansion/install change: spawn-function swap vs roster regression" with the
  proof order and the glue-gate seam.
* `harness2_dream --days 1 --limit 40 --apply` — scanned 40 captures / 40
  experiences / 529 principles, refreshed `~/.harness2/dream/{wiki,core}.md`,
  no duplicate or newly promotable principle.
* Brain: `Key Decisions.md` → "melee-scene-tft (ledger 417)";
  `Patterns.md` → "Melee scene census + glue-gate seam" and the
  `GAME_HUD_RECTS` contract sample.
* Test runs were kept serial (`-p wc3-game --test melee_map_paths` for the
  scene loop) under the machine's high load, per the tech-lead's addendum.

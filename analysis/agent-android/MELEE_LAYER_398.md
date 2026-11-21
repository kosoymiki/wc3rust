# Layer Android + Melee Map Setup — ledger 398

Date: 2026-11-15. Owner paths: `apps/android-native/**`,
`crates/wc3-game/src/{sim_bridge,assets}.rs`, `crates/wc3-game/tests/melee_map_paths.rs`,
`analysis/agent-android/**`. `game.rs`/`hud.rs`/`mobile_hud.rs` read-only.

Scenario: the Android app launches straight into a 1v1 on `(2)Justice.w3x`
(byte-identical to the device file `/data/data/org.herofolends.p0/files/Melee.w3x`,
sha256 `91e22d5ba14b4bcc896f46ac41db31679b8f3226e13f2588b3e617ab1628f1ee`),
survives rotation/lifecycle, and resumes without resurrecting a finished match.

## 1. Per-race melee opening from `war3map.w3i`

`sim_bridge.rs`:

- `map_player_slots()` (`sim_bridge.rs:373`) parses the w3i player block from
  `wc3_re::mapfiles::MapInfo::raw_tail` using the War3Net v23..v26 read order
  (`donors/War3Net/src/War3Net.Build.Core/Serialization/Binary/Info/MapInfo.cs`,
  `PlayerData.cs`). It returns slot number, `PlayerController`, fixed
  `PlayerRace`, flags, name and the w3i start position translated into local map
  space. Unknown layouts/races return an empty list — no invented race.
- `spawn_melee_bases()` (`sim_bridge.rs:442`) is called from
  `spawn_map_scene()` when the map places exactly two `sloc` markers
  (`sim_bridge.rs:239`). It maps each `sloc` marker's creator slot to the w3i
  race of that slot and spawns that seat's canonical hall/worker ids from the
  SLK roster (`wc3_sim::ai::hall_for_race` / `worker_for_race`), one hall and
  five workers per seat, honouring map-owned halls/workers.
- The map's own `sloc` order is preserved for `MapSpawns::start_points`; no
  re-seating. `spawn_map_scene`'s `units`/`destructables`/`gold_mines`
  counters remain the map's own placements — the melee opening is reported
  separately in `MapSpawns::melee` and is never counted as map content.

Justice map facts (observed, parser + donor SLK, 2026-11-15):

| slot | w3i name | controller | race | start (local) | sloc marker |
|---:|---|---|---|---|---|
| 0 | Player 1 | 1 (user) | Human (1) | (2176, 1984) | player=0 |
| 1 | Player 2 | 1 (user) | Orc (2) | (14272, 1984) | player=1 |

`war3mapUnits.doo` places the east marker (Orc seat) first, so owner 0 (the
app player) is the Orc seat and owner 1 the Human seat — the delivered scenario
is **Orc player vs Human enemy**, now derived from the map instead of the old
hardcoded `ogre`/`htow` pair in `game.rs`.

## 2. Map-parity counts (no synthetic mines/units)

Device baseline line remains exactly the map's own content:

```
GAME_SCENARIO melee ... units=88 doodads=1997 starts=2 mines=7 trees=1997
```

Verified by `justice_scene_counts_match_the_map_placements` and
`justice_map_scene_keeps_map_counts_and_spawns_race_bases`:

- `war3mapUnits.doo` = 99 records → 2 `sloc` + 7 `ngol`/`ugol` + 88 def-covered
  units + 2 undefined `nrac` markers (no melee SLK row; dropped).
- `war3map.doo` = 2902 doodads → 1997 destructable-data-backed (trees/rocks),
  905 decoration-only.
- `sim.gold_mines.len() == 7`; no mine is added by the melee opening, the
  scenario, or the AI before its own expansion schedule.
- `Game::new` on Justice: `units=100` (88 map + 12 melee), `mines=7`,
  `destructables=1997`, `melee=true`.

New device lines emitted through the `wc3_game` log sink:

```
GAME_MELEE_RACE owner=0 slot=1 race=Orc hall=ogre worker=opeo hall_id=Some(89) workers=5 start=(14272,1984) source=w3i
GAME_MELEE_RACE owner=1 slot=0 race=Human hall=htow worker=hpea hall_id=Some(95) workers=5 start=(2176,1984) source=w3i
GAME_MELEE_SETUP player=Orc enemy=Human player_hall=Some(89) enemy_hall=Some(95) player_workers=5 enemy_workers=5 map=TRIGSTR_010 source=w3i
```

## 3. Checkpoint/resume identity and phase

`sim_bridge.rs:580-751`: `MapStamp` (path + length + FNV-1a64 of the map file),
`CheckpointMeta` (map stamp, grid, phase, tick, save digest) with strict
encode/decode, `verdict()` (finished phase / map / grid) and `matches()`
(tick+digest). `MapStamp::of_file` reads the map in 64 KiB chunks.

`apps/android-native/src/lib.rs`:

- `save_checkpoint` (`lib.rs:746`): refuses to save a non-`Playing` match
  (`GAME_CHECKPOINT_SKIPPED reason=finished`, checkpoint cleared), requires map
  identity (`reason=no_map` otherwise), and skips a duplicate save for the same
  sim tick (`GAME_CHECKPOINT_SKIP reason=same_tick`) — kills watcher +
  lifecycle double-save storms with identical bytes. Writes `*.sav` then
  `*.sav.meta`, logs `GAME_CHECKPOINT … map=<path> phase=playing`.
- `restore_checkpoint` (`lib.rs:814`): a save without meta is rejected
  (`reason=missing_meta`), then `CheckpointMeta::verdict` rejects
  `reason=finished` (and deletes the pair), `reason=map`, `reason=grid`; after
  decoding it rejects `reason=digest`. Only then `game.sim = sim` with
  `phase = Playing` and `LAST_SAVED_TICK = tick`.
- `clear_checkpoint` removes both the save and its meta (clean quit,
  finished-match rejection, silent-clear paths).
- Existing EACCES fix kept: writable path is
  `/data/data/org.herofolends.p0/files/herofolends-resume.sav`.

First launch after this build: the pre-existing baseline save has no meta and
is rejected once (`GAME_RESUME_REJECT … reason=missing_meta`, by design — it
cannot be proven to belong to this map/grid). The next pause writes the pair;
the following launch prints `GAME_RESUME … phase=playing map=…`.

## 4. Rotation / lifecycle

- Rotation path unchanged in intent: `WINDOW_RESIZED`/`PIXEL_SIZE_CHANGED`
  → `Lifecycle` effects (gesture reset only, no save) → `game.resize()`.
  `GAME_SURFACE_RESIZED` now also logs `selection`, `phase`, `tick`.
- `WILL_ENTER_BACKGROUND`/`TERMINATING`: the SDL watcher saves synchronously
  once; the lifecycle effect save is suppressed by `WATCHER_SAVED`, and the
  same-tick guard removes any remaining duplicate write.
- Test `rotation_keeps_selection_card_and_match`: selection, 12-card command
  card, phase `Playing`, tick, unit count and both base positions are identical
  across 1080x2414 ↔ 2414x1080 ↔ 1080x2414, and the sim keeps ticking.

## 5. Audio verification line (tech-lead handoff)

`apps/android-native/src/lib.rs:1286`: after the scenario spawned its units and
`game.take_log()` is drained, one line is logged:

```
GAME_AUDIO_HOOKS device=<backend> acks=<started>/<requested> missing=<n> ui=<n> events=<n> music=<started>/<missing> ambient=<started>/<missing> taps=<n> unresolved=<n> buffered_ms=<n> underruns=<n> dropped=<n> occluded=<n> ducked_blocks=<n> voices=<n> max_active=<n>
```

(`wc3_audio::game_audio::GameAudio::stats_line()`, `game_audio.rs:189`.) The
existing `GAME_AUDIO device=sdl3 …` wiring line is untouched; audio-layer
`AUDIO_*` event lines are not filtered or swallowed by the app.

## 6. Gates (all rc=0, 2026-11-15)

| gate | command | result |
|---|---|---|
| Android check | `CARGO_BUILD_JOBS=2 cargo check -p herofolends-android --offline` | rc=0 (`Finished dev profile`) |
| sim_bridge lib | `cargo test -p wc3-game --lib --offline sim_bridge` | rc=0, 18 passed |
| melee paths | `cargo test -p wc3-game --test melee_map_paths --offline` | rc=0, 7 passed |
| regression | `playable_loop`, `menu_probe`, `touch_playability` | rc=0 (1/2/2 passed) |

Key observed test output:

```
JUSTICE melee=true player=0 enemy=1 units=100 mines=7 destructables=1997 base=Vec2(14288.0, 2000.0) enemy_base=Vec2(2192.0, 2000.0)
JUSTICE races player=Orc enemy=Human
JUSTICE workers player=["opeo","opeo","opeo","opeo","opeo"] enemy=["hpea","hpea","hpea","hpea","hpea"]
ROTATE 1080x2414 selection=[UnitId(90)] cards=12 phase=Playing tick=45 units=100 base=Vec2(14288.0, 2000.0)
```

## 7. Findings routed outside this layer

1. `crates/wc3-game/tests/playability_real_map.rs` (2 red tests, pre-existing,
   not this layer): it runs campaign `Orc01.w3m` through the melee path, but
   Orc01 has **0 gold mines in `war3mapUnits.doo`** and **0 mined starts**
   (mines are script-created; the melee path intentionally skips scripts) and
   its start 3 (26688,12672) is unwalkable — hence `no gold mine near the base`
   and `place_building … site occupied`. Evidence:
   `target/debug/donorintake maps verify analysis/war3_full3/Maps/Campaign/Orc01.w3m`
   (`gold_mines 0`, 5 sloc, start 3 `walkable=false`). Owner: scenario policy
   (`game.rs`, read-only here) / tech-lead; fix options: point the test at
   Justice, or seed script-created mines before the economy assertions.
2. `game.rs` enemy AI still hardcodes `composition: [("hfoo",3),("hrif",2)]`,
   `worker: "hpea"` — correct for Justice (Human enemy) but not data-driven for
   other maps. `game.rs` is read-only in this layer.
3. Seat rule: owner 0 = first `sloc` in `war3mapUnits.doo` order; that is the
   existing behaviour and is preserved. On Justice this yields Orc player /
   Human enemy; a map whose doo orders the other way will seat the other way
   with the same per-seat race lookup.
4. Legacy checkpoint (no meta) is rejected once by design; see §3.

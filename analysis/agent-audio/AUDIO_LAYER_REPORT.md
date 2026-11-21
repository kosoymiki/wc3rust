# Audio layer — WC3 sound parity (ledger 397)

Goal: every unit speaks with its own shipped `unitSound` set, per-race music follows
the player, every tileset has its retail day/night ambient score, all gameplay
events play their shipped row, and the device counters/log lines are honest.

Status: **code + tests complete in `crates/wc3-audio`**. The remaining
`crates/wc3-game/src/game.rs` call-site edits are specified line-by-line in
[`GAME_RS_HOOKS.md`](GAME_RS_HOOKS.md) because that file is READ_ONLY for this
layer (another agent owns it).

---

## 1. Audit — what was already correct (kept)

| Location | Behaviour | Verdict |
|---|---|---|
| `game.rs:1274-1299` `play_unit_ack` | resolves `Assets::unit_sound_set(def_id)` (def id, not `UnitKind`) | correct owner of the selection voice |
| `game.rs:1303-1329` `play_unit_attack` | `{set}YesAttack` from the def id's set | correct, fires from `issue(Command::Attack)` at `1429` |
| `game.rs:1331-1356` `play_unit_death` | `{set}Death` from the def id's set | correct, fires from `Event::UnitDied` at `2045` |
| `game.rs:1358-1382` `play_unit_ready` | `{set}Ready` from the def id's set | correct, fires from `Event::UnitTrained` at `2095` |
| `game.rs:2082` `Event::BuildingComplete` | `audio.job_done(race)` -> `JobDoneSound<Race>` | correct |
| `game.rs:2176-2200` `finish()` | `music_for_race(race, Victory/Defeat)` | correct |
| `game.rs:3000-3009` `perform_cast` | `AbilitySounds.slk` row via `spell_sound` | correct (Blizzard only; other sim spells have no retail row) |
| `game.rs:2112-2126` `Event::Error` | `NoGold/NoLumber/NoFood` by race, `UiError` otherwise | correct |
| `audio_race(unit.def().race)` at `2066`/`2152` | race-scoped *UI* rows only, never unit voices | correct by design |

Remaining defects found in the audit (all fixed in `wc3-audio`, except the
call-site items in `GAME_RS_HOOKS.md`):

1. `hgrunt` (campaign/map Grunt id) had no sound set -> silent. Fixed with a
   documented alias `hgrunt -> ogru` (same `Grunt` family).
2. SLK parser dropped rows that begin with a formatting record (`F;Y…`):
   8 lost `UnitAckSounds` rows (`HuntressWarcry`, `MurlocReady`, `OgreReady`,
   `OgreWarcry`, `SatyrReady`, `SatyrWarcry`, `ChaosWarlordReady`,
   `ChaosWarlordWarcry`) and 9 lost `MIDISounds` rows. Fixed.
3. No "pissed" path (repeated selection taps) and no debounce between the tap
   acknowledgement and the per-frame selection acknowledgement. Added
   `GameAudio::selection_tap` / `selection_reset`.
4. Music was hardcoded `Human1` (`game.rs:450`) even on the Orc melee arena
   (Justice). Added `music_for_player_race` + exact patch.
5. No per-tileset ambient at all: the retail MIDI+DLS score was never played.
   Implemented `WorldEditData.txt` -> `MIDISounds.slk` -> MIDI+DLS renderer.
6. Research completion (`Event::UpgradeResearched`) had no sound arm. Patch in
   `GAME_RS_HOOKS.md` (line 2096).
7. No per-event line for build/train/music/ambient; `GameAudioStats` could not
   show unresolved clips. Added `stats_line()` + `last_event_line()` +
   `unresolved()`.

## 2. Changes in `crates/wc3-audio`

### 2.1 `soundinfo.rs` — per-unit voice profile

* `UnitNameMap::profile(def)` -> `UnitSoundProfile { unit_id, set, name }`.
* `UnitNameMap::sound_set` now resolves map/campaign aliases
  (`resolve_id` walks `DEFAULT_UNIT_ALIASES`, bounded, cycle-safe).
* `DEFAULT_UNIT_ALIASES = [("hgrunt", "ogru")]`, plus `add_alias`, `aliases`,
  `raw_sound_set` (proves the alias is not a shipped row).
* Aliases are also applied on parse (a table parsed from disk gets them too).
* New lookups `SoundInfoDb::unit_pissed` and `unit_ready`.
* `parse_slk` honours `F;` records that start a row (the 8+9 lost rows above).

### 2.2 `ambient.rs` (new) — retail tileset ambience

Chain, all shipped data:

```
war3map.w3e tileset byte
  -> UI/WorldEditData.txt [DayAmbience]/[NightAmbience]      L=LordaeronSummerDay
  -> UI/SoundInfo/MIDISounds.slk                             Sound\Ambient\...\*.mid + *.dls
  -> DLS Level 2 bank (14 PCM waves, 21 program/key regions)
  -> MIDI sequencer (tempo map, note pairing, DLS key regions, loops)
  -> mono 22.05 kHz PCM loop cached per theme
```

Public surface: `AmbientTable`, `AmbientTheme`, `DlsBank`, `DlsSample`,
`DlsRegion`, `MidiSong`, `MidiNote`, `render_ambient`.

### 2.3 `game_audio.rs` / `events.rs` — hooks and diagnostics

* `GameAudio::ambient_for_tileset(letter, night)`:
  lazily loads `MIDISounds.slk` + `WorldEditData.txt` through the existing
  `SoundProvider`, resolves the MIDI/DLS pair, renders and caches the loop,
  plays it on the Ambient bus and replaces the previous loop.
* `GameAudio::ambient_theme_for_tileset`, `ambient_theme_names`, `stop_ambient`.
* `GameAudio::selection_tap(unit_id, pos)` — 4th consecutive tap on the same
  unit plays `{set}Pissed`; `selection_reset()` clears the counters.
* `GameAudio::unit_pissed`, `unit_pissed_set`; `unit_ready` now alias-aware.
* `GameAudio::music_for_player_race(race)` (in `events.rs`) — in-game theme by
  player race, `Victory/Defeat` unchanged.
* `GameAudioStats` gained `ambient_started/ambient_missing/events_played/
  events_missing/selection_taps`.
* `GameAudio::unresolved()` — every clip/row/theme that failed to resolve, so
  the device log lists gaps instead of showing silence.
* `GameAudio::stats_line()` — one line with hook counters + engine counters:
  `GAME_AUDIO_HOOKS device=… acks=… missing=… ui=… events=… music=… ambient=…
  taps=… unresolved=… buffered_ms=… underruns=… dropped=… occluded=…
  ducked_blocks=… voices=… max_active=…`
* `GameAudio::last_event_line()` and uniform `AUDIO_*` lines
  (`AUDIO_ACK/ATTACK/DEATH/PISSED/READY/AMBIENT/MUSIC/EVENT/…`) with
  `set=`, `def=`, `theme=`, `path=`, `midi=`, `dls=` fields.

## 3. Evidence

Command: `CARGO_BUILD_JOBS=2 cargo test -p wc3-audio --offline`

```
test result: ok. 39 passed; 0 failed   (lib, includes ambient + soundinfo)
test result: ok. 13 passed; 0 failed   (tests/audio.rs)
test result: ok. 1 passed; 0 failed    (tests/no_alloc.rs)
```

### 3.1 Per-unit set table (acceptance gate)

`cargo test -p wc3-audio --offline --test audio audio_unit_sets_table -- --nocapture`

```
UNIT_SET_TABLE def=opeo   set=Peon        ack=PeonWhat        attack=PeonYesAttack        death=PeonDeath        pissed=PeonPissed
UNIT_SET_TABLE def=hgrunt set=Grunt       ack=GruntWhat       attack=GruntYesAttack       death=GruntDeath       pissed=GruntPissed
UNIT_SET_TABLE def=hfoo   set=Footman     ack=FootmanWhat     attack=FootmanYesAttack     death=FootmanDeath     pissed=FootmanPissed
UNIT_SET_TABLE def=hpea   set=Peasant     ack=PeasantWhat     attack=PeasantYesAttack     death=PeasantDeath     pissed=PeasantPissed
UNIT_SET_TABLE def=nftr   set=ForestTroll ack=ForestTrollWhat attack=ForestTrollYesAttack death=ForestTrollDeath pissed=ForestTrollPissed
UNIT_SET_PLAY def=opeo set=Peon voices=1,2,3,4 ... (all five start real voices)
```

`hgrunt` proves the alias: `units.raw_sound_set("hgrunt") == None`,
`resolve_id("hgrunt") == "ogru"`, `sound_set == "Grunt"`; the test also asserts
`units.aliases() == [("hgrunt","ogru")]`.

### 3.2 Tileset ambience (Justice is W3E tileset `'Y'` = CityScape)

`cargo test -p wc3-audio --offline --test audio audio_justice_tileset -- --nocapture`

```
JUSTICE_TILESET Y
JUSTICE_AMBIENT_DAY CityScapeDay
JUSTICE_AMBIENT_PLAY voice=1 line=AUDIO_AMBIENT tileset=Y night=false theme=CityScapeDay
  midi=Sound\Ambient\CityScape\CityScapeDay.mid
  dls=Sound\Ambient\CityScape\CityScape.dls seconds=180.0 samples=4012412 peak=0.208 voice=1
```

`audio_tileset_ambient_renders_lordaeron_summer` renders the full 124 s score
(3 818 902 frames, peak 0.458 pre-normalisation) and asserts audible mixer
output; `stats_line()` from that run:

```
GAME_AUDIO_HOOKS device=headless acks=0/0 missing=0 ui=0 events=0 music=0/0 ambient=1/0
taps=0 unresolved=0 buffered_ms=None underruns=0 dropped=0 occluded=0 ducked_blocks=0
voices=1 max_active=1
```

### 3.3 Clip resolution table (all shipped tilesets)

`AMBIENT_CLIPS themes=26 unresolved=0` (`every_midi_sounds_theme_has_its_mid_and_dls`).
Day letters from `WorldEditData.txt`, all present on disk:

| letter | day theme | MIDI | DLS |
|---|---|---|---|
| A | AshenvaleDay | AshenvaleDay.mid | Ashenvale.dls |
| B | BarrensDay | BarrensDay.mid | Barrens.dls |
| C | FelwoodDay | LordaeronSummerDay.mid | LordaeronSummer.dls |
| D | DungeonDay | NorthrendDay.mid | Northrend.dls |
| F | LordaeronFallDay | LordaeronFallDay.mid | LordaeronFall.dls |
| G | DungeonDay | NorthrendDay.mid | Northrend.dls |
| L | LordaeronSummerDay | LordaeronSummerDay.mid | LordaeronSummer.dls |
| N | NorthrendDay | NorthrendDay.mid | Northrend.dls |
| Q | VillageFallDay | LordaeronFallDay.mid | LordaeronFall.dls |
| V | VillageDay | LordaeronSummerDay.mid | LordaeronSummer.dls |
| W | LordaeronWinterDay | LordaeronWinterDay.mid | LordaeronWinter.dls |
| X | DalaranDay | CityScapeDay.mid | CityScape.dls |
| Y | CityScapeDay | CityScapeDay.mid | CityScape.dls |

Night letters mirror them (`…Night`). Unresolved: **none** on this install; any
gap at runtime is recorded in `GameAudio::unresolved()` with the exact path.

### 3.4 Production wiring probe (real MPQ, SDL3)

`cargo run -q --offline -p wc3-assetdb --bin audio_scenario_probe -- --install analysis/agent-assets/wc3-install`

```
GAME_AUDIO_WIRING audio device=sdl3 source=wc3-install layers=["War3xlocal","War3x","War3"] tables=8 defs=2025 units=812 themes=42
GAME_AUDIO_SELECT unit=hpea set=Peasant def=PeasantWhat voice=1
GAME_AUDIO_MOVE unit=hpea set=Peasant def=PeasantYes voice=2
GAME_AUDIO_BUILD_START race=human def=ConstructingBuildingDefault
GAME_AUDIO_BUILD_LOOP race=human def=BuildingConstructionLoop
GAME_AUDIO_BUILD_COMPLETE race=human def=JobDoneSoundHuman
GAME_AUDIO_TRAIN_COMPLETE unit=hfoo set=Footman def=FootmanReady
GAME_AUDIO_FIGHT_ATTACK unit=hfoo set=Footman def=FootmanYesAttack
GAME_AUDIO_FIGHT_DEATH unit=ogru set=Grunt def=GruntDeath
GAME_AUDIO_HERO_DEATH race=human def=HeroDiesHuman
GAME_AUDIO_DENY race=human def=NoGoldHuman
GAME_AUDIO_QUEST def=QuestCompleted
GAME_AUDIO_MAP_AMBIENT sound=gg_snd_WaterLakeLoop1 region=LakeLoop01 pos=(-1568,-1232)
GAME_AUDIO_VICTORY race=human
GAME_AUDIO_SUMMARY started=13/13 voices=13 underruns=0 backend_errors=0
```

`defs=2025` is the post-fix table count (739+8 UnitAckSounds rows included).
The first run of this probe reported `underruns=1` while another cargo build was
running on the same device; the immediate clean rerun reported `underruns=0`
(`rc=0`). The counter is the device's own SDL3 queue report, not a hook counter.

## 4. Device verification (what to assert in logcat)

1. `GAME_AUDIO_HOOKS …` once every 2 s: `ambient=` grows on map load, `events=`
   grows per gameplay event, `unresolved=0`, practice counters
   `underruns/dropped/occluded/ducked_blocks` visible.
2. `AUDIO_EVENT event=… unit=… race=… voice=…` (or `GAME_AUDIO_ACK/ATTACK/
   DEATH/READY/UPGRADE`) for select/ack/attack/death/build/train/research/error.
3. `AUDIO_AMBIENT tileset=Y night=false theme=CityScapeDay midi=… dls=…`.
4. `AUDIO_MUSIC theme=Orc1 path=…/Orc1.mp3` on Justice (player is Orc), and
   `theme=Human1` when the host overrides.

## 5. Risks / next

* Each rendered tile loop is 22.05 kHz mono f32 (~15.9 MB at the 180 s cap,
  cached once per theme). Only the active tileset is held at a time.
* Rendering happens on first use (debug build ~3 s for 124 s of score; release
  is expected far lower — not yet measured on device).
* `game.rs` call sites are not applied here (READ_ONLY); the exact patches are
  in `GAME_RS_HOOKS.md`. Until they land, the device run keeps `Human1` on the
  Orc arena and does not start tileset ambience.
* Map `war3map.w3s` ambience positions are still opaque in `parse_w3s`; that is
  a separate hook (see `GAME_RS_HOOKS.md` §5) and does not affect tileset
  ambience.

---

## 6. Follow-up fix (ledger 400): "звуки с карты играют"

Symptom: unit death/ack lines from the far side of the map were audible at
camera position. Two measured causes, both fixed:

### 6.1 Retail distance fields were inverted

`sound_probe` (shipped `UnitAckSounds.slk` / `AnimSounds.slk`):

```
PeonDeath      min=600   max=10000 cutoff=3000
FootmanDeath   min=600   max=10000 cutoff=3000
GruntWhat      min=3000  max=10000 cutoff=100000
```

`MinDistance` is the near radius (unity gain), `MaxDistance` the far end
(silence), `DistanceCutoff` an earlier hard cull. The old code used
`MaxDistance` as the near radius and `DistanceCutoff` as the far end, so
`GruntWhat` played at **unity gain up to 10 000 units** (the whole map) and
decayed only toward 100 000.

* `soundinfo.rs:95` `SoundDef::playback_distances()` — `(near, far)` =
  `(MinDistance, MaxDistance)`, falling back to `DistanceCutoff` when
  `MaxDistance` is absent.
* `game_audio.rs:361` uses it for `PlayParams.reference_distance / max_distance`;
  `game_audio.rs:363-385` enforces the per-def `DistanceCutoff` as a hard cull.
* `engine.rs:952` — at/past `max_distance` the attenuation is **0** (kira maps
  the far distance to `Decibels::SILENCE`); previously it floored at 1e-3.

Tests: `audio_retail_distance_fields_map_to_near_and_far`,
`audio_voice_curve_zeroes_at_retail_max_distance` — 12 000 units from the
listener yields energy **0.0** (< 1e-4), 10× MinDistance (30 000) also 0.0.

### 6.2 Audibility gate — per-row `DistanceCutoff` (r2-proven, ledger 409)

`game.rs:2136` `Event::UnitDied` played every death on the map. The first pass
used a flat 4 000-unit radius; r2 reverse engineering of TFT `Game.dll`
(`analysis/agent-re/ui_audio/ENGINE_MAP.md`, `constants.json`) proved the real
rule:

> `StartSound` `0x6f102980`, `fcom` at `0x6f102b0b`: a 3-D sound never starts
> when `dist² > DistanceCutoff²`; the instance keeps `cutoff²` at `+0x50`.
> SoundData parser defaults (`0x6f3a2530`): Min 0, Max 10000, Cutoff 3000.

Implemented in `game_audio.rs:336-397` (`play_def_with`):

* hard gate `(position - listener).length_squared() > cutoff²`, where
  `cutoff = def.audible_cutoff()` (`soundinfo.rs:76`) — the row's own
  `DistanceCutoff`, or the parsed default **3000** when the row stores 0.
* equality is audible (`>` not `>=`).
* `MinDistance` / `MaxDistance` remain only the volume-ramp bounds.
* `set_audible_radius` is now an optional diagnostic cap (`0` = off, default);
  it is never the primary rule.
* `play_event` counts a culled event as `events_gated`, never as a data gap;
  `acks_gated` separates gated acks from missing rows in `stats_line()`.

Measured per-class cutoffs from the shipped SLKs (row counts in parentheses):

| table | DistanceCutoff |
|---|---|
| `UnitAckSounds.slk` | **100000** (747 rows, all) |
| `UnitCombatSounds.slk` | **2100** (60, all) |
| `AnimSounds.slk` (deaths) | **3000** (399), 2000 (8), 5000 (4), 10000 (2) |
| `AbilitySounds.slk` | **2000** (33, all) |
| `AmbienceSounds.slk` | 2500 (37), **900** (20), 1000 (1), 1200 (1) |
| `UISounds.slk` | 10000 (106), 2500 (4), 2000 (1) |

No `game.rs` edit is required for the gate: `update_audio` already calls
`audio.set_listener(...)` each frame. `stats_line()` prints `acks_gated=`,
`gated=` and `events_gated=`.

Tests:
* `audio_retail_cutoffs_per_class` — `GruntWhat`/`PeonWhat` 100000,
  `AxeMediumChopWood` 2100, `PeonDeath`/`FootmanDeath` 3000,
  `BlizzardWave` 2000, `BuildingConstructionLoop` 2500,
  `AmbientSoundRain` 1000.
* `audio_row_cutoff_gates_distant_voices` — ack at 9 000 starts (cutoff
  100000), ack at 100 000 starts, at 100 001 culled; death at 3000 starts, at
  3001 culled; combat culled at 3000; gated death event counts `events_gated`,
  `unresolved` stays empty.
* `audio_squared_cutoff_compare_matches_linear_at_boundary` — squared vs linear
  agree at ±1 unit; a cutoff-0 row uses 3000 (`3 000` audible, `3 001` not).

### 6.3 Ambient follows the region

`game_audio.rs:906` `ambient_for_region(region, tileset, night)`:
starts the loop once per `(region, tileset, day/night)` key, reuses the running
voice while the key is unchanged (no per-frame restart), stops the old loop
when the key changes, and `leave_ambient_region()` stops it on exit.
`ambient_region()` exposes the current key for logs. Cached renders make a
switch restart cost-free. Test: `audio_ambient_region_switches_and_stops`.

Acceptance numbers for this fix:

| item | value |
|---|---|
| check | `CARGO_BUILD_JOBS=2 cargo check -p wc3-audio --offline` rc=0 |
| tests | 39 lib + 17 integration + 1 no_alloc, 0 failed |
| 12 000-unit voice gain | 0.0 (< 1e-4) |
| 9 000-unit ack | started (row cutoff 100000) |
| 3 001-unit death | culled (row cutoff 3000), `events_gated` +1 |
| 3 000-unit combat hit | culled (row cutoff 2100) |
| device probe | `started=13/13 voices=13 underruns=0 backend_errors=0` |

---

## 7. Process-exit hang fix (ledger 415)

Symptom: every heavy test binary that opened the SDL3 backend hung after its
last assertion (`playable_loop` rc=124 at the 1800 s timeout); `H2_AUDIO=null`
exited rc=0.

### 7.1 Blocking call, measured

Stage logs (`H2_AUDIO_SHUTDOWN_LOG=1`) from `audio_shutdown_probe`:

```
PROBE_DROP_BEGIN
AUDIO_SHUTDOWN begin            # AudioEngine::drop
AUDIO_SHUTDOWN signalled        # running=false
AUDIO_SHUTDOWN thread_loop_exit # render thread left its loop
AUDIO_SHUTDOWN sdl_destroy_begin
<no sdl_destroy_end, no PROBE_DROPPED, process killed by timeout>
```

The blocking call is **`SDL_DestroyAudioStream`** (called from
`AudioEngine`'s render thread inside `Sdl3Backend::close`), and the old
`Drop` joined that thread without a timeout. `SDL_ClearAudioStream` and
`SDL_PauseAudioStreamDevice` before destroy do not help. A fresh stream with
one small block destroys in 0 ms; a stream with ~150 ms queued on a
non-consuming device blocks forever — SDL's destroy waits for the device to
drain, and Termux's Pulse sink does not consume in the headless test process.

### 7.2 Fix (`crates/wc3-audio/src/engine.rs`, `engine/sdl3.rs`)

Key lines: handoff send `engine.rs:1167`; `stop_render_thread` `engine.rs:1363`
(timeouts at `:67/:71/:77`); non-blocking SDL write `engine/sdl3.rs:217-230`;
SDL teardown `engine/sdl3.rs:274`; culled-row fallback stop
`events.rs:449`; death rows only `soundinfo.rs:265` / `events.rs:500`.

* Render thread **hands the backend back** to the owner over an mpsc channel
  instead of closing it (`thread_backend_handed_off`).
* `AudioEngine::stop_render_thread` (used by `Drop` and `shutdown`):
  1. signal the thread (`running=false`);
  2. wait ≤ `SHUTDOWN_HANDOFF_TIMEOUT` (100 ms) for the handoff;
  3. close the device on a dedicated `h2-audio-close` thread and wait
     ≤ `SHUTDOWN_CLOSE_TIMEOUT` (100 ms); on timeout the closer is detached
     (`close_timeout`) so the process can exit;
  4. join the render thread within the remaining budget
     (`SHUTDOWN_JOIN_TIMEOUT` = 200 ms total), else detach (`join_timeout`).
* `Sdl3Backend::write` is non-blocking: it skips a block when the queue is at
  `effective_buffer_ms + 50` instead of entering a blocking
  `SDL_PutAudioStreamData` on a stalled device.
* `Sdl3Backend::close` pauses + clears the stream, then destroys and quits the
  subsystem, with one stage log per call (`sdl_pause_*`, `sdl_clear_*`,
  `sdl_destroy_*`, `sdl_quit_subsystem_*`).

### 7.3 Evidence

`audio_shutdown_probe` (plays one voice, drops the engine, prints markers):

| backend | before | after |
|---|---|---|
| SDL3 | `PROBE_DROPPED` never printed; killed by timeout (rc=137) | rc=0, `PROBE_DROPPED ms=101`, wall 460 ms |
| null | rc=0 | rc=0, `PROBE_DROPPED ms=11-16`, wall 291-325 ms |

New tests (`crates/wc3-audio/tests/shutdown.rs`):
`audio_engine_drop_exits_process_with_null_backend` and
`audio_engine_drop_exits_process_with_platform_backend` — both spawn the probe
and require exit + `dropped_ms <= 250`.

`cargo test -p wc3-game --test playable_loop` (the reported hang):
**rc=0**, test 137.8 s, wall 147 s (was rc=124 at 1800 s);
`GAME_AUDIO_DEATH` lines = **0** (all map-wide deaths stay culled).

### 7.4 Related audibility fix found while verifying

`playable_loop` still logged `GAME_AUDIO_DEATH … dist=8063/11872` after the
cutoff gate: `unit_death_set` fell back to `{set}What` (cutoff 100000) when
`{set}Death` was culled or missing (`GreatHall` had no death row). Fixed:
`play_first_named` stops at a culled row instead of trying the next name, and
`unit_death`/`unit_death_set` have **no** `{set}What` fallback (a death row or
silence). Test: `audio_set_voice_does_not_fall_back_when_culled`.

# AUDIO EVENT WIRING, PLAYLIST, MIXER RULES (ledger 470)

**Date:** 2026-11-17
**Mission:** every gameplay event makes the right retail sound, music follows
race and region, UI feedback is driven by `UiAction` results, and the mixer
keeps the retail limits (distance gate, panning, one ack per unit per window,
64 MB decoded PCM).
**Scope (owner):** `crates/wc3-audio/**`, `crates/wc3-assetdb/src/audio_provider.rs`,
`crates/wc3-audio/tests/**`, `analysis/audio/**`.
**Read-only:** `crates/wc3-game/src/**` — the hook lines this layer needs are
listed in §10; lanes 467/468 own that file.

---

## 1. Mapping contract (shipped data only)

Every sound is selected by a table cell, never by a heuristic:

| layer | shipped source | what it decides |
|---|---|---|
| unit voices | `Units\unitUI.slk` `unitSound` → `UI\SoundInfo\UnitAckSounds.slk` / `AnimSounds.slk` | `{set}What` / `{set}Yes` / `{set}YesAttack` / `{set}Death` / `{set}Ready` / `{set}Pissed` |
| weapon impact | `Units\UnitWeapons.slk` `weapType1` × `Units\unitUI.slk` material class → `UI\SoundInfo\UnitCombatSounds.slk` | `<weapType><material>` (`MetalMediumSlice` + `Flesh`) |
| interface / deny / warnings | `UI\SoundInfo\UISounds.slk` | `InterfaceClick`, `InterfaceError`, `NoGold<Race>`, `GoldMineLow<Race>`, `NoFood<Race>`, `JobDoneSound<Race>`, `UpgradeComplete<Race>`, `HeroDies<Race>`, `RallyPointPlace` |
| abilities | `UI\SoundInfo\AbilitySounds.slk` | `BlizzardWave`, … |
| music | `UI\TriggerData.txt` `musictheme` rows | `Human1..3`, `Orc1..3`, `NightElf1..3`, `Undead1..3` + victory/defeat |
| ambience | `UI\SoundInfo\MIDISounds.slk` + `UI\WorldEditData.txt` | `LordaeronSummerDay`, `CityScapeNight`, … rendered through the shipped DLS bank |

`DistanceCutoff` stays authoritative (retail squared compare, audited in ledger
442): ack rows 100 000, combat 2 100, death 3 000.

### 1.1 Event groups

`AudioEvent::group()` partitions all 34 events into the six mixer/test groups:

| group | events |
|---|---|
| `ack` | `move_order`, `attack_order`, `worker_ack`, `selected`, `train_complete` |
| `attack` | `attack` (launch `{set}YesAttack`), `attack_impact` (`UnitCombatSounds`) |
| `death` | `death` (`{set}Death`), `hero_death` (`HeroDies<Race>`), `gold_mine_collapse` |
| `build` | `build_start`, `build_loop`, `repair_loop`, `build_complete` (`JobDoneSound<Race>`), `upgrade_complete` (`UpgradeComplete<Race>`) |
| `ui` | `ui_click`, `waypoint` (`RallyPointPlace`), `item_pickup`, `item_reward`, `item_drop`, `quest_new`, `quest_update`, `quest_completed`, `quest_failed`, `ability_cast` |
| `error` | `ui_error`, `deny_no_gold`, `deny_no_lumber`, `deny_no_food`, `low_gold` (`GoldMineLow<Race>`), `low_food` (`NoFood<Race>`), `deposit_gold`, `deposit_lumber`, `level_up` |

### 1.2 Data gaps are reported, never invented

Three events have no row in any shipped `SoundInfo` table:
`deposit_gold`, `deposit_lumber`, `level_up` (hero level-up). They stay silent
and are reported exactly once per event:

```text
AUDIO_EVENT missing=level_up unit=- race=human voice=none
```

The first gap emits the line; every later occurrence only increments
`events_missing`, so a 10-minute match cannot flood the device log with one
line per frame. A ranged unit's impact has no row either: `weapType1` is `_`
for `hrif`/`earc`/…, so `attack_impact` reports
`missing=attack_impact reason=no_weapon` once instead of guessing a melee row.

---

## 2. Event → file table (observed, 2026-11-17)

Produced by `cargo run -p wc3-audio --bin audio_event_table` against
`analysis/war3_full3` (`EVENT_ROW` lines, `rc=0`, `rows=16 missing=[]`):

| event | group | retail row | file that started |
|---|---|---|---|
| `move_order` (`hpea`) | ack | `PeasantWhat` | `Units/Human/Peasant/PeasantWhat4.wav` |
| `selected` (`hfoo`) | ack | `FootmanWhat` | `Units/Human/Footman/FootmanWhat4.wav` |
| `attack` (`hfoo`) | attack | `FootmanYesAttack` | `Units/Human/Footman/FootmanYesAttack3.wav` |
| `attack_impact` (`hfoo`→`ogru`) | attack | `MetalMediumSliceFlesh` | `Sound/Units/Combat/MetalMediumSliceFlesh1.wav` |
| `death` (`ogru`) | death | `GruntDeath` | `Units/Orc/Grunt/GruntDeath.wav` |
| `build_complete` (human) | build | `JobDoneSoundHuman` | `Sound/Buildings/Human/PeasantBuildingComplete1.wav` |
| `upgrade_complete` (orc) | build | `UpgradeCompleteOrc` | `Sound/Buildings/Orc/GruntUpgradeComplete1.wav` |
| `train_complete` (`hfoo`) | ack | `FootmanReady` | `Units/Human/Footman/FootmanReady1.wav` |
| `ui_click` | ui | `InterfaceClick` | `Sound/Interface/MouseClick1.wav` |
| `ui_error` | error | `InterfaceError` | `Sound/Interface/Error.wav` |
| `deny_no_gold` (human) | error | `NoGoldHuman` | `Sound/Interface/Warning/Human/KnightNoGold1.wav` |
| `low_gold` (human) | error | `GoldMineLowHuman` | `Sound/Interface/Warning/Human/KnightGoldMineLow1.wav` |
| `low_food` (human) | error | `NoFoodHuman` | `Sound/Interface/Warning/Human/KnightNoFood1.wav` |
| `waypoint` | ui | `RallyPointPlace` | `Sound/Interface/RallyPointPlace1.wav` |
| `hero_death` (nightelf) | death | `HeroDiesNightElf` | `Sound/Interface/Warning/NightElf/SentinelHeroDies1.wav` |
| `ability_cast` (`Blizzard`) | ui | `BlizzardWave` | `abilities/Spells/Human/Blizzard/BlizzardTarget2.wav` |

`GameAudio::last_sound_file()` returns the archive path that actually started,
and the `AUDIO_EVENT` device line now carries `def=` and `file=`, so any device
log proves the same mapping on the phone.

Weapon families are parsed from `Units/UnitWeapons.slk` (148 rows with a real
`weapType1`, e.g. `hfoo` → `MetalMediumSlice`, `ogru` → `MetalMediumChop`,
`hpea` → `MetalLightChop`); target material comes from `unitUI.slk` column 34
(`hfoo` → `Metal`, `ogru` → `Flesh`, `htow` → `Wood`). The full
`<weapType><material>` matrix (81 `UnitCombatSounds` rows) is therefore
data-addressed, not a per-unit lookup table.

---

## 3. Music: hash-ordered race playlist with cross-fade

`crates/wc3-audio/src/playlist.rs`:

* `MusicPlaylist::build(race, state)` orders the race's shipped themes by
  `fnv1a("<race>/<track>")` — deterministic across runs, no RNG in the sim path
  (replays and tests hear the same rotation).
* `GameAudio::start_race_playlist(race)` starts the first theme that resolves
  and makes the rotation current; `music_for_player_race(race)` (the call the
  game already makes at scenario start) now routes here, so the running game
  gets the rotation without a new call site.
* `GameAudio::tick_music()` — called once per frame after `set_clock_ms` —
  replaces the theme when the streamed MP3 has played out (length from
  `probe_mp3`), instead of looping one track forever or rotating on a guessed
  timer. A theme with an unknown length keeps looping.
* Every change is a cross-fade: the outgoing MP3 voice is ramped down over
  `GameAudioConfig::crossfade_ms` (default 1 200 ms) while the incoming voice
  ramps up with `PlayParams::fade_in_ms`, implemented in the mixer envelope
  (`AudioEngine::fade_out`).

`audio.music.playlist-per-race` proves, per race: three tracks, the exact hash
order (recomputed independently), rotation through all three, ≥2 cross-fades
per rotation, a live two-voice overlap on a race change (`active=2`), and the
clock-driven rotation (`tick_music` rotates after the theme length, not at
10 s).

Victory/defeat stingers keep the `music_for_race(race, state)` path and fade
from the playlist the same way.

## 4. Ambience: region switch cross-fades

`GameAudio::ambient_for_region(region, tileset, night)` keys on
`(region, tileset, day/night)`:

* same key → the running loop is reused (no restart, no double voice);
* different key → new MIDI→DLS render starts and the old loop fades out over
  `ambient_crossfade_ms` (default 800 ms);
* a region whose theme cannot be resolved stops the old loop (never leak the
  previous region's ambience);
* `leave_ambient_region()` fades the loop out and clears the key.

`audio.ambient.region-switch` asserts the cross-fade window has two loops
(`active=2`), the old one is released, and leaving stops everything.

## 5. Mixer rules

| rule | mechanism | test |
|---|---|---|
| distance per category | row `DistanceCutoff` squared gate (`dist² > cutoff²` culls); ack 100 000, combat 2 100, death 3 000; ack/death never fall through to a longer row | `audio_row_cutoff_gates_distant_voices`, `audio_voices_distance_gate_ack_1200_combat_5000`, `audio_set_voice_does_not_fall_back_when_culled` |
| 3-D panning | listener yaw → right/forward vectors, ILD ear model; a source right of the camera is louder on the right | `audio_mixer_pan` |
| ack limiter | per-unit window on the deterministic sim clock (`GameAudioConfig::ack_min_interval_ms`, default 150 ms); shared by `{set}What`/`{set}Yes`/`{set}YesAttack`; a burst collapses to one line | `audio_mixer.ack-limiter` |
| one voice per channel | row `Channel` + `CHANNELFULLPREEMPT` (retail `UnitAckSounds` channels 1..4) | `audio.stability`, engine channel counters |
| stop vs fade | `Stop`/`StopAll`/`StopCategory` are "now": they reset the envelope step to the 5 ms anti-click ramp (a cross-fade voice carries a 1 200 ms step that must not stretch an explicit stop); only `fade_out` ramps slowly | `real_unit_ack_attack_death_ui_and_music` (assetdb) |
| voice ducking | a `Voice` line ducks Music −8 dB / Ambient −3 dB with a fast attack and slow release | `gameplay_voice_line_ducks_the_music_bus` |
| pause | `set_paused(true)` mutes the Music and Ambient buses (saved and restored), UI/voices keep playing | 60 s session (`MIX_PAUSE`) |

The limiter is clock-driven (`set_clock_ms` / `advance_ms`), never wall-clock,
because replays must take the same decision for the same input. A limited ack
is not a data gap: it increments `acks_limited` and logs `limited=1`, it does
not touch `events_missing`.

## 6. Decoded PCM budget (64 MB)

* `AmbientPcmCache` is a byte-bounded LRU over rendered themes (one theme is a
  180 s 22 050 Hz mono render ≈ 15.9 MB, so 64 MB keeps the current theme plus
  the neighbours and evicts the rest — War3 ships 42 themes, caching all was
  670 MB).
* `GameAudio::pcm_bytes()` = cache bytes + engine-resident PCM **minus** the
  ambience voice currently playing, because that voice shares its `Arc<Sound>`
  with the cache; counting it twice made the cap look exceeded while the
  process held one copy.
* `audio.budget.no-overflow`: 600 simulated seconds, a region change every 30 s
  over 7 real tileset themes, 8 gameplay events per second — asserts
  `pcm_bytes() <= 64 MB` on **every** second, and that the cache evicts rather
  than growing (`evicted > 0`). Observed: peak 61.6 MB, 6 cache entries,
  14 evictions, 20 region switches, 4 703 events.

## 7. UI feedback from `UiAction` results

`UiFeedback` is the contract between the HUD and the audio layer:

| feedback | retail row | `UiFeedback::for_result` trigger examples |
|---|---|---|
| `Click` | `InterfaceClick` | `ok`, `click`, `select` |
| `Error` | `InterfaceError` | `cannot`, `can't`, `invalid`, `busy`, `deny`; lumber denial |
| `LowGold` | `GoldMineLow<Race>` → `GoldMineLowGeneric` → `NoGoldGeneric` | any result containing `gold` |
| `LowFood` | `NoFood<Race>` → `UpkeepLevel` | `food`, `supply` |
| `Waypoint` | `RallyPointPlace` → `MapPing` | `waypoint`, `rally`, `move order` |

`GameAudio::ui_feedback(feedback, race)` counts `ui_feedback`, plays the row,
and emits `AUDIO_UI feedback=<id> race=<id>`. `audio.events.ui` asserts the five
prompts and the classifier; the 60 s session drives them from `UiAction`
outcomes (`started=452`, `ui=96`, `feedback=144`).

## 8. Tests (this ledger's additions)

| test | mechanic |
|---|---|
| `audio_events_every_retail_event_has_a_mapped_sound` | `audio.events` — 31 of 34 events have data; the three gaps are named |
| `audio_events_every_mapped_event_starts_a_voice` | `audio.events` — every mapped event starts a voice; gaps reported once |
| `audio_events_ack_group_maps_order_selection_and_ready` | `audio.events.ack` |
| `audio_events_attack_group_launch_and_impact_rows` | `audio.events.attack` |
| `audio_events_death_group_rows` | `audio.events.death` |
| `audio_events_build_group_rows` | `audio.events.build` |
| `audio_events_ui_group_rows` | `audio.events.ui` |
| `audio_events_error_group_and_missing_once` | `audio.events.error` |
| `audio_music_playlist_per_race` | `audio.music.playlist-per-race` |
| `audio_ambient_region_switch_crossfades_and_stops` | `audio.ambient.region-switch` |
| `audio_mixer_ack_limiter` | `audio.mixer.ack-limiter` |
| `audio_mixer_pan_by_camera` | `audio.mixer.pan` |
| `audio_budget_no_overflow` | `audio.budget.no-overflow` |

Plus the pre-existing voice/cutoff/ducking tests, updated only where the new
deterministic clock or the cross-fade fade-in changed the timing (the ack
limiter is per unit, so sequential ack/attack probes now step the clock).

## 9. Device check — 60-second headless mix session

`cargo run -p wc3-audio --bin audio_mix_session -- 60` (real tables, streamed
MP3 music, rendered MIDI ambience, moving listener, region/music rotation,
5-second pause window). Counters printed by the run are recorded in §11.

---

## 10. Hook lines needed in `crates/wc3-game/src/game.rs` (read-only here)

The audio API is complete and test-covered; the game loop still needs to call
it at these points (line numbers observed 2026-11-17, function names stable):

| where | line (obs.) | call |
|---|---|---|
| per frame (top of `update`) | — | `audio.set_clock_ms(sim.tick * 1000 / TICKS_PER_SEC)` then `audio.tick_music()` (rotation + ack limiter clock) |
| `start_audio_for_scenario` | 934 | unchanged call `music_for_player_race(race)` now starts the race playlist (rotation + cross-fade) |
| `issue` / `activate_action` | 2343 / 2359 | `audio.ui_feedback(UiFeedback::Click | Error | Waypoint, race)` from the action result |
| `Event::GoldDeposited` / `Event::LumberDeposited` | 3146 / 3161 | `audio.play_event(AudioEvent::DepositGold | DepositLumber, …)` (documented gap, reported once) |
| `Event::UnitDied` | 3169 | `audio.play_event(AudioEvent::Death, Some(def_id), None, Some(pos))` |
| `Event::BuildingComplete` | 3223 | already `job_done(race, pos)` — keep |
| `Event::UpgradeResearched` | 3241 | already `play_event(UpgradeComplete)` — keep |
| `Event::UnitTrained` | 3262 | already `play_unit_ready` — keep |
| `Event::Damage` | 3275 | `audio.attack_impact(Some(attacker_def), Some(target_def), Some(pos))` |
| `Event::ProjectileHit` | (same block) | `audio.attack_impact(Some(from_def), Some(to_def), Some(pos))` |
| `Event::SpellCast` / `AbilityUsed` | (ability block) | `audio.spell(<AbilitySounds row>, Some(pos))` |
| `Event::Victory` | 3283 | already `music_for_race(race, Victory | Defeat)` — keep |
| `Event::Error(message)` | 3298 | already `deny_no_*`; add `audio.ui_feedback(UiFeedback::for_result(&message))` fallback |
| build order issued | `Command::Build` path | `audio.play_event(AudioEvent::BuildStart, None, Some(race), Some(pos))` |
| ground tap placement | order path | `audio.ui_feedback(UiFeedback::Waypoint, race)` |
| low resource/upkeep | economy tick | `audio.ui_feedback(UiFeedback::LowGold | LowFood, race)` |
| hero level | `Event::` level path | `audio.play_event(AudioEvent::LevelUp, …)` (reported once, no row) |
| pause menu | `MenuAction::Pause` handling | `audio.set_paused(true/false)` |

## 11. Measured evidence

### 11.1 Tests (2026-11-17, `h2-cargo test -p wc3-audio --offline`, `TEST_EXIT=0`)

```text
running 49 tests            (lib: playlist hash order/wrap, wav/mp3, slk parse)
test result: ok. 49 passed; 0 failed
running 32 tests            (tests/audio.rs)
test audio_events_ack_group_maps_order_selection_and_ready ... ok
test audio_events_attack_group_launch_and_impact_rows ... ok
test audio_events_death_group_rows ... ok
test audio_events_build_group_rows ... ok
test audio_events_ui_group_rows ... ok
test audio_events_error_group_and_missing_once ... ok
test audio_events_every_retail_event_has_a_mapped_sound ... ok
test audio_events_every_mapped_event_starts_a_voice ... ok
test audio_music_playlist_per_race ... ok
test audio_ambient_region_switch_crossfades_and_stops ... ok
test audio_mixer_ack_limiter ... ok
test audio_mixer_pan_by_camera ... ok
test audio_budget_no_overflow ... ok
test result: ok. 32 passed; 0 failed
running 1 test (shutdown)   ok
running 2 tests (no_alloc)  ok
TEST_EXIT=0
```

Key lines from the runs:

```text
ATTACK_GROUP launch=1 impact=2 line=AUDIO_IMPACT unit=hfoo target=ogru row=MetalMediumSliceFlesh material=Flesh voice=2
ACK_LIMIT line=AUDIO_ACK hpea limited=1 gap_ms=0 voice=none
ACK_BURST played=1 limited=11 started=4
PAN right-of-camera left=0.1393 right=0.4857
PAN left-of-camera  left=0.4857 right=0.1393
PAN centre         left=0.3536 right=0.3536
PLAYLIST race=human tracks=["Human1", "Human2", "Human3"] started=3 music_started=3
PLAYLIST race=orc tracks=["Orc1", "Orc2", "Orc3"] started=3
MUSIC_CROSSFADE active=2 started=14 crossfades=13
AMBIENT_REGION line=AUDIO_AMBIENT tileset=Y night=true theme=CityScapeNight ... voice=2
BUDGET seconds=600 regions=7 events_started=4703 peak_pcm_mb=61.6 cache_entries=6 cache_mb=60.7 evicted=14 ambient_started=20 ambient_crossfades=19 acks_limited=0 missing=0
```

### 11.2 Event table probe (`audio_event_table`, `TABLE_EXIT=0`)

```text
EVENT_ROW name=order_ack def=PeasantWhat file=Units/Human/Peasant/PeasantWhat4.wav
EVENT_ROW name=unit_selected def=FootmanWhat file=Units/Human/Footman/FootmanWhat4.wav
EVENT_ROW name=attack_launch def=FootmanYesAttack file=Units/Human/Footman/FootmanYesAttack3.wav
EVENT_ROW name=attack_impact def=- file=Sound/Units/Combat/MetalMediumSliceFlesh1.wav
EVENT_ROW name=death def=GruntDeath file=Units/Orc/Grunt/GruntDeath.wav
EVENT_ROW name=build_complete def=JobDoneSoundHuman file=Sound/Buildings/Human/PeasantBuildingComplete1.wav
EVENT_ROW name=upgrade_complete def=UpgradeCompleteOrc file=Sound/Buildings/Orc/GruntUpgradeComplete1.wav
EVENT_ROW name=train_complete def=FootmanReady file=Units/Human/Footman/FootmanReady1.wav
EVENT_ROW name=ui_click def=- file=Sound/Interface/MouseClick1.wav
EVENT_ROW name=ui_error def=- file=Sound/Interface/Error.wav
EVENT_ROW name=deny_no_gold def=NoGoldHuman file=Sound/Interface/Warning/Human/KnightNoGold1.wav
EVENT_ROW name=low_gold def=- file=Sound/Interface/Warning/Human/KnightGoldMineLow1.wav
EVENT_ROW name=low_food def=- file=Sound/Interface/Warning/Human/KnightNoFood1.wav
EVENT_ROW name=waypoint def=- file=Sound/Interface/RallyPointPlace1.wav
EVENT_ROW name=hero_death def=HeroDiesNightElf file=Sound/Interface/Warning/NightElf/SentinelHeroDies1.wav
EVENT_ROW name=ability_cast def=- file=abilities/Spells/Human/Blizzard/BlizzardTarget2.wav
EVENT_TABLE rows=16 missing=[] events_played=12 impacts=1 ui_feedback=5 unresolved=0
```

### 11.3 60-second mix session (`audio_mix_session -- 60`, `RUN_EXIT=0`)

```text
MIX_SESSION begin seconds=60 tables=13 defs=2529 units=466 weapons=148 themes=30 regions=13
MIX_REGION second=0 region=Region0 tileset=Y night=false
MIX_REGION second=10 region=Region1 tileset=L night=true
MIX_MUSIC second=20 race=orc tracks=3
MIX_REGION second=30 region=Region3 tileset=D night=true
MIX_SCORE paused=1 music_bus=0 ambient_bus=0 rms=0.00000
MIX_SCORE paused=0 music_bus=1 ambient_bus=1 rms=0.02602
MIX_SESSION done seconds=60 started=452 impacts=20 music_rotations=2 culled=4 evicted=1
  peak_pcm_mb=55.8 cache_entries=5 cache_mb=50.5 budget_mb=64 active=6 max_active=12
  chan_full=72 chan_preempt=111 crossfades_music=4 crossfades_ambient=5
  acks_limited=0 pauses=1 missing=24 unresolved=1
GAME_AUDIO_HOOKS device=headless acks=48/48 limited=0 missing=0 acks_gated=0 gated=4
  ui=96 feedback=144 events=336 events_gated=0 impacts=20 music=5/0 crossfades=4
  ambient=6/0 ambient_crossfades=5 taps=0 unresolved=1 pcm_mb=51.1 cache=5/50_mb
  evicted=1 budget_mb=64 buffered_ms=None underruns=0 dropped=0 occluded=0
  ducked_blocks=6060 voices=6 max_active=12 chan_full=72 chan_preempt=111 deduped=21
```

`missing=24` is the counter for `attack_impact` on ranged units
(`weapType1 = _`), reported **once** as
`AUDIO_EVENT missing=attack_impact reason=no_weapon unit=hrif target=hrif`.
`culled=4` are distance-gate drops; `evicted=1` is one PCM cache eviction.

## 12. rc

| check | rc |
|---|---:|
| `scripts/h2-cargo check -p wc3-audio --tests --bins --offline` | 0 |
| `scripts/h2-cargo test -p wc3-audio --offline` (49 + 32 + 1 + 2) | 0 |
| `scripts/h2-cargo check -p wc3-assetdb --offline` | 0 |
| `h2-cargo test -p wc3-assetdb --test audio_parity --test audio_hooks --test hook_resolution` | 0 (4 + 3 + 2 passed) |
| `scripts/h2-cargo check -p wc3-game --offline` (read-only consumer) | 0 |
| `cargo run -p wc3-audio --bin audio_event_table` | 0 (16/16 rows, 0 missing) |
| `cargo run -p wc3-audio --bin audio_mix_session -- 60` | 0 |
| `cargo test -p wc3-audio --test audio audio_budget_no_overflow` | 0 (peak 61.6 MB ≤ 64 MB, 14 evictions) |

## 12.1 Environment repair met in-session

The layered-install tests were red because
`analysis/agent-assets/wc3-install/War3.mpq` pointed into
`$PREFIX/tmp/opencode/wc3_iso/`, which the device restart had wiped (only the
root-owned `.symlink-bak` was left). The base archive was re-extracted from the
operator's own ISO
(`/storage/emulated/0/Download/wr3/iso/warcraft3_roc.iso` → `War3.mpq`,
438 529 036 B, `7z x`, rc=0) and the tests went green again.
`MpqLayers::open` now requires all three archives and prints
`SKIP layered-install audio tests: N of 3 archives present [...]` instead of
failing with a confusing "voice did not start" when the operator's base
archive is not mounted.

## 12.2 Known gaps / next

* Ranged weapon impact has no row in the shipped tables (`weapType1 = _` for
  `hrif`/`earc`/…; `UnitWeapons.slk` has no missile-sound column in RoC and
  `UnitCombatSounds.slk` only carries the melee matrix). Reported once as
  `missing=attack_impact reason=no_weapon`, not invented.
* `hero level-up` has no retail row (`AUDIO_EVENT missing=level_up`, once).
* `crates/wc3-assetdb/src/bin/audio_matrix.rs` (not this lane's file) does not
  yet enumerate `attack_impact`/`selected` in its dedicated-hook match
  (lines 140-152), so its `events_missing` list will name those two; the probe
  still exits 0. Owner: assetdb/tooling lane.
* `block_peak=1.000` in the 60 s stress mix (8 events/s + 12 voices): the mixer
  clamps at ±1 instead of brickwalling; a look-ahead limiter is not part of
  this ledger and was not added silently.

## 13. H2

Ledger 470 (`wc3rust/audio-wiring`): register → checkpoint per phase →
capture → experience-add → close. IDs in the session report.

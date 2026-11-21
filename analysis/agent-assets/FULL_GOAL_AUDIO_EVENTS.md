# FULL GOAL — complete sound coverage + asset path resolution (ledger 351)

**Date:** 2026-11-14
**Gate:** `cargo test -p wc3-audio` rc=0 · `asset_catalog` rc=0 · `sound_paths` rc=0
**Owner:** `crates/wc3-audio/**`, `crates/wc3-assetdb/**`, `analysis/agent-assets/**`; `game.rs` read-only (patch delivered).

## 1. `audio.events` — every gameplay event mapped to its retail sound

Policy module: `crates/wc3-audio/src/events.rs` (`AudioEvent::ALL` = 15 events),
`event_defs()` names the retail `SoundInfo` rows, `GameAudio::play_event()` plays them.

| Event | Retail source | Definitions | Status |
|---|---|---|---|
| attack | `UnitAckSounds.slk` via `unitUI.slk` sound set | `<set>YesAttack/Attack/What` | mapped |
| death | `UnitAckSounds.slk` via sound set | `<set>Death/What` | mapped |
| ability cast | `AbilitySounds.slk` | e.g. `BlizzardWave` | mapped |
| build complete | `UISounds.slk` | `JobDoneSoundHuman/Orc/NightElf/Undead` | mapped (per race) |
| worker ack | `UnitAckSounds.slk` via sound set | `<set>What` (`PeasantWhat` …) | mapped |
| ui click | `UISounds.slk` | `InterfaceClick` (+ fallback chain) | mapped |
| ui error / deny | `UISounds.slk` | `InterfaceError`, `ErrorMessage` | mapped |
| **level-up** | **no row in any shipped table** (`grep -i level` → only `UpkeepLevel`) | — | **documented data gap** (stays silent, nothing invented) |
| item pickup | `UISounds.slk` | `ItemGet` → `ItemReward` | mapped |
| item reward | `UISounds.slk` | `ItemReward` → `ItemGet` | mapped |
| item drop | `UISounds.slk` | `ItemDrop` | mapped |
| quest new | `UISounds.slk` | `QuestNew` → `QuestLogModified` | mapped |
| quest update | `UISounds.slk` | `QuestUpdate` → `QuestLogModified` → `QuestNew` | mapped |
| quest completed | `UISounds.slk` | `QuestCompleted` | mapped |
| quest failed | `UISounds.slk` | `QuestFailed` | mapped |

**Coverage: 14 / 15 events have retail data and start a voice; 1 documented gap (level-up).**

Tests (`crates/wc3-audio/tests/audio.rs`):
- `audio_events_every_retail_event_has_a_mapped_sound` — asserts 14 mapped / `["level_up"]` absent against the real tables (2,316 defs, 463 unit names), and that every named definition exists.
- `audio_events_every_mapped_event_starts_a_voice` — plays each event through `GameAudio`; 14 voices, level-up silent.

`game.rs` event hooks (patch, not applied — file is HUD-owned):
`analysis/agent-assets/patches/0007-game-audio-event-hooks.patch` (77 lines, parses clean):
`Event::UnitDied → unit_death(unit, pos)`, `Event::BuildingComplete → job_done(race, pos)`,
`finish(winner) → music_for_race(race, Victory|Defeat)`, plus `audio_race()` / `player_audio_race()` helpers.

## 2. `audio.race-themes` — race music + victory/defeat stingers

`Race::theme_names(state)` → retail `TriggerData.txt` keys:
playing `Human1..3` / `Orc1..3` / `NightElf1..3` / `Undead1..3`;
victory `VictoryHuman|HumanVictory` (same per race); defeat `DefeatHuman|HumanDefeat`.
`GameAudio::music_for_race(race, state)` streams the shipped MP3.

Test `audio_race_themes_victory_defeat_resolve`: loads `UI/TriggerData.txt` (30 RoC `musictheme` rows; 42 with TFT), verifies **12 / 12** race×state themes exist in the table and every MP3 file exists in the archive (`Sound/Music/mp3Music/*.mp3`), then starts all 4 victory and 4 defeat stingers headlessly.

## 3. `audio.spatial` — 3D attenuation invariants

`engine.rs` mixer: distance ≤ `reference_distance` → unity; beyond → `(ref/d)²` gain (energy `(ref/d)⁴`), constant-power pan.

Tests:
- `audio_spatial_attenuation_follows_reference_distance`: 50 vs 100 units equal (≤2 %), 200 units < 10 % of reference, 400 units < 10 % of 200, all audible.
- `audio_spatial_larger_reference_radius_raises_far_gain`: ref 200 at 400 units > 10× ref 100 energy.

## 4. `assets.paths` — resolution counts (current tree)

| Asset class | Result |
|---|---|
| Textures (BLP) | **3,935 / 3,935** decode ok; mips **28,748 / 28,748**; incomplete 0; layout_bad 0 |
| Models (MDX) | **2,048 / 2,048** parsed; 5,216 texture refs; **0 unresolved** |
| Icons | **1,408** decoded; **15 / 15** biome tile sets ok |
| Sounds (install layers `War3xlocal > War3x > War3`) | **4,177 / 4,232 (98.7 %)** exact+listfile; ambiguous **0**; by layer: War3 2,992, **War3xlocal 1,017**, War3x 168 |
| Absent sound paths | **55** unique, enumerated per path with searched layers (`sound-paths-install-absent.tsv`); basenames verified nowhere in the mounted set (e.g. `TreantDeath1.wav`, `HeroRangerYesAttack{1,2,3}.wav`) — genuine data gaps, not resolution bugs |

Artifacts refreshed: `asset-catalog.{json,md}`, `sound-paths-install.{json,md,tsv}`.

## 5. `audio.stability` — device evidence

- Probe (`H2_DEVICE_SLOT=1 … audio_underrun_probe --seconds 6`): rc=0, `backend=sdl3 target_ms=150 min_buffered_ms=53 max=160 frames=294400 **underruns=0** backend_errors=0`.
- APK session (`H2_DEVICE_SLOT=1 am start`), 19 `GAME_AUDIO_STATS` samples: **all `underruns=0`**, buffered **107–128 ms** (required 100–130), 1,264,640 frames (~26 s), `backend_errors=0 dropped=0`, 58–61 fps.
- `analysis/agent-assets/device_audio.log`; `analysis/agent-assets/device_run.json` (+ tech-lead copy) with `status=PASS` and APK `sha256 703a2d58…` matching the built APK.
- Mixer policy test `audio_stability_buffer_policy_is_a_150ms_queue`: target 150 ms, clamp 20–1000 ms, low watermark 128 ms (inside the device band).

## 6. Tests + rc

| Check | rc | Result |
|---|---:|---|
| `cargo test -p wc3-audio` | **0** | 5 suites / **35 passed** (was 29; +6 mechanic tests) |
| `asset_catalog --mpq …` (full) | **0** | `all_ok=true`, 71.7 s |
| `sound_paths --install analysis/agent-assets/wc3-install` | **0** | 4,177/4,232 |
| `accept_full_port.sh` | 0 | **P0-11 PASS with verified provenance** (`stats=19 underruns=[0×19] provenance=device_run status=PASS apk_sha_match=True`); overall 4/14, remaining rows belong to sim/data/HUD/QA layers |

Mechanic runner: `analysis/tech-lead/run_mechanic_tests.sh:232` already targets `audio.engine → wc3-audio`; new tests live in one audio-group file `tests/audio.rs` per `TEST_POLICY.md`.

## 7. Remaining gaps (named, honest)

1. `level_up` has no sound in the shipped `SoundInfo` tables — silent by data, documented in code + test.
2. 55 TFT sound refs absent from the mounted layer set (need a TFT patch archive; per-path list kept).
3. `game.rs` hooks are a patch until the HUD owner merges `0007` (file read-only this round).
4. FULL PORT remains `FULL_PORT_NOT_ACCEPTED` 4/14; audio row is verified.

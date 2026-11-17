# ASSETS FULL GOAL — every triggered sound mapped and playing (ledger 351)

**Date:** 2026-11-14
**Gate:** `cargo test -p wc3-audio` rc=0 · `asset_catalog` rc=0 · `sound_paths` rc=0
**Owner:** `crates/wc3-audio/**`, `crates/wc3-assetdb/**`, `analysis/agent-assets/**`; `game.rs` read-only (hook patch 0008).

## 1. Event → definition → played → evidence

| Event | Retail definition | Played | Evidence |
|---|---|---|---|
| select worker | `{set}What` (own set) | yes | `GAME_AUDIO_SELECT unit=hpea set=Peasant def=PeasantWhat voice=1` |
| move order | `{set}Yes` (own set) | yes | `GAME_AUDIO_MOVE unit=hpea set=Peasant def=PeasantYes voice=2` |
| attack order | `{set}YesAttack` (own set) | yes | `GAME_AUDIO_FIGHT_ATTACK unit=hfoo set=Footman def=FootmanYesAttack voice=7` |
| attack (combat) | `{set}YesAttack` | yes | same hook (`issue`/`play_unit_attack`) |
| death | `{set}Death` (own set) | yes | `GAME_AUDIO_FIGHT_DEATH unit=ogru set=Grunt def=GruntDeath voice=8` |
| build start | `ConstructingBuilding<Default/NightElf/Orc/Undead>` | yes | `GAME_AUDIO_BUILD_START race=human def=ConstructingBuildingDefault voice=3` |
| build loop | `BuildingConstructionLoop` / `OrcBuildingConstructionLoop` / `UndeadBuildingConstructionLoop` | yes | `GAME_AUDIO_BUILD_LOOP race=human def=BuildingConstructionLoop voice=4` |
| repair loop | `Repair` (AnimSounds, `PeonRepair*`) | yes | unit test `audio_events_every_mapped_event_starts_a_voice` |
| build complete | `JobDoneSound<Race>` | yes | `GAME_AUDIO_BUILD_COMPLETE race=human def=JobDoneSoundHuman voice=5` |
| training complete | `{set}Ready` (own set) | yes | `GAME_AUDIO_TRAIN_COMPLETE unit=hfoo set=Footman def=FootmanReady voice=6` |
| upgrade complete | `UpgradeComplete<Race>` | yes | mapped + played in the event test (no sim event yet) |
| gold/lumber deposit | **no retail row** | no (documented) | matrix `gaps` |
| level-up | **no retail row** | no (documented) | matrix `gaps`; `grep -i level` → only `UpkeepLevel` |
| error/deny | `InterfaceError`/`ErrorMessage`; `NoGold/NoLumber/NoFood<Race>` | yes | `GAME_AUDIO_DENY race=human def=NoGoldHuman voice=10` |
| hero death | `HeroDies<Race>` | yes | `GAME_AUDIO_HERO_DEATH race=human def=HeroDiesHuman voice=9` |
| quest new/update/complete/fail | `QuestNew/QuestUpdate/QuestCompleted/QuestFailed` | yes | `GAME_AUDIO_QUEST def=QuestCompleted voice=11` |
| victory / defeat | `Victory<Race>` / `Defeat<Race>` music | yes | `GAME_AUDIO_VICTORY race=human voice=13` |
| ambient (map) | `war3map.w3s` custom sound at `w3r` region centre | yes | `GAME_AUDIO_MAP_AMBIENT sound=gg_snd_WaterLakeLoop1 region=LakeLoop01 pos=(-1568,-1232) voice=12` |
| race ambience / music | `AmbienceSounds.slk`, `TriggerData musictheme` | yes | `audio_race_themes_victory_defeat_resolve` (12/12) |
| gold mine collapse | `GoldMineCollapse<Race>` | mapped | matrix |

## 2. Map sounds (`war3map.w3s` / `war3map.w3r`)

- Loader: `crates/wc3-audio/src/map_sounds.rs` (version 1 w3s: name/file/eax + 68-byte trailer with `flags`; w3r v5: bounds, name, creation/weather/ambient/RGBA). Playback in `game_audio.rs` (`play_map_sound`, `play_map_ambient`, MP3 streamed, WAV decoded, 3D position at the region centre).
- Verified on the shipped map `analysis/war3_map/Human04`: **26 sounds / 87 regions** parsed; `gg_snd_WaterLakeLoop1`/`WaterStreamLoop1` carry the looping flag (7), dialogue flags 0, 3D unit sounds flag 6; ambient played at `LakeLoop01` centre `(-1568, -1232)`.
- **`(2)Ponds_v1.4.w3x` is not present on this device** (searched `analysis/**`, app files dir and `/sdcard/Download`, case-insensitive: no file). Its specific ambient list cannot be verified; the loader is generic and tested against the shipped map. Documented gap, not fabricated.

## 3. Audit — `analysis/agent-assets/AUDIO_MATRIX.json`

`audio_matrix --install analysis/agent-assets/wc3-install --out analysis/agent-assets/AUDIO_MATRIX.json` rc=0:

| Family | Total | Have | Missing |
|---|---:|---:|---|
| units (`unitUI.slk` id → set → ack row+file) | 812 | **696** | 116 (`…:set norow` — buildings/doodads/neutral objects ship no ack row) |
| abilities (`AbilitySounds.slk` def → file) | 57 | **57** | 0 |
| events (29 events; race-scoped per race) | 56 checks | **53** | 0 missing; **3 documented gaps** (`deposit_gold`, `deposit_lumber`, `level_up`) |

`units` have-count only claims an ack definition + resolvable file; the 116 "norow" entries are objects with no acknowledgement sound in retail, listed per id in the JSON.

## 4. Device proof (H2_DEVICE_SLOT=1)

`audio_scenario_probe` on the real SDL3 device: **13/13 scenario sounds started, underruns=0, backend_errors=0**.
Logcat: `analysis/agent-assets/device_audio_scenario.log` (SELECT, MOVE, BUILD_START, BUILD_LOOP, BUILD_COMPLETE, TRAIN_COMPLETE, FIGHT_ATTACK, FIGHT_DEATH, HERO_DEATH, DENY, QUEST, MAP_AMBIENT, VICTORY, SUMMARY).

## 5. Tests + rc

| Check | rc | Result |
|---|---:|---|
| `cargo test -p wc3-audio` | **0** | 5 suites / **37 passed** (event coverage 26/29, playback 26, unit voices, map sounds, spatial, buffer policy) |
| `asset_catalog --mpq … --skip-models` | **0** | BLP 3,935/3,935; mips 28,748/28,748 |
| `sound_paths --install …` | **0** | 4,177/4,232; ambiguous 0; absent 55 |
| `audio_matrix` | **0** | AUDIO_MATRIX.json written |
| `audio_scenario_probe` (device) | **0** | 13/13, underruns=0 |
| `cargo check -p wc3-game --lib` | 101 | blocked by other layers' in-flight edits (`wc3-render/fog.rs` wgpu type, then `wc3-re/mapcontent.rs` Reader::skip); no errors in owned crates |

`game.rs` hooks (read-only): `analysis/agent-assets/patches/0008-game-audio-remaining-hooks.patch` (78 lines, parses clean) — `UnitTrained → play_unit_ready` (`GAME_AUDIO_READY`), `Event::Error → deny/error` rows (`GAME_AUDIO_DENY`). Build start/loop, upgrade, deposit and hero death have no sim event/hero unit in the current game slice; the audio API is ready (events + tests).

## 6. Remaining gaps (named)

1. `deposit_gold`, `deposit_lumber`, `level_up`: no retail row — silent by data.
2. 116 unit ids ship no ack row (buildings/doodads/neutral); listed in AUDIO_MATRIX.json.
3. `(2)Ponds_v1.4.w3x` absent from disk — loader ready, generic test on Human04.
4. 55 TFT sound refs absent from mounted layers (existing `sound-paths-install-absent.tsv`).
5. `game.rs` patch 0008 not applied (HUD-owned); full APK not rebuilt this round.

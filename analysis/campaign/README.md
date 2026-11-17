# Campaign scaffold — data model and the `Human01` worked example

Ledger 448. This directory documents the campaign layer: how a campaign maps onto
missions, cutscenes and triggers, what was parsed from the real game data, and
how to regenerate the artifacts.

## What is actually shipped (evidence)

| fact | value | source |
| --- | --- | --- |
| campaign maps in `War3.mpq` | **44** under `Maps\Campaign\` | `analysis/war3_mpq_listing.txt` (`FILE Maps\Campaign\…`) |
| cinematic call inventory (44 maps) | 9981 cinematic, 1313 transmissions, 11243 camera calls, 999 camera setups | `analysis/agent-re/phase0/cutscenes.json` (`ok=true`) |
| `Human01.w3m` | 245 798 B, script 385 683 B / 7824 lines / 606 functions | `crates/wc3-re` cutscene inventory |
| `Human01` cinematics | 241 cinematic calls, 40 transmissions, 219 camera calls, 18 camera setups | same |
| campaign descriptor (`war3campaign.w3f` in `.w3n`) | **not in these archives** (no `.w3n` entry in the listfile) | `grep -i '\.w3n' analysis/war3_mpq_listing.txt` → 0 |

**Correction to the mission brief:** `war3map.w3c` is the **camera list**
(`wc3-re/src/mapfiles.rs`, `wc3-re/src/cutscenes.rs` both parse it as cameras),
not campaign metadata. Campaign metadata is `war3campaign.w3f` inside a `.w3n`
campaign archive, which this installation does not carry. The model below is
therefore built from what the maps themselves contain — no invented chapter
titles or objectives.

## Data model (`crates/wc3-game/src/campaign.rs`)

```
Campaign { name, chapters: Vec<Chapter> }          // from Maps\Campaign\*.w3m
Chapter  { id, title, map, interlude }             // Human02Interlude etc.
Mission  { id, title, author, description,
           recommended_players, map,
           objectives: Vec<Objective>,
           cinematics: Vec<Cinematic>,
           movies: Vec<String>,
           triggers: Vec<String>,
           script_bytes }
Objective { variable, title, description, icon, required, discovered,
            items: Vec<QuestItem> }
Cinematic { name, kind: Intro|Outro|Victory|Defeat|Scene,
            events: Vec<CinematicBeat> }
CinematicBeat { kind: Camera|Transmission|Scene|Movie|Filter|CinematicMode|Timer|Order,
                line, text }
```

Chapter ordering follows the retail campaign order: `Prologue*`, `Human*`,
`Undead*`, `Orc*`, `NightElf*`, credits; an interlude sorts after the mission it
closes (`Human02` < `Human02Interlude` < `Human03`).

### Where each field comes from

| field | source | rule |
| --- | --- | --- |
| `Mission.title/author/description/players` | `war3map.w3i` + `war3map.wts` | `TRIGSTR_n` resolved through the string table; a blank title falls back to the file stem |
| `objectives` | `war3map.j` | `CreateQuestBJ(type, title, desc, icon)` + `set udg_X = GetLastCreatedQuestBJ()` + `CreateQuestItemBJ(udg_X, text)`; `REQ` in the type constant sets `required`, `DISCOVERED` sets `discovered` |
| `cinematics` | `war3map.j` | one entry per `function … endfunction` that contains a cinematic-family call; `kind` from the function name (`Intro`/`Outro`/`Victory`/`Defeat`) |
| `movies` | `war3map.j` | `PlayCinematic` / `PlayMovieBJ` / `PlayModelCinematic` arguments |
| `triggers` | `war3map.j` | `function InitTrig_<name>` in declaration order |

Everything is script-driven, so a mission whose script does not create quests
reports zero objectives instead of a fabricated list.

## Worked example

```text
scripts/h2-cargo run -p wc3-game --example campaign_report -- \
    --mission Human01 --frames 900 --write
```

The generated `human01.json` in this directory is the artifact. Observed output
(2026-11-16, `EXITR=0`):

| field | value |
| --- | --- |
| mission id / title / author | `Human01` / `Альянс01` (from `TRIGSTR_043`) / `Blizzard Entertainment` |
| recommended players | `1` |
| objectives | **3** — `Странбард` (required, 2 checklist items), `Атаман разбойников` (optional, 2), `Спасите Тимми` (optional, 2) |
| cinematics | **25** functions, **87** beats: 17 camera, 40 transmissions, 23 fades, 7 interface toggles |
| camera setups driven | **15** (`gg_cam_Start01` … `gg_cam_CamEndCounterAngle`) |
| trigger init functions | **130** |
| campaign view | 44 chapters, `Prologue01` … `WarCraftIIICredits` |

The file lists every quest with its checklist, every cutscene beat with its
`war3map.j` line number, and the camera setups the cutscenes apply.

## Playable campaign runtime (ledger 468)

The data model above is the *static* view. Ledger 468 adds the running layer in
the same module (`crates/wc3-game/src/campaign.rs`):

| type | role |
| --- | --- |
| `CampaignRegistry` | the 44 chapters in retail order plus `CampaignProgress`; `rows()` is the mission-select list (`cleared` / `unlocked` / `locked`), `complete(index)` marks a chapter and unlocks the next |
| `CampaignProgress` | `W3CP` file (magic, version, payload, FNV-1a checksum — the `wc3-sim` `W3SV` container discipline), written through temp + `rename()`, `$HEROFOLENDS_CAMPAIGN_SAVE` or `~/.local/share/herofolends/campaign.sav` |
| `QuestLog` / `QuestRow` | the objectives panel, live from the VM's `world.quests`; static objectives bind to the script's `CreateQuestBJ` handles by title then by script order, and live text is resolved through the map's `war3map.wts` |
| `CameraDirector` / `CameraKeyframe` | the cutscene camera stream: every `CameraSetupApply*`, `SetCameraField`, `PanCameraTo*` becomes a keyframe (`t`, target, distance, yaw, pitch, z, blend) the renderer samples |
| `FadeEvent` | `CinematicFadeBJ` (via `SetCineFilter*` + `DisplayCineFilter`) as a timed tint |
| `SubtitleQueue` / `SubtitleLine` | `TransmissionFromUnitWithNameBJ` → `SetCinematicScene` (and `DisplayTimedTextToPlayer`) as speaker + text + duration + portrait unit handle |
| `CampaignSession` | one running mission: registry, mission, quests, director, subtitles, movies, victory/defeat, contract log |
| `MissionSessionReport` | the storyboard generator's input (frames, keyframes, fades, subtitles, quests) |

### Game API (the HUD/screens lane consumes this)

| call | meaning |
| --- | --- |
| `Game::start_campaign_mission(index)` / `_by_id("Human01")` | loads the chapter's map (script entry points included), sets the human seat, logs `CAMPAIGN_MISSION idx=… name=…` |
| `Game::campaign_chapters()` | mission-select rows with `ChapterState` |
| `Game::quest_rows()` | `Vec<QuestRow>` (objectives panel) |
| `Game::campaign_frame(dt)` | one call per frame: syncs quests/camera/subtitles, starts a requested movie, restores the mission if the melee end rule fired |
| `Game::camera_director()` / `cutscene_camera()` | keyframe stream / pose at the cutscene-clock time |
| `Game::screen_fade()` | `(alpha, colour)` of the running fade |
| `Game::subtitle_now()` | the line on screen (speaker, text, duration, portrait handle) |
| `Game::cutscene_active()` | the script holds the camera away from gameplay (letterbox on) |
| `Game::campaign_cine_summary()` | `CAMPAIGN_CINE frames=… cameras=N fades=N transmissions=N subtitles=[…]` |
| `Game::campaign_next_mission()` | loads the next unlocked chapter |

### Contract lines

`CAMPAIGN_MISSION`, `CAMPAIGN_HUMAN_SLOT`, `QUEST add=`, `QUEST completed=`,
`CAMPAIGN_DONE`, `CAMPAIGN_SAVE_FAILED`, `MOVIE_PLAY`, `MOVIE_MISSING <name>`,
`MOVIE_END`, `MOVIE_SKIP`, `CUTSCENE subtitle …`, `CAMPAIGN_CINE …`.

### What the VM had to record (cross-crate, additive)

| gap | fix |
| --- | --- |
| `QuestCreateItem` returned a handle with no item; `QuestItemSetDescription/Completed` addressed the quest, not the item | `World.quest_items` handle table + the three natives implemented to `common.j` |
| `PlayCinematic` fell through unimplemented | recorded into `World.movie_requests`; the game layer resolves and decodes it |
| `CameraSetupCreate/SetField/SetDestPosition/Apply*` were no-ops | setup table + apply events; the director turns them into keyframes |
| `SetCineFilter*`/`DisplayCineFilter` were no-ops (so `CinematicFadeBJ` vanished) | typed `CineFilter` state + fade events |
| `SetCinematicScene`/`DisplayTimedTextToPlayer` produced no subtitle | `World.subtitles` queue |
| `GetPlayersAll` missing, `IsPlayerInForce` argument order reversed | fixed; `GetLocalPlayer` + single-player wildcard (`local_player_any`) so Blizzard.j's `if GetLocalPlayer() == whichPlayer` blocks run for the map's seat |
| camera getters always returned 0 | live `CameraState` + `GetCameraTargetPosition*`/`GetCameraField` |
| cutscene order lost | `World.cinematic_log`: one ordered beat per camera/fade/scene/subtitle |

## Tests

| id (H2 mechanic map) | Rust test |
| --- | --- |
| `campaign.scaffold.parses-human01` | `crates/wc3-game/src/campaign.rs::campaign_scaffold_parses_objectives_and_beats` (synthetic script) **and** `examples/campaign_report` on the real `Human01.w3m` (artifact in this directory) |
| `campaign.chapter-order` | `campaign_chapter_order_follows_the_retail_campaign` |
| `cinematic.human01-opening` | `crates/wc3-sim/tests/cinematic_campaign.rs::cinematic_human01_opening_emits_camera_and_transmission_events` (300 ticks, ≥1 camera/transmission beat, no unresolved cinematic natives) |
| `campaign.mission-load` | `crates/wc3-game/tests/campaign.rs::campaign_mission_load` (Human01 loads, script boots, 300 ticks, the human slot's units live) |
| `campaign.quest-log` | `campaign_quest_log` (3 objectives parsed; live table binds, checklist fills, completion flips the panel) |
| `campaign.progress-persists` | `campaign_progress_persists` (`W3CP` written, reload keeps `Human01` cleared, `Human02` unlocked) |
| `campaign.cine.camera-path` | `campaign_cine_camera_path` (4 setups → ≥4 keyframes, monotone time, ≤400 units per 30 Hz sample) |
| `campaign.cine.subtitle-queue` | `campaign_cine_subtitle_queue` (speaker, duration, portrait handle, `TRIGSTR` resolution, reading-time floor) |
| `campaign.cutscene-decodable` | `campaign_cutscene_decodable` (`#[ignore]`, needs the movie pack: `--features movie_decoder -- --ignored`; first picture frame decodes, `MOVIE_MISSING` for an absent name) |
| `campaign.chapter-order` | `campaign_chapter_order` (44 chapters in retail order) |

Rust identifiers cannot contain dots, so the H2 ids map to the snake_case test
names above. The real-map parse is also asserted by the `Human01` artifact being
regenerated by the example (the JSON in this directory is the checked output).

Movie media and the playback path are documented in `../media/BINK_INVENTORY.md`
and `../media/README.md` (nine AVI/`BLZ0` cinematics, own decoder only).

## Storyboard

`Human01-storyboard.md` in this directory is generated from the running runtime:

```text
scripts/h2-cargo run -p wc3-game --example campaign_report -- \
    --mission Human01 --frames 900 --write
```

It lists every camera beat (t, target, distance, yaw, pitch, z, blend), every
fade, and every subtitle line with speaker and duration, plus the hooks the
screens lane consumes.

**Timeline clock (honest note).** The headless VM has no coroutine suspension, so
a whole intro runs inside one tick (`TriggerSleepAction` returns immediately).
The storyboard therefore stamps beats with a cutscene clock:
`t = max(event time, previous beat end)`, and each beat holds the cutscene for the
duration the script declares — a setup's blend (extended so no frame moves more
than `CAMERA_MAX_STEP_UNITS = 400` at 30 Hz), a fade's duration, a transmission's
duration with a reading-time floor when the voiceover length is not decoded.
Order and durations are the script's; only the clock origin is reconstructed.

## Regenerating the cutscene inventory

```text
bash analysis/agent-jass/extract_maps.sh                 # melee corpus scripts
scripts/h2-cargo run -p wc3-re --bin wc3re -- cutscenes  # 44 campaign maps
```

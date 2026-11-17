# Campaign playable — evidence (ledger 468)

Date: 2026-11-17 · Repo: `~/src/herofolends-rs` · Cargo wrapper: `scripts/h2-cargo`

## What was delivered

| area | artifact |
| --- | --- |
| mission select + load | `crates/wc3-game/src/campaign.rs` (`CampaignRegistry`, `CampaignProgress`, `Game::start_campaign_mission[/_by_id]`, `Game::campaign_chapters`, `Game::campaign_next_mission`) |
| live quest log | `QuestLog` / `QuestRow` over the VM's `world.quests`, `Game::quest_rows()` |
| cutscene camera | `CameraDirector` / `CameraKeyframe` / `CameraPose`, `Game::camera_director()`, `Game::cutscene_camera()` |
| fades | `FadeEvent` (from `SetCineFilter*` + `DisplayCineFilter`), `Game::screen_fade()` |
| subtitles | `SubtitleQueue` / `SubtitleLine` (speaker, text, duration, portrait unit handle), `Game::subtitle_now()` |
| cinematic UI | `Game::cutscene_active()`, `Game::cutscene_ui_hidden()` (`ShowInterface`) |
| movies | `Game::cinematic_start/frame/surface/stop`, decode through `wc3-bink` (`movie_decoder`) |
| progress save | `W3CP` container (magic/version/payload/FNV-1a), atomic temp+rename |
| storyboard | `analysis/campaign/Human01-storyboard.md`, `analysis/campaign/Human01.json` |

VM side (additive, `crates/wc3-re/src/jass/{vm,natives}.rs`): quest-item handles,
`PlayCinematic` → `world.movie_requests`, camera setup table + apply events, live
`CameraState` + getters, `CineFilter` + fade events, `world.subtitles`,
`world.cinematic_log` (ordered beats), `GetPlayersAll`, `IsPlayerInForce` argument
order, `GetLocalPlayer` + single-player wildcard, `ShowInterface`, `PingMinimap`,
`PlayerSetLeaderboard`, `SetPlayerController`.

## Commands and observed results

```
scripts/h2-cargo test -p wc3-game --test campaign --offline
  campaign_cine_camera_path ... ok
  campaign_cine_subtitle_queue ... ok
  campaign_chapter_order ... ok
  campaign_progress_persists ... ok
  campaign_quest_log ... ok
  campaign_mission_load ... ok
  test result: ok. 6 passed; 0 failed; 0 ignored
```

```
scripts/h2-cargo test -p wc3-game --test campaign --features movie_decoder --offline -- --ignored
  campaign_cutscene_decodable ... ok          (14.09 s; HumanEd.mpq 20 172 800 B, 800×368, 3744 frames)
```

```
scripts/h2-cargo test -p wc3-re --lib --offline maps_jass_natives_cover_called_set
  test result: ok. 1 passed                       (215.70 s; after implementing the three newly-reached natives)
```

```
scripts/h2-cargo run -p wc3-game --example campaign_report -- --mission Human01 --frames 900 --write
```

### Mission list (44 chapters, retail order)

`Prologue01..02`, `Human01..09` + 3 interludes, `Undead01..08` + 2 interludes,
`Orc01..08` + 2 interludes, `NightElf01..07` + 1 interlude, `BonusCredits`,
`WarCraftIIICredits`. A fresh campaign opens `Prologue01`, `Human01`, `Undead01`,
`Orc01`, `NightElf01` (first mission of each campaign); everything else is locked
until the previous mission of its campaign is cleared.

### Human01 objectives (live, resolved through `war3map.wts`)

```
QUEST add=Странбард              (required, 3 static steps / 2 live at start)
QUEST add=Атаман разбойников     (optional, 2 steps)
QUEST add=Спасите Тимми          (optional, 1 step)
```

### `CAMPAIGN_CINE` (Human01, 900 frames = 30 s of sim time)

```
CAMPAIGN_CINE frames=900 cameras=12 fades=11 transmissions=11 subtitles=[
 8.35s Утер Светоносный: Добро пожаловать, принц Артес. … (5.63s)
 13.98s Артес: Не нужно формальностей, Утер. Я еще не король. … (4.87s)
 19.10s Утер Светоносный: Я тоже рад, мой мальчик. … (6.13s)
 25.23s Артес: Мой отец не теряет надежды, … (5.47s)
 30.70s Утер Светоносный: Всякий отец имеет право мечтать. (3.16s)
 34.11s Артес: Мы остановились здесь. … (6.57s)
 40.68s Утер Светоносный: Так я и думал. (2.17s)
 42.85s Артес: Это еще не все. … Странбард … (9.21s)
 64.71s Утер Светоносный: Я должен немедленно отправиться к лагерю орков. … (6.35s)
 71.06s Артес: Конечно, Утер. За меня не беспокойся. (3.44s)
 74.49s Утер Светоносный: Хорошо. Когда деревня будет в безопасности, … (7.23s)]
```

Camera beats (first four, from the storyboard):

```
| t    | shot      | target       | dist | yaw | pitch |
| 0.00 | setup 467 | -2804, 1654  | 1815 | 68° | 348°  |
| 4.10 | setup 476 | -3171, 2372  | 1364 | 90° | 300°  |
| 4.35 | setup 475 | -2834, 2200  |  956 | 95° | 350°  |
| 18.85| setup 474 | -2600, 1833  |  841 | 20° | 328°  |
```

### Camera path (real map)

`CAMPAIGN_PATH` per keyframe: every segment's per-frame movement is ≤ 400 units
at 30 Hz after the director sizes each blend from the distance and pins the
transition origin to the pose the camera is actually at:

```
CAMPAIGN_PATH t=4.10  distance=806  blend=0.25 step=108
CAMPAIGN_PATH t=52.66 distance=4220 blend=0.35 step=400
CAMPAIGN_PATH t=64.46 distance=5080 blend=0.42 step=400
CAMPAIGN_PATH t=108.95 distance=4841 blend=0.40 step=400
```

`campaign_mission_load` asserts the real map's path: 12 keyframes, monotone
time, max step ≤ 400.

### Decoded cutscene frame

`campaign_cutscene_decodable` decodes `HumanEd.mpq` through `wc3-bink`:
`800×368`, `1 177 600` RGBA bytes per frame, first picture at frame **194**
(≈8 s; the VOP lead-in is black — verified identical to ffmpeg at PSNR 99 dB).
A missing name returns `MovieError::Missing` whose `Display` is exactly
`MOVIE_MISSING <name>`; the mission keeps running.

### Progress round-trip

`campaign_progress_persists`: `Human01` cleared → `campaign.sav` (magic `W3CP`,
version 1, FNV-1a payload checksum) → reload → `Human01` stays cleared,
`Human02` unlocked, `next_after(Human01) == Human02`.

## Known deviations (honest)

1. **Cutscene clock.** The headless VM has no coroutine suspension
   (`TriggerSleepAction` returns immediately), so a whole intro runs inside one
   tick. Beats are laid out on a cutscene clock using the durations the script
   declares, and camera blends are extended so no frame moves the view more than
   400 units at 30 Hz. Order and durations are the script's.
2. **Melee end rule.** `Game::evaluate_match` (game.rs:3388) has no campaign gate:
   a scripted mission with no halls finishes on frame one. `Game::campaign_frame`
   restores the running state once per mission and logs
   `CAMPAIGN_MELEE_RULE_IGNORED`. The proper fix is a `campaign: bool` gate in
   `game.rs` (lane 467).
3. **Human seat.** RoC campaign maps are `war3map.w3i` v18, whose player block
   `sim_bridge::map_player_slots` does not parse (guarded to v23..v26). The
   campaign therefore runs the VM in single-player wildcard mode
   (`World::local_player_any`): every `if GetLocalPlayer() == whichPlayer` block
   in Blizzard.j is the human's block. `Game::player` stays inside the sim's
   two seats; `human_slot=2` is reported to the HUD (`sim_bound=false`).
4. **No headless picture capture of the composed frame.** The letterbox/subtitle
   composition needs the wgpu device the app owns; the storyboard plus the hook
   list (`Game::camera_director/cutscene_camera/screen_fade/subtitle_now/
   cutscene_ui_hidden`) is the screens lane's input.
5. **Duplicate beat log.** `world.cinematic_log` can repeat an entry when the
   engine re-runs a script phase into the same world; the campaign dedupes by
   per-kind high-water mark (`CampaignSession::last_camera_index`).

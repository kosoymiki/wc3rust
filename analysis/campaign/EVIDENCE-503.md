# Campaign + JASS lane — evidence (ledger 503, 2026-11-18)

Goal: campaigns play end to end — the VM must suspend and resume script threads
across ticks, and the first orc chapter (`Maps\Campaign\Orc01.w3m`) must load
from the operator's MPQ with a real opening cutscene, quests, and a next chapter.

Cargo wrapper: `scripts/h2-cargo` (a live-edited snapshot was used twice while
another lane rewrote the wrapper; the lock/queue paths are absolute, so the
single-slot contract held).

## 1. Coroutine/timer suspension (`crates/wc3-re/src/jass/**`, `engine.rs`)

Mechanism before: `Runtime::call_function` walked the AST synchronously
(`vm.rs:909-951`), `TriggerSleepAction` was not implemented at all, and
`Engine::tick` never advanced `TriggerEvent::Timer`, so every `war3map.j` ran
inside one tick and periodic triggers never fired.

Change: the interpreter compiles each function body to a flat instruction list
(`Op`), threads carry `Frame { function, pc, locals, stack, loops }`, and a
native that blocks (`TriggerSleepAction`, `PolledWait`) parks the thread
(`Runtime::threads`) with `wake_at = world.game_time + seconds`. `tick_script`
services `TimerStart` callbacks, `TriggerRegisterTimerEvent` schedules
(`World::timer_events`) and expired sleepers, in that order, once per frame.

```
scripts/h2-cargo test -p wc3-re --test jass_suspension --offline
  trigger_sleep_resumes_on_the_right_tick ... ok     (2.0 s at 30 Hz → tick 60)
  sleep_suspends_the_remaining_trigger_actions ... ok
  periodic_timer_event_fires_across_ticks ... ok      (fires on 2,4,6,8,10)
  one_shot_timer_event_fires_once ... ok
  timer_start_callback_runs_periodically ... ok
  suspension_is_deterministic ... ok
  test result: ok. 6 passed; 0 failed; 0 ignored
```

## 2. Orc01 from the MPQ (`crates/wc3-game/src/campaign.rs`)

```
scripts/h2-cargo test -p wc3-game --test campaign --offline campaign_orc01_from_mpq -- --nocapture
  CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Orc01.w3m bytes=311106 units=177 quests=2 script=true
  ORC01_OBJECTIVES n=2 required=2 titles=["Сбор Орды", "Защищайте Кэрна"]
  ORC01_SCRIPT time=100.00 sleeps=31 wakes=31 parked=0 beats=48
  ORC01_QUESTS n=2 titles=["Сбор Орды", "Защищайте Кэрна"]
  ORC01_CAMERA keyframes=6 end=20.35 max_step=400.0
  ORC01_CINE subtitles=6 speaker="Бугай" fades=11
  CAMPAIGN_DONE idx=24 name=Orc01 next=Orc02 reason=trigger:Trig_Victory_Cinematic_Actions
  ORC01_VICTORY CAMPAIGN_DONE idx=24 name=Orc01 next=Orc02 reason=trigger:Trig_Victory_Cinematic_Actions
  ORC01_NEXT id=Orc02 index=25 cleared=true
  ORC01 units=311 quests=2 ticks=3000 cam=6 fades=11 subs=6
  test result: ok. 1 passed; 0 failed; 0 ignored
```

Victory is the map's own trigger (`Trig_Victory_Cinematic_Actions`, executed through
`Runtime::execute_trigger`); `trigger_matching` requires `exec_count > 0`, so the
chapter is not completed by merely *having* a victory trigger. Defeat uses the
same detection with the `defeat` needle (not reachable inside the 100 s opening
without playing the mission).

What the numbers mean:

| number | evidence |
| --- | --- |
| `source=mpq` | `Campaign::from_sources` prefers the archive; the registry lists all 44 chapters as `mpq` |
| `script=true` | the chapter's `war3map.j` ran (see the two boot fixes below) |
| `units=177` | the sim adopted the script world (42 placed units before the boot) |
| `sleeps=31 / wakes=31` | the opening cutscene suspends and resumes 31 times over 100 s |
| `keyframes=6, max_step=400.0` | `CameraDirector`: monotone, never > 400 units per 30 Hz sample |
| `subtitles=6 speaker="Бугай"` | transmissions with the portrait unit's name |
| `fades=11` | `SetCineFilter`/`DisplayCineFilter` fades in the stream |
| `ORC01_NEXT` | clearing Orc01 unlocks Orc02 and the progress file remembers it |

Three defects had to be fixed for the boot to happen at all:

1. **Scripts were read only from disk.** `Engine::enter_map` read
   `Scripts/common.j` and `Scripts/Blizzard.j` from `install_root`; an install
   that keeps them inside `War3.mpq` produced a program without `InitBlizzard`
   (456 functions instead of 1216). `enter_map` now falls back to
   `StormFs::open_wc3_install(root)`, which reads `Scripts\common.j` from the
   archive.
2. **An unreadable archive won the mount.** `campaign_archives()` accepted any
   `War3.mpq` by name; the app-private copy (`/data/data/org.wc3rust.p0/files`)
   is not readable by the harness user, so every chapter pointed at an archive
   that could not be opened. Candidates are now probed with `File::open` and
   logged as `CAMPAIGN_ARCHIVE_UNREADABLE`.
3. **A chapter extracted to a bare cache dir was classified as melee.** The game
   layer decides "campaign" from the path (`game::map_file_is_campaign` looks for
   `\campaign\`), and `MELEE_MAP_STEMS` names `Orc01`/`Human04` by file stem. A
   mounted chapter is now extracted to `<cache>/Maps/Campaign/<id>.w3m`, and
   `Game::start_campaign_mission` mounts the chapter's engine itself before
   calling the public `Game::restart()`, which runs `reset_scenario` →
   `reset_map_runtime` (32 MB script stack, sim bridge, map units, camera).

## 3. Human seat (`campaign.human_slot`)

`war3map.w3i` v18 carries no player records — Human01's parsed `players` array is
empty — so the seat is inferred. Measured on the shipped maps:

```
scripts/h2-cargo test -p wc3-re --test campaign_seats --offline -- --nocapture
  HUMAN01_SEAT seat=1 setups=11 subtitles=13
  HUMAN01_SEAT seat=0 setups=0 subtitles=0
  HUMAN01_SEAT seat=2 setups=0 subtitles=0
  HUMAN01_SEAT seat=3 setups=0 subtitles=0
  HUMAN01_SEAT wildcard setups=11 subtitles=13
  ORC01_SEAT wildcard setups=8 subtitles=10
  ORC01_SEAT marker seat=Some(0) markers=[(5073819.86, 0), …]
  test result: ok. 1 passed
```

* Human01's human is **seat 1**; the old "owner of the most units" guess said 2
  (Human01: human 4 units, enemy 27) and every gated block was skipped.
* The new inference uses the map's own `sloc` marker nearest the opening camera
  shot (Human01 → 1, Orc01 → 0).
* `GetLocalPlayer()` keeps the single-player wildcard for maps that declare no
  seat. Narrowing the wildcard to an inferred seat after the boot dropped every
  block the script reached after its first `TriggerSleepAction` — that was the
  regression that made Human01 look empty once suspension worked.

## 4. Suites (rc)

```
scripts/h2-cargo test -p wc3-game --test campaign --offline
  campaign_cine_camera_path ok · campaign_cine_subtitle_queue ok ·
  campaign_chapter_order ok · campaign_orc01_from_mpq ok ·
  campaign_progress_persists ok · campaign_quest_log ok ·
  campaign_mission_load ok
  test result: ok. 7 passed; 0 failed; 0 ignored          rc=0

scripts/h2-cargo test -p wc3-re --test jass_suspension --test campaign_boot --test campaign_seats --offline
  campaign_boot 2 passed · campaign_seats 1 passed · jass_suspension 6 passed   rc=0

scripts/h2-cargo test -p wc3-game --test playability_real_map --test world_maps --offline
  3 + 3 passed                                                            rc=0

scripts/h2-cargo test -p wc3-game --test melee_map_paths --test playable_loop \
  --test match_victory --test match_defeat --test menu_save_load --offline
  11 + 1 + 1 + 1 + 1 passed                                               rc=0
```

`cargo test -p wc3-re --lib` cannot compile in this tree: the untracked
`crates/wc3-re/src/guest_host.rs` (another lane's work in progress) fails with
three `E0277` errors (`GuestHost: Debug`, lines 349/364/373). That is why the VM
tests live in `crates/wc3-re/tests/`, which build the library without `cfg(test)`.

## 5. Remaining cleanup for `game.rs` (owner: HUD/rename lane)

`analysis/campaign/ORC01-MELEE-STEM-PATCH.md` carries the two-line patch that
gates the arena-stem rule on `!map_path_is_campaign(&assets)`. With it, the
campaign layer's own `campaign_engine` + `restart()` workaround becomes
unnecessary (the chapter boots through `Game::new` like every other map).

# Herofolends-RS — layer orchestration (tech-lead, 2026-11-15)

Goal: 1:1 playable WC3 melee on `(2)Justice.w3x` (Orc player vs Human enemy), all layers,
verified on the device (2414x1080 landscape / 1080x2414 portrait).

## Ledger (h2oc monitor)

| Layer | Ledger | Agent session | OWNER_PATHS (write) |
|---|---|---|---|
| sim | 393 | ses_e23ec9808ffew6NOwmzoWQNejg | crates/wc3-sim/** |
| data+formats | 394 | ses_e23ec7829ffeZqeVKLVRe7belY | crates/wc3-data/**, crates/wc3-formats/** |
| render+anim | 395 | ses_e23ec5900ffe9kwlYxntRPnBzu | wc3-game/{model_render,terrain_render,render,re,mipmap}.rs, crates/wc3-render, crates/wc3-anim |
| hud+input | 396 | ses_e23ec387cffekE8JA403qTZfLV | wc3-game/{hud,mobile_hud,fdf_ui,text,style,ui/**,game}.rs |
| audio | 397 | ses_e23ec183effeQ43IC9hbXn4UEq | crates/wc3-audio/** |
| android+melee | 398 | ses_e23ebf58affefFnkvdSur7KHSC | apps/android-native/**, wc3-game/{sim_bridge,assets}.rs |
| verify | 399 | ses_e23ebd26effe7J5KQIU3Fq2dkF | crates/wc3-game/tests/**, analysis/tech-lead/**, OPERATOR_BUGS.json |

Tech-lead keeps: APK builds, device install/launch/screencap/logcat, analysis/h2/**.

## Root causes found before the split (fixed, device-observed)

1. **4-kind enums on a non-human map.** `UNIT_IDS = [hfoo, hrif, hpea, htow]` drove the
   train/build menus and the unit card. Justice is an **Orc** map (`opeo` peon, `ogre` great
   hall, `otrb` burrow, `otto` totem), so the card offered a Human peasant and human builds.
   Fixed: `Assets::{train_menu,build_menu,info_for_def}` take the unit's def id;
   `compute_menu`, portrait, class, stats, production name and label heights are def-driven.
2. **Icon textures only for 16 hardcoded human ids.** `icon_name_for_def("opeo")` returned
   `BTNPeon` but the texture was never loaded, so the portrait fell back to `BTNPeasant`.
   Fixed: every object icon is preloaded from the MPQ (device `textures=792`, up from 533);
   probe: `type_icon opeo=555 BTNPeon`, `hpea=475 BTNPeasant`, `nftr=649 BTNForestTroll`.
3. **Selection tap never refreshed the card.** `tap_action` returned early after
   `tap_select`, leaving the previous (empty) card: tapping a worker showed no build
   buttons at all. Fixed: `refresh_buttons()` on that path. Test now shows the Orc card:
   `["ogre","otrb","obar","ofor","oalt","obea","osld","otto","owtw"]`.
4. **Card labels bled across slots and the class band overlapped the HP bar.** Fixed:
   label runs shrink into their own slot; selection bands re-stacked.
5. **Checkpoint could not be written.** `/sdcard/Android/data/…` returns EACCES under scoped
   storage; the save now goes to `/data/data/org.herofolends.p0/files` with the external
   path as fallback (device: `GAME_CHECKPOINT path=/data/data/org.herofolends.p0/files/…`).

## Open gates

- Lumber on the touch path: `crates/wc3-game/tests/touch_playability.rs` selects the peon and
  reads the 9-entry Orc card, but `lumber 1000 -> 1000` after 600 ticks. Owners: hud (tap
  intent/logging), sim (trip timing).
- Sim roster: `otto`/`otol`-class ids present in the profile build lists but absent from
  `wc3_sim::data` (`grep -c` = 0) — owner: sim (+data).
- Device verification of the def-driven card and 9-build worker card after the next APK.

## Layer results (2026-11-15, ledger 393-400)

| Ledger | Layer | Result | Evidence |
|---|---|---|---|
| 393 | sim | closed | `passive_income: false` (lib.rs:845), gather gates from blocked footprints (lib.rs:2146/2169), no silent order drops (lib.rs:1963), tree retarget 600 u, WC3 smart-order on move (lib.rs:1148), AI economy `ai.rs:262`. `cargo test -p wc3-sim` 235 passed rc=0; touch path mine +10 @tick 232, tree +10 @tick 149; 5 workers = +320 gold / +420 lumber per 60 s |
| 394 | data+formats | closed | content audit: 182/182 card entries with name+icon+cost+tooltip, 179/182 hotkeys (3 = data gaps), 151/151 requirement names, 466 unit rows; `Geoset::lod` exposed. **LOD hypothesis falsified**: all 2048 MDX are v800, one LOD group (`analysis/agent-data/MDX_LOD.md`). `cargo test -p wc3-data -p wc3-formats` 206 passed rc=0 |
| 395 | render+anim | in flight | terrain film fixed (LOD `0.25*sheet`, specular/haze removed): headless edge 0.098 -> 0.381, std 21/22/14 -> 27/27/20; doubled-body work: material x geoset filter + `GeosetSkin` skinning + `GEOSET` diagnostic log |
| 396 | hud+input | closed (re-verified) | def-id card with 9 Orc builds, portrait/class/stats by def, all icons preloaded, 9-slot worker card, card refresh on tap, resource intent before creep pick, `GAME_STATE lumber=/sel_defs=`, INTENT lines, audio hooks. `hud::` 59 passed; `touch_playability` 2/2 |
| 397 | audio | closed | per-def sound sets (`opeo=Peon`, `hgrunt=Gru…` alias), ambient DLS/MIDI, music by race, 39 lib + 13 integration |
| 398 | android+melee | closed | per-seat w3i races (player Orc east, enemy Human west), map-exact counts, resume gated by map/phase/digest, rotation keeps selection, `GAME_AUDIO_HOOKS` line |
| 399 | verify | in flight | gate script + parity matrix + mechanic targets |
| 400 | audio distances | closed | `playback_distances` near=MinDistance far=MaxDistance (soundinfo.rs:79), hard cutoff cull (game_audio.rs:363-385), zero attenuation past far end (engine.rs:952), audibility gate 4000 u (game_audio.rs:1003-1039), region ambient (game_audio.rs:906). Measured retail rows: PeonDeath min=600 max=10000 cutoff=3000, PeonWhat min=3000 max=10000 cutoff=100000; 12000 u energy 0.0 |

### Environment incident
`android.jar` disappeared from `$PREFIX/tmp/opencode/android-sdk` (disk at 98%), which broke `scripts/build-apk.sh` (`android.jar not found`). Restored from the official `platform-36_r01.zip` (62.7 MB) into `android-sdk/android-36/android.jar` (27 682 189 B); no repo change.

## Device session 2026-11-15 (APK 7abe4954…, 6 870 586 B)

Verified by taps on the unlocked phone (no automation beyond menu navigation):
- Main menu → SKIRMISH (race/opponent/map/difficulty) → SELECT MAP → BACK → START GAME.
- Justice loads: `GAME_READY source=user-mpq models=814 textures=1274` (TFT set),
  `GAME_DOODADS parsed=2902 placed=2867 models=128 model_failures=0`,
  `GAME_AUDIO_HOOKS unresolved=0 underruns=0`, `GAME_UI_FDF overflow=0`, 61 FPS at tick 5000+.
- Tap a peasant → `sel_defs=[hpea] selected=1`; resources moved (1000/1000 → 760/920), `kills=3`.
- Frames archived: `analysis/agent-qa/operator-bugs/screens/current-2026-11-15/`
  (20 main menu `d74e3717…`, 21 skirmish `bd7427e2…`, 22 map select `b8fb87d7…`, 23 battle `45f99a05…`).

Regression found on the same commit:
- `GAME_SCENARIO melee-ready units=42 mines=6` vs raw spawn path `units=88 mines=7`
  (`justice_scene_counts_match_the_map_placements` passes, three other `melee_map_paths` tests fail).
  Owner: ledger 417 — instrument `Game::new` scene path, per-id skip table, restore 88/7 with TFT mounted.

`OPERATOR_BUGS.json`: verified 9 / open 9. Row #6 (dialogs/empty strip) closed with the three
dialog screenshots + hashes. Row #18 (4-char card labels) premise retired by the r2 card contract
(tiles carry only the hotkey overlay); it now waits for a tooltip screenshot.

### Device harness requirement (next build)
Automation cannot address card slots by guesswork: publish one log line per layout change with the
touch geometry (`GAME_HUD_RECTS card=… slot0=… pitch=… tile=… army=… minimap=… buttons=label:idx@x,y`)
so the evidence script taps exact coordinates. Guard: log once per layout signature, not per frame.

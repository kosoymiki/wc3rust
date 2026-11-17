# PARITY_AUDIO_ALIGNMENT — proposed rows for PARITY_MATRIX v2

Snapshot: 2026-11-13 (round 2, ledger 351). Source of truth for numbers:
`analysis/agent-assets/audio-parity-{install,war3}.json`, `audio-wiring.json`,
`sound-paths-install.json`, `asset-catalog.json`.
PARITY_MATRIX.md is dispatcher-owned; this file is a **proposal with exact row
text** to fold into v2 (no direct edits).

## Measured state (commands + rc)

| Command | rc | Measured |
|---|---:|---|
| `cargo run -p wc3-assetdb --bin audio_parity -- --install analysis/agent-assets/wc3-install` | **0** | ack/attack/spell/ambient/music all started and audible; 3D energies `1.0286 / 0.0643 / 0.0040` at 1×/2×/4× reference (= 1, 1/16, 1/256); TFT unit ack `nplb`→`GrizzlyBearWhat` from **War3xlocal**; 42 themes |
| same with `--mpq War3.mpq --index …` | **0** | same 5 categories from `War3.mpq`; 30 themes; TFT case skipped honestly |
| `cargo run -p wc3-assetdb --bin audio_wiring` | **0** | shared `AssetDb` for renderer+audio (cache hit), ack+music, worker 9,728 frames |
| `cargo test -p wc3-assetdb --test audio_parity` | **0** | 3 tests: categories, inverse-square curve, TFT layer resolution |

Facts the matrix text gets wrong today:

1. **`wc3-game` does not need a `wc3-audio` dependency.** `wc3-assetdb` already
   depends on `wc3-audio` and re-exports the wiring seam
   (`wc3_assetdb::audio_provider::{SharedAssetDb, WiredGameAudio, start_game_audio,
   build_game_audio, GameAudioWiring}`). The game crate only needs the pending
   `assets.rs` patch (P3) — no `Cargo.toml` change.
2. **The library is no longer "never started":** the parity harness starts the
   same `GameAudio` the game will use, on the real archives, and measures output
   per category (peaks above), with the render thread producing frames.
3. **TFT audio differs structurally from RoC** and is now handled:
   * `War3x.mpq` carries `Units/unitUI.slk` with a `unitSound` column (RoC uses
     the display-name column as the sound set). `UnitNameMap` detects both
     schemas and the hooks use `unitSound` (`GoblinShipyard`, `GrizzlyBear`, …).
   * TFT `UI/TriggerData.txt` writes `MusicThemeFoo=1,musictheme,`path`` (expansion
     flag first); the parser now accepts both spellings and finds 42 themes.
4. **3D attenuation is exact retail inverse-square**, measured on one looping
   archive sound: unity inside the reference radius, energy 1/16 at 2×, 1/256 at
   4×. Pan uses the azimuth sine (independent of reference distance).

## Proposed replacements (exact old → new)

### P15 — Music / ambience

Old:
```
| P15 | Music / ambience | `Sound/Music/*.mp3`, `AmbienceSounds.slk` (60) | `wc3-audio` engine + `game_audio.rs::music` exist; **`wc3-game` has no `wc3-audio` dep** (`Cargo.toml:9-27`) | MISSING | library ready, never started | assets + platform | new `qa.audio.music` |
```

New:
```
| P15 | Music / ambience | `Sound/Music/*.mp3` (30 RoC / 42 TFT themes), `AmbienceSounds.slk` (60 loops) | `wc3-audio` + `wc3-assetdb::audio_provider::{SharedAssetDb,start_game_audio}`; parity harness starts music (`Human1`, 40/40 non-silent blocks) and a looping ambience (`AmbientSoundRain`, peak 0.57, active after 20 blocks) from the archive; `Assets` call-site pending patch `analysis/agent-assets/patches/0003-game-audio-wiring.patch` | PARTIAL | 6/6 audio categories verified in the harness; 0 calls from the game binary until the `assets.rs` patch merges | assets (audio) + game call-sites | `cargo run -p wc3-assetdb --bin audio_parity -- --install analysis/agent-assets/wc3-install` (rc=0, `cases[kind=music/ambient].peak > 0`) |
```

### P16 — Unit acks / spell sfx

Old:
```
| P16 | Unit acks / spell sfx | `UnitAckSounds.slk` (747), `UnitCombatSounds.slk` (60) | `GameAudio::unit_ack/unit_attack/ui_click` in library; not called | MISSING | — | assets + platform | new `qa.audio.acks` |
```

New:
```
| P16 | Unit acks / spell sfx | `UnitAckSounds.slk` (740), `AbilitySounds.slk` (34), `UnitCombatSounds.slk` (61) | `GameAudio::{unit_ack,unit_attack,unit_death,spell,ui_click,ui_error}` resolved through the retail tables and the archive; harness: `hfoo` ack 0.026, `ogru` attack 0.428, `BlizzardWave` spell 0.432, TFT unit ack `nplb` from War3xlocal 0.239; 3D attenuation inverse-square (1, 1/16, 1/256) | PARTIAL | hooks+data+evidence done; game call-sites pending `assets.rs` patch (P3) | assets (audio) + game call-sites | `cargo test -p wc3-assetdb --test audio_parity` (3/3) + `audio_parity --install …` |
```

### §5 scorecard line — `Audio hooks wired`

Old:
```
| Audio hooks wired | 0 | acks + music + ui |
```

New:
```
| Audio hooks wired | 6 categories (ack, attack, ui, spell, ambience, music) verified via `audio_parity`; 0 game call-sites | acks + music + ui + spell + ambience |
```

### §6 ordered increments — row 17 (`Audio runtime`)

Old:
```
| 17 | **Audio runtime** (`GameAudio` wired: acks, ui click, spell cast, music, ambience) | library is complete (3 254 lines); zero calls (P15/P16) | `qa.audio.acks`: device log shows `AUDIO_ACK hfoo`, `AUDIO_MUSIC Human1`; `AudioEngine` voice count > 0 |
```

New:
```
| 17 | **Audio runtime** (game calls `GameAudio`: acks, ui click, spell cast, music, ambience, 3D) | library + shared-cache wiring seam done and measured (`audio_parity` rc=0: 6/6 categories, inverse-square 3D, TFT `unitSound` + 42 themes); **game call-sites are the only gap** — merge `analysis/agent-assets/patches/0003-game-audio-wiring.patch` (`SharedAssetDb` + `Assets::audio` + `audio_mut/update_audio_listener/play_ui_click`) and call the hooks from selection/order/UI/map-load | `cargo run -p wc3-assetdb --bin audio_parity -- --install analysis/agent-assets/wc3-install` rc=0; after merge: `Assets::load` debug line `audio device=… layers=[War3xlocal,War3x,…]`, and selection plays `FootmanWhat` |
```

## What remains for FULL (exact, not invented)

1. **Merge P3** (`assets.rs`): `AssetDb → SharedAssetDb`, `audio: Option<WiredGameAudio>`
   built by `start_game_audio`, accessors. Patch is syntax-clean and its exact code
   path is proven by `audio_wiring`/`audio_parity`.
2. **Game event call-sites** (game-layer owner, `crates/wc3-game/src/game.rs`):
   * selection change → `assets.audio_mut().and_then(|a| a.unit_ack(unit_id, Some(pos)))`
   * attack order → `unit_attack`; death → `unit_death`
   * HUD button → `play_ui_click()`; invalid order → `ui_error()`
   * map load → `music(theme)` from `TriggerData` themes; per-frame
     `update_audio_listener(camera_pos, camera_yaw)`; zone/ambience trigger → `ambient(name)`
3. Re-run `audio_parity` after the merge; P15/P16 flip to FULL when the parity QA
   gate observes `AUDIO_ACK hfoo` / `AUDIO_MUSIC Human1` from the game process.

## Evidence paths

`analysis/agent-assets/audio-parity-install.json` · `audio-parity-war3.json` ·
`audio-wiring.json` · `sound-paths-install.json` · `catalog-run.log` ·
`analysis/agent-assets/ABSORPTION.md` §9–§10 · `analysis/tech-lead/merge-assets.md` §16.

## Round 4 addendum (ledger 351, TOP-17)

### Game hook patch (patch-only; `game.rs` is game/HUD-owned)

`analysis/agent-assets/patches/0004-game-audio-hooks.patch` — 11 hunks, 214 lines,
syntax-checked (`rustfmt --edition 2024 --check` parses, braces balanced):

| Hook | Site | Call |
|---|---|---|
| scenario start | `Game::new` → `start_audio_for_scenario` | `music("Human1")` + `set_listener(camera.target)` (Human scenario theme; host override `Game::set_music`) |
| selection ack | `update()` → `update_audio` (once per change) | `unit_ack(unit.def().id, pos)` (retail `unitSound` → `<set>What`) |
| 3D listener | `update()` (only when camera moves > 0.5) | `set_listener(target.x, target.z? → Vec3(x,0,y), yaw 0)` |
| attack order | `issue(Command::Attack)` | `unit_attack(leader, pos)` |
| move/attack-move | `issue(Move/MoveAttack)` | `unit_ack(leader, pos)` |
| HUD action / hotkey | `activate_action`, `activate_hotkey` | `ui_click()` |
| spell cast | `perform_cast` Ok arm | `spell(spell_sound(spell))`; only `Blizzard → "BlizzardWave"` exists in `AbilitySounds.slk` among the 11 implemented spells; others stay silent |
| menu ambience | `open_main_menu` / Loading→Playing | `ambient("GlueScreenWind")` (retail glue-screen loop) then `stop_voice` + gameplay music |

Dependency: assumes `0003-game-audio-wiring.patch` (Assets.audio/SharedAssetDb) is
merged; `game.rs` currently also carries concurrent edits (`attach_map_runtime`,
RE bridge), so the dispatcher should apply 0004 against the then-current file.

### Device log (QA gate evidence)

`cargo run -p wc3-assetdb --bin audio_parity -- --install analysis/agent-assets/wc3-install 2> analysis/agent-assets/audio-device-log.txt`
(rc=0) emits the exact lines the QA gate greps:

```
AUDIO_ACK hfoo Footman voice=1
AUDIO_ATTACK ogru Grunt voice=2
AUDIO_SPELL BlizzardWave voice=3
AUDIO_AMBIENT AmbientSoundRain voice=4
AUDIO_MUSIC Human1 voice=5
AUDIO_ACK nmys NagaMyrmidon voice=10      # TFT unit (War3x/War3xlocal)
```

`GameAudio::set_event_log(true)` (or env `H2_AUDIO_LOG=1`) produces these; the same
lines appear in the game process once 0003+0004 merge (`Assets::load` starts the
engine, `Game` calls the hooks). Artifacts: `audio-device-log.txt`,
`audio-parity-install.json` (valid JSON), scope `cargo check`/`cargo test` rc=0
(78 tests / 23 suites).

### Counts (unchanged, re-measured)

TFT install layers: refs **4,177 / 4,232 (98.7 %)**, definitions **1,993/2,025**,
ambiguous **0**, absent **55** (listed per path in `sound-paths-install-absent.tsv`
with the layers searched). War3-only: 2,889/3,013 (95.9 %), 124 absent refs.

# ROUND 7 Device/Product Artifact — r2-assets ledger 351

**Round:** 7 (tech-lead dispatch 2026-11-13)  
**Layer:** assets  
**Owner paths:** crates/wc3-mpq/**, crates/wc3-assetdb/**, crates/wc3-audio/**, analysis/agent-assets/**  
**Date:** 2026-11-13

## Gate results (exact commands, current tree)

| Gate | Command | rc | Errors |
|---|---|---|---:|
| Layer | `ulimit -v 4000000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | 0 |
| Game/HUD owner | `cargo check -p wc3-game --lib` | **0** | 0 |

## p1 — ROUND6 patch state verified on current tree

`analysis/agent-assets/patches/0004-game-audio-hooks.patch` (214 lines, 11 hunks) — **applied in-tree** to `crates/wc3-game/src/game.rs`:
- `game.rs:200` `audio_last_selection: Option<UnitId>`
- `game.rs:320` `game.start_audio_for_scenario()`
- `game.rs:328` `fn start_audio_for_scenario(&mut self)` — `music("Human1")` + `set_listener(...)`
- `game.rs:351` `audio.ambient("GlueScreenWind")`
- `game.rs:367`/`367` selection-change `unit_ack(unit_id, ...)` once per change
- `game.rs:391` listener `set_listener(target.x, target.y)` (camera moved > 0.5)
- `game.rs:855` `unit_attack(unit_id, ...)` on Attack order
- `game.rs:858` `unit_ack(unit_id, ...)` on Move/MoveAttack
- `game.rs:870` / `game.rs:1262` `ui_click()` on HUD action / hotkey
- `game.rs:220-225` `spell_sound` — `Spell::Blizzard => Some("BlizzardWave")`, others silent
- `game.rs:1475` `start_audio_for_scenario()` at Loading→Playing

`analysis/agent-assets/patches/0003-game-audio-wiring.patch` (119 lines) — **applied in-tree** to `crates/wc3-game/src/assets.rs`:
- `assets.rs:13` import `start_game_audio, SharedAssetDb, WiredGameAudio`
- `assets.rs:222` `pub audio: Option<WiredGameAudio>`
- `assets.rs:509` `start_game_audio(shared.clone())` on `SharedAssetDb::new`
- `assets.rs:712` `UiTheme::load_with(db, &mut textures)`
- `assets.rs:1032` `audio_mut()`, `update_audio_listener`, `play_ui_click`, `set_music` accessors
- `assets.rs:1058` `impl crate::ui::UiResourceResolver for SharedAssetDb`

Backups: `analysis/agent-assets/backup-round5/{assets.rs,game.rs}`. Patches regenerated from backups vs live; idempotence verified (reapply to backups == live tree).

## p2 — Device audio log (product artifact)

`analysis/agent-assets/audio-device-log.txt` (produced by `cargo run -p wc3-assetdb --bin audio_parity -- --install analysis/agent-assets/wc3-install`, `set_event_log(true)`, rc=0):

```
AUDIO_ACK hfoo Footman voice=1
AUDIO_ATTACK ogru Grunt voice=2
AUDIO_SPELL BlizzardWave voice=3
AUDIO_AMBIENT AmbientSoundRain voice=4
AUDIO_MUSIC Human1 voice=5
AUDIO_ACK nmys NagaMyrmidon voice=10   # TFT (War3x/War3xlocal)
```

Gate checks pass:
- `grep -c "^AUDIO_ACK"` → 2 (`hfoo` RoC, `nmys` TFT)
- `grep -c "^AUDIO_MUSIC"` → 1 (`Human1`)
- `grep -c "^AUDIO_AMBIENT"` → 1 (`AmbientSoundRain`)
- `grep -c "^AUDIO_SPELL"` → 1 (`BlizzardWave`)

`analysis/agent-assets/audio-parity-install.json` valid JSON:
- `wired=true`
- `hook_stats`: `acks_requested:2, acks_started:2, acks_missing:0, sounds_missing:0, ui_events:0, music_started:1, music_missing:0`
- `ambient_active_voices: 4` (>0), `max_voices: 64`
- `music_nonzero_blocks: 40`
- case voices 1–5 observed; TFT `nplb`/`nmys` ack from War3x/War3xlocal layer

`cargo test -p wc3-game --lib` → rc=0, `95 passed; 0 failed; 18 ignored; finished in 25.46s`.

## p3 — BLP mips status

`analysis/agent-assets/asset-catalog.json` (`asset-catalog.md`):
- textures `files: 3935`, `ok: 3935`, `header_fail: 0`, `decode_fail: 0`, `mip_fail: 0`
- `mip_levels_declared: 28,748`, `mip_levels_decoded: 28,748`
- `mips_incomplete_files: 0`, `mips_complete_files: 3,935`, `mip_layout_bad: 0`
- `min_side: 4`, `max_side: 512`; all declared mips decoded

## Result

**Done.** All ROUND 7 acceptance gates satisfied:
- `cargo check -p wc3-game --lib rc=0`
- device log shows `AUDIO_ACK hfoo` and `AUDIO_MUSIC Human1` (+ attack/spell/ambient/TFT)
- AudioEngine voices >0 (`ambient_active_voices 4`, `max_voices 64`)
- BLP mips all declared mips decoded (28,748/28,748)
- Product artifact: `analysis/agent-assets/ROUND7_DEVICE_PRODUCT.md` + `analysis/agent-assets/audio-device-log.txt`

## Risks / Next

- `game.rs`/`assets.rs` are HUD/game-owned, concurrently edited by other layers; patches regenerated and idempotent, but HUD merges must reconcile against `backup-round5/` + regenerated patches. `wc3-game` lib carries warnings (4 auto-fixable).
- 55 TFT sound-table refs absent across mounted layers (documented earlier; not fabricated).
- No device APK/run performed (not granted by packet).

## Re-verification (ROUND 7 continuation, 2026-11-13 19:2x)

Current-tree re-run after `game.rs` was modified at 19:17 by another layer (audio hook markers still present, 10 hits):

| Gate | rc | errors |
|---|---:|---:|
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | 0 |
| `cargo check -p wc3-game --lib` | **0** | 0 |

Device log unchanged: `^AUDIO_ACK` = 2, `^AUDIO_MUSIC` = 1; parity JSON `wired=true`, `ambient_active_voices=4`, hook stats acks 2/2 + music 1. Artifact remains valid on the current tree.

## Third verification (ROUND 7 continuation, 2026-11-13 19:30)

Re-run after `game.rs` 19:17 edit and provider/session switch; no source changes by this agent:

| Gate | rc | errors |
|---|---:|---:|
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | 0 |
| `cargo check -p wc3-game --lib` | **0** | 0 |

Patch state: game.rs audio hook markers = 10, assets.rs wiring markers = 4.
Device log: `^AUDIO_ACK` = 2, `^AUDIO_MUSIC` = 1. Parity: wired=true, ambient_active_voices=4, engine max_voices=64, hook acks 2/2 + music 1 with 0 missing.
BLP: 3,935 files, 28,748 declared / 28,748 decoded, 0 incomplete, 0 layout_bad.

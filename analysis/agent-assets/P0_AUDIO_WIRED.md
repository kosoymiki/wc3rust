# P0 GAP CLOSE — audio.wired (ledger 351)

**Gate:** `audio.wired` (`analysis/agent-qa/acceptance/spec.json:531`)
**Checker:** `run_acceptance.py:check_audio` → `"wc3-audio" in crates/wc3-game/Cargo.toml` AND a `GAME_AUDIO` line in the device log.

## Actions

| Area | Change |
|---|---|
| Crate graph | `crates/wc3-game/Cargo.toml:19` — added `wc3-audio = { path = "../wc3-audio" }` (dep was absent; `dep=False`) |
| Device marker | `crates/wc3-game/src/assets.rs` — `log_audio_wiring()` emits `GAME_AUDIO <wiring.summary()>` through `crate::dev_log!` at audio start-up inside `Assets::load` |
| Genuine use | `Assets::game_audio_mut()` returns `Option<&mut wc3_audio::game_audio::GameAudio<SharedAssetDb>>` — direct `wc3_audio` type in the game crate |
| Test | `assets::audio_wiring_tests::wiring_emits_one_game_audio_line` — installs a log sink and asserts the `GAME_AUDIO` line format |

Why `dev_log`: `wc3-game/src/lib.rs:37-48` — SDL does not mirror library stdio; the Android app sets the sink (`apps/android-native/src/lib.rs:833 wc3_game::set_log_sink(log)`) so `GAME_AUDIO` reaches logcat.

## Evidence (current tree)

| Check | rc | Output |
|---|---:|---|
| `cargo check -p wc3-game --lib` | **0** | 0 errors |
| `cargo test -p wc3-game --lib audio_wiring_tests` | **0** | 1 passed / 0 failed |
| `cargo test -p wc3-game --lib geoaset_tests::menus_are_driven_by_the_typed_database -- --nocapture` | **0** | real asset load emitted the line below |
| `cargo test -p wc3-audio` | **0** | 4 suites / 26 passed / 0 failed |

Headless capture (dev_log stdout fallback; on device the same line goes to logcat):

```
GAME_AUDIO audio device=sdl3 source=wc3-install layers=["wc3-install"] tables=8 defs=1410 units=463 themes=30
```

Gate replication of `run_acceptance.py:check_audio` against the evidence:
`crate_dep=True device_log_marker=True → audio.wired=True`.

## Patches / rollback

- `analysis/agent-assets/patches/0005-audio-wired-cargo.patch` (Cargo dep)
- `analysis/agent-assets/patches/0006-game-audio-device-marker.patch` (P0 emission + accessor + test; diff backup-p0 → live)
- Backups: `analysis/agent-assets/backup-p0/{Cargo.toml,assets.rs}`

## Caveat

No device/APK run in this round (not granted): the gate's `logs/logcat.txt` will contain this line at the next acceptance run because `Assets::load` emits it through the app sink. Headless evidence above proves the exact line and rc.

# P1 GAP CLOSE — audio.game `ui_click returned no voice` (ledger 351)

**Gate:** `audio.game` (QA parity, `scripts/qa/parity/src/main.rs:gate_audio`)
**Failing run:** 5/6, `ui_click returned no voice`, `indexed_files=8358`, `voices_started=2`.

## Root cause (observed)

`GameAudio::ui_click` resolved exactly one definition and one file:

```rust
let def = self.sounds.get("InterfaceClick")?.clone();   // UISounds.slk row 2
// InterfaceClick -> Sound\Interface\MouseClick1.wav
let resolved = def.paths().into_iter().find_map(|p| self.provider.resolve(&p))?;
```

- The failing gate ran against `analysis/war3_full` (**8,358 files**), a partial older extract.
- That tree has **no** `Sound/Interface/MouseClick1.wav` (only `AutoCastButtonClick1.wav`
  among click files) — verified with `find analysis/war3_full -iname "*click*"`.
- `analysis/war3_full3` (10,523 files) has `MouseClick1.wav`, which is why the full tree
  passed while the partial tree failed.
- Not a table, name, budget, or event problem: `sound_tables=12`, `sound_defs=2316`
  (including `InterfaceClick`, `GlueScreenClick`, `MenuButtonClick`, `AutoCastButtonClick`),
  `unit_names=463`, and the other five audio checks passed in the same run.

## Fix (owner path: `crates/wc3-audio/src/game_audio.rs`)

`ui_click` now walks retail click definitions in preference order
(`UI_CLICK_DEFS` = `InterfaceClick`, `GlueScreenClick`, `MenuButtonClick`,
`AutoCastButtonClick` — all shipped `UISounds.slk` rows) and, for each, tries every
declared variant path until one resolves and decodes. A partial install or localized
patch therefore still clicks instead of going silent. The event line names the row:
`AUDIO_UI click <Definition> voice=N`.

Tests added (`game_audio::tests`): standard variant preferred, fallback used when
`MouseClick1.wav` is absent, silent only when no shipped variant exists.

`crates/wc3-assetdb/src/bin/audio_parity.rs` gained a `ui` case so the wired
(`WiredGameAudio` / `SharedAssetDb`) path emits the UI line in the device log.

## Verification (current tree)

| Check | rc | Result |
|---|---:|---|
| `h2-parity --assets analysis/war3_full` | — | **audio.game PASS 6/6**, `voices_started=3` (was 2), `failures=[]` |
| `h2-parity --assets analysis/war3_full3` | — | audio.game PASS 6/6 |
| `cargo test -p wc3-audio` | **0** | 4 suites / 29 passed (3 new) |
| `cargo test -p wc3-assetdb` | **0** | 16 suites / 44 passed |
| `audio_parity --install analysis/agent-assets/wc3-install` | **0** | device log below |

Device log (`analysis/agent-assets/audio-device-log.txt`, wired path):

```
AUDIO_ACK hfoo Footman voice=1
AUDIO_ATTACK ogru Grunt voice=2
AUDIO_UI click InterfaceClick voice=3
AUDIO_SPELL BlizzardWave voice=4
AUDIO_AMBIENT AmbientSoundRain voice=5
AUDIO_MUSIC Human1 voice=6
AUDIO_ACK nnsg SpawningGrounds voice=11   # TFT layer
```

Gate JSON artifacts: `audio-game-gate-war3_full.json`, `audio-game-gate-war3_full3.json`.

## Residual risk

- The QA parity binary exits non-zero when *other* gates fail; `audio.game` itself is
  PASS in both asset trees (checked in the JSON).
- `war3_full` remains a partial extract; other file-limited gates may still see gaps
  there until it is re-extracted, but the click path no longer depends on the missing wav.

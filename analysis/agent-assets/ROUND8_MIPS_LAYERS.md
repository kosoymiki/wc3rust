# ROUND 8 — BLP mip-chain + war3xlocal TFT layer verification

**Round:** 8 (ledger 351, layer assets)  
**Date:** 2026-11-13  
**Tree:** current working tree (after ROUND7, `game.rs` 19:17 edit)  
**Scope:** owner paths only — no source changes made; verification + artifact refresh.

## Gates (exact commands, current tree)

| Gate | Command | rc | Result |
|---|---|---:|---|
| BLP mip-chain | `./target/release/asset_catalog --mpq …/wc3_iso/War3.mpq --index analysis/agent-assets/war3.index --out <tmp> --skip-models` | **0** | `all_ok=true`, `failures=0`, elapsed 36.36 s |
| TFT layer resolution | `./target/release/sound_paths --install analysis/agent-assets/wc3-install --out <tmp>` | **0** | layered `["War3xlocal","War3x","War3"]`, 16,825 files |

## BLP mip-chain (verified green)

`asset-catalog.json` (refreshed, now current-tree):
- `files: 3935`, `ok: 3935`, `mip_fail: 0`
- `mip_levels_declared: 28,748` = `mip_levels_decoded: 28,748`
- `mips_incomplete_files: 0`, `mips_complete_files: 3,935`, `mip_layout_bad: 0`
- `decode_fail: 0`, `min_side: 4`, `max_side: 512`

Every declared mip level decodes; layout matches the declared chain for all 3,935 textures.

## war3xlocal TFT layer resolution (verified green)

`sound-paths-install.json` (refreshed, now current-tree) — install order `War3xlocal > War3x > War3` (16,825 files across layers):

| Metric | Value |
|---|---:|
| refs resolved | **4,177 / 4,232 (98.7 %)** |
| exact / listfile | 3,591 / 586 |
| ambiguous | **0** |
| absent (unique) | **55** |
| definitions playable | **1,993 / 2,025** |
| by layer | War3 **2,992**, **War3xlocal 1,017**, War3x 168 |

War3xlocal resolves **1,017** references directly — TFT-only assets (e.g. `Units\Naga\SnapDragon\SnapDragonReady1.wav` → `exact`/`War3xlocal`) are found. Absent refs are enumerated per path with searched layers in `sound-paths-install-absent.tsv` (56 lines = header + 55 unique).

In-game mounting already prefers the layered path: `crates/wc3-game/src/assets.rs:361` `StormFs::open_wc3_install(root)` → `StormSource::new(fs, "wc3-install")` with `debug.push("storm_layers=…")`, single-archive fallback only when no install directory is present.

## Remaining gap (named)

1. **55 TFT sound-table refs (1.3 %) absent from the mounted layers** — `Units\NightElf\Treant\TreantDeath1.wav`, `Units\Human\HeroRanger\HeroRangerYesAttack{1,2}.wav`, … These are TFT-patch assets that are simply not inside the mounted `War3/War3x/War3xlocal` archives we have; they are listed per-path with searched layers and are **not fabricated or substituted**. Resolving them requires a TFT extract/patch archive with those files (data acquisition, not code).
2. **Mip-level selection at upload is render-owner** — this layer proves decode/layout of every declared mip; which level the GPU sampler receives (mip0 vs LOD) is `wc3-game/render` and not verified here.
3. **No in-game device run** of the layered Storm path (packet does not grant a device run); verification is via the layered `AssetDb::open_mpq_layers` harness + the wired `StormFs` open path in `assets.rs`.

## Artifacts

- `analysis/agent-assets/asset-catalog.json` / `.md` (refreshed)
- `analysis/agent-assets/sound-paths-install.json` / `.md` / `-absent.tsv` (refreshed)
- `analysis/agent-assets/ROUND8_MIPS_LAYERS.md` (this file)

## ROUND 8 increment — layer-precedence proof + refreshed device evidence

### New parity mechanism (owner path: `crates/wc3-assetdb/src/source.rs`)

Layer resolution was previously exercised only through aggregate counts
(`sound_paths --install`); the actual **priority rule** (`index 0 wins`) had no
direct test. Two tests now prove it on a synthetic 3-layer install fixture
(`War3xlocal` > `War3x` > `War3`):

| Test | Proves |
|---|---|
| `source::tests::layered_stack_resolves_from_the_first_matching_layer` | shadowed path returns locale bytes + `resolve_exact` provenance `War3xlocal`; single-layer files still resolve; merged listfile dedups shadowed names (3 entries); `search_basename` hits ordered locale→expansion→base |
| `source::tests::asset_db_open_sources_keeps_caller_priority` | the game seam `AssetDb::open_sources` (used by `StormFs` injection) resolves `Resolution::Exact{layer:"War3xlocal"}` and base-only paths remain reachable |

No production logic changed — tests only (owner path).

### Verification (current tree)

| Command | rc | Result |
|---|---:|---|
| `cargo test -p wc3-assetdb --lib source::tests` | **0** | 5 passed / 0 failed |
| `cargo test -p wc3-assetdb` | **0** | 16 suites, 44 passed / 0 failed |
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | 0 errors |
| `audio_parity --install analysis/agent-assets/wc3-install` | **0** | device log refreshed |

### Device audio log (refreshed 2026-11-13 20:0x, current tree)

```
AUDIO_ACK hfoo Footman voice=1
AUDIO_ATTACK ogru Grunt voice=2
AUDIO_SPELL BlizzardWave voice=3
AUDIO_AMBIENT AmbientSoundRain voice=4
AUDIO_MUSIC Human1 voice=5
AUDIO_ACK nmam Mammoth voice=10   # TFT layer pick
```

Counts: `ACK=2 MUSIC=1 ATTACK=1 SPELL=1 AMBIENT=1`; parity `wired=true`,
`ambient_active_voices=4`, `music_nonzero_blocks=40`,
`GameAudioStats{acks 2/2, acks_missing 0, sounds_missing 0, music_started 1}`.

### Remaining gap (unchanged, named)

55/4,232 TFT sound refs absent from mounted layers (per-path in
`sound-paths-install-absent.tsv`); mip LOD selection at upload is render-owner;
no device run granted.

# Assets/audio absorption verification (full merge)

Date: 2026-11-13. Scope: `crates/wc3-mpq/**`, `crates/wc3-assetdb/**`,
`crates/wc3-audio/**` (owner), with read-only patches for the game layer.
Commands run from `~/src/herofolends-rs` with `CARGO_BUILD_JOBS=1`,
`CARGO_PROFILE_DEV_DEBUG=0`, `ulimit -v 4000000`.

## 1. One implementation per concern (absorption map)

| Concern | Canonical implementation | Consumers (no byte logic of their own) | Proof |
|---|---|---|---|
| MPQ header/hash/block/decrypt/sectors/listfile | `wc3_mpq::MpqArchive`, `wc3_mpq::stream::MpqFileReader` (`crates/wc3-mpq/src/stream.rs:21`), `MpqIndex` (`src/index.rs:206`) | `wc3-assetdb::source::MpqSource` (`crates/wc3-assetdb/src/source.rs:145`), `wc3-re::storm::StormFs` (`storm.rs:129`), `wc3-re::mpqex::MpqEx` (`mpqex.rs:55`), `wc3-re::mapfiles` (`mapfiles.rs:91`), `wc3-re::scan` (`scan.rs:214`), `wc3-formats::map` (map base + `open_at`) | grep §2: no `decrypt_words` / `crypt_table` / `read_exact_at` outside `wc3-mpq`; all consumers call `MpqArchive::*` |
| MPQ codecs (PKWARE/Huffman/ADPCM) | `wc3_mpq::{pkware,huffman,adpcm}` | `wc3-re::mpqex::{pkware,huffman,adpcm}` are `pub use wc3_mpq::*` re-exports (verified) | `crates/wc3-re/src/mpqex/{pkware,huffman,adpcm}.rs` contain only doc + `pub use` |
| BLP decode | `wc3_formats::blp::decode` (all mips, after the merged patch) | `wc3-assetdb` (`blp_report::validate`, `texture_shared`), `wc3-re::blp` (probe), `wc3-re::icons`, `wc3-re::terrain` | catalog: 3,935/3,935, mips 28,748/28,748 |
| MDX parse | `wc3_formats::mdx::MdxModel::parse` | `wc3-assetdb::model_shared`, `wc3-re::mdx`, `wc3-anim` | catalog: 2,048/2,048, 0 unresolved texture refs |
| Asset provider (bytes) | `wc3_assetdb::source::{AssetSource,MpqSource,DirSource,LayeredSource}` | game (`AssetSource` impl over `StormFs`, merged), audio hooks | grep §2: no other provider/cache |
| Asset cache | `wc3_assetdb::AssetDb` + `lru::LruCache` | game renderer, audio hooks (`SharedAssetDb`), catalogs | one cache; wiring run shows a hit across renderer+audio |
| WAV decode | `wc3_audio::wav` | `AssetDb::sound_shared` | 2,696/2,696 decoded |
| MP3 decode/stream | `wc3_audio::mp3` (`nanomp3`) | `AssetDb::sound_shared`/`sound_stream_bytes`, `AudioEngine::play_mp3` | 1,140/1,140 probed, 40 full decodes, 0 duration mismatch |
| Mixer/engine | `wc3_audio::engine::AudioEngine` | `GameAudio`, game wiring | only mixer in the tree (legacy `Mixer` deleted) |
| Sound tables/hooks | `wc3_audio::soundinfo`, `game_audio` | `wc3-assetdb::audio_provider` | 1,410 defs, 1,993/2,025 playable with the full install |

## 2. Duplicate scan (commands and results)

```
grep -rn "MPQ\x1a|b\"MPQ|decrypt_words|crypt_table|read_exact_at" --include=*.rs crates apps | grep -v wc3-mpq/
```
→ only magic **detection** constants: `wc3-formats/src/map.rs` (locate the MPQ at
offset 512 for `.w3m`), `wc3-re/src/{storm,mpqex,mapfiles}.rs` (same detection).
No hash table, decrypt loop or sector reader outside `wc3-mpq`.

```
grep -rln "fn explode|struct HuffmanTree|IMA_STEP|MS_ADAPT" --include=*.rs crates apps | grep -v wc3-mpq
```
→ `crates/wc3-audio/src/wav.rs` only: RIFF **IMA/MS ADPCM block** framing
(4-byte channel preamble, nibble order, alpha/predictor tables). This is a
different container from MPQ sector ADPCM (`wc3_mpq::adpcm`) and cannot be shared
without moving tables into a third crate; it is not a duplicate decoder of the
same format. Documented, accepted.

```
grep -rn "LruCache|HashMap<String, Vec<u8>>" --include=*.rs crates apps | grep -v wc3-assetdb
```
→ no hits: `wc3-assetdb` is the only cache.

```
grep -rln "RIFF|nanomp3|decode_mp3|decode_wav|WAVE_FORMAT" --include=*.rs crates apps | grep -v wc3-audio
```
→ only `wc3-assetdb` call sites (`sound_shared`, bins) — no decoder code.

```
grep -rn "MpqArchive::open" --include=*.rs crates apps | grep -v wc3-mpq | grep -v /tests/
```
→ consumers only: `wc3-formats::map`, `wc3-assetdb::source`, `wc3-re::{storm,mapfiles,scan,mpqex}`.

## 3. Full extraction / corpus counts

| Check | Result |
|---|---|
| `audit_extract` (rc=0) | **10,523/10,523** files, read_fail 0, missing 0, size_mismatch 0, sha_mismatch 0; mpq_bytes = disk_bytes = 710,907,954; open 24 ms, index 8 ms, full re-read+sha256 23.9 s |
| StormLib oracle (previous phase) | 10,525/10,525 entries byte-identical (0 mismatches) |
| `wc3re scan` (rc=0, 155 s) | BLP 3,935/3,935 · MDX 2,048/2,048 · SLK 33/33 · TXT 67/67 · FDF 71/71 · JASS 3/3 · map scripts 44/44 · AI 91/91 · maps 44/44 (708 inner files, 4,428 WTS strings) · Object DB 1,665 records · TriggerData 676/676 · TerrainArt 98/98 · **overall true** |
| `asset_catalog` (rc=0, 42 s) | textures 3,935/3,935; **mips 28,748 declared = 28,748 decoded**, `mips_incomplete_files=0`; models 2,048 parsed, 5,216 refs, 0 unresolved; icons 691 BTN + 717 DIS, 1,408 decoded, 0 fail; biomes 15; failures 0 |
| `audio_catalog` (rc=0, 16 s) | WAV 2,696/2,696 (131.8M frames, 5,958 s); MP3 1,140/1,140 (12,042 s, 0 duration mismatches); defs 1,410, **playable 1,359**, unplayable 51; listfile-resolved 6; ambiguous 0; absent 124 refs |
| Skin subsystem (other layer) | 22 failures in `wc3re scan` — **not assets/audio**, reported for the FDF/HUD owner |

## 4. Sound paths: War3-only vs full install (TFT layered)

Resolver: exact → unique listfile basename (same-directory candidates preferred,
layers in priority order) → ambiguous → absent. No name is ever synthesised.

| Variant | definitions | playable | refs | exact | listfile | ambiguous | absent | resolved |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| War3.mpq only | 1,410 | **1,359** | 3,013 | 2,883 | 6 | **0** | 124 | **2,889 (95.9%)** |
| War3xlocal + War3x + War3 | 2,025 | **1,993** | 4,232 | 3,591 | 586 | **0** | 55 | **4,177 (98.7%)** |

Artifacts: `analysis/agent-assets/sound-paths-{war3,tft}.{json,md,tsv}`,
`sound-paths-{war3,tft}-absent.tsv` (each absent row lists the layers searched).
The 55 remaining references are genuinely absent from the provided archives
(e.g. `Units\Human\HeroRanger\HeroRangerWhat*`, `TreantAttack*`);
the TFT archives in this workspace are partial (`War3x.mpq` 5,634 blocks).

**Bug found and fixed during full merge:** `AbilitySounds.slk` /
`AmbienceSounds.slk` store `DirectoryBase` **without** a trailing backslash
(`...\Blizzard`), while `UnitAckSounds.slk` includes it. `SoundDef::paths()`
concatenated blindly, producing `...\BlizzardBlizzardTarget1.wav` and masking
real files. `paths()` now joins with exactly one separator
(`crates/wc3-audio/src/soundinfo.rs`, test
`directory_base_join_handles_both_retail_spellings`). Effect: War3-only playable
definitions 1,323 → **1,359**; TFT 1,989 → **1,993**; absent refs 164 → 124
(War3) and 119 → 55 (TFT); ambiguities 4 → **0** after same-directory preference.

## 5. Audio wired to the game

* `wc3_assetdb::audio_provider::SharedAssetDb` (`crates/wc3-assetdb/src/audio_provider.rs:106`):
  `Arc<Mutex<AssetDb>>` facade + `SoundProvider` impl, so the renderer and the
  hooks share **one** cache (bytes/textures/models/sounds).
* `start_game_audio(shared)` (`audio_provider.rs:294`): opens the platform backend
  (`wc3_audio::engine::open_default_backend`: SDL3 with a documented `null`
  device fallback; data never falls back), loads the 8 retail sound tables, the
  unit map, and the 30 music themes from the same database, and returns
  `GameAudio<SharedAssetDb>` plus a `GameAudioWiring` summary
  (`audio_provider.rs:223`).
* `crates/wc3-game/src/assets.rs` patch:
  `analysis/agent-assets/patches/0003-game-audio-wiring.patch` —
  `db` becomes `Option<SharedAssetDb>`, a `audio: Option<WiredGameAudio>` field is
  built in `Assets::load`, and `Assets::{audio_mut, update_audio_listener,
  play_ui_click}` are added. `assets.rs` already carries the merged
  `StormFs`-backed `open_asset_db` (P2).
* Verification binary `crates/wc3-assetdb/src/bin/audio_wiring.rs` (rc=0):
  ```
  wiring: audio device=headless source=War3.mpq tables=8 defs=1410 units=454 themes=30
  voices: ack=VoiceId(1) attack=VoiceId(2) click=VoiceId(3) music=VoiceId(4)
  texture 512x256 mips=10   model Footman textures=3
  acks_started=1 acks_missing=0 music_started=1 ui_events=1
  peak=0.5266  nonzero_blocks=281/281  render 3 s of audio in 0.010 s
  live worker (render thread, null device) frames=9216
  cache hits=1 misses=11 bytes=5.2 MB  (shared renderer+audio)
  ```
  Artifact: `analysis/agent-assets/audio-wiring.json`.

Game-logic event wiring (selection → `unit_ack`, order → `unit_attack`,
death → `unit_death`, screen → `music`) belongs to the game/SIM owner; the hooks
and listener API (`GameAudio::{unit_ack, unit_attack, unit_death, ui_click,
ui_error, music, set_listener}`) are exposed through `Assets::audio_mut()`.

## 6. rc summary (this verification)

| Command | rc |
|---|---:|
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | 0 (only `wc3-formats/src/map.rs` warnings, other layer) |
| `cargo test -p wc3-mpq -p wc3-assetdb -p wc3-audio` | 0 — **74 tests** |
| `audit_extract` | 0 |
| `wc3re scan` | 0 |
| `asset_catalog` | 0 |
| `audio_catalog` | 0 |
| `sound_paths` (War3-only) | 0 |
| `sound_paths` (TFT layers) | 0 |
| `audio_wiring` | 0 |

## 7. Remaining conflicts (other owners)

1. `crates/wc3-game/src/render.rs:777` — pre-existing `(position, …)` type error
   (`&Vec3` vs `Vec3`); blocks `cargo check -p wc3-game`. Render owner.
2. `crates/wc3-game/src/assets.rs` — patch 0003 pending dispatcher merge (file
   frozen). Until merged, `Assets` does not construct `GameAudio`; the same code
   path is proven by `audio_wiring`.
3. `crates/wc3-formats/src/map.rs` — 8 unused-import warnings (map/RE owner);
   they do not affect assets/audio.

## 8. H2

see `analysis/tech-lead/merge-assets.md` §13 for ledger/capture/experience IDs.

## 9. Round 2 verification (ledger 351)

### 9.1 TFT layer for the missing sounds

`assetdb` now builds an ordered MPQ stack itself
(`wc3_assetdb::source::LayeredSource::from_mpq_layers` +
`AssetDb::open_mpq_layers`, `crates/wc3-assetdb/src/source.rs`,
`src/lib.rs`), and `sound_paths --install DIR` mounts the classic loader order
(`War3Patch.mpq` > `patch.mpq` > `War3xlocal.mpq` > `War3x.mpq` > `War3.mpq`).

A test install dir was created with the real archives
(`analysis/agent-assets/wc3-install/{War3,War3x,War3xlocal}.mpq` symlinks to
`analysis/tft_files_Files100_War3x*.mpq` and the WC3 ISO `War3.mpq`):

| Variant | definitions | playable | refs | exact | listfile | ambiguous | absent | resolved |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| War3.mpq only | 1,410 | 1,359 | 3,013 | 2,883 | 6 | 0 | 124 refs / 103 unique | 2,889 (95.9%) |
| `--install` (War3xlocal+War3x+War3) | 2,025 | **1,993** | 4,232 | 3,591 | 586 | 0 | 55 refs / 55 unique | **4,177 (98.7%)** |

TFT layers gained **51 unique paths** (48 references net) over War3-only,
including 48 `HeroArchDruid*` lines, `MarketplaceWhat1`, `ObservatoryWhat1`,
`ThievesGuildWhat1`. 52 paths are absent from both variants; 55 unique remain
absent overall in these partial TFT archives (War3x.mpq has 5,634 blocks).

Artifacts: `sound-paths-install.{json,md,tsv}`, `sound-paths-install-absent.tsv`,
`sound-paths-install.log`.

### 9.2 3D pan (game audio from MPQ)

The pan law was **azimuth-based** already in intent but divided by
`reference + distance`, so a large audible radius flattened every pan. Changed to
the sine of the angle between the listener's right vector and the source
direction (`crates/wc3-audio/src/engine.rs::pan_gains`): attenuation still uses the
reference distance, panning no longer does. `audio_wiring` now isolates the
spatial voice (stops music) and asserts the channel split:

```
3d: voice=VoiceId(5) left_energy=0.4406 right_energy=0.0000
```

Full wiring run (rc=0): ack + attack + click + music from `War3.mpq`, 3 s of
audio rendered in 0.010 s, peak 0.6402, 281/281 non-silent blocks, render worker
9,728 frames, shared-cache hit 1 (renderer + audio share one `AssetDb`).
Artifact: `audio-wiring.json`.

### 9.3 BLP mips re-verified

`asset_catalog` (rc=0) on the current tree: textures 3,935/3,935, declared mips
**28,748 = decoded 28,748**, `mips_incomplete_files=0`, `layout_bad=0`; models
2,048/2,048 with 0 unresolved refs; icons 1,408 decoded / 0 failed; biomes 15;
`all_ok=true`.

### 9.4 rc

| Command | rc |
|---|---:|
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | 0 (0 errors) |
| `cargo test -p wc3-mpq -p wc3-assetdb -p wc3-audio` | 0 — 74 tests / 21 suites |
| `sound_paths --install analysis/agent-assets/wc3-install` | 0 |
| `audio_wiring --seconds 3` | 0 |
| `asset_catalog` | 0 |

## 10. Audio parity (P15/P16, ledger 351 round 2)

`audio_parity` (new bin, `crates/wc3-assetdb/src/bin/audio_parity.rs`) exercises the
exact game-facing path on the real archives, per category:

| Category | Source table | Measured (install layers) | Layer |
|---|---|---|---|
| ack | `UnitAckSounds.slk` `FootmanWhat` (4 variants) | voice starts, peak 0.026 | War3 |
| attack | `GruntYesAttack` (3 variants) | peak 0.428 | War3 |
| spell | `AbilitySounds.slk` `BlizzardWave` (3 variants) | peak 0.432; retail dir `Abilities\…\Blizzard` resolves via listfile to `abilities\…` | War3 |
| ambience | `AmbienceSounds.slk` `AmbientSoundRain` | looping, active after 20 blocks, peak 0.570 | War3 |
| music | `TriggerData` theme `Human1` → `Sound\Music\mp3Music\Human1.mp3` | 40/40 non-silent blocks, peak 0.737 | War3 |
| 3D attenuation | looping `FootmanWhat1` probe, reference 600 | energy 1.0286 / 0.0643 / 0.0040 at 1×/2×/4× = **1, 1/16, 1/256** | War3 |
| TFT acks | `unitUI.unitSound` → `GrizzlyBearWhat` (`nplb`) | peak 0.239, sound file from **War3xlocal** | TFT |

Both modes rc=0: `--install analysis/agent-assets/wc3-install` (layers
War3xlocal+War3x+War3, 42 themes) and `--mpq War3.mpq` (30 themes, TFT case skipped
with a note). Library-level regression: `crates/wc3-assetdb/tests/audio_parity.rs`
(3 tests: categories, inverse-square, TFT layer resolution).

Two retail-schema gaps found and fixed while aligning with `PARITY_MATRIX.md`:

1. **TFT `unitUI.slk` has a different schema** (header row with `unitSound` and
   `name` columns; RoC uses the display-name column as the sound set).
   `UnitNameMap` now detects both, exposes `sound_set(id)`, and
   `unit_ack/attack/death` prefer `unitSound` — without this, every TFT unit ack
   looked up `<file-path>What` and failed.
2. **TFT `TriggerData.txt` writes `MusicThemeFoo=1,musictheme,`path``** (expansion
   flag first) vs RoC `=musictheme,`path``. The parser accepts both; theme count
   rose 30 → 42 with TFT and music playback keeps working.

Alignment proposal for the dispatcher: `analysis/agent-assets/PARITY_AUDIO_ALIGNMENT.md`
(exact row replacements for P15, P16, the §5 "Audio hooks wired" line and §6
increment 17; P15/P16 stay PARTIAL until `patches/0003-game-audio-wiring.patch`
merges and the game process emits the QA audio gate).

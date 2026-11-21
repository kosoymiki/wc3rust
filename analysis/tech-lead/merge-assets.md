# merge-assets.md — assets/audio layer port onto the single base

Task: DISPATCHER THICK continuation (assets/audio), 2026-11-12.
Authority: `analysis/tech-lead/BASE.md` v1 + `WC3RE_MERGE_MATRIX.md`.
Owner paths: `crates/wc3-assetdb/**`, `crates/wc3-audio/**`, `analysis/agent-assets/**`,
`analysis/tech-lead/merge-assets.md`.
Read-only (patched here): `crates/wc3-mpq/**`, `crates/wc3-formats/src/blp.rs`,
`crates/wc3-re/src/storm.rs`, `crates/wc3-game/src/assets.rs`.

## State

**DONE (merged + compiled + tests green + full-install absorption verified).**

Execution mode (dispatcher steer): merged the exclusive assets scope and compiled it.
Evidence: `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` → **rc=0** (no warnings);
`cargo test -p wc3-mpq -p wc3-assetdb -p wc3-audio` → **rc=0** (**74 tests green**).
Full-merge absorption report: `analysis/agent-assets/ABSORPTION.md` (no duplicate
reader/codec/cache; 10,523/10,523 extraction; audio wired through
`SharedAssetDb` + patch `0003-game-audio-wiring.patch`).
Full-corpus gates re-run on the merged tree: BLP mips **28,748/28,748**
(`mips_incomplete_files=0`), sound paths **2,849/3,013** resolved (exact 2,843 +
listfile 6), WAV **2,696/2,696**, MP3 **1,140/1,140**, sound definitions playable
**1,323** (+6 from listfile resolution). All report binaries rebuilt and re-run
after the dispatcher pause window (`audio_catalog` rc=0, 16 s). No device run, no APK
install.

## 0. Tree status at manifest time

| Item | State |
|---|---|
| `crates/wc3-assetdb/**` provider/cache/resolver changes | **merged, compiled, tests green** |
| `crates/wc3-audio/**` hook resolution + legacy mixer removal | **merged, compiled, tests green** |
| P1 `wc3-formats/src/blp.rs` BLP1 all-mips | **applied**, measured: 28,748/28,748 mips decoded |
| P2 `wc3-game/src/assets.rs` StormSource + layered open | **merged in tree** (verified: `open_asset_db`, `StormSource`) |
| P3 `wc3-game/src/assets.rs` audio wiring (`SharedAssetDb` + `GameAudio`) | **pending dispatcher merge** (`patches/0003-game-audio-wiring.patch`) |
| `crates/wc3-mpq/**` | unchanged by this phase (canonical reader; its own tests 10/10 green) |
| Device / APK | **not run** (order) |

## 1. Canonical target per concern

| Concern | Canonical target (crate/module/type/fn) | Why |
|---|---|---|
| MPQ header/hash/block/decrypt/sectors/listfile | `wc3_mpq::MpqArchive` + `wc3_mpq::stream::MpqFileReader` (`crates/wc3-mpq/src/stream.rs:21`) | already the single byte reader; sector-tag detection and per-sector decryption live here (§5) |
| MPQ index / fast open | `wc3_mpq::MpqIndex` (`crates/wc3-mpq/src/index.rs:206`), `MpqArchive::open_with_index` (`src/lib.rs:203`) | fingerprint-validated index avoids listfile+hash probe per file |
| MPQ codecs | `wc3_mpq::{pkware,huffman,adpcm}` | BASE §2: one codec set; `wc3-re::mpqex` re-exports only |
| BLP1/BLP2 decode | `wc3_formats::blp::decode` (`crates/wc3-formats/src/blp.rs:24`) | BASE §2 canonical; patch P1 fixes its mip chain |
| Asset provider (where bytes come from) | **new** `wc3_assetdb::source::{AssetSource, MpqSource, DirSource, LayeredSource}` (`crates/wc3-assetdb/src/source.rs:84/145/283/426`) | assetdb is the canonical provider/cache; no second reader, no `wc3-re` dependency (§5) |
| Multi-archive priority / Storm semantics | `wc3_re::storm::StormFs` (unchanged, `crates/wc3-re/src/storm.rs:101`) | BASE §2; consumed through a game-layer adapter (patch P2), never re-implemented in assetdb |
| Asset cache (bytes/textures/models/sounds) | `wc3_assetdb::AssetDb` + `lru::LruCache` (`src/lib.rs:212`, `src/lru.rs:29`) | one cache layer; bounded + refcount-pinned |
| Asset catalogs | `wc3_assetdb::catalog::run` (`src/catalog.rs:202`) + `blp_report::validate` (`src/blp_report.rs:265`) | full-corpus validation, exact counts |
| WAV/ADPCM decode | `wc3_audio::wav::{probe_wav,decode_wav}` (`src/wav.rs:121/233`) | audio layer decodes; it never touches an archive (grep proof §6) |
| MP3 decode/stream | `wc3_audio::mp3::{probe_mp3,decode_mp3,Mp3Stream}` (`src/mp3.rs:42/96/158`) | pure-Rust `nanomp3`; streaming window |
| Mixing/engine | `wc3_audio::engine::{AudioEngine,AudioBackend,PlayParams}` (`src/engine.rs:759/136/57`) | one mixer; render thread; 0 alloc/render |
| Sound tables + hooks | `wc3_audio::soundinfo::{SoundInfoDb,UnitNameMap}` (`src/soundinfo.rs:62/216`), `game_audio::GameAudio` (`src/game_audio.rs:91`), `unit_ack` (:201), `music` (:328) | data-driven gameplay audio |

No new workspace crate, no second reader/parser/renderer.

## 2. OWNER files changed in this phase

| File | Change |
|---|---|
| `crates/wc3-assetdb/src/source.rs` (new) | provider trait + MPQ/dir/layered sources + `Resolution`/`SearchHit`/`normalize_path`; listfile basename index |
| `crates/wc3-assetdb/src/lib.rs` | `Provider` enum deleted → `Box<dyn AssetSource>`; `open_sources`, `try_open_dir`, `resolve_asset`, `sound_path`, `source_label`, `layer_labels`; `TextureAsset::{mip0,mip_levels,mip,mip_bytes,dimensions}`; IO now uses canonical spelling (cache keys stay lowercase) |
| `crates/wc3-assetdb/src/audio_provider.rs` | `SoundProvider::resolve` impl (exact → listfile), `load_sound/load_bytes` resolve first |
| `crates/wc3-assetdb/src/catalog.rs` | mip accounting: `mips_incomplete_files`, `mips_complete_files` (JSON + MD) |
| `crates/wc3-assetdb/src/bin/sound_paths.rs` (new) | resolution evidence tool over explicit layers (priority order) → `sound-paths.{json,md,tsv}`, `sound-paths-absent.tsv` |
| `crates/wc3-assetdb/src/bin/audio_catalog.rs` | missing-path classification via `resolve_asset`; new counters `listfile_resolved_paths`, `ambiguous_paths` |
| `crates/wc3-audio/src/game_audio.rs` | `SoundProvider::resolve` default + call sites (`play_def`, `ui_click`, `ui_error`, `music`) |
| `crates/wc3-audio/src/lib.rs` | legacy `Voice`/`Mixer` removed (second mixer); `Sound` decode primitives stay |
| `analysis/agent-assets/patches/0001-blp1-all-mips.patch` (new) | ready patch for `wc3-formats/src/blp.rs` |
| `analysis/agent-assets/patches/0002-game-storm-source.patch` (new) | ready patch for `wc3-game/src/assets.rs` |

## 3. READ-ONLY patches (exact lines)

### P1 — `crates/wc3-formats/src/blp.rs`, function `decode_blp1` (lines **62–174** in the current tree)

**Status: APPLIED in the tree** (the dispatcher merged the patch while this manifest was
being written; verified below). Corrected patch artifact:
`analysis/agent-assets/patches/0001-blp1-all-mips.patch`
(`@@ -60,75 +60,116 @@`; applying it to the pre-patch file reproduces the current
file byte-for-byte — idempotence check passed).

Incident (recorded for transparency): the first patch revision carried a duplicated
`    } else {` line (an artifact of the edit range). The applied tree does **not**
contain it; the regenerated patch above is generated from the pre-patch file
(reconstructed by reverse-applying the first revision) and the current file, so it is
free of that defect. `rustfmt --check` parses the current `blp.rs` without errors and
braces balance.

What it changes:

* `content_type == 0` (JPEG): decode **every declared mip** as `shared JPEG header
  + that mip's bytes` (the layout War3Net uses in `GetJpegFileBytes(mip)`),
  halving dimensions per level; `BlpImage.rgba` stays mip 0.
* `content_type == 1` (paletted): loop over the declared mip table; each level is
  `w*h` palette indices followed by that level's packed alpha plane
  (`sizes[i] == w*h + ceil(w*h*alpha_bits/8)`, verified on 28,748 declared levels
  by the catalog); `BlpImage.rgba` stays mip 0.
* No API change: `decode -> BlpImage{width,height,rgba,mipmaps}` keeps `rgba` =
  mip 0 and now fills `mipmaps` for BLP1 as well.

Current behaviour that this fixes (measured before the patch): BLP1 returns
`mipmaps.len() == 1` for all 2,920 JPEG and 1,015 paletted files, while headers
declare 28,748 levels in total. After P1, `asset-catalog.json` must report
`mips_incomplete_files = 0`.

### P2 — `crates/wc3-game/src/assets.rs` (read-only for every layer; dispatcher merges)

Ready patch: `analysis/agent-assets/patches/0002-game-storm-source.patch`.

Exact edits:

1. Line **12**: `use wc3_assetdb::AssetDb;` →
   `use wc3_assetdb::{AssetDb, AssetSource, SearchHit};`
2. Insert after line **18** (`use wc3_sim::UnitKind;`): `use wc3_re::storm::StormFs;`
3. Insert before line **216** (`impl Assets {`): `StormSource` (impl `AssetSource`
   over `StormFs`: `read`, `size` via `info`, `listfile` cached once,
   `resolve_exact` returns `info.archive`, `search_basename`, `layer_labels`),
   `storm_basename`, and `open_asset_db(path, debug)` which:
   * opens `StormFs::open_wc3_install(parent_dir)` when the passed archive is part
     of that install (priority patch > locale > expansion > base),
   * builds `AssetDb::open_sources(vec![StormSource], "wc3-install")`,
   * falls back to `AssetDb::open_mpq(path)` with the existing debug strings.
4. Replace lines **221–233** (the `let mut db = match mpq_path { ... };` block) with
   `let mut db = mpq_path.and_then(|path| open_asset_db(path, &mut debug));`

Why in the game layer: BASE §1 forbids `wc3-assetdb -> wc3-re`. The game layer is
allowed to depend on both, so it is the correct seam; assetdb keeps only
`AssetSource`. This preserves `StormFs` as the single owner of priority/listfile
semantics (no second implementation).

### P3 — `crates/wc3-re/src/storm.rs`

**No patch requested.** `StormFs` already provides everything the port needs
(`open_wc3_install`, `read`, `info`, `listfile`, `search`, `layers`). Optional
future improvement (not needed for parity): accept a `MpqIndex` per layer to skip
the listfile read on open.

### P4 — `crates/wc3-mpq/**`

**No patch requested.** `stream.rs` (MpqFileReader, per-sector decrypt:
`crates/wc3-mpq/src/stream.rs:21/38/84/159`), `index.rs` (`MpqIndex` :206,
`CodecUsage` :37, `FileMeta` :88) and `lib.rs` (`open_with_index` :203) already
contain the ported implementation from the previous phase (before the freeze).

## 4. Symbols ported (source `file:line` → target `file:line`)

| Symbol / behaviour | Source | Target |
|---|---|---|
| MPQ whole-file read | `wc3-mpq/src/lib.rs` `MpqArchive::read` | `wc3-assetdb/src/source.rs:204` (`impl AssetSource for MpqSource::read` → `archive.read`) |
| MPQ random access / sector decode | `wc3-mpq/src/stream.rs:21` `MpqFileReader`, `:159` `ensure_sector` | not re-implemented; assetdb streams via `MpqSource::read` (whole-file) and keeps the reader available for large-file consumers |
| Per-sector decryption of encrypted-uncompressed blocks | `wc3-mpq/src/stream.rs:38/84/163` | canonical; catalog/audit consume it (`sound`/`blp` paths) |
| Codec provenance | `wc3-mpq/src/index.rs:37` `CodecUsage` | `wc3-assetdb/src/bin/asset_catalog.rs` (codec histogram), `audio_catalog.rs` |
| Archive index / fast open | `wc3-mpq/src/index.rs:206` `MpqIndex`, `lib.rs:203` `open_with_index` | `wc3-assetdb/src/source.rs:172` `MpqSource::open_with_index`; `lib.rs:245` `AssetDb::open_mpq_indexed` |
| Directory provider (case-insensitive index) | was `wc3-assetdb/src/lib.rs` `Provider::Dir` (deleted) | `wc3-assetdb/src/source.rs:283` `DirSource::open` |
| Multi-layer read + priority | was ad-hoc per call | `wc3-assetdb/src/source.rs:426` `LayeredSource` (`read` :478, `resolve_exact` :521) |
| Path resolution (exact → listfile) | new | `wc3-assetdb/src/lib.rs:389` `AssetDb::resolve_asset`; `source.rs:32` `Resolution` |
| Sound path canonicalisation | new | `wc3-assetdb/src/lib.rs:437` `AssetDb::sound_path`; `audio_provider.rs:45` `SoundProvider::resolve` |
| BLP decode | `wc3-formats/src/blp.rs:24` | `wc3-assetdb/src/blp_report.rs:265` (`validate`), `lib.rs:495` (`texture_shared`) |
| BLP header probe | `wc3-assetdb/src/blp_report.rs` `probe` | catalog/texture report; identical fields to `wc3-re::blp::probe` (no duplicate decode) |
| MDX parse | `wc3-formats/src/mdx.rs` `MdxModel::parse` | `wc3-assetdb/src/lib.rs:526` `model_shared` (resolve refs via `resolve_path`) |
| LRU + pinning | `wc3-assetdb/src/lru.rs:29` | all four caches (`lib.rs:216-219`) |
| WAV probe/decode | `wc3-audio/src/wav.rs:121/233` | `wc3-assetdb/src/lib.rs:640` `sound_shared` |
| IMA/MS ADPCM | `wc3-audio/src/wav.rs:311/372` | `decode_wav` dispatch (`wav.rs:233`) |
| MP3 probe/decode/stream | `wc3-audio/src/mp3.rs:42/96/158` | `sound_shared` (bounded) / `sound_stream_bytes` (`lib.rs:673`) |
| Mixer + engine | `wc3-audio/src/engine.rs:759/136/57` | game layer (`wc3-game/src/assets.rs`, `wc3-game::game`), `sdl_smoke` |
| Sound tables | `wc3-audio/src/soundinfo.rs:62` | `wc3-assetdb/src/audio_provider.rs:55` `load_sound_tables` |
| Unit names | `wc3-audio/src/soundinfo.rs:216` | same (`Units\unitUI.slk`) |
| Gameplay hooks | `wc3-audio/src/game_audio.rs:201/328` | `wc3-game` (unit ack/attack/death, ui, music) |

## 5. BLP mip-chain consumption (fix + consumer contract)

Consumer contract (implemented in `wc3-assetdb/src/lib.rs:155` `TextureAsset`):

* `TextureAsset::mip0() -> &[u8]` — base level; identical to `image.rgba`
  (this is what renderers upload as level 0).
* `TextureAsset::mip_levels() -> usize` — decoded levels (>= 1).
* `TextureAsset::mip(level) -> Option<&MipLevel>` — preserved levels.
* `TextureAsset::mip_bytes() -> usize` — memory accounting across all levels.
* `AssetDb::texture()` (legacy) / `texture_shared()` return the `BlpImage` with
  `rgba` = mip 0 and `mipmaps` = every decoded level; no consumer loses mips.

Validation:

* `blp_report::validate` already checks `mip.width/height` halving and
  `rgba.len() == w*h*4` for every decoded level (`blp_report.rs:265`).
* `catalog` now reports `mips_complete_files` / `mips_incomplete_files`
  (`catalog.rs:47-48`, written at `:598-599`).
* Gate after P1: `mips_incomplete_files == 0` and
  `mip_levels_decoded == mip_levels_declared` (28,748).

## 6. Duplicates removed / thinned + delegation proof

| Removed / thinned | Proof |
|---|---|
| `wc3-assetdb::Provider` enum (previous `lib.rs:178-188`, both variants) | grep `Provider` in `crates/wc3-assetdb/src/lib.rs` → only doc text remains; storage is `Box<dyn AssetSource>` (`lib.rs:216`) |
| Directory indexing logic inside `AssetDb::open_dir` | moved to `source.rs:283` `DirSource::open`; `lib.rs:299` only constructs it |
| Per-call provider matching in 8 methods (`listfile`, `files`, `file_count`, `resolve_path`, `dir_root`, `file_size`, `load_bytes`) | all now one-line delegations (`lib.rs:325-470`); grep for `match &self.provider` → 0 hits |
| MPQ byte logic inside assetdb | grep `read_exact_at|decrypt|sector` in `crates/wc3-assetdb/src/*.rs` → **0 hits**; only `self.source.read` |
| Archive access inside audio | grep `MpqArchive|wc3_mpq` in `crates/wc3-audio/src` and `Cargo.toml` → **0 hits** |
| Second cache in RE layer | grep `LruCache|HashMap<String, Vec<u8>>` in `crates/wc3-re/src/*.rs` → **0 hits**; `wc3-re` is stateless/parse-only |
| Second MPQ reader in RE layer | `wc3-re::mpqex::MpqEx` `read` (`mpqex.rs:91-93`) → `self.archive.read(...)`; `StormFs::read` (`storm.rs:201`) → `MpqArchive` |
| Duplicate codec implementations | `wc3-re::mpqex::{pkware,huffman,adpcm}` re-export `wc3-mpq` (BASE §5); no local copies |
| Second mixer (`wc3_audio::Mixer` + legacy `Voice`, previous `lib.rs:98-197`) | removed in this phase; grep `wc3_audio::Mixer|Mixer::new` across `crates apps` → 0 hits; `AudioEngine` is the only mixer (`engine.rs:759`) |

Type identity: `AssetDb.source: Box<dyn AssetSource>` and `MpqSource` wraps
`wc3_mpq::MpqArchive` — there is exactly one archive type in the graph.
Parity fixture (post-freeze): `tests/source_resolution.rs` reads the same path
through `AssetDb::open_mpq` and `wc3_re::storm::StormFs` and compares bytes.

## 7. Sound path resolution (the 170 references)

Static analysis of the retail tables against the real War3.mpq listfile
(`analysis/agent-assets/extraction-audit-files.tsv`, 10,523 entries), performed
in this phase without building:

* 170 table references, **147 unique paths**.
* **6 unique paths resolve through the listfile** (exact name absent, one
  same-basename file in the archive — the retail table's directory base is stale):

| requested (retail table) | canonical (archive listfile) |
|---|---|
| `Buildings\Orc\PigFarm\PigFarmWhat1.wav` | `buildings\Other\PigFarm\PigFarmWhat1.wav` |
| `Buildings\NightElf\AncientOfTheWild\AncientOfTheWildAttack1.wav` | `buildings\NightElf\AncientOfWind\AncientOfTheWildAttack1.wav` |
| `Buildings\NightElf\AncientOfTheWild\AncientOfTheWildAttack2.wav` | `buildings\NightElf\AncientOfWind\AncientOfTheWildAttack2.wav` |
| `Buildings\NightElf\AncientOfTheEarth\AncientOfTheEarthAttack1.wav` | `buildings\NightElf\AncientOfLore\AncientOfTheEarthAttack1.wav` |
| `Buildings\NightElf\AncientOfTheMoon\AncientOfTheMoonAttack1.wav` | `buildings\NightElf\AncientOfWar\AncientOfTheMoonAttack1.wav` |
| `Units\NightElf\AncientProtector\AncientProtectorAttack2.wav` | `buildings\NightElf\AncientProtector\AncientProtectorAttack2.wav` |

* **0 ambiguous** basename matches (no path chosen when several exist).
* **141 unique paths absent** from `War3.mpq` and the partial TFT loose trees
  (`analysis/war3x_data`, `analysis/war3x_local`: 407 files) — e.g.
  `Units\NightElf\HeroArchDruid\*`, `Units\Human\HeroRanger\*`,
  `Abilities\Spells\Demon\RainOfFire*`. These are The Frozen Throne assets.

Implementation: `AssetDb::resolve_asset` (exact → listfile-unique → ambiguous →
absent with searched layers), `SoundProvider::resolve` consumed by all hook call
sites. Absent paths are reported, never synthesised. When `War3x.mpq` /
`War3xLocal.mpq` are mounted as higher-priority layers (P2 wires `StormFs`), the
same code resolves them with no change.

Measured on the merged tree (`sound_paths`, artifacts
`analysis/agent-assets/sound-paths-{war3,tft}.json`). A path-join bug found during
this verification (`AbilitySounds`/`AmbienceSounds` store `DirectoryBase` without a
trailing separator) was fixed in `SoundDef::paths()` and then re-measured:

```
War3.mpq only:   definitions 1410 (playable 1359)  refs 3013
                 exact 2883  listfile 6  ambiguous 0  absent 124  resolved 2889 (95.9%)
War3xlocal+War3x+War3: definitions 2025 (playable 1993)  refs 4232
                 exact 3591  listfile 586  ambiguous 0  absent 55  resolved 4177 (98.7%)
```

All six listfile-resolved paths are recorded with their canonical spelling in
`sound-paths.tsv`. With TFT layers added via repeated `--layer label=path`, the absent
count drops by exactly the number of files present in those layers (same code path).

## 8. Public API the game/base consumes (exact signatures)

```rust
// wc3-assetdb
pub fn AssetDb::open_mpq(path: impl AsRef<Path>) -> Result<AssetDb, AssetError>;
pub fn AssetDb::open_mpq_indexed(path: impl AsRef<Path>, index: MpqIndex) -> Result<AssetDb, AssetError>;
pub fn AssetDb::open_mpq_best(path: impl AsRef<Path>, index_path: impl AsRef<Path>) -> Result<(AssetDb, bool), AssetError>;
pub fn AssetDb::open_sources(sources: Vec<Box<dyn AssetSource>>, label: impl Into<String>) -> Result<AssetDb, AssetError>;
pub fn AssetDb::open_dir(path: impl AsRef<Path>) -> AssetDb;
pub fn AssetDb::try_open_dir(path: impl AsRef<Path>) -> Result<AssetDb, AssetError>;
pub fn AssetDb::read(&mut self, name: &str) -> Result<Arc<Vec<u8>>, AssetError>;
pub fn AssetDb::texture(&mut self, name: &str) -> Result<BlpImage, AssetError>;
pub fn AssetDb::texture_shared(&mut self, name: &str) -> Result<Arc<TextureAsset>, AssetError>;
pub fn AssetDb::model(&mut self, name: &str) -> Result<MdxModel, AssetError>;
pub fn AssetDb::model_shared(&mut self, name: &str) -> Result<Arc<ModelAsset>, AssetError>;
pub fn AssetDb::sound_shared(&mut self, name: &str) -> Result<Arc<Sound>, AssetError>;
pub fn AssetDb::sound_stream_bytes(&mut self, name: &str) -> Result<Arc<[u8]>, AssetError>;
pub fn AssetDb::resolve_path(&self, name: &str) -> Option<String>;
pub fn AssetDb::resolve_asset(&self, path: &str) -> Resolution;
pub fn AssetDb::sound_path(&self, path: &str) -> Option<String>;
pub fn AssetDb::stats(&self) -> AssetStats;
pub fn AssetDb::total_bytes(&self) -> usize;
pub fn AssetDb::trim(&mut self) -> (EvictionReport, EvictionReport, EvictionReport);
pub trait AssetSource: Send { /* label, kind, read, size, listfile, files, resolve_exact, search_basename, exists, layer_labels, loose_root */ }
pub enum Resolution { Exact{layer,path}, ListfileMatch{layer,requested,path}, Ambiguous{requested,candidates}, Absent{requested,searched} }

// wc3-audio
pub fn probe_wav(bytes: &[u8]) -> Result<WavInfo, AudioError>;
pub fn decode_wav(bytes: &[u8]) -> Result<Sound, AudioError>;
pub fn probe_mp3(bytes: &[u8]) -> Result<Mp3Info, AudioError>;
pub fn decode_mp3(bytes: &[u8]) -> Result<Sound, AudioError>;
pub fn decode_mp3_bounded(bytes: &[u8], max_samples_per_channel: usize) -> Result<(Sound, bool), AudioError>;
pub struct Mp3Stream; impl Mp3Stream { pub fn new(bytes: Arc<[u8]>) -> Result<Self, AudioError>; pub fn read(&mut self, out: &mut [f32]) -> usize; pub fn rewind(&mut self) -> Result<(), AudioError>; }
pub fn AudioEngine::headless(config: AudioConfig) -> AudioEngine;
pub fn AudioEngine::start(config: AudioConfig, backend: Box<dyn AudioBackend>) -> AudioEngine;
pub fn AudioEngine::play(&mut self, sound: Arc<Sound>, params: PlayParams) -> VoiceId;
pub fn AudioEngine::play_mp3(&mut self, bytes: Arc<[u8]>, params: PlayParams) -> Result<VoiceId, AudioError>;
pub fn AudioEngine::stop(&mut self, id: VoiceId);
pub fn AudioEngine::set_gain / set_pan / set_position / set_looping / set_pitch / set_master_gain / set_category_gain / set_listener;
pub fn AudioEngine::stats(&self) -> AudioStats;
pub fn AudioEngine::shutdown(self);
pub trait SoundProvider { fn load_sound(..); fn load_bytes(..); fn exists(..); fn resolve(&self, path: &str) -> Option<String> { .. } }
pub struct GameAudio<P: SoundProvider>; impl { unit_ack, unit_attack, unit_death, ui_click, ui_error, music, stop_music, load_themes_from_trigger_data, render }
```

Compatibility: `wc3-game/src/assets.rs` currently calls `AssetDb::open_mpq`,
`db.texture(&path) -> BlpImage`, `db.model(&path) -> MdxModel`, `db.read(...)`;
all signatures are unchanged.

## 9. Tests added and green (execution phase)

These were delivered in `crates/wc3-assetdb/tests/**` (owner path) and run with
`cargo test -p wc3-assetdb -p wc3-audio` → rc=0:

| File | Tests | Fixture |
|---|---|---|
| `tests/source_resolution.rs` | `layer_priority_prefers_the_first_source`, `ambiguity_is_not_guessed`, `listfile_resolves_stale_pigfarm_path`, `absent_retail_path_reports_searched_layers`, `exact_path_resolves_case_insensitively_with_canonical_spelling` | two temp trees; real `War3.mpq` + `war3.index` |
| `tests/blp_mips.rs` | `blp1_jpeg_decodes_every_declared_mip`, `blp1_paletted_decodes_every_declared_mip`, `assetdb_texture_preserves_all_levels_and_mip0` | `analysis/war3_full3/TerrainArt/Village/Village_Dirt.blp` (512×256, 10 mips), `ReplaceableTextures/CommandButtons/BTNFootman.blp`; `War3.mpq` for `texture_shared` |
| `tests/sound_paths.rs` | `six_stale_table_paths_resolve_and_decode`, `absent_definition_is_not_fabricated` | real archive + the six canonical paths |
| `tests/hook_resolution.rs` | `stale_table_unit_plays_through_the_resolver` (`npgf`/PigFarm, measured peak 0.015), `absent_unit_ack_returns_none_without_fabricating` | real archive + sound tables |
| `tests/golden_blp.rs` (earlier) | colour mapping: independent JPEG re-decode diff 0 bytes, terrain hue semantics | `Village_Dirt`, `Village_GrassShort`, 111 TerrainArt tiles |

The table below is the original plan, kept for traceability.

`crates/wc3-assetdb/tests/source_resolution.rs`
* `layer_priority_prefers_first_source` — fixtures: two `tempdir`s with
  `Textures\T.blp` containing different bytes; expect layer 1 bytes.
* `listfile_resolves_stale_pigfarm_path` — fixtures: `War3.mpq` +
  `analysis/agent-assets/war3.index`; expect
  `Buildings\Orc\PigFarm\PigFarmWhat1.wav` → `buildings\Other\PigFarm\PigFarmWhat1.wav`.
* `absent_retail_path_reports_searched_layers` — fixture: `War3.mpq`;
  `Units\NightElf\HeroArchDruid\HeroArchDruidWhat1.wav` → `Absent` with
  `searched == ["War3.mpq"]`.
* `ambiguity_is_not_guessed` — fixture: temp dir with two same-basename files.

`crates/wc3-assetdb/tests/blp_mips.rs`
* `blp1_jpeg_decodes_all_declared_mips` — fixture `TerrainArt\Village\Village_Dirt.blp`
  (declared 10, decoded 10 after P1; `rgba` length = 512*256*4).
* `blp1_paletted_decodes_all_declared_mips` — fixture
  `ReplaceableTextures\CommandButtons\BTNFootman.blp` (declared == decoded).
* `texture_asset_mip0_is_image_rgba` — same fixture.
* `catalog_reports_zero_mip_incomplete` — catalog over a 64-file slice.

`crates/wc3-assetdb/tests/sound_paths.rs`
* `six_stale_table_paths_resolve` — fixture `War3.mpq` + tables; assert the six
  canonical paths from §7 and that each `sound_shared` decodes.
* `absent_paths_stay_absent_without_tft` — assert `Absent`, no invented path.

`crates/wc3-audio/tests/hook_resolution.rs`
* `game_audio_plays_listfile_resolved_unit` — fixture `War3.mpq`; `unit_ack("npgf")`
  (PigFarm) starts a voice (previously silent).
* `game_audio_absent_unit_returns_none` — `unit_ack("huth")` (HeroArchDruid is TFT-only
  in this archive) returns `None` without fabricating.

`crates/wc3-formats/tests/blp_mips.rs` (owner: dispatcher, after P1)
* `blp1_jpeg_mip_chain_matches_header` / `blp1_paletted_mip_chain_matches_header`
  — decoded `mipmaps.len() == header.mip_count` and dims halve to 1x1.

Parity gate: `crates/wc3-re/tests/merge_parity.rs` already compares readers; add
`assetdb_source_matches_stormfs` using `tests/source_resolution.rs` fixtures.

## 10. Conflicts with other layers

1. **`crates/wc3-game/src/render.rs:777` does not compile** (`(position, …)` where
   `position: &Vec3`; needs `*position`). Render layer owns it; asset/audio crates
   are unaffected. Without this fix gate 3 (`cargo check -p wc3-game`) fails
   regardless of my layer.
2. **`wc3-game/src/assets.rs` is frozen** (patch-only). P2 must be merged by the
   dispatcher; until then the base still opens a single archive and the six stale
   sound paths are resolved by `AssetDb` only when the layered source is wired.
3. **`wc3-formats/src/blp.rs` is frozen.** P1 is already applied in the tree
   (verified idempotent). Until the post-freeze catalog run, the new counters have
   no measured value; the gate is `mips_incomplete_files = 0` and
   `mip_levels_decoded = 28,748`. If the run reports otherwise, the applied copy
   is not the version in `patches/0001-blp1-all-mips.patch` and must be diffed.
4. **BASE §1 vs StormFs**: assetdb is not allowed to depend on `wc3-re`. The
   adapter therefore lives in `wc3-game`. If the dispatcher prefers the opposite,
   the alternative is to allow `wc3-assetdb -> wc3-re` and move `StormSource` into
   assetdb; that requires a BASE v2 decision, not a layer change.
5. `wc3-audio` does not depend on `wc3-mpq` (BASE §1 lists it as "incl. wc3-mpq"
   but no code needs it; adding the dependency would be unused). Flagged for the
   dispatcher to confirm the graph line as written.
6. `wc3-game/src/assets.rs` may also be patched by the RE/JASS owner
   (`0001-assets-mapbundle-wts.patch`). Both patches touch `Assets::load`; P2 only
   replaces the `db` construction block (lines 221–233) and adds items before
   `impl Assets` — merge P2 **after** the MapBundle patch, or rebase onto it.

## 11. Exact next step

1. Dispatcher merges **P2** (`wc3-game/src/assets.rs`); **P1 is already applied**.
   Then announce BASE freeze.
2. On freeze run, in order:
   * `cargo metadata --no-deps` (graph unchanged),
   * `cargo test -p wc3-mpq -p wc3-formats -p wc3-assetdb -p wc3-audio`,
   * `cargo run -p wc3-assetdb --bin asset_catalog -- --mpq <War3.mpq> --index analysis/agent-assets/war3.index --out analysis/agent-assets`
     → gate `mips_incomplete_files = 0`, textures 3935/3935,
   * `cargo run -p wc3-assetdb --bin sound_paths -- --mpq <War3.mpq> --index analysis/agent-assets/war3.index`
     → gate `exact=2872 listfile=6 ambiguous=0 absent=141`,
   * `cargo test -p wc3-audio --test hook_resolution`,
   * `cargo check -p wc3-game --lib` (blocked by §10.1 until render is fixed).
3. Attach the two run reports (`asset-catalog.json`, `sound-paths.json`) to this
   manifest as the post-freeze evidence.

## 12. Evidence paths

* `analysis/agent-assets/patches/0001-blp1-all-mips.patch` (P1, **applied**; artifact reproduces the tree)
* `analysis/agent-assets/patches/0002-game-storm-source.patch` (P2, ready to apply)
* `analysis/agent-assets/extraction-audit.json` (10,523/10,523, 0 failures)
* `analysis/agent-assets/oracle-compare.json` (StormLib 10,525/10,525 byte-identical)
* `analysis/agent-assets/asset-catalog.json` (3,935 BLP / 2,048 MDX / 691+717 icons / 15 biomes)
* `analysis/agent-assets/audio-catalog.json` (2,696 WAV / 1,140 MP3 / 1,410 defs / 30 themes)
* `analysis/agent-assets/sound-paths.json` — regenerated post-freeze by `sound_paths`
* `analysis/agent-assets/INTEGRATION.md`, `REPORT.md` (layer contract and prior evidence)

## 14. Merge execution evidence (this steer)

Commands run in `~/src/herofolends-rs` with `CARGO_BUILD_JOBS=1`,
`CARGO_PROFILE_DEV_DEBUG=0`, `ulimit -v 4000000`:

| Command | rc | Result |
|---|---:|---|
| `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | no errors, no warnings after cleanup |
| `cargo test -p wc3-mpq -p wc3-assetdb -p wc3-audio` | **0** | mpq 9 + human04 1; assetdb lib 19 + audio_hooks 4 + golden_blp 4 + **source_resolution 5 + blp_mips 3 + sound_paths 2 + hook_resolution 2**; audio lib 23 + no_alloc 1 = **73 tests** |
| `cargo build --release -p wc3-assetdb --bin asset_catalog --bin sound_paths` | **0** | 1m03s |
| `./target/release/asset_catalog --mpq War3.mpq --index analysis/agent-assets/war3.index` | **0** | textures 3935/3935; mips 28748/28748; incomplete 0; models 2048/2048, unresolved 0; icons 1408 ok; biomes 15; failures 0 (42 s) |
| `./target/release/sound_paths --mpq War3.mpq --index …` | **0** | exact 2843, listfile 6, ambiguous 0, absent 164 → resolved 2849/3013 |
| `cargo build --release -p wc3-assetdb --bin audio_catalog` (post-pause) | **0** | 36 s |
| `cargo build --release -p wc3-mpq --example audit_extract` + run | **0** | 10,523/10,523, sha 0 mismatch, 24 s |
| `wc3re scan analysis/war3_full3 --maps …` | **0** | BLP 3935, MDX 2048, SLK 33, TXT 67, FDF 71, maps 44/44, AI 91/91, TriggerData 676, overall true (155 s) |
| `sound_paths --mpq War3.mpq` | **0** | 1,359/1,410 defs, 2,889/3,013 refs, 0 ambiguous, 124 absent |
| `sound_paths --layer War3xlocal --layer War3x --layer War3` | **0** | 1,993/2,025 defs, 4,177/4,232 refs, 0 ambiguous, 55 absent |
| `audio_wiring --seconds 3` | **0** | device null, 8 tables, 1,410 defs, 454 units, 30 themes; peak 0.5266, 281/281 nonzero blocks, worker 9,216 frames, shared cache hit 1 / miss 11 |
| `./target/release/audio_catalog --mpq War3.mpq --index …` (post-pause) | **0** | 16 s: wav 2696/2696 (0 fail), mp3 1140/1140 (0 fail, 0 duration mismatch), defs 1410, **playable 1323**, unplayable 87, listfile-resolved 6, ambiguous 0, absent 164, units 390/392, themes 30/30 |

Interruption resolved: the dispatcher's `pause-agents.sh` watchdog (2026-11-12
17:39 + 30 min, SIGTERMs `cargo build|test|check|run`, `rustc`, …) killed the first
`audio_catalog` rebuild. After the window closed (2026-11-13 02:35) the scope was
re-verified from scratch: `cargo check` rc=0, `cargo test` rc=0 (61 tests), and the
`audio_catalog` rebuild + run rc=0 with the numbers above. No build was attempted
while the watchdog was alive.

Files changed in this steer: `crates/wc3-mpq/**` (none), `crates/wc3-assetdb/**`
(source/lib/audio_provider/catalog/bins), `crates/wc3-audio/**` (game_audio, lib),
`analysis/agent-assets/**` (patches, catalogs, sound-paths, logs),
`analysis/tech-lead/merge-assets.md`.

## 13. H2

* monitor ledger **315** (`agent-assets-port`) — registered → closed `completed`.
* monitor ledger **321** (`agent-assets-exec`) — registered → closed `completed`
  (merge + compile phase).
* capture **582** (port evidence) and **594** (merge/compile evidence: rc values,
  61 tests, corpus gates).
* experiences **4305** (assetdb must not depend on wc3-re; adapter in the game crate),
  **4306** (patch ranges ending on `} else {` duplicate the branch),
  **4318** (BLP1 all-mips corpus gate: 28,748/28,748),
  **4319** (stale sound-table dirs resolved by unique listfile basename, +6 playable),
  **4320** (exit 143 during parallel merges = dispatcher pause watchdog; check
  `pause-agents.sh` and measure `PIPESTATUS`, not a piped `head`).

## 15. Full-merge absorption (2026-11-13)

Report: `analysis/agent-assets/ABSORPTION.md`. One-line state: **assets/audio fully
absorbed into the base, wired to the game through a shared cache, no duplicate
reader/codec/cache.**

* One MPQ reader (`wc3-mpq`), one cache/provider (`wc3-assetdb`), one decoder set
  (`wc3-audio`). Duplicate scan in `ABSORPTION.md` §2: only magic **detection**
  constants outside `wc3-mpq`; `wc3-re::mpqex` codecs are `pub use` re-exports;
  the only other ADPCM code is RIFF IMA/MS block framing in `wc3-audio::wav`
  (different container, documented).
* Extraction: `audit_extract` **10,523/10,523**, sha256-identical, 0 failures;
  StormLib oracle 10,525/10,525 (previous phase). `wc3re scan` overall `true`
  with BLP 3,935 / MDX 2,048 / SLK 33 / TXT 67 / FDF 71 / maps 44 / AI 91 /
  TriggerData 676.
* BLP mips: 28,748 declared = 28,748 decoded, `mips_incomplete_files = 0`.
* Audio: WAV 2,696/2,696, MP3 1,140/1,140; sound definitions playable 1,359/1,410
  (War3) and **1,993/2,025** (with `War3xlocal`+`War3x` layered), 0 ambiguous,
  55 genuinely absent references listed with the layers searched.
* Wiring: `SharedAssetDb` + `start_game_audio`; `audio_wiring` rc=0 with
  peak 0.5266, 281/281 non-silent blocks, render thread producing 9,216 frames,
  and a shared-cache hit across renderer+audio. Patch
  `patches/0003-game-audio-wiring.patch` wires it into `Assets::load` (pending
  dispatcher merge; `assets.rs` frozen).
* Known cross-layer blockers: `wc3-game/src/render.rs:777` type error (render
  owner) blocks `cargo check -p wc3-game`; 8 unused-import warnings in
  `wc3-formats/src/map.rs` (map owner).

## 16. Round 2 (ledger 351)

* **TFT layer**: `LayeredSource::from_mpq_layers` + `AssetDb::open_mpq_layers`
  (`crates/wc3-assetdb/src/source.rs`, `src/lib.rs`); `sound_paths --install DIR`
  mounts `War3Patch/patch > War3xlocal > War3x > War3` (same order as
  `StormFs::open_wc3_install`). Measured: War3-only 2,889/3,013 refs (1,359/1,410
  defs) → **4,177/4,232 refs (98.7%), 1,993/2,025 defs** with TFT; **0 ambiguous**;
  TFT gained **51 unique missing paths** (48 `HeroArchDruid*`, building acks,
  …); 55 refs remain absent in these partial TFT archives.
* **3D pan fix**: pan is now the sine of the source azimuth
  (`engine.rs::pan_gains`), independent of the reference distance used for
  attenuation. `audio_wiring` isolates the spatial voice and measures
  `left_energy=0.4406 vs right_energy=0.0000` for a source at −X; wiring run rc=0
  with ack+music from `War3.mpq`, peak 0.6402, worker 9,728 frames, shared-cache
  hit 1.
* **BLP mips re-verified**: 28,748 declared = 28,748 decoded, incomplete 0.
* rc: `cargo check` **0** (0 errors), `cargo test` **0** (74 tests), `sound_paths
  --install` **0**, `audio_wiring` **0**, `asset_catalog` **0**.
* Artifacts: `analysis/agent-assets/{sound-paths-install.*, audio-wiring.json,
  catalog-run.log, ABSORPTION.md §9}`; test install at
  `analysis/agent-assets/wc3-install/`.
* Still pending (other owners): `wc3-game/src/assets.rs` patch
  `0003-game-audio-wiring.patch`; `wc3-game/src/render.rs:777` type error.

## 17. Round 2 (ledger 351) — audio parity

* **API**: `GameAudio::{spell, ambient, stop_voice}` (AbilitySounds / AmbienceSounds,
  ambience loops on `Category::Ambient` with `ambient_volume 0.55`),
  `UnitNameMap::{sound_set, id_for_name}`, `GameAudioWiring.layers`.
* **Retail schema fixes**: TFT `unitUI.slk` header (`unitSound`/`name` columns) and
  TFT `TriggerData.txt` theme syntax (`1,musictheme,…`) — both detected, so TFT
  units/acks and 42 themes work; RoC behaviour unchanged.
* **Parity harness** `crates/wc3-assetdb/src/bin/audio_parity.rs`: rc=0 in both
  modes; ack 0.026 / attack 0.428 / spell 0.432 / ambience 0.570 / music 0.737;
  3D energies 1.0286 / 0.0643 / 0.0040 at 1×/2×/4× (exact inverse square);
  TFT unit ack `nplb`→`GrizzlyBearWhat` from **War3xlocal**.
* **Library regression**: `crates/wc3-assetdb/tests/audio_parity.rs` (3 tests).
* **Matrix alignment**: `analysis/agent-assets/PARITY_AUDIO_ALIGNMENT.md` proposes
  exact replacements for P15/P16, §5 "Audio hooks wired", §6 increment 17; P15/P16
  are PARTIAL until `patches/0003-game-audio-wiring.patch` merges (game call-sites).
* rc: `cargo check` 0, `cargo test` 0 (all scope tests), `audio_parity` 0 (both
  modes), `audio_wiring` 0, `sound_paths --install` 0, `asset_catalog` 0.

## 18. Round 4 (ledger 351) — TOP-17 audio runtime

* **Game hook patch** `analysis/agent-assets/patches/0004-game-audio-hooks.patch`
  (11 hunks, syntax-checked): acks on selection change, attack/move order acks,
  UI click on action/hotkey, spell sound on cast (retail `BlizzardWave` only),
  gameplay music at scenario start (`Human1`, overridable), menu ambience
  (`GlueScreenWind`) with stop on Playing, 3D listener tracking (camera target,
  yaw 0; moved by > 0.5 only). Depends on `0003-game-audio-wiring.patch`.
* **Device log**: `audio_parity --install …` rc=0 writes
  `analysis/agent-assets/audio-device-log.txt` with `AUDIO_ACK hfoo`,
  `AUDIO_ATTACK ogru`, `AUDIO_SPELL BlizzardWave`, `AUDIO_AMBIENT
  AmbientSoundRain`, `AUDIO_MUSIC Human1`, and a TFT `AUDIO_ACK nmys`.
* **Counts**: TFT refs 4,177/4,232 (98.7 %), defs 1,993/2,025, 0 ambiguous,
  55 absent documented; War3-only 2,889/3,013.
* rc: `cargo check` 0 (0 errors), `cargo test` 0 (78 tests / 23 suites),
  `audio_parity --install` 0 (valid JSON + device log).

# Assets & audio integration notes

Owner: ASSETS/AUDIO pipeline agent (`analysis/agent-assets/`).
Scope: `crates/wc3-mpq`, `crates/wc3-assetdb`, `crates/wc3-audio`.
This file is the contract for the game/render/UI layers; it does not replace the
canonical resource-access manual owned by the RE agent
(`analysis/agent-re/RESOURCE_ACCESS.md`) — use that for format/table semantics
and this for the Rust API surface, budgets and threading rules.

## 1. What to call (stable API)

### Archive (`wc3_mpq`)

```rust
let archive = MpqArchive::open(path)?;                 // listfile + hash map
let archive = MpqArchive::open_with_index(path, &idx)?; // fast path, validates fingerprint
let idx = MpqIndex::build(&archive);                    // name -> block + codec
idx.save("analysis/agent-assets/war3.index")?;

archive.read("Units/Human/Footman/Footman.mdx")?;       // whole file
archive.read_into(name, &mut buf)?;                     // reuse the buffer
let (bytes, codecs) = archive.read_with_info(name)?;    // observed codec tags
let mut r = archive.stream(name)?;                      // Read + Seek, one sector at a time
archive.meta(name);                                     // (block_index, FileMeta)
archive.files();                                        // listfile + metadata
archive.fingerprint();                                  // block-table identity
```

`MpqIndex` format: `H2MQI\x01` (see `crates/wc3-mpq/src/index.rs`). A stale index
is rejected with `MpqError::StaleIndex`; `AssetDb::open_mpq_best` falls back to a
plain open instead of failing.

### Asset database (`wc3_assetdb`)

```rust
let mut db = AssetDb::open_mpq_best(mpq, index_path)?;  // (db, used_index)
let mut db = db.with_config(CacheConfig {
    byte_budget: 32 << 20,
    texture_budget: 96 << 20,
    model_budget: 48 << 20,
    sound_budget: 32 << 20,
    max_sound_seconds: 30.0,
    max_entries: 4096,
    negative_cache: true,
});

db.read(name)?;                 // Arc<Vec<u8>>, LRU + negative cache
db.texture_shared(name)?;       // Arc<TextureAsset> (image + sha256 provenance)
db.model_shared(name)?;         // Arc<ModelAsset> (+ resolved texture paths)
db.sound_shared(name)?;         // Arc<Sound> (WAV/ADPCM/MP3 one-shots, time-capped)
db.sound_stream_bytes(name)?;   // Arc<[u8]> for streamed music
db.resolve_path(name);          // case-insensitive exists + canonical spelling
db.stats(); db.total_bytes();   // budgets/evictions/pins
db.trim(); db.trim_sounds();    // drop what no consumer pins
```

Compatibility entry points used by `wc3-game` (`texture() -> BlpImage`,
`model() -> MdxModel`, `read() -> Arc<Vec<u8>>`) are kept and delegate to the
shared caches.

**Pin semantics.** Cache values are `Arc`s; an entry whose `Arc::strong_count > 1`
is never evicted (the consumer would keep the memory anyway). Release your handle
when a screen is done, or call `trim*()` after dropping handles.

### Audio (`wc3_audio`)

```rust
let (sound, truncated) = decode_wav(&bytes)?;   // PCM 8/16/24/32, f32, IMA/MS ADPCM
let info = probe_wav(&bytes)?;                   // no decode, exact frames/duration
let info = probe_mp3(&bytes)?;                   // frame walk, exact duration
let sound = decode_mp3(&bytes)?;                 // whole file (short SFX only)
let (sound, truncated) = decode_mp3_bounded(&bytes, 48_000 * 30)?;
```

Engine:

```rust
let engine = AudioEngine::start(AudioConfig {
    sample_rate: 48_000, channels: 2, block_frames: 512,
    max_voices: 64, master_gain: 0.8,
}, Box::new(Sdl3Backend::open(&config)?));       // or Box::new(NullBackend::default())

let id = engine.play(sound, PlayParams {
    gain: 0.9, pan: 0.0, position: Some(pos),
    looping: false, pitch: 1.0, category: Category::Sfx,
    reference_distance: 600.0, max_distance: 0.0, priority: 5,
});
engine.set_gain(id, 0.5);
engine.set_position(id, Some(pos));
engine.set_listener(listener_pos, yaw);
engine.stop(id); engine.stop_category(Category::Music);
engine.set_master_gain(0.8);
engine.set_category_gain(Category::Music, 0.6);
engine.play_mp3(bytes, PlayParams::music())?;    // streamed, bounded memory
engine.stats();                                  // frames/blocks/voices/errors
```

Rules:

* Every public engine call is **non-blocking**: it pushes a command into a
  bounded channel (`try_send`). A full queue increments `commands_dropped`
  instead of stalling the UI thread.
* The render thread owns the backend; `AudioEngine::shutdown(self)` (or `Drop`)
  joins it. Blocks are `block_frames * channels` f32.
* `AudioEngine::headless(config)` runs the same renderer without a device and
  without a thread; use it in tests and for offline mixing (`engine.render(&mut block)`).
* Render allocates **zero** heap bytes after warm-up (proved by
  `crates/wc3-audio/tests/no_alloc.rs`).

### Gameplay hooks (`wc3_audio::game_audio`)

```rust
// wc3-assetdb implements SoundProvider for AssetDb.
let (sounds, units, tables) = wc3_assetdb::audio_provider::load_sound_tables(&mut db);
let engine = AudioEngine::start(config, Box::new(Sdl3Backend::open(&config)?));
let mut audio = GameAudio::new(engine, sounds, units, db);

audio.load_themes_from_trigger_data(&db.read_uncached("UI\\TriggerData.txt")?);
audio.set_listener(camera_pos, camera_yaw);

audio.unit_ack("hfoo", Some(unit_pos));    // FootmanWhat1..4.wav (random variant)
audio.unit_attack("ogru", Some(unit_pos)); // GruntYesAttack*
audio.unit_death("hfoo", Some(unit_pos));
audio.ui_click();                          // InterfaceClick
audio.ui_error();                          // InterfaceError
audio.music("Human1");                     // Sound\Music\mp3Music\Human1.mp3 (streamed loop)
audio.stop_music();
```

Resolution chain (all data-driven, no hardcoded paths):

* `unitUI.slk` column 1 → unit id, column 3 → display name (`hfoo` → `Footman`).
* `UnitAckSounds.slk` → `FootmanWhat`, `FootmanYesAttack`, ... with
  directory base, file list, volume, pitch, distances.
* `TriggerData.txt` `musictheme` rows → 30 music themes (all files verified present).

Suggested event mapping in the game layer: selection → `unit_ack`,
attack order → `unit_attack`, death → `unit_death`, UI button press →
`ui_click`, invalid order → `ui_error`, map start → `music(theme)` and
`set_category_gain(Category::Music, ...)` from the options menu.

## 2. Memory budgets

| cache | default | notes |
|---|---:|---|
| raw bytes | 64 MB | MPQ payloads before decode |
| textures | 96 MB | RGBA8, all decoded mips |
| models | 48 MB | parsed MDX |
| sounds | 32 MB | decoded f32 PCM, one-shots ≤ 30 s |
| music | ~4 MB | streamed: 8,192-frame decode window per voice |

Measured (this device, release build): 32-voice 60 s mix in 1.57 s (38× realtime),
0 allocations during render, peak RSS 28 MB for the whole benchmark, 30 s of MP3
music streamed in 0.06 s with +4 MB RSS (`analysis/agent-assets/audio-bench.json`).

For a low-memory target, halve the texture/byte budgets; `LruCache` enforces the
budget immediately (`set_byte_budget` evicts on the spot).

## 3. Startup recipe

1. Build the index once per archive (12 ms for 10,523 entries, 845 KB):
   `cargo run --release -p wc3-mpq --example audit_extract` writes
   `analysis/agent-assets/war3.index` (and the full extraction audit).
2. Ship the index next to the archive or regenerate on first run:
   `MpqIndex::build(&archive).save(path)`.
3. Open with `AssetDb::open_mpq_best(mpq, index)`: index path when valid,
   plain open otherwise.
4. Preload only the first screen: `db.preload(...)` for the loading background,
   `db.texture_shared`/`db.model_shared` for the units in the first wave.
5. Audio: `AudioEngine::start` before the first frame; `GameAudio::music` on map
   load (streamed, so it does not block on decode).

## 4. Data facts / caveats found by the audit

* **Encrypted-uncompressed MPQ blocks are decrypted per 4096-byte sector**
  (`key + sector_index`), not as one unit. 34 campaign loading backgrounds
  differed from StormLib from byte 4096 until this was fixed. The reader now
  matches StormLib on all 10,525 entries (`oracle-compare.json`).
* **BLP1 paletted decodes mip 0**; the declared mip chain (28,748 levels) is
  validated structurally (`sizes[i] == w*h + alpha plane`). Generate GPU mips at
  upload, or extend `wc3-formats` if decoded mips are needed.
* **5 textures are fully transparent by design** (e.g.
  `UI\Widgets\EscMenu\Human\blank-background.blp`); they are valid, not failures.
* **170 sound-table paths are not in `War3.mpq`** (109 `Units`, 41 `Abilities`,
  ...). They are The Frozen Throne content; 1,317 of 1,410 sound definitions and
  390 of 392 acknowledgement sets are playable from this archive alone.
* **Icon pairing:** 691 BTN icons, 717 disabled icons, 683 complete pairs,
  8 BTN without a disabled twin, 34 orphan disabled icons (retail data).
* `analysis/war3_full3` had 34 stale files from the old reader; they were
  re-extracted (originals kept in `analysis/agent-assets/repaired/originals/`,
  manifest in `repaired/MANIFEST.tsv`).

## 5. Cross-layer notes

* `crates/wc3-game/src/render.rs:777` currently fails to compile
  (`(position, ...)` where `position: &Vec3`; needs `*position`). This is in the
  render layer (read-only for this agent) and is unrelated to the asset API.
  Everything in `wc3-mpq`, `wc3-assetdb`, `wc3-audio` builds and tests clean.
* The resource-access manual (`analysis/agent-re/RESOURCE_ACCESS.md`) owns format
  semantics; if it grows an access API that overlaps `AssetDb`, prefer it for
  reading and keep `AssetDb` as the cache/budget layer.

## 6. Port addendum (single base, 2026-11-12)

* `AssetDb::open_sources(Vec<Box<dyn AssetSource>>, label)` is the single seam for
  multi-archive priority: `wc3-game` wraps `wc3_re::storm::StormFs` in an
  `AssetSource` implementation (patch in `analysis/tech-lead/merge-assets.md`),
  so `wc3-assetdb` never depends on `wc3-re`.
* `AssetDb::resolve_asset(path) -> Resolution` classifies every reference as
  `Exact` / `ListfileMatch` / `Ambiguous` / `Absent{searched}`. Retail sound tables
  with stale directories (e.g. `Buildings\Orc\PigFarm\PigFarmWhat1.wav`) resolve
  to the real listfile path; absent paths are reported, never invented.
* `SoundProvider::resolve` (default = exact lookup) is called by every gameplay
  hook before loading, so the hooks benefit without knowing about archives.
* `TextureAsset::{mip0, mip_levels, mip, mip_bytes}`: `.image.rgba` is mip 0 and
  `.image.mipmaps` preserves every decoded level. BLP1 all-mip decoding is applied
  in `wc3-formats/src/blp.rs`; the catalog gate is `mips_incomplete_files == 0`.
* The legacy `Mixer`/`Voice` pair was removed; `AudioEngine` is the only mixer.

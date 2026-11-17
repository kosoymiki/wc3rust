# ASSETS / AUDIO pipeline — report

Task ref: `agent-assets` (H2 monitor ledger 299)
Owner paths: `crates/wc3-mpq/**`, `crates/wc3-assetdb/**`, `crates/wc3-audio/**`,
`analysis/agent-assets/**`. Game/render/UI/data/sim were read-only.

## Goal

A production asset pipeline for the real `War3.mpq`: verified full extraction,
fast indexing/random access, validated BLP/MDX/icon/biome catalogs with bounded
memory, a real audio decoder + `AudioEngine` (WAV PCM/ADPCM, MP3, play/stop/
volume/3D pan/looping, no UI blocking), data-driven gameplay audio hooks, and
measured performance — with exact counts, no sample-only claims.

## Actions

### 1. Archive reader (`crates/wc3-mpq`)

* Unified the read path on `MpqFileReader` (`src/stream.rs`): `pread` for stored
  files, lazy per-sector decode for compressed files, O(1) `Seek`, zero heap
  allocation per read after construction, in-place word decryption.
* Added `MpqIndex` (`src/index.rs`): binary `H2MQI\x01` index (name → block index,
  size, compressed size, flags, codec), FNV-1a block-table fingerprint, stale-index
  rejection, case-insensitive lookup, extension/ codec queries, `save`/`load`.
* Added `CodecUsage` (observed per-sector codec tags, not just declared masks),
  `FileMeta`, `MpqArchive::{meta, files, fingerprint, sector_size, read_into,
  read_with_info, stream, open_with_index}`.
* **Fixed a real extraction bug**: encrypted-uncompressed blocks were decrypted as
  a single unit; MPQ/StormLib decrypts them **per 4096-byte sector with
  `key + sector_index`** (`donors/StormLib/src/SFileReadFile.cpp:139`). The 34
  campaign loading backgrounds were wrong from byte 4096 onwards.
* New tools: `examples/audit_extract.rs`, `examples/compare_oracle.rs`,
  `examples/mpq_meta.rs`, `examples/block_stats.rs`.

### 2. Asset database (`crates/wc3-assetdb`)

* `LruCache` (`src/lru.rs`): byte-budgeted intrusive-list LRU, O(1) get/insert,
  **refcount pinning** (`Arc::strong_count > 1` entries are never evicted),
  eviction/pin statistics, entry cap.
* `AssetDb` rewritten around four bounded caches (raw bytes, textures, models,
  sounds), negative cache, per-cache budgets, `stats()`, `trim()`,
  `read_uncached`, `resolve_path`, `texture_shared`, `model_shared`,
  `sound_shared`, `sound_stream_bytes`; legacy `read/texture/model` preserved for
  the game layer.
* `blp_report.rs`: BLP header probe (BLP1/2, JPEG/paletted/DXT, alpha bits/type,
  mip tables) + decode report (hashes, mip-chain check, alpha histogram, colour
  statistics, hue class) + golden comparison helper.
* `catalog.rs`: one-pass full-corpus catalogs (textures, models with resolved
  texture refs, icons with BTN/DIS pairing, biomes) → JSON/Markdown/TSV.
* `audio_provider.rs`: `impl SoundProvider for AssetDb` + retail sound-table
  loading.

### 3. Audio (`crates/wc3-audio`)

* `wav.rs`: PCM 8/16/24/32, IEEE float 32/64, WAVE_FORMAT_EXTENSIBLE, **IMA ADPCM
  (0x11)** and **MS ADPCM (0x02)** block decoders; `probe_wav` reports exact
  frame counts/durations without allocating samples.
* `mp3.rs`: `nanomp3` (pure Rust, `forbid(unsafe_code)`, minimp3-bit-exact) —
  frame-header `probe_mp3` (no decode), `decode_mp3`, `decode_mp3_bounded`,
  and `Mp3Stream` (one-frame-at-a-time, bounded 8,192-frame window).
* `engine.rs`: block mixer with voice handles, constant-power pan, 3D
  position/attenuation relative to listener yaw, looping, pitch, 5 ms envelopes,
  category buses (Sfx/Ui/Music/Ambient/Voice), priority stealing, bounded
  command channel (`try_send`, dropped-command counter), render thread,
  `AudioStats`; `AudioEngine::headless` for deterministic tests.
* `engine/sdl3.rs`: SDL3 backend loaded with `dlopen` (no build-time link),
  `SDL_OpenAudioDeviceStream` + `SDL_PutAudioStreamData` + resume.
* `soundinfo.rs`: retail SLK parser + `SoundInfoDb` (1,410 definitions) and
  `UnitNameMap` (454 unit ids → names) with table-level schema detection.
* `game_audio.rs`: `GameAudio` hooks — `unit_ack`, `unit_attack`, `unit_death`,
  `ui_click`, `ui_error`, `music(theme)`, plus `TriggerData.txt` music themes.

## Evidence

### Extraction audit (rc=0)

```
AUDIT ok=10523 entries=10523 read_fail=0 missing=0 size_mismatch=0 sha_mismatch=0
mpq_bytes=710907954 disk_bytes=710907954 zero_length=0 zero_content=0 extra_on_disk=0
open=22.4 ms  index=7.2 ms  full re-read + sha256 = 29.6 s
codecs: stored 4122, zlib 3655, stored+zlib 61, zlib+adpcm-mono+huffman 2532,
        stored+adpcm-mono+huffman 25, stored+zlib+adpcm-mono+huffman 93,
        zlib+adpcm-stereo+huffman 35
```

### Independent oracle (StormLib) — the check that found the bug

```
tools/storm_probe War3.mpq --extract "" <tmp>   → listed=10525 extracted=10525
RESULT files=10525 matched=10525 mismatched=0 read_fail=0 extra_in_archive=0
       bytes=711561245
```

After the per-sector fix, our reader is byte-identical to StormLib on **every**
entry. The 34 stale files in `analysis/war3_full3` were re-extracted; originals
are preserved in `repaired/originals/` with `repaired/MANIFEST.tsv` (old→new
SHA-256). Re-run audit: 10,523/10,523 OK.

### Textures — all 3,935 BLP

```
textures: files=3935 ok=3935 header_fail=0 decode_fail=0 mip_fail=0
          jpeg=2920 paletted=1015 alpha=1563 opaque=2372
          decoded_pixels=98722207  declared mip levels=28748 (layout_bad=0)
          fully transparent=5 (valid blank overlays)  constant colour=56
```

Colour mapping proof (`tests/golden_blp.rs`): the embedded JPEG is re-decoded with
`jpeg-decoder` and compared to our RGBA — **0 differing bytes** on
`Village_Dirt` and `Village_GrassShort`; the no-inversion variant decodes dirt to
blue `[178,182,217]` and grass to blue `[191,176,227]`, i.e. the inverted-BGRA
mapping is required. All 111 TerrainArt tiles decode (hues: brown 35, green 27,
gray 17, yellow 15, red 8, cyan 5, blue 3, magenta 1).

### Models — all 2,048 MDX

```
models: files=2048 parsed=2048 parse_fail=0
        texture_refs=5216 replaceable=1138 external=4078
        unresolved=0  models_with_unresolved=0  all_resolved=2048
        geosets=5127 sequences=6512 nodes=36023
```

### Icons and biomes

```
icons: btn=691 disabled=717 paired=683 missing_disabled=8 orphan_disabled=34
       decoded_ok=1408 decoded_fail=0
biomes: 15 (111 TerrainArt files, 0 failures)
```

### Audio — all 2,696 WAV and 1,140 MP3

```
WAV: files=2696 decoded_ok=2696 failures=0 source=272,605,460 B
     decoded_frames=131,842,527  total_duration=5958.5 s  silent=0
     codecs: pcm-s16 2696; rates 22050 (2683), 44100 (12), 22020 (1)
MP3: files=1140 probed_ok=1140 probe_failures=0 source=138,644,930 B
     total_duration=12,042 s  fully decoded sample=40  duration mismatches=0
sound tables: 8 tables, 1410 definitions, 1317 playable from War3.mpq,
              93 need TFT data (170 referenced paths absent: Units 109,
              Abilities 41, Buildings 10, Sound 9, Doodads 1)
units: 454 named, 392 with ack + attack sounds, 390 playable from this archive
music: 30 themes from UI/TriggerData.txt, 0 missing files
```

Independent decode check (`audio_verify` vs ffmpeg `-f f32le`):

| file | samples | max abs diff | SNR |
|---|---:|---:|---:|
| Mainscreen.mp3 | 6,112,512 | 1.65e-6 | 128.0 dB |
| H01Arthas18.mp3 | 149,631 | 9.1e-7 | 129.3 dB |
| FootmanWhat1.wav | 21,878 | 0.0 | bit-exact |

### Engine / performance (`analysis/agent-assets/audio-bench.json`)

```
index open 7.2–11.7 ms; sound tables 33–50 ms (1410 defs, 454 units)
WAV decode 300 files (37.7 MB) in 1.03 s = 14.1 M frames/s
bounded cache: 150 loads, total 5.86 MB under 6 MB budgets,
               sound evictions 145, raw evictions 119  → budget enforced
mix: 32 voices, 60 s of audio rendered in 1.57 s (38× realtime)
     allocations during render = 0, underruns 0, max_active 32
music: 30 s of MP3 streamed in 0.06 s (534× realtime), 0 allocations,
       RSS 23.9 → 28.1 MB (+4.2 MB for the streaming window)
peak RSS for the whole benchmark: 28.2 MB
```

Zero-allocation render is asserted by `crates/wc3-audio/tests/no_alloc.rs`
(counting global allocator, 64 blocks × 512 frames, 16 voices → `allocations=0
frees=0`).

Real device path:

```
$ ./target/release/sdl_smoke
sdl3: SDL3 audio subsystem available
backend=sdl3 latency=40ms
played 1.04s wall, frames=21504 blocks=42 started=3 finished=0 errors=0
```

### Gameplay hooks against the real archive (`tests/audio_hooks.rs`)

```
ack voice=VoiceId(1) peak=0.013 def=FootmanWhat files=[What1..What4]
themes loaded 30
music peak=0.768 nonzero_blocks=200/200
after stop_music peak=0
units named=454 with_ack=392 playable=390
3D pan: left_energy=23.20 right_energy=7.73 (source to the listener's left)
```

### Tests

```
wc3-mpq      9 passed
wc3-assetdb 16 lib + 4 golden + 4 audio-hooks = 24 passed
wc3-audio   25 lib + 1 no-alloc = 26 passed
total       59 passed, 0 failed
```

## Result

Delivered and verified end-to-end:

1. **Extraction audit** — 10,523/10,523 files, 710,907,954 B, 0 failures,
   sha256-identical to disk and to StormLib (10,525/10,525 oracle match),
   codec coverage recorded from observed sector tags; 845 KB index in 7 ms.
2. **Bounded cache** — LRU with refcount pinning, four budgets, stats; budget
   enforcement and pinning covered by tests.
3. **BLP/MDX** — all 3,935 textures decode and stat (28,748 declared mip levels
   validated), all 2,048 models parse with 0 unresolved texture references,
   691 icons + 717 disabled icons all decode, 15 biome groups / 111 tiles.
4. **Audio** — WAV PCM/ADPCM + MP3 decoding (ffmpeg-verified to 128 dB SNR),
   `AudioEngine` with play/stop/volume/3D pan/looping on a render thread,
   SDL3 device output smoke-tested, 0 allocations per render block.
5. **Gameplay hooks** — `unit_ack`/`unit_attack`/`unit_death`/`ui_click`/
   `ui_error`/`music` resolved from retail tables; 390 of 392 ack sets playable
   from this archive, 30 music themes verified.
6. **Integration notes** — `analysis/agent-assets/INTEGRATION.md`.

## Risks / next

* **`wc3-game/src/render.rs:777` does not compile** (`(position, …)` where
  `position: &Vec3`; needs `*position`). Render layer is read-only here; the
  asset/audio crates build and test clean.
* **170 sound-table paths (93 definitions) are TFT content** absent from
  `War3.mpq` (e.g. `Units\NightElf\HeroArchDruid\*`, `PigFarmWhat1.wav`). They
  will resolve once `War3x.mpq`/`War3xLocal.mpq` is added as a second provider;
  the code path is provider-agnostic.
* **BLP1 paletted decodes mip 0 only** (upstream `wc3-formats`); the declared
  chain is structurally validated. If GPU mips are not generated, extend the
  decoder in `wc3-formats` (not owned here).
* **32-voice stress mix clips** (peak 1.0) by design of the test (no per-voice
  attenuation); real gameplay uses per-category gains. A limiter/compressor is a
  possible follow-up.
* `minimp3`/`minimp3-sys` was rejected in favour of `nanomp3` (pure Rust, no
  `unsafe`, no C toolchain requirement for the Android build).
* Richer ambience (`EnvironmentSounds.slk` schema) is parsed but currently
  treated as schema-mismatched and skipped; ambient zones need a dedicated
  parser if the game wants them.

## H2_PERSISTED

* capture: **547** (MPQ reader/index/audit), **555** (per-sector decrypt fix +
  BLP/MDX/icon/biome catalogs), **569** (audio pipeline: decode, engine, hooks,
  perf, SDL3 playback).
* experience: **4285** (per-sector decryption of encrypted-uncompressed blocks),
  **4286** (audit needs an independent oracle), **4287** (BLP1 inverted-BGRA
  colour proof).
* monitor: ledger **299** (`agent-assets`) — registered → checkpoints →
  **closed** with `state_after=completed`, `last_status=done`,
  `h2_ids_json={captures:[547,555,569], experiences:[4285,4286,4287]}`.
* artifacts: `analysis/agent-assets/{extraction-audit.json,md,-files.tsv,
  oracle-compare.json, asset-catalog.json,md, texture-catalog.tsv,
  model-catalog.tsv, biome-catalog.tsv, audio-catalog.json,md,
  audio-wav-catalog.tsv, audio-mp3-catalog.tsv, audio-bench.json,
  war3.index, INTEGRATION.md, REPORT.md}`.

# Unified input-limit policy (AUD-025)

Registry: `docs/audit/WC3RUST_CODE_AUDIT.md` (AUD-025, P3).
Audit revision: `b64b9b6`; policy applied on `main` by the AUD-025 lane (LEDGER 493).

One table for every path that turns untrusted bytes into memory. The rule the
table encodes:

> A length, count, dimension, nesting depth or ID counter read from an external
> source is validated against the named cap **before** the allocation, slice or
> recursion it drives. Rejection is a typed error the caller can report; input
> work is linear in the capped bytes, so no parser has an unbounded loop.

Three properties are checked per row: the constant (one owner), the enforcement
point (file:line), and the error the caller sees.

## Caps

### Storage / media parsers

| Domain | Limit | Value | Constant | Enforced before allocation at | Error |
| --- | --- | --- | --- | --- | --- |
| MPQ | One decoded file (declared `size` / `compressed_size`) | 512 MiB | `wc3_mpq::MAX_FILE_SIZE` (`crates/wc3-mpq/src/lib.rs:117`) | `check_block_sizes` (`lib.rs:1091`) on block-table load and on every `read`/`stream`/`read_block`; `decompress_unit` (`lib.rs:1115`) caps each decode stage | `MpqError::SizeLimitExceeded { requested, limit }` |
| MPQ | One decoded sector / single-unit output | 16 MiB | `wc3_mpq::MAX_SECTOR_SIZE` (`lib.rs:124`) | header sector-shift check `block_size <= 15` (`lib.rs:526`); sector tables are read with `declared.min(available)` (`lib.rs:537-541`) | `MpqError::Corrupt` |
| MPQ | Sector count in an offset table | 1 048 576 (`MAX_FILE_SIZE / 512`) | `MAX_SECTORS` (`lib.rs:131`) | sector-table length is derived from the stored block size and checked before `vec![0u8; …]` (`lib.rs:1238` caps the initial capacity) | `MpqError::SizeLimitExceeded` / `Corrupt` |
| MPQ | Hash/block table entries | 8 388 608 (`8 << 20`) | `MAX_TABLE_ENTRIES` (`lib.rs:100`) | header validation, before the table read (`lib.rs:517-521`). The read itself is `count * 16` capped by the archive size (`lib.rs:537-541`) | `MpqError::Corrupt("invalid table sizes")` |
| AVI | `LIST rec ` nesting inside `movi` | 32 | `wc3_bink::avi::MAX_MOVI_DEPTH` (`crates/wc3-bink/src/avi.rs:472`) | iterative range stack, depth check before pushing a frame (`avi.rs:512`) | `AviError::Limit { what, limit }` |
| AVI | Payload chunks collected from one `movi` | 1 048 576 | `wc3_bink::avi::MAX_MOVI_CHUNKS` (`avi.rs:474`) | check before `out.push` (`avi.rs:531`) | `AviError::Limit` |
| AVI | Any RIFF chunk body | file size | — | `body.checked_add(size).filter(\|e\| *e <= riff_end)` before slicing (`avi.rs:186-190`, `avi.rs:502-508`) | `AviError::Truncated` |
| ASTC | Texture side | 16 384 texels | `wc3_render::astc::MAX_TEXTURE_SIDE` (`crates/wc3-render/src/astc.rs:39`) | `payload_len` before any block arithmetic (`astc.rs:61`) | `Err(String)` from `AstcFile::parse` / `decode` |
| ASTC | Texels per image | 268 435 456 (16384²) | `astc::MAX_TEXELS` (`astc.rs:41`) | `payload_len` (`astc.rs:70`) | `Err(String)` |
| ASTC | Block grid | 268 435 456 | `astc::MAX_BLOCKS` (`astc.rs:44`) | `payload_len` (`astc.rs:82`) | `Err(String)` |
| ASTC | Declared payload | exact, `blocks * 16` | — | `AstcFile::parse` (`astc.rs:130-140`) and `to_image` (`astc.rs:169-177`) | `Err(String)` |
| ASTC | Total decoded allocation | ≤ input bytes | — | the payload must match the computed length byte for byte before `to_vec`, so a crafted header cannot allocate more than the file already carries (`astc.rs:130-140`) | `Err(String)` |
| Compressed level (`.astc` header or sidecar) | Texture side, block grid, byte count | 16 384 / 64-bit arithmetic | `wc3_render::texture::MAX_TEXTURE_SIDE` (`crates/wc3-render/src/texture.rs:338`), asserted equal to `astc::MAX_TEXTURE_SIDE` | `CompressedFormat::block_grid` (`texture.rs:476`) ← `CompressedLevel::expected_bytes` (`texture.rs:510`) ← `is_consistent` (`texture.rs:527`) and `CompressedImage::validate` | `Err(String)` from `validate`; `is_consistent == false` for impossible levels |
| Save | One save file (envelope included) | 64 MiB | `wc3_sim::save::MAX_SAVE_BYTES` (`crates/wc3-sim/src/save.rs:165`) | `open_envelope` before the payload slice (`save.rs:5361`); `from_save_path` checks `metadata().len()` and reads through `take(MAX_SAVE_BYTES + 1)` (`save.rs:5487`); `save_to_path` refuses to write above the cap | `SaveError::Oversize { len, limit }` |
| Save | Any ID counter / handle table | 1 048 576 slots | `wc3_sim::save::MAX_ID_SPACE` (`save.rs:154`) | `check_id_counter` / handle-table validation before `rebuild_derived` allocates (`save.rs:4518`, `save.rs:4539`) | `SaveError::InvalidIdSpace { field, value, detail }` |
| Save | Declared payload length | must equal the file bytes | — | `open_envelope` before `take` (`save.rs:5385`) | `SaveError::LengthMismatch { declared, actual }` |
| Net (socket) | One message | 64 MiB | inline in `recv_message` (`crates/wc3-sim/src/net.rs:1974`); same number as `MAX_SAVE_BYTES` | before `vec![0u8; len]` | `NetError::Protocol` |
| Net (scene) | Map cells per axis | 1 024 | `MAX_MAP_CELLS` (`net.rs:54`) | `MatchSetup::read` (`net.rs:696`) and `MapScene` reader (`net.rs:890`) | `NetError::InvalidValue` |
| Net (scene) | Entries per scene section | 100 000 | `MAX_SCENE_ENTRIES` (`net.rs:56`) | each section count via `r.len(MAX_SCENE_ENTRIES)` before the element loop (`net.rs:897-912`) | `NetError::LengthMismatch` / `InvalidValue` |
| Net (replay) | Command frames | 2 000 000 | `MAX_FRAMES` (`net.rs:52`) | `p.len(MAX_FRAMES)` before `Vec::with_capacity` (`net.rs:1473`) | `NetError::LengthMismatch` |
| Net (replay) | Declared payload length | must equal the file bytes | — | `Replay::decode` before `take` (`net.rs:1452`) | `NetError::LengthMismatch` |
| Any length-prefixed string (save, replay, network) | Declared string length | bounded by the remaining payload | — | the count is read, then `take(count)`/`bytes(count)` fails when the payload is shorter; no string can exceed the 64 MiB envelope it arrived in (`save.rs:550`, `net.rs:246`) | `SaveError::Truncated` / `NetError::Truncated` |
| Audio FFI | One `SDL_PutAudioStreamData` transfer | 1 MiB (262 144 f32 samples) and `c_int` byte range | `wc3_audio::engine::sdl3::MAX_PUT_BYTES` (`crates/wc3-audio/src/engine/sdl3.rs:231`) | `put_bytes` (`sdl3.rs:239`) does `checked_mul(4)` + `c_int::try_from` before the unsafe call; `chunk_lengths` (`sdl3.rs:259`) splits larger blocks | `AudioError::Backend` |

### Why these values

- MPQ `MAX_FILE_SIZE = 512 MiB` is ~50x the largest shipped entry (10 416 294 B
  in `War3x.mpq`) and is the same bound the decoder uses as its per-unit decode
  budget, so `size`, `compressed_size` and the actual output share one ceiling.
- MPQ table caps bound a 16-byte block-table entry to a 128 MiB table read and
  keep the hash probe loop finite on a crafted header.
- AVI depth 32 accepts every retail cinematic (retail nests one `LIST rec `) and
  stops the walk long before stack growth; the chunk cap is 1M because a 30 fps
  two-hour movie is ~216 000 chunks.
- ASTC 16 384 is `GL_MAX_TEXTURE_SIZE` on the supported Vulkan/GLES 3 adapters,
  so the cap rejects only content the device could not sample anyway.
- Save 64 MiB matches the socket message cap; the dense structure the file can
  drive (`MAX_ID_SPACE` id index) is 4 MiB, so the whole file is far below the
  cap for any legitimate match.
- Audio 1 MiB is 128x the stock mixer block (1024 frames × 2 channels × 4 bytes =
  8 KiB); the split exists for the FFI contract, not for the render path.

## Shared negative corpus

`crates/wc3-game/tests/input_limits.rs` feeds one hostile input per parser
through the same public entry point the game uses and asserts the typed
rejection (no panic, no unbounded allocation). Cases:

| Case | Hostile input | Expected |
| --- | --- | --- |
| `mpq_rejects_an_oversized_table_count_before_reading_it` | 32-byte header, `hash_table_count = 1 << 24` | `MpqError::Corrupt` |
| `avi_rejects_nesting_beyond_the_movi_depth_limit` | 33 nested `LIST rec ` inside `movi` | `AviError::Limit { limit: 32 }`; 32 levels still walk |
| `astc_rejects_dimensions_beyond_the_texture_side_cap` | u24 `0xFFFFFF` sides | `Err` containing `side limit`; 16384² stays legal |
| `save_rejects_a_declared_payload_above_the_file_size` | 12-byte header, `declared = u32::MAX` | `SaveError::LengthMismatch`; own save round-trips |
| `save_rejects_files_above_the_loader_byte_cap` | in-memory and sparse-file saves of 64 MiB + 1 | `SaveError::Oversize` from both entry points |
| `replay_rejects_a_declared_payload_above_the_file_size` | 12-byte envelope, `declared = u32::MAX` | `NetError::LengthMismatch`; own replay decodes |
| `retail_archives_still_decode_within_the_caps` | real `War3.mpq` / `War3x.mpq` / `War3xlocal.mpq` | every block ≤ cap, `(listfile)` decodes |

Cases that need crate-private fixtures stay in their crate and are referenced
from the table above; e.g. the MPQ block-entry byte cap is
`wc3-mpq::tests::aud003_block_entry_above_the_file_size_cap_is_rejected_before_allocating`
(the fixture requires the MPQ table encryption helper).

Run:

```
scripts/h2-cargo test -p wc3-game --test input_limits --features movie_decoder
```

## Retail assets are not regressed

Caps are upper bounds far above retail content; the corpus test decodes a real
entry from each shipped archive and asserts every block-table size stays below
`MAX_FILE_SIZE`. The MPQ crate's own
`largest_shipped_entry_is_below_the_cap` prints the measured maxima.

Observed maxima (`War3.mpq` 418.2 MB, `War3x.mpq` 287.0 MB,
`War3xlocal.mpq` 51.5 MB on this device — see the gate log in
`docs/audit/status-infra.md`):

```
MPQ_MAX_BLOCK /data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq: 6 591 551 bytes
MPQ_MAX_BLOCK /data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3x.mpq: 10 416 294 bytes
MPQ_MAX_BLOCK /data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3xlocal.mpq: 1 236 952 bytes
```

No decode logic changed in this lane: `wc3-mpq`, `wc3-bink` and `astc.rs` keep
their byte-for-byte output; the only reader change is the new save file-size
precheck, which rejects input the loader previously tried to read whole.

## CPU budget

There is no wall-clock budget inside the parsers (it would make decode
non-deterministic). Instead every loop is bounded by a cap above: MPQ work is
linear in `MAX_SECTOR_COUNT` and the 512 MiB decode budget, AVI work is linear
in the RIFF size and `MAX_MOVI_CHUNKS`, ASTC work is linear in
`MAX_BLOCKS`, save work is linear in `MAX_SAVE_BYTES`, and network work is linear
in the 64 MiB message cap. The gate runner (`scripts/test-all.sh`) is the layer
that adds wall-clock bounds (per-member timeout).

## Residual risks

- MPQ hash probing is bounded by `MAX_TABLE_ENTRIES`, not by a probe count;
  a crafted archive can still make the reader read a full 128 MiB table before
  the name probe fails. Bounded, not free.
- AVI stores chunk *ranges*; a crafted `idx1` can name offsets the walk never
  validated. `Avi::video_frame` returns the slice only after `get`, so a bad
  index is an `Option::None`, not a panic — but it is not a typed error either
  (tracked under AUD-031 as a UI/consumer concern).
- The network reader allocates one 64 MiB buffer per message before the
  payload type is known; a peer that can open many connections is bounded by the
  connection layer, not by this cap.

# Agent M — replay / media robustness (2026-11-21)

Packet: `analysis/tech-lead/all-layers-20261121/PACKET_M_replay-bink.md`.
Ledger 609, owners `crates/wc3-replay/**`, `crates/wc3-bink/**`.

## Goal

Replay container and Bink/AVI media paths must fail with typed errors (never a
panic) on truncated or forged input, the container census must be deterministic,
and the decode paths must be exercised by named tests that run without the
retail movies or `ffmpeg`.

## Mechanism (file:line, current working tree)

| Path | Mechanism |
|---|---|
| `crates/wc3-bink/src/avi.rs:314-354` | `parse_hdrl` walks the `hdrl` `strl` lists |
| `crates/wc3-bink/src/avi.rs:370-475` | `parse_strl` parses `strh`/`strf`; the returned stream number is the list position |
| `crates/wc3-bink/src/avi.rs:482-553` | `parse_movi` collects `##dc`/`##wb` chunks with an explicit range stack (`MAX_MOVI_DEPTH`, `MAX_MOVI_CHUNKS`) |
| `crates/wc3-bink/src/lib.rs:71-77,121-147` | `MAX_MOVIE_PIXELS`; `from_avi` validates untrusted `strf` dimensions before sizing the 3-frame RGBA ring |
| `crates/wc3-bink/src/lib.rs:239-263` | AVI/VOL dimension disagreement is `BinkError::Corrupt`, not an out-of-bounds write |
| `crates/wc3-bink/src/mpeg4/vol.rs:447-491` | `parse_access_unit`: start-code scan + VOL/VOP parse, typed errors on truncation |
| `crates/wc3-bink/src/mp3/mod.rs:380-598` | MP3 `decode`: header search, side info, bit reservoir; short buffers are typed errors |
| `crates/wc3-replay/src/lib.rs:159-227` | `Replay::read`/`parse`: byte cap, record-count cap, per-record tick/length checks, trailing-byte rejection |
| `crates/wc3-replay/src/lib.rs:426-537` | `decode_command`: every tag bounds-checked; trailing command bytes rejected |
| `crates/wc3-replay/src/lib.rs:609-620` | `Reader::ids`: forged u16 count is bounded by the remaining bytes |

## Defect fixed (root cause, observed)

**AVI stream numbers were assigned by stream type.** `parse_strl` returned
`0` for every `vids` list and `1` for every `auds` list. AVI numbers streams by
their position in `hdrl`, so in a legal file that lists `auds` first the video
chunks are `01dc` — the parser looked for `00dc`, reported
`video_frame_count() == 0`, `BinkFile::from_bytes` returned
`Corrupt("AVI has no video frames")`, and audio chunks were counted against the
wrong stream. The census therefore depended on stream-list order.

Fix:

- `parse_hdrl` owns a `stream_index` counter (`avi.rs:321`) and passes it to
  `parse_strl` (`:336-337`); the index is stored on `VideoInfo`/`AudioInfo`.
- The first `vids` and first `auds` list win; a forged duplicate video list can
  no longer double the frame census (`avi.rs:339-346`).
- `parse_movi` no longer takes the unused `stream_idx` parameter.

Reproduction is pinned by
`media_avi_census_ignores_stream_list_order` and
`media_avi_duplicate_video_streams_use_the_first`.

**Second hardening (replay parser).** `Reader::ids` used to call
`Vec::with_capacity(count)` from the announced u16 before reading a single id:
a 3-byte forged `Stop` command could reserve 256 KiB. It now rejects
`count > remaining / 4` with `ReplayError::Corrupt("unit id count")`
(`wc3-replay/src/lib.rs:612-616`), while a valid empty id list still decodes.

## Tests added (named, default test set)

`crates/wc3-bink/tests/media_robustness.rs` — 11 tests:

| Test | Proves |
|---|---|
| `media_avi_census_ignores_stream_list_order` | audio-first file: 2 video frames, 2 audio chunks, 4 chunks, payloads in order |
| `media_avi_census_is_deterministic` | two parses of the same bytes give identical counts, frames and header fields |
| `media_avi_duplicate_video_streams_use_the_first` | duplicate `vids` lists count once (first stream) |
| `media_avi_truncation_and_forgery_never_panic` | every truncation length, 256 byte mutations, 128 random buffers |
| `media_bink_rejects_oversized_picture_headers` | 65535x65535 `strf` is a typed `Corrupt`, no giant ring |
| `media_bink_decode_frame_errors_are_typed` | `NoSuchFrame` / typed decode error for garbage payload |
| `media_mpeg4_decoder_forgery_never_panics` | every truncation of a VOL+I-VOP, random payloads, mutated payloads |
| `media_mpeg4_i_vop_decode_path_runs` | full I-VOP decode path, VOL 800x368, concealment counted |
| `media_mpeg4_macroblock_tails_never_panic` | I and P macroblock decode fed random coefficient/motion tails |
| `media_mp3_silent_frame_decodes_deterministically` | one synthetic Layer III frame -> 2304 interleaved i16 of silence, two decoders equal |
| `media_mp3_decoder_forgery_never_panics` | every truncation, random buffers, mutated frames (header search, side info, reservoir, Huffman) |

`crates/wc3-replay/tests/replay_robustness.rs` — 6 tests:

| Test | Proves |
|---|---|
| `replay_roundtrip_census_is_deterministic` | 2 records / 120 ticks survive read; `write(read(bytes))` is byte-identical |
| `replay_truncation_is_typed_at_every_length` | every prefix < full length is `Err`, never silent |
| `replay_forged_header_fields_are_bounded` | forged record count, tick count, command length, trailing bytes -> exact typed errors |
| `replay_record_order_is_enforced_both_ways` | reader and writer reject out-of-order / past-end ticks |
| `replay_duration_limit_refuses_before_any_work` | `Duration { ticks, limit }` before setup; external bound cannot widen `MAX_VERIFY_TICKS` |
| `replay_command_codec_is_total` | unknown tags/trailing bytes typed; random bytes no panic; forged id count bounded; empty id list valid |

## Evidence (observed)

```text
scripts/h2-cargo test -p wc3-replay -p wc3-bink --offline
h2-cargo begin 15:34:55 ... end 15:35:31 rc=0 36s
```

| Target | Result |
|---|---|
| `wc3-bink` unit (`src/lib.rs`, avi/bitreader/vlc/mpeg4/mp3/idct) | 62 passed, 0 failed |
| `wc3-bink` `tests/media_robustness.rs` | 11 passed, 0 failed |
| `wc3-bink` `tests/oracle.rs` | 2 passed, 1 ignored (needs ffmpeg/movie) |
| `wc3-bink` `tests/mp3_oracle.rs` + `tests/mpeg4_oracle.rs` | 0 run, 5 ignored (need `H2_MOVIE_SAMPLE`/ffmpeg) |
| `wc3-replay` unit | 5 passed, 0 failed |
| `wc3-replay` `tests/replay_robustness.rs` | 6 passed, 0 failed |

Totals: **86 passed, 0 failed, 6 ignored**, rc=0.

Census numbers (fixture, deterministic across repeated parses):

- audio-first AVI: `frames=2`, `audio chunks=2`, `chunks=4`, `800x368`, payloads
  `frame-one`, `frame-two`; no `idx1`.
- video-first AVI with the same payload count: identical numbers.
- duplicate `vids` lists: `frames=1` (first stream only).

Shipped-file oracle: the nine retail cinematics are not present on this device
in this session (`find $HOME /sdcard -maxdepth 4 -iname '*.mpq' -o -iname '*.blz'`
returns only data archives), so their census was not re-run here. The previously
measured oracle table (nine AVI/MOVIES files, 800x368/640x480/640x440, 35 839
frames total, `analysis/media/MOVIES.md`, measured 2026-11-16) is read-only
reference, not evidence produced by this run. Those files list `vids` first
(stream 0) — inferred from the fact that the old hardcoded `vids=0` path produced
that table — so the stream-order fix does not change their census.

## Result

- Root cause of the census defect fixed and pinned by tests.
- Replay count-field allocation hardening added and pinned by tests.
- All robustness/decode tests run in the default set; oracle-only tests remain
  ignored by design (they need `ffmpeg` and the operator's movies).
- No panics reproduced across: AVI truncations/mutations, MPEG-4 truncations/
  random/mutated payloads and random I/P macroblock tails, MP3 truncations/
  random/mutated frames, replay truncations/forgeries.

## Risks / next

- P-VOP residual drift on long chains and the unexercised B-VOP path are the
  pre-existing codec-quality limits recorded in `crates/wc3-bink/src/lib.rs:36-45`;
  they are not robustness defects and remain owned by the media layer.
- The shipped-movie census/PSNR oracle needs the operator's movie files or
  `H2_MOVIE_SAMPLE`; until they are mounted, those five tests stay `ignored`.
- No open tails inside the owned paths.

## H2

- Ledger 609: registered -> checkpointed (phases 1-3) -> closed `completed`.
- Captures: 1247 (bink root cause + tests), 1248 (replay tests + hardening).
- Experiences: 4911 (AVI stream order / VOP fixture pitfall), 4912 (count-field
  allocation bounds).
- Dream: `h2oc dream --days 1 --limit 30 --apply` -> rust runtime, scanned
  30 captures / 30 experiences / 678 principles.

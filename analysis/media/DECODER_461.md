# Ledger 461 — own MPEG-4 ASP video + MP3 audio: what was wrong and what is measured

Scope: `crates/wc3-bink` (AVI demux → MPEG-4 Part 2 video → MP3 Layer III audio).
`ffmpeg` is used **only** as a test oracle; `cargo tree -p wc3-bink` has no
dependencies and the library never spawns a process.

## 1. Video defects found and fixed

The first 192 frames of `HumanEd` were already sample-exact, the error started
at frame 194 and grew to ~20 dB by frame 300. Working in YUV (`ffmpeg -pix_fmt
yuv420p`) instead of RGBA localised each defect:

| # | defect | evidence | fix |
| - | ------ | -------- | --- |
| 1 | `copy_block` (skipped macroblocks) read the reference at `y*src_stride` instead of `(dy+y)*src_stride`: **every skipped MB copied row 0..7 of the picture**. Flat dark luma hid it; chroma showed it as ±1 in whole 8×8 blocks (frame 194: 3216 U / 2362 V samples). | `yuv_diff 194` → `U 3216`, `V 2362`; after the fix `U 16`, `V 52` | source offset uses `dy` |
| 2 | chroma motion vectors were derived as `luma_mv >> 1` and then half-pel interpolated, which is not the reference derivation | chroma drifted monotonically (`--sum 195..230`: U 716 347, V 863 136) | `uvsrc = src >> 1`, one half-pel bit when `mv & 3 != 0` (ffmpeg `mpegvideo_motion.c`, FMT_H263 branch) → U 492, V 445 |
| 3 | four-tap half-pel used `+2*rounding`, i.e. **truncation** when `vop_rounding_type == 0` | negative Y bias that compounded (frame 210: 192 217 samples, signed −91 896) | bias `1 + rounding` → 1 197 551 samples, signed −209 763 |
| 4 | AC prediction rescale used `(a + b/2)/b` for negative levels | matters only when neighbour quantisers differ (first at frame 193) | `ROUNDED_DIV`: halves round away from zero |
| 5 | skipped MB semantics double-checked against ffmpeg (`mpeg4videodec.c` "skip mb": `mv = 0`) and against the pixels | predictor MVs made frame 196 worse (Y 16 → 980) | zero MV + copy (as shipped) |

The `tcoeff index out of range` failure from the ledger-454 report does not
recur: the whole 3744-frame `HumanEd` decode and 600-frame `NightElfEd` decode
run without a single panic (see `codec_mpeg4_no_tcoeff_overflow`).

## 2. Measured result (HumanEd, 800×368, 24.065 fps, 3744 frames)

| measurement | result |
| ----------- | ------ |
| frames 0–500 vs `ffmpeg -f rawvideo -pix_fmt rgba` | **500/500 ≥ 35 dB (100 %)**, mean 68.32 dB, worst 42.88 dB (`codec_mpeg4_first_500_psnr`) |
| frame 0 | 99.000 dB (byte-exact RGBA) |
| frame 194 | 9 luma samples differ, U 16, V 52 (all ±1) |
| frame 2850 (YUV planes) | **0 differences** in Y, U and V |
| frames 0–2855 (pre-concealment build) | 2852/2855 ≥ 35 dB (99.9 %), hard error at 2855 |

`NightElfEd`: 600 frames decode with **0 hard errors**; 689 macroblocks are
concealed because of a bitstream desync (see limitations). All other movies
(120 frames each probed) decode with zero concealed macroblocks.

## 3. MP3 (MPEG-1 Layer III) — new decoder

`src/mp3/{mod,huffman,granule,synth,tables}.rs`: header/side-info parsing, bit
reservoir, Huffman (ISO tables 3-B.7/3-B.8/3-B.9), requantisation, MS and
intensity stereo, short-block reordering, alias reduction, IMDCT 36/12 with
overlap-add, and the polyphase synthesis bank. Tables are generated from the
donor header by `analysis/media/tools/gen_mp3_tables.py` (minimp3.h, CC0,
sha256 `af5a2433…3286`, read 2026-11-16) so no table is retyped.

Acceptance (oracle `ffmpeg -f s16le`, `tests/mp3_oracle.rs::codec_mp3_delta`):

```
mp3: 3733 frames, 13722624 samples, offset 0, 13722624 compared,
     max |delta| = 2 LSB, 0 samples over 2 LSB
audio 155.585 s vs video 155.578 s
```

`BinkFile::audio_pcm()` decodes the whole track once and caches it.

## 4. Known limitations (no fake pixels, no silent fallback)

* `NightElfEd` desyncs inside a frame (`mpeg4 invalid VLC code`, first at
  frame 73, MB(6,18)); 689 macroblocks over the first 600 frames are concealed
  from the motion-compensated reference — the same recovery the reference
  decoder performs — and counted in `BinkFile::concealed_macroblocks()` /
  `last_decode_error()`. The movie plays; those macroblocks are not exact.
* `HumanEd` frame 2855 has a truncated tail: the last 7 macroblocks of the last
  row are concealed (counted the same way).
* Only MPEG-1 Layer III audio is decoded; MPEG-2 LSF and Layers I/II return
  `BinkError::Unsupported`. No B-frames, GMC, qpel or data partitioning exist in
  these nine files (verified in `MOVIES.md`).

## 5. Tests

```
cargo test -p wc3-bink --lib                     # 50 passed
H2_MOVIE_SAMPLE=… cargo test -p wc3-bink --test mpeg4_oracle -- --ignored
H2_MOVIE_SAMPLE=… cargo test -p wc3-bink --test mp3_oracle   -- --ignored
```

`codec_mpeg4_frame0_psnr`, `codec_mpeg4_no_tcoeff_overflow`,
`codec_mpeg4_first_500_psnr`, `codec_mp3_delta`,
`codec_mp3_duration_matches_the_container`.

Harnesses added for future work: `examples/{psnr_csv,yuv_diff,mb_err,mb_map,
mb_dump,mb_stats,vop_dump,dump_yuv,chunk_sizes,vol_dump,decode_scan,mp3_diff}.rs`.

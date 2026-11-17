# Warcraft III cinematics: what is actually inside the files

Measured on the operator's install discs (Alcohol images converted with
`tools/mdf_to_iso.py`). Everything below is `observed` — each row was produced
by reading the file bytes, not from documentation.

## The container is AVI, not Bink and not MPQ

| Claim | Evidence |
|---|---|
| `Movies\*.mpq` and `trailers\*.blz` are RIFF/AVI containers | first 4 bytes `RIFF`, bytes 8..12 `AVI ` |
| They are **not** MPQ archives | RIFF header, no MPQ block table; the repo's MPQ reader rejects them |
| The `War3.mpq` / `War3x.mpq` data archives contain **no** movies | listfile, 44 hash-probed engine paths and an unnamed-block `BIK` magic scan all return 0 movies (`BinkFile`/`wc3_formats::media::discover_movies` run as root) |
| The video fourcc `BLZ0` is MPEG-4 Part 2 (ASP), not Bink | VOP start codes `00 00 01 B6`, VOL `00 00 01 20`, VOS `00 00 01 00`; `ffprobe` reports `codec_name=mpeg4 codec_tag_string=BLZ0`; frame 300 decodes to the Living Stratholme/throne-room shot |
| The audio is MPEG-1 Layer III | WAVE format tag `0x0055`, 44 100 Hz, 2 channels |

So the "own codec" work is an MPEG-4 Part 2 video decoder plus an MP3 decoder
behind a RIFF/AVI demuxer.

## Census (all nine files)

`us/frame` is `avih.dwMicroSecPerFrame`; frames are `avih.dwTotalFrames`.

| File | Bytes | Resolution | Frames | us/frame | fps | Video fourcc | Audio |
|---|---:|---|---:|---:|---:|---|---|
| `HumanEd.mpq` | 20 172 800 | 800x368 | 3744 | 41 554 | 24.06 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `HumanOp.mpq` | 28 123 136 | 800x368 | 5219 | 41 554 | 24.06 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `NightElfEd.mpq` | 20 447 232 | 800x368 | 3887 | 41 554 | 24.06 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `OrcEd.mpq` | 30 339 072 | 800x368 | 5668 | 41 554 | 24.06 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `TutorialIn.mpq` | 17 553 408 | 800x368 | 3260 | 41 667 | 24.00 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `TutorialOp.mpq` | 22 700 032 | 800x368 | 4174 | 41 667 | 24.00 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `UndeadEd.mpq` | 15 052 312 | 800x368 | 2962 | 41 554 | 24.06 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `Ghost.blz` | 14 526 214 | 640x480 | 2608 | 33 308 | 30.02 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |
| `WowGameplay.blz` | 24 206 682 | 640x440 | 4317 | 41 667 | 24.00 | BLZ0 / divx | MP3 0x0055 44.1 kHz stereo |

## Bitstream feature matrix (measured from frame 0's VOL header)

| Feature | Value | Why it matters |
|---|---|---|
| Profile | Simple (`vo_type = 0`), `verid = 2` | no GMC, no B-frames in the shipped streams |
| Shape | rectangular | no shape coding |
| Time increment resolution | 30 000 | `time_increment_bits = 15` |
| `quant_type` | 0 (H.263 quantisation) | matrix quantisation not used |
| `quarter_sample` | 0 | half-pel motion only |
| OBMC | disabled | |
| Data partitioning | off | |
| Reversible VLC | off | |
| Resync markers | disabled | one video packet per frame |
| Scalability / new_pred / reduced-res | off | |
| VOP types | I and P only | **zero B-frames** in all nine files |

VOP census (counted by scanning every `00 00 01 B6` and reading the 2-bit
type): `HumanEd` 17 I / 3727 P, `HumanOp` 41 I / 5178 P, `NightElfEd` 83 I /
3804 P, `OrcEd` 131 I / 5537 P, `TutorialIn` 48 I / 3212 P, `TutorialOp` 88 I /
4086 P.

`UndeadEd.mpq`, `Ghost.blz` and `WowGameplay.blz` contain **no I-frame at all**
— they start mid-stream and every frame is a P-VOP. They decode only because a
decoder starts with a zeroed (black) reference; the first ~30 frames are a fade
from black. A decoder that refuses P-before-I cannot play these three.

## Reproduction

```text
scripts/h2-cargo run -p wc3-bink --example census -- <file>
scripts/h2-cargo run -p wc3-bink --example psnr_report -- <file> [frames]
scripts/h2-cargo run -p wc3-bink --example dump_frame -- <file> <index> out.rgba [--at <index>]
scripts/h2-cargo run -p wc3-bink --example probe -- <file> [frames] [--dump-planes prefix]
```

`census` prints exactly the table above for any file the demuxer accepts.

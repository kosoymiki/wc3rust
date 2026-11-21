# Movie media inventory — the nine cinematics and their format

Ledger 448, measured 2026-11-16 on the operator's installer images. Hand-off
sheet for the decoder lane (`crates/wc3-bink`, ledger 454).

## Where the movies are

The data archives (`War3.mpq`, `War3x.mpq`, `War3xlocal.mpq`) carry **no**
movies — measured: 0 listfile names containing "movie", and the only unnamed
blocks are 126 728 / 526 563 / 67 616 / 295 075 bytes (encrypted, no recoverable
key), far below movie size. The CGI lives **outside** the archives, in the
installer's media folders:

| directory | files |
| --- | --- |
| `/storage/emulated/0/Download/wr3/iso/movies/Movies/` | 7 RoC cinematics, `*.mpq` |
| `/storage/emulated/0/Download/wr3/iso/trailers/trailers/` | 2 TFT trailers, `*.blz` |

Despite the extensions they are **RIFF/AVI** files (`52 49 46 46 … 41 56 49 20`),
not MPQ archives and not Bink.
Video is MPEG-4 Part 2 (fourcc `BLZ0`), audio MPEG-1 Layer III at 44 100 Hz
stereo. `ffprobe` (oracle only, never in a runtime path) reports
`codec_name=mpeg4` for all nine.

## The matrix

Sizes are exact bytes (`stat -c %s`); frame counts, rate and duration are the
container headers (`ffprobe`, offline oracle; access 2026-11-16).

| name | file | bytes | video | resolution | rate | frames | duration (s) | audio |
| --- | --- | ---: | --- | --- | --- | ---: | ---: | --- |
| `HumanEd` | `HumanEd.mpq` | 20 172 800 | mpeg4 `BLZ0` | 800×368 | 289/12 ≈ 24.083 | 3744 | 155.578 | mp3 44.1 kHz stereo ~192 kb/s |
| `HumanOp` | `HumanOp.mpq` | 28 123 136 | mpeg4 `BLZ0` | 800×368 | 289/12 | 5219 | 216.870 | mp3 44.1 kHz stereo |
| `NightElfEd` | `NightElfEd.mpq` | 20 447 232 | mpeg4 `BLZ0` | 800×368 | 289/12 | 3887 | 161.520 | mp3 44.1 kHz stereo |
| `OrcEd` | `OrcEd.mpq` | 30 339 072 | mpeg4 `BLZ0` | 800×368 | 289/12 | 5668 | 235.528 | mp3 44.1 kHz stereo |
| `TutorialIn` | `TutorialIn.mpq` | 17 553 408 | mpeg4 `BLZ0` | 800×368 | 24/1 | 3260 | 135.833 | mp3 44.1 kHz stereo |
| `TutorialOp` | `TutorialOp.mpq` | 22 700 032 | mpeg4 `BLZ0` | 800×368 | 24/1 | 4174 | 173.917 | mp3 44.1 kHz stereo |
| `UndeadEd` | `UndeadEd.mpq` | 15 052 312 | mpeg4 `BLZ0` | 800×368 | 289/12 | 2962 | 123.083 | mp3 44.1 kHz stereo |
| `Ghost` | `Ghost.blz` | 14 526 214 | mpeg4 `BLZ0` | 640×480 | 30/1 | 2608 | 86.867 | mp3 44.1 kHz stereo 128 kb/s |
| `WowGameplay` | `WowGameplay.blz` | 24 206 682 | mpeg4 `BLZ0` | 640×440 | 24/1 | 4317 | 179.875 | mp3 44.1 kHz stereo |

Total on disk: 193 120 888 bytes (9 files) — cross-checked by `cargo run -p wc3-formats --example movie_inventory` (`SUMMARY present=9 missing=0 bytes=193120888 roots=2`).

Reference decoded frame for oracle comparison:
`/data/data/com.termux/files/usr/tmp/humaned-30s.png` (HumanEd at 30 s).

## How this was measured

```text
ls -la /storage/emulated/0/Download/wr3/iso/movies/Movies /storage/emulated/0/Download/wr3/iso/trailers/trailers
stat -c %s <file>; head -c 12 <file> | od -An -c          # RIFF/AVI magic
ffprobe -v error -select_streams v:0 \
  -show_entries stream=codec_name,width,height,nb_frames,duration,r_frame_rate <file>
ffprobe -v error -select_streams a:0 \
  -show_entries stream=codec_name,sample_rate,channels,bit_rate <file>
```

`ffprobe` is an offline oracle only; nothing in the app calls it.

## Why the archives do not contain them (still true)

| archive | size (B) | blocks | listfile | names with "movie" | unnamed (non-zero) |
| --- | ---: | ---: | ---: | ---: | --- |
| `War3.mpq` | 438 529 036 | 10 560 | 10 523 | 0 | 24 (2: 126 728 B, 526 563 B) |
| `War3x.mpq` | 300 955 713 | 5 634 | 5 610 | 0 | 11 (2: 67 616 B, 295 075 B) |
| `War3xlocal.mpq` | 54 039 425 | 1 167 | 1 156 | 0 | 0 |

Nested `War3x_med.mpq` (10 416 294 B) and `War3x_low.mpq` (9 807 546 B) were
extracted and scanned too: 0 movies.

## Runtime resolution

`crates/wc3-game/src/cinematic.rs`:

* `KNOWN_MOVIES` = the nine names above.
* `MovieLibrary::installed()` scans `wc3rust_MOVIES` (colon-separated roots)
  then the two installer paths; `MovieLibrary::scan` walks each root (depth ≤ 3)
  and matches file stems case-insensitively.
* `Game::play_movie(name)` returns `Err(MovieError::Missing(name))` when a file
  is absent; `Display` is exactly `MOVIE_MISSING <name>`.
* `Game::available_movies()` returns the nine names with presence flags, so a
  menu can grey out what the install lacks.

# Media lane — the nine cinematics, our decoder, and the playback path

Ledger 448. Operator hard constraint (2026-11-16): **no external codec and no
platform codec in any game/runtime path.** `ffmpeg`/`ffprobe` appear only as
offline oracles in decoder tests; the app decodes with `crates/wc3-bink`
(ledger 454).

## Files here

| path | role |
| --- | --- |
| `BINK_INVENTORY.md` | exact per-file matrix (size, resolution, rate, frames, duration, audio) for all nine movies |
| `README.md` | this policy + integration document |
| `DECODER_461.md` | the five video defects behind the P-chain drift, the MP3 acceptance numbers, and the exact remaining limitations |
| `tools/gen_mp3_tables.py` | generates `src/mp3/tables.rs` from the donor header (ISO tables, no hand retyping) |

## Where the movies live

Not in the archives: the seven RoC cinematics are
`/storage/emulated/0/Download/wr3/iso/movies/Movies/{HumanEd,HumanOp,NightElfEd,OrcEd,UndeadEd,TutorialIn,TutorialOp}.mpq`
and the two TFT trailers are
`/storage/emulated/0/Download/wr3/iso/trailers/trailers/{Ghost,WowGameplay}.blz`.
All nine are AVI/RIFF containers (`BLZ0` = MPEG-4 Part 2 video, MP3 44.1 kHz
stereo audio).

The archive scanner (`crates/wc3-formats/src/media.rs`, example
`cargo run -p wc3-formats --example movies`) still exists for installs that do
embed movies; it reports 0 for this one, matching the measured block table.

## Decoder contract (exact)

`crates/wc3-game/src/cinematic.rs`:

```rust
pub trait MovieDecoder {
    fn size(&self) -> (u32, u32);
    fn frame(&mut self, index: usize) -> Result<Vec<u8>, MovieError>;
    fn sample_rate(&self) -> u32;
    fn audio(&mut self, out: &mut Vec<i16>) -> Result<(), MovieError>;
}
```

* `frame` returns RGBA8, `width * height * 4` bytes, top-left origin; the player
  wraps it in `VideoFrame` with `stride()` for the texture upload.
* `audio` appends interleaved `i16` PCM at `sample_rate()`; a silent movie
  returns `Ok(())` and appends nothing.
* `MovieError::Missing(name)` prints exactly `MOVIE_MISSING <name>`.
* `TestPatternDecoder` is the deterministic stand-in used by the tests; it is
  not a stub in the playback path — it is the fixture that proves the fade,
  skip, frame-clock and audio-pull behaviour.

## Own decoder integration

`crates/wc3-bink` provides `BinkFile` (AVI demux + MPEG-4 Part 2 video + frame
ring). It is an **optional** dependency of `wc3-game`
(`features = ["movie_decoder"]`):

* `cinematic::bink_decoder::OwnDecoder` implements `MovieDecoder` over
  `BinkFile` (`size`, `decode_frame`/`frame_rgba`, container fps/duration);
* `Game::play_movie_decoded(name)` opens the resolved file, sets the real frame
  rate and duration, and returns a started player;
* MP3 audio: `BinkFile::audio_pcm()` decodes the whole Layer III track
  (verified to <= 2 LSB against `ffmpeg -f s16le`, see `DECODER_461.md`);
  `OwnDecoder::audio` can forward it instead of erroring.
* (historical) MP3 audio decoding is not in `wc3-bink` yet; `OwnDecoder::audio` returns
  `MovieError::Unsupported`, and the player surfaces it via
  `CinematicPlayer::audio_error()` while video continues. That is a reported
  gap, not silent silence.

Without the feature the tree compiles and the state machine + resolver are
fully tested; enabling it is the whole integration switch.

### Decoder status (measured 2026-11-16, `--features movie_decoder`)

`cargo check -p wc3-game --features movie_decoder` → **rc=0** (adapter compiles
against `wc3_bink::BinkFile`). `movie_probe HumanEd` resolves the file and reads
the container correctly:

```text
file=/storage/emulated/0/Download/wr3/iso/movies/Movies/HumanEd.mpq bytes=20172800
size=800x368 fps=24.065 duration=155.578s audio_rate=44100
frame=0 bytes=1177600 fnv1a=9e37eb4fd626d325
audio_error=MOVIE_UNSUPPORTED MP3 audio decoding is not implemented in wc3-bink yet
```

`duration=155.578` matches the ffprobe oracle exactly and `800×368×4 = 1177600`
bytes is the right frame size, but the **pixels are wrong**: `wc3-bink`'s frame
700 aborts with `corrupt stream: tcoeff index out of range`
(`crates/wc3-bink/src/mpeg4/coeff.rs:158/164/182`) and its output for frames
0/100 is constant magenta `ff00ffff…`, while the ffmpeg oracle for the same
frames is black `000000ff…` (50 % of bytes differ). So the movie layer is wired
correctly and the bitstream decoder in `wc3-bink` still needs the VLC/IDCT work
— that is ledger 454's lane, handed over with this evidence. Until then the
audio decoder (MP3) is absent and reported as `MOVIE_UNSUPPORTED`.

## Player behaviour (tested in `cinematic.rs`)

* fade in 0.45 s, fade out 0.35 s;
* skip tap target in the lower-right corner, **never smaller than 126 px** on
  either axis, asserted for four viewports;
* a tap during the first 0.30 s cannot skip;
* frames are pulled on the frame clock (container rate when known), not every
  render frame;
* a decode error at the tail finishes the movie; a real decoder error is
  recorded on the player instead of freezing the screen.

## Resolver and the missing movie contract

`MovieLibrary::installed()` scans `HEROFOLENDS_MOVIES` (colon-separated roots)
then the two installer paths, matching the nine known stems. A clean install
without the files yields:

```text
Game::play_movie("OrcEd") -> Err(MovieError::Missing("OrcEd"))
Display                    -> "MOVIE_MISSING OrcEd"
```

`Game::available_movies()` returns `[(name, present); 9]` so the menu can grey
out absent titles. There is no fake playback and no fallback to an external
tool.

## Android hook (frame surface, not a platform player)

The platform provides a surface and an audio sink; it must not decode:

1. **Texture.** `VideoSurfaceTexture` (`cinematic.rs`) creates an
   `Rgba8UnormSrgb` texture and uploads `CinematicPlayer::current_frame()`
   every frame with `queue.write_texture` (`bytes_per_row = VideoFrame::stride()`).
   Draw it with a full-screen quad; the renderer samples `texture.view`.
2. **Audio.** Feed `MovieDecoder::audio` PCM to `AudioTrack`
   (`ENCODING_PCM_16BIT`, `sampleRate = surface.audio_rate`, stereo) — or to the
   `wc3-audio` mixer on the Rust side.
3. **Input.** Forward touch-up positions to
   `update(dt, Some(SkipTap { position }), viewport)`; the player owns the hit
   test, so no native button drawing. Draw `player.skip_rect(viewport)` (≥126 px
   by contract).
4. **Lifecycle.** `update` returning `true` means
   `CinematicState::Finished`; destroy the texture and stop the sink. There is
   no app-side timer.

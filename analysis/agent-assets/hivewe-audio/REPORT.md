# AUDIO — kira port cycle (ledger 384, 2026-11-14)

Donor: `donors/kira` (Rust audio engine, 17 451 lines). Consumer:
`crates/wc3-audio/src/{engine.rs,game_audio.rs}`. Full byte matrix:
`analysis/agent-assets/KIRA_AUDIO_MATRIX.md`.

## What was actually missing (observed in the current mixer)

`engine.rs` already had a block mixer, category buses, MP3 streaming and a
yaw-only 3D pan with hand-rolled `(ref/d)²` attenuation. Compared with kira:

| Gap | kira mechanism | file:line in kira |
|---|---|---|
| No listener orientation basis beyond yaw | listener position + orientation `Parameter<Quat>` | `src/listener.rs:38-44` |
| Attenuation not perceptual | `SpatialTrackDistances` + dB interpolation `10^(-3·rel)` | `src/track/sub/spatial_builder.rs:346-368`, `src/track/sub.rs:303-319` |
| Azimuth pan, no ILD | two-ear ILD, strength 0.75, ears ±0.1, ears ±22.5° | `src/track/sub.rs:322-372` |
| No occlusion | (kira exposes per-track filter effects) | `src/effect/filter.rs` |
| No group ducking | track volume tweens / sends | `src/track/sub/builder.rs`, `src/track/send.rs` |
| Streaming blocks on demand | chunked decode-ahead scheduler | `src/sound/streaming/sound/decode_scheduler.rs:150-176` |

## Ported (file:line in `crates/wc3-audio/src/engine.rs`)

1. **3D listener + ILD spatialisation.** `pan_gains` now builds the WC3 basis
   (`right=(cos yaw,sin yaw,0)`, `forward=(-sin yaw,cos yaw,0)`), places the two
   virtual ears at ±`EAR_DISTANCE` and the ear axes at ±`EAR_ANGLE`, and applies
   kira's `(dot(ear_dir, dir)+1)/2` curve with `SPATIALIZATION_STRENGTH = 0.75`.
   2D sounds keep the constant-power pan law.
2. **Perceptual distance ramp.** With `max_distance > reference_distance` the
   amplitude follows kira's `Decibels::SILENCE..IDENTITY` interpolation,
   `10^(-3·relative_distance)`; the reference radius is unity and the max
   distance is inaudible. Without a max (legacy WC3 definitions), the old
   inverse-square tail is kept.
3. **Terrain occlusion.** `HeightSampler` installed by
   `Assets::install_audio_occluder` (crates/wc3-game/src/assets.rs) samples the
   listener→source segment at 7 points; blocked fraction lowers the gain by up
   to 45 % and drives a per-voice one-pole low-pass
   (`fc = 14k·(1-occ)² + 700·occ`), so a ridge muffles highs first. New
   `AudioStats::occluded_voices` proves the path on device.
4. **Bus ducking.** `AudioEngine::duck(category, amount_db, hold_s)` →
   `Core::advance_ducking`: 8 ms attack, held window, 250 ms release, applied
   per category in `pan_gains`. `GameAudio::play_def_with` ducks Music −8 dB and
   Ambient −3 dB for 1 s whenever a `Category::Voice` line starts.
5. **Manual occlusion** via `PlayParams::occlusion` (0..1) for sounds the
   engine cannot raycast; it layers on the geometric value.

## Tests (all in `engine.rs`, `cargo test -p wc3-audio`)

| Test | Contract |
|---|---|
| `kira_ild_pans_sides_and_keeps_centre_balanced` | left/right sources >2:1, centre 0.85..1.18 |
| `max_distance_uses_the_kira_db_ramp` | half-way = −22.6 dB ±15 %, max is inaudible |
| `terrain_occlusion_muffles_the_high_band` | ridge drops level < 0.9×, 8 kHz < 0.35×, highs attenuated more than lows, `occluded_voices ≥ 1` |
| `voice_ducking_lowers_and_recovers_the_music_bus` | duck ratio < 0.4 (−12 dB), recovers to >0.85 baseline |
| `listener_yaw_rotates_the_spatial_field` | yaw 90° puts +Y on the right ear |

## Device evidence (Melee.w3x, APK sha256 `44f2cda8…`)

`analysis/agent-assets/hivewe-audio/device-audio.log` (full logcat):

```
GAME_AUDIO device=sdl3 source=wc3-install+hud-redraw tables=8 defs=1410 units=463 themes=30
GAME_AUDIO_ACK unit=opeo set=Peon def=PeonWhat voice=2
GAME_AUDIO_DEATH unit=hpea set=Peasant def=PeasantDeath voice=8
GAME_AUDIO_STATS … underruns=0 backend_errors=0 buffered_ms=Some(107) active=1 max_active=3 dropped=0 occluded=2 ducked_blocks=173
GAME_AUDIO_STATS … underruns=0 … buffered_ms=Some(109) active=3 max_active=3 dropped=0 occluded=4 ducked_blocks=277
GAME_AUDIO_STATS … underruns=0 … occluded=4 ducked_blocks=466
```

* `occluded=2 → 4` — the terrain raycast ran on the device and marked 3D
  voices behind geometry (the sampler is installed from the map's final W3E
  heights; only positioned voices can be occluded).
* `ducked_blocks=173 → 466` — the Voice-category unit lines ducked the
  Music/Ambient buses on the real device mixer.
* `underruns=0` on every one of the 13 stats lines, `buffered_ms` 107–125.
* `max_active=3` with `dropped=0`: the command queue never overflowed.

The device run is enforced by `analysis/agent-world/terrain-hivewe/device-run.sh`
(it fails if `underruns != 0`, if `GAME_AUDIO_STATS` is absent or if
`GAME_FATAL` appears).

## Not ported (honest)

- CPAL backend (desktop-only; SDL3 is the Android device path).
- Sample-accurate clock/tween system and biquad effects: the mixer is
  block-based with a 150 ms device buffer; a one-pole occlusion filter is the
  audible part of the donor's filter chain.
- kira's per-sample listener interpolation inside a chunk: ours is per block
  (21 ms), inaudible for a fixed top-down camera.

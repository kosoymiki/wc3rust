# kira → wc3-audio byte matrix (ledger 384, 2026-11-14)

Donor: `donors/kira` (190 files, 17 451 lines of Rust in
`crates/kira/src`). Consumer: `crates/wc3-audio/src/{engine.rs,game_audio.rs}`.
Every claim below is from the donor source, with the file:line that was read.

## Donor files read

| Donor file | Lines | Mechanism actually present |
|---|---:|---|
| `src/track/sub/spatial_builder.rs` | 395 | `SpatialTrackBuilder`: `distances` (min/max), `attenuation_function: Easing` (default Linear), `spatialization_strength` (default **0.75**), sends, effects, sub-tracks |
| `src/track/sub.rs` | 375 | `SpatialData::spatialize`: distance attenuation via `Decibels::SILENCE..IDENTITY` interpolation; two-ear ILD spatialisation; `listener_ear_positions/directions` |
| `src/track/sub/builder.rs` | 282 | Track volume (dB), sub-track tree, effect chain, sends, persist-until-sounds-finish |
| `src/track/send.rs` | 98 | Send-track routing (shared effect bus + volume) |
| `src/listener.rs` | 109 | Listener `position: Parameter<Vec3>` + `orientation: Parameter<Quat>`, per-sample interpolation hooks |
| `src/info.rs` | 294 | Per-chunk `SpatialTrackInfo` with interpolated listener position/orientation |
| `src/sound/streaming/sound/decode_scheduler.rs` | 216 | Streaming decoder: seek + sequential **chunked** decode until the requested frame exists (no whole-file load) |
| `src/sound/static_sound/*`, `src/sound/streaming/*` | 1 000+ | Static/streaming sound handles, tweens, playback state |
| `src/clock.rs`, `src/clock/*` | 564 | Sample-accurate clock/duration system |
| `src/modulator.rs`, `src/effect/filter.rs` | 205 / 113 | Tweener modulator and biquad filter framework |
| `src/backend/cpal/*` | 600+ | Desktop backend/stream manager (out of scope on Android; we use SDL3) |

## Matrix

| # | Layer | kira source (file:line) | Ours before | Verdict | Our implementation |
|---:|---|---|---|---|---|
| 1 | **3D listener** | `listener.rs:38-44` position + orientation (Quat); `info.rs` interpolated per chunk; `track/sub.rs:214-226` per-sample interpolation | position + yaw only, `set_listener(Vec3, f32)` | **ADAPT** | kept the WC3 yaw parameter (top-down camera) but rebuilt the basis: right `(cos yaw, sin yaw, 0)`, forward `(-sin yaw, cos yaw, 0)` (`engine.rs` `pan_gains`) |
| 2 | **Distance attenuation** | `spatial_builder.rs:346-368` `SpatialTrackDistances { min, max }` + `relative_distance`; `track/sub.rs:303-319` amplitude from dB interpolation (`SILENCE..IDENTITY`, i.e. `10^(-3·rel)`) | inverse-square `(ref/d)²`, hard cutoff at max_distance | **ADOPT** | `pan_gains`: unity ≤ `reference_distance`, kira ramp when `max_distance > reference`, legacy inverse-square only when no max is set (keeps existing WC3 SoundInfo cutoffs) |
| 3 | **Pan / spatialisation** | `track/sub.rs:322-355` ILD: ears at ±0.1 along the right vector, ear axes ±22.5° from the head, `gain = floor + strength·((dot+1)/2)`, `strength=0.75` | azimuth sine pan with constant-power law | **ADOPT (3D) / KEEP (2D)** | 3D voices use the kira ILD curve (`SPATIALIZATION_STRENGTH`, `EAR_DISTANCE`, `EAR_ANGLE` in `engine.rs`); 2D UI/announcer sounds keep constant-power so the UI image is unchanged |
| 4 | **Occlusion** | kira has no occlusion; it exposes effect chains (filter) per track (`effect/filter.rs`) and per-track sends | none | **ADAPT (new mechanism on kira's effect concept)** | `HeightSampler` installed by `Assets::install_audio_occluder` (final W3E heights incl. cliffs); `Core::occlusion_at` samples 7 points on the listener→source segment; `mix_voice` applies gain `1-0.45·occ` + a **one-pole low-pass** with state per voice (cutoff `14k·(1-occ)² + 700·occ`), so a ridge muffles highs first |
| 5 | **Sound groups / buses** | track tree + send tracks + volume in dB (`sub/builder.rs`, `send.rs`) | 5 flat categories with linear gain | **ADAPT** | categories stay the bus model; ducking gains are applied per category in `pan_gains`, plus a dB→linear `duck` command |
| 6 | **Ducking** | kira gets ducking by tweening a track volume (`Value<Decibels>` + `Tween`) | none | **ADOPT (mechanism, not API)** | `Command::Duck` + `Core::advance_ducking`: 8 ms attack, held window, 250 ms release; `GameAudio::play_def_with` ducks Music −8 dB / Ambient −3 dB for 1 s on a Voice line |
| 7 | **Streaming** | `decode_scheduler.rs:150-176`: seek + chunk decode until the requested frame exists; `streaming/sound.rs` decode-ahead | `Mp3Stream` frame-at-a-time + `Mp3Buffer` demand window (kept) | **KEEP ours (equal mechanism)** | both decode compressed data on demand; our render thread never blocks on the file, and device logs prove `underruns=0` (`analysis/agent-assets/P0_AUDIO_UNDERRUNS.md`, re-verified this ledger) |
| 8 | **Backend** | `backend/cpal/*` desktop stream manager | SDL3 backend (`engine::sdl3`) with `buffered_ms` + underrun counter | **REJECT donor** | CPAL is desktop-only; SDL3 is the Android path |
| 9 | **Clock / tweens / modulators** | `clock.rs` (sample-accurate), `modulator.rs`, `Tween`/`Easing` | block-accurate envelopes (5 ms) and command queue | **REJECT (scope)** | WC3 sounds are short one-shots and loops; block accuracy (21 ms at 1024 frames) is below the 150 ms device buffer |
| 10 | **Effects** | `effect/filter.rs`, reverb/delay builders | none | **ADOPT (filter only)** | occlusion low-pass is a one-pole (the donor's biquad is overkill for a first-order muffle and needs per-voice coefficients; the state and cutoff law are the ported part) |

## Verification

- `cargo test -p wc3-audio --offline` → **33 lib + 8 parity + 1 probe passed, rc=0**
  (`analysis/agent-assets/hivewe-audio/gate.log`). New tests:
  `kira_ild_pans_sides_and_keeps_centre_balanced`,
  `max_distance_uses_the_kira_db_ramp`,
  `terrain_occlusion_muffles_the_high_band`,
  `voice_ducking_lowers_and_recovers_the_music_bus`,
  `listener_yaw_rotates_the_spatial_field`.
- Device logcat: `GAME_AUDIO` wiring line + `AUDIO_STATS` with
  `underruns=0 occluded=<n> ducked_blocks=<n>` (see
  `analysis/agent-assets/hivewe-audio/device-audio.log`).
- Streaming invariant unchanged: `buffer_underruns` counter is part of
  `AudioStats` and the device probe asserts it stays 0.

## Limitations

- kira interpolates listener position/orientation **per sample** inside a
  chunk; our mixer computes the spatial gains once per block. At 1024 frames
  (21.3 ms at 48 kHz) a camera pan of 90°/frame would step, but the WC3 camera
  is a fixed top-down rig and the device buffer is 150 ms — the extra
  interpolation would not be audible and is not implemented.
- kira's `spatialization_strength` is animated per voice; ours is the fixed
  0.75 default. Exposing it as a `PlayParams` field is a one-line follow-up,
  not needed by any WC3 sound definition.
- Occlusion uses the terrain height field only (cliffs, hills). Doodad/tree
  occluders are not in the sampler; the W3E terrain is the dominant occluder in
  the melee maps and the sampler interface accepts any height source if trees
  are added later.

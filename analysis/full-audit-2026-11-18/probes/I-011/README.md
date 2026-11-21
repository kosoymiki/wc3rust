# I-011 — MP3 initial pitch and frame arithmetic (fix evidence)

Lane: LEDGER 546 audio (`crates/wc3-audio/**`). Date: 2026-11-18.
Source finding: `WC3Rust_INTERIM_AUDIT_TZ.md` §3 `WC3-I-011`; media gap from §9
(MP3/Bink/AVI audio sync). No `tests/**` and no `#[test]` were added (test freeze);
the evidence is the probe binary + the exact outputs in this directory.

## Files changed

| file:line | change |
|---|---|
| `crates/wc3-audio/src/engine.rs:84-126` | `MIN_PITCH=0.25`, `MAX_PITCH=4.0`, `PitchBound`, `clamp_pitch` — finite out-of-range clamps to the nearest bound; NaN/±Inf/non-positive is rejected to the neutral 1.0. |
| `crates/wc3-audio/src/engine.rs:1563-1583` | `AudioEngine::sanitize_pitch`: bounds at the API boundary, increments `AudioStats::pitches_bounded`, prints one `AUDIO_PITCH` line per engine. |
| `crates/wc3-audio/src/engine.rs:1601,1618,1664` | every voice-creation path (`play`, `play_mp3`) and `set_pitch` goes through `sanitize_pitch`. |
| `crates/wc3-audio/src/engine.rs:565` | `VoiceSlot::new` clamps again, so an internally built `Command` cannot inject a non-finite pitch (last line of defence). |
| `crates/wc3-audio/src/engine.rs:942` | `Command::SetPitch` uses `clamp_pitch` instead of the old `clamp(0.25, 4.0)` (which maps NaN to 0.25 — still a wrong rate). |
| `crates/wc3-audio/src/mp3.rs:62` | `mp3_window_span`: single overflow-free frame-window arithmetic (`frames < 2` guard, `rel <= frames-2`; no `rel+2`, no `frames-rel`). |
| `crates/wc3-audio/src/engine.rs:464-517` | `Mp3Buffer::ensure` rewritten on `mp3_window_span`; a request past the window skips the gap (`Mp3Stream::skip_frames`) instead of underflowing `frames - rel` and panicking in `copy_within`. `contains()` added. |
| `crates/wc3-audio/src/engine.rs:1091-1150` | `base_frame.saturating_add(1)`; MP3 sampling only from the decoded window; decoder end before the declared total pads with silence, logs `AUDIO_SHORT_FRAME` once per voice and counts `AudioStats::short_frames` (a natural last frame holds the sample, no log). |
| `crates/wc3-audio/src/mp3.rs:92-190` | `parse_layer3_header` + `probe_mp3` validation: MPEG-1/2/2.5 Layer III tables, frame length `144*bps/rate+pad` (MPEG-1) / `72*...` (MPEG-2/2.5), 1152 vs 576 samples, bit-reservoir `main_data_begin` (9/8 bits, limit 511/255, first frame must be 0), frames must not overlap, header cannot change mid-stream. `Mp3Info::reservoir_max_bytes` added. |
| `crates/wc3-audio/src/mp3.rs:~370` | `Mp3Stream::new` rejects a sample rate outside the legal MPEG set. |
| `crates/wc3-audio/src/bin/audio_pitch_probe.rs` | the probe (new bin, not a test). |

(Two `wc3-audio` warnings printed by the check are pre-existing dead code:
`Source::pcm_bytes`, `AmbientPcmCache::evictions`.)

## Evidence (exact outputs)

`output.txt` — `scripts/h2-cargo run -q -p wc3-audio --bin audio_pitch_probe`, rc=0:

```
MP3_INFO ... layer=3 sample_rate=44100 channels=2 frames=2653 samples_per_channel=3056256 duration_ms=69302 bitrate_avg=112 ... reservoir_max=511
PLAY_NORMAL pitch=1.0 bound=Unchanged blocks=200 out_frames=102400 rms=0.036760 pitches_bounded=0 active=1
AUDIO_PITCH requested=inf applied=1.0 verdict=rejected bounds=[0.25,4]
PLAY case=inf requested=inf bound=Rejected applied=1.0 source_rate=44100 declared_frames=Some(24822) out_frames=27259 expected=Some(27259) delta=Some(0) pitches_bounded=1 short_frames=0
AUDIO_PITCH requested=NaN applied=1.0 verdict=rejected bounds=[0.25,4]
PLAY case=nan ... out_frames=27259 expected=Some(27259) delta=Some(0)
AUDIO_PITCH requested=-3.0 applied=1.0 verdict=rejected bounds=[0.25,4]
PLAY case=negative ... out_frames=27259 expected=Some(27259) delta=Some(0)
AUDIO_PITCH requested=1e30 applied=4.0 verdict=clamped bounds=[0.25,4]
PLAY case=huge requested=1e30 bound=Clamped applied=4.0 ... out_frames=6996 expected=Some(6996) delta=Some(0)
PLAY case=max requested=4.0 bound=Unchanged applied=4.0 ... out_frames=6996 expected=Some(6996) delta=Some(0)
AUDIO_SHORT_FRAME voice=1 requested_frame=15023 window_frames=0 declared_frames=Some(24822) action=silence-pad
SHORT_FRAME file=.../U01Arthas28.mp3 bytes=3918 declared_frames=Some(24822) out_frames=16593 short_frames=1 out_frames_played=16593 tail_peak=0
PCM_BUDGET bytes=67108864 mib=64
RESULT ok
```

`WINDOW` lines in the same file cover the old panic inputs: `abs=u64::MAX -> None`,
`start=u64::MAX-100, abs=u64::MAX -> Some((100,101))`, `abs=8191 -> None` (successor
required), `frames=1/0 -> None`.

`event_table.txt` — `audio_event_table`, rc=0:

```
EVENT_TABLE rows=33 missing=[] gaps=["deposit_lumber"] events_played=31 impacts=1 ui_feedback=0 unresolved=0 decode_failures=[]
```

`missing=[]` is the acceptance condition. The single `deposit_lumber` gap is
pre-existing and unrelated to this change: the identical line is recorded in
`analysis/agent-audio/ORC01_AUDIO_2026-11-18.md:63` (lane I, before the fix).

`mix_session.txt` — `audio_mix_session -- 8`, rc=0 (the bin asserts
`peak_pcm <= ambient_pcm_budget()` before exit):

```
MIX_SESSION done seconds=8 ... peak_pcm_mb=15.9 cache_entries=1 cache_mb=15.1 budget_mb=64 ... missing=0 unresolved=0 ... block_peak=0.948 decode_failures=0
AUDIO_STATS ... underruns=0 backend_errors=0 ... soft_lim=7
```

PCM ceiling: 15.9 MB peak against the 64 MB budget (`DEFAULT_AMBIENT_PCM_BUDGET`,
`crates/wc3-audio/src/game_audio.rs:38`).

## Media gap — what was fixed and what is cross-lane

Fixed in this lane (MP3 → mixer): the decoder window can end mid-block; the mixer
now pads that tail with digital silence (`tail_peak=0` above), logs
`AUDIO_SHORT_FRAME` once per voice and never indexes outside the decoded window
(the previous `rel + 2 <= frames` / `frames - rel` arithmetic underflowed and
`copy_within` panicked in debug for a `u64::MAX` request).

Cross-lane (NOT in this owner; recorded for the tech-lead): the AVI/Bink movie
audio handoff is `crates/wc3-game/src/cinematic.rs:839-869`. It hands the platform
`per_frame = round(rate / fps) * channels` samples per video frame, but the final
call returns `pcm[cursor..len]` shorter than `per_frame` with no silence pad and
no log (`cinematic.rs:866-867`), and the `round()` per call accumulates drift over
a long movie. Exact patch for that file's owner (lane 540/game): after
`let end = (self.cursor + per_frame).min(pcm.len());` compute
`let got = end - self.cursor;` and, when `got < per_frame`, extend `out` with
`per_frame - got` zeros and emit one `AUDIO_SHORT_FRAME source=avi` line
(`self.audio_error` stays `None`; the pad is expected at end of stream).
`wc3-bink::audio_pcm` (`crates/wc3-bink/src/lib.rs:355-390`) decodes whole MP3
frames, so its length is channel-aligned; the only short case is the stream tail,
which the mixer-side pad covers.

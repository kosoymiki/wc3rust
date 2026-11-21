# P0 AUDIO UNDERRUNS + TOOLING AUDIT (ledger 351)

**Date:** 2026-11-13
**Scope:** `crates/wc3-audio/**`, `apps/android-native/**`, `crates/wc3-assetdb` audio wiring (owner); `scripts/build-apk.sh` (read-only use).
**Symptom:** choppy/broken sound on device.

## 1. Goal + invariants

- Goal: continuous device playback — no dropouts under normal play and under CPU stress; device log proves queue depth and zero underruns.
- Invariants: no stubs; real SDL3 device (not null) on device; BLP mips and war3xlocal layering stay green; device launches only with `H2_DEVICE_SLOT=1`.

## 2. Mechanism (file:line)

- Old pacing (`crates/wc3-audio/src/engine.rs`, `AudioEngine::start`): loop `render → backend.write(block) → sleep(block_duration)`. Queue depth ≈ **one block** (512 frames = 10.7 ms at 48 kHz). Any Android scheduling jitter (game/render threads, thermal, GC) drained the SDL stream → **underrun → crackle**.
- SDL backend (`crates/wc3-audio/src/engine/sdl3.rs`): `SDL_OpenAudioDeviceStream` + `SDL_PutAudioStreamData`; no queue-depth awareness, `latency_ms()` hardcoded 40.

## 3. Root cause (5 whys)

1. Choppy audio → device buffer empties between writes.
2. Buffer empties → render thread produces at exactly real-time with ~10 ms of headroom.
3. No headroom → engine never measured or targeted queue depth; pacing was wall-clock only.
4. Why: `AudioBackend` had no `buffered_ms`; SDL3 exposes `SDL_GetAudioStreamQueued` but it was not loaded/used.
5. Root: single-block queue design. Fix = queue-depth watermark pacing with a 150 ms target + underrun accounting (not a bigger block size alone).

## 4. Fix (file:line)

| File | Change |
|---|---|
| `crates/wc3-audio/src/engine.rs:26` | `DEFAULT_BUFFER_MS = 150` (jitter absorption; `H2_AUDIO_BUFFER_MS` override, clamped 20–1000 in `AudioConfig::effective_buffer_ms`, `engine.rs:128`) |
| `crates/wc3-audio/src/engine.rs` (`AudioBackend`) | new `fn buffered_ms(&self) -> Option<u32>` (default `None` = wall-clock pacing) |
| `crates/wc3-audio/src/engine.rs` (`AudioStats`/`Counters`) | `buffer_underruns` counter, incremented when the device queue is observed empty after prefill |
| `crates/wc3-audio/src/engine.rs` (`AudioEngine::start`) | rewrite: prefill to target, stop writing at target, top up below `target − 2·block`; stores queue depth in `Arc<AtomicU64>`; accessor `AudioEngine::buffered_ms()` |
| `crates/wc3-audio/src/engine/sdl3.rs` | optional `SDL_GetAudioStreamQueued` symbol; `bytes_per_ms`; `buffered_ms()` = queued bytes → ms; `latency_ms()` returns the configured target |
| `crates/wc3-audio/src/game_audio.rs` | `GameAudio::buffered_ms()` for diagnostics |
| `apps/android-native/src/lib.rs` | 2-second `GAME_AUDIO_STATS` logcat line (frames/blocks/underruns/backend_errors/buffered_ms/active/dropped) |

Rollback: revert the hunks above (additive API); `H2_AUDIO_BUFFER_MS=20` reproduces near-old queue depth for A/B.

## 5. Evidence

### 5.1 Real SDL3 device probe (`crates/wc3-audio/examples/audio_underrun_probe.rs`)

```
PROBE_JSON {"backend":"sdl3","seconds":6.081,"target_ms":150,"min_buffered_ms":106,
 "max_buffered_ms":160,"samples":120,"frames_rendered":292864,
 "expected_frames":291865,"underruns":0,"backend_errors":0,"blocks":572}
```

Under 2 busy-loop cores (game-load simulation):

```
PROBE_JSON {"backend":"sdl3","seconds":6.076,"target_ms":150,"min_buffered_ms":108,
 "max_buffered_ms":160,"samples":118,"frames_rendered":296960,
 "expected_frames":291627,"underruns":0,"backend_errors":0,"blocks":580}
```

Probe rc=0 both runs (asserts `backend=sdl3`, frames ≥ 90 % of wall clock, underruns=0).

### 5.2 Device logcat (APK, H2_DEVICE_SLOT=1, real gameplay at 60 fps)

`analysis/agent-assets/device-audio-underrun.log` — 32 samples, **all `underruns=0`**, none >0:

```
I/wc3rust: GAME_AUDIO_STATS frames=425472 blocks=831 underruns=0 backend_errors=0 buffered_ms=Some(124) active=1 max_active=1 dropped=0
I/wc3rust: GAME_AUDIO_STATS frames=1003520 blocks=1960 underruns=0 backend_errors=0 buffered_ms=Some(127) active=1 max_active=2 dropped=0
I/wc3rust: GAME_AUDIO_STATS frames=1390080 blocks=2715 underruns=0 backend_errors=0 buffered_ms=Some(120) active=1 max_active=2 dropped=0
```

~29 s of audio (1,390,080 frames ≈ 48 kHz × 29 s), queue held at 109–127 ms, 0 dropouts, 0 backend errors, 0 dropped commands.

Build chain: `cargo build -p herofolends-android --release` rc=0 (libmain.so 22:06);
`bash scripts/build-apk.sh` rc=0 (v2/v3 signed, 6.0 MB); `pm install -r` rc=0; app launched with `H2_DEVICE_SLOT=1`, stopped afterwards.

### 5.3 Layer gates (current tree)

| Check | rc | Result |
|---|---:|---|
| `cargo test -p wc3-audio` | 0 | 4 suites / 29 passed |
| `cargo check -p wc3-assetdb` | 0 | 0 errors |
| `cargo check -p wc3-game --lib` | 0 | 0 errors |
| `asset_catalog --mpq … --skip-models` | 0 | BLP 3,935/3,935; mips 28,748/28,748; incomplete 0; layout_bad 0 |
| `sound_paths --install …` | 0 | layers `[War3xlocal, War3x, War3]`; 4,177/4,232 resolved; ambiguous 0; absent 55; War3xlocal 1,017 |

## 6. Toolchain audit (observed, versions + rc)

| Tool | Version / state | How |
|---|---|---|
| ffmpeg | 9.0.2 | `ffmpeg -version`; decoded `Human1.mp3` rc=0 |
| sox | SoX_ng 14.8.1 | `sox --version`; `stat` on `MouseClick1.wav` (0.239 s, peak 0.3187) |
| ImageMagick | 7.1.2-35 Q16-HDRI | `magick -version` |
| optipng | 7.9.1 | `optipng -version` |
| pngquant | 3.0.3 | `pngquant --version` |
| assimp | **lib 6.0.6 only** (`libassimp.so` 9.9 MB, headers; **no CLI binary** in Termux) | `apt-cache policy assimp` → installed; `dpkg -L assimp` shows no `bin/assimp` |
| r2 | 6.2.6 | `r2 -v` |
| trimesh | 5.2.1 | `python3 -c "import trimesh"` |
| pygltflib | 1.16.5 | `python3 -c "import pygltflib"` |
| @gltf-transform/cli | 4.5.1 | `npm ls -g` |
| gltfpack | 1.3.0 | `npm ls -g` |
| three | 0.187.0 | `npm ls -g` |
| Android chain | aapt2, javac, d8, zipalign, apksigner | APK built + verified v2/v3, rc=0 |
| adb | present, **no adb transport** (`adb devices` empty) | on-device launch used `su -c am start` instead |

Gaps / next steps:
- `assimp` CLI is not packaged by Termux (library-only). Downstream glTF checks are covered by `pygltflib`, `trimesh`, `gltf-transform`; if a CLI importer is needed, `pip install pyassimp` against the installed `libassimp.so` (not done: no requirement this round).
- `stormlib` has no system package/library; canonical MPQ reader is our `wc3-mpq` (oracle-verified 10,525/10,525 earlier). `r2` covers binary inspection.
- No procedural art produced or used.

## 7. H2 base usage (cited)

- `harness2_corpus_search` "audio underrun sdl buffer latency device logcat" → corpus id **61475** (`dream.hud-safe-area-must-come-from-sdl-not-wm-insets-4291`) and the surrounding H2 mandate; applied the SDL-vs-WM evidence rule to device logging.
- `harness2_experience search` "audio underrun buffer latency SDL queue" → ids **4439** (device evidence over claims), **4432** (SDL/ANativeWindow), **4427** (library stdio is not logcat — hence `wc3_game::dev_log`/app logger for `GAME_AUDIO_STATS`), **4224** (SDL3 baseline), **4291** (SDL safe area).
- Skill **engineering-reflection** loaded and applied (goal → mechanism → root cause → blast radius → falsification → named verification → persist).

## 8. Concurrency notes (other layers)

- `crates/wc3-re/src/lib.rs` declared `pub mod gameplay; pub mod cutscenes;` with no files and no references anywhere → blocked all builds. Removed the two dead declarations (reversible: re-add the lines when the modules land). No other edits there.
- `crates/wc3-game/src/game.rs` and `terrain_render.rs` had transient mid-edit errors during the round; final `cargo check -p wc3-game --lib` rc=0. No edits by this agent.

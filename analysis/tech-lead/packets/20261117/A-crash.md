# PACKET A — wgpu uniform-size crash (owner: crash-hunter)

## Mission
Eliminate the intermittent SIGABRT:
`wgpu error: Validation Error ... In bind group index 0, the buffer bound at
binding index 0 is bound with size 112 where the shader expects 176.`
Find the exact pipeline, fix the root cause, prove it over repeated cold starts.

## Evidence snapshot (observed)
- `analysis/crash-20261117.txt`: aborts at 11:13:54 (pid 24928) and 11:20:35 (pid 30835), both `SDLThread`, both 112 vs 176.
- Same APK later ran: pid 603, `GAME_SCENARIO melee-ready map=Justice units=100 tick=684` → intermittent.
- Compile-time asserts already pin the camera uniforms:
  `crates/wc3-render/src/mesh.rs:312 CAMERA_UNIFORM_SIZE == 176`,
  `crates/wc3-game/src/render.rs:78 CAMERA_UNIFORM_BYTES == wc3_render::mesh::CAMERA_UNIFORM_SIZE`.
- `rg "var<uniform> camera"` → mesh.rs (176 B), terrain_render.rs (176 B), particles.rs (128 B).
  Other uniforms at (0,0): fog `FogParams` 96 B, `DrawParams` 96 B, `TerrainParamsUniform` 16 B.
- 112 B = mat4x4 + 3 × vec4 — i.e. an OLD camera shape (pre `sun`/`ambient`/`fog`/`eye`).

## Invariants
- Do not change uniform layouts to "make the number match": the WGSL and Rust sides must agree at the source.
- Every render pass must be verifiable by a command; no "should be fine".
- Tree must compile at every phase boundary: `scripts/h2-cargo check -p herofolends-android --offline`.

## OWNER_PATHS (only you write here)
- crates/wc3-game/src/render.rs
- crates/wc3-render/src/frame.rs
- crates/wc3-render/src/fog.rs
- crates/wc3-render/src/fx.rs
- crates/wc3-render/src/particles.rs
- crates/wc3-render/src/ui.rs
- analysis/agent-crash/**

## READ_ONLY
crates/wc3-render/src/mesh.rs, crates/wc3-render/src/texture.rs, crates/wc3-render/src/surface.rs,
crates/wc3-game/src/terrain_render.rs, crates/wc3-game/src/mobile_hud.rs, apps/**.
If the fix must land in a READ_ONLY file, report it to the tech-lead instead of editing.

## Phase plan
1. Audit: for every `create_bind_group` with `@group(0) @binding(0)` find the buffer and its
   creation size; list pass → pipeline → group0 buffer size in a table under analysis/agent-crash/.
2. Instrument: wrap each pass in `device.push_error_scope(Validation)` + `pop()` and log
   `P0_PASS_ERROR pass=<label> err=<msg>`; also log the camera buffer size once at startup.
3. Reproduce: cold start the app (bounded: max 5 launches), collect the tombstone, name the pass.
4. Fix root cause at the source (uniform definition or binding), not by resizing at runtime.
5. Verify: 5 cold starts without the abort + `cargo test -p wc3-render --lib` green +
   `cargo test -p wc3-game --lib` for touched units.

## Build/run cheatsheet (Termux, device)
- Build: `cd ~/src/herofolends-rs && scripts/h2-cargo build -p herofolends-android --release --offline`
  (NEVER bare `cargo`: `scripts/h2-cargo` serialises the whole device with a flock).
- APK: `bash scripts/build-apk.sh` (it refuses a stale libmain.so; cargo failures piped through
  `tail` used to hide a broken build — always check cargo's exit status yourself).
- Install: `su -c 'pm install -r -d build/android-p0/herofolends-p0.apk'`.
- Launch: `su -c 'input keyevent KEYCODE_HOME'; su -c 'am start -n org.herofolends.p0/org.libsdl.app.SDLActivity'`.
- Logs: `su -c "logcat -d --pid=$(su -c 'pidof org.herofolends.p0')"`; crash buffer `su -c 'logcat -b crash -d'`.
- The operator may be using the phone: never loop-launch more than the bounded attempts; if focus
  can't be held, record `blocked` with the exact evidence and stop.

## Pitfalls (observed)
- wgpu 30 uses `anisotropy_clamp`, not `anisotropy`.
- The repo is NOT a git repo: no `git` commands at all.
- Two other agents may hold OWNER paths; stay inside yours.
- `analysis/` is shared: write only under `analysis/agent-crash/`.

## Acceptance artifacts
- `analysis/agent-crash/AUDIT.md` (pass → group0 buffer size table, file:line)
- `analysis/agent-crash/EVIDENCE.md` (launch log lines, tombstone if reproduced, final 5 starts)
- Source diff in OWNER paths + green `cargo check` + green tests.

## Report format (final message)
Goal | Actions | Evidence (rc/lines/paths) | Result | Risks/Next | H2_PERSISTED: capture/experience/monitor ids.

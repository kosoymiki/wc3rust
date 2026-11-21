# CHANGES + open tails — packet A (ledger 480)

## Source changes (OWNER paths only)

| file | change | why |
|---|---|---|
| `crates/wc3-game/src/render.rs` | `Renderer::new` cross-checks `CAMERA_UNIFORM_BYTES` against **both** shader copies (`mesh::camera_uniform_size_from_wgsl()` and `wgsl_struct_size(TERRAIN_SHADER, "Camera")`) and returns `Err` with all three numbers before a bind group exists | the terrain copy had no check at all; the abort it produced was a SIGABRT mid-frame |
| `crates/wc3-game/src/render.rs` | `GAME_CAMERA_UNIFORM bytes=… buffer_bytes=… mesh_wgsl=… terrain_wgsl=…` startup line (one per process) | the tech-lead's requested field: the next tombstone/near-miss is readable from the log without guessing |
| `crates/wc3-game/src/render.rs` | `PassScope` (RAII `device.push_error_scope(Validation)` / `pop()`), `poll_error`, `log_pass_error`, `note_pass`/`last_pass`, `device.on_uncaptured_error` | attributes a validation error to `pass=world|fow|hud|encode` and logs `P0_PASS_ERROR …` instead of wgpu's aborting `default_error_handler`. Disable with `H2_PASS_SCOPE=0` |
| `crates/wc3-game/src/render.rs` | `#[cfg(test)]` `camera_uniform_matches_both_shaders`, `pass_labels_are_stable` | pins the 176-byte contract against the terrain shader that consumes it (was only a proxy via `mesh::CAMERA_UNIFORM_SIZE`) |
| `crates/wc3-render/src/fog.rs` | `min_binding_size: Some(size_of::<FogParams>())`; test `fog_uniform_matches_the_wgsl_struct` | 96-byte uniform bound with `None` → a drift aborts at draw time; now `BindingSizeTooSmall` at bind-group creation, and the test names the file |
| `crates/wc3-render/src/ui.rs` | `min_binding_size: Some(size_of::<ViewportUniform>())`; test `viewport_uniform_matches_the_wgsl_struct` | same class, HUD pass |
| `crates/wc3-render/src/particles.rs` | `const _: () = assert!(size_of::<CameraUniform>() == 128)`; test `camera_uniform_matches_the_wgsl_struct` | the particles camera is a *second* camera shape; nothing tied Rust to WGSL |
| `crates/wc3-render/src/frame.rs` | none (reverted) | `UniformRing` is dead code (`rg UniformRing` → declaration only); its hardcoded `"perf-uniform-ring"` label needs a struct field to fix, not worth touching a type nothing constructs — see `READONLY_PATCHES.md` §3 |

No layout was resized to make a number match; the WGSL and Rust sides are
compared, and the guards make a future drift fail loudly and early.

## Verification summary

* `scripts/h2-cargo check -p herofolends-android --offline` → rc 0 (11:47).
* `scripts/h2-cargo build -p herofolends-android --release --offline` → rc 0, 3 m 25 s (12:00).
* `bash scripts/build-apk.sh` → rc 0; `apksigner` v2+v3 verified.
* `su -c 'pm install -r -d …'` → `Success`; installed `base.apk` sha256
  `82a8b672…` == local APK.
* 5 cold starts with `GAME_FIRST_FRAME surface=2414x1080 … units=13`, zero
  `P0_PASS_ERROR`, zero FATAL (details in `EVIDENCE.md` §5).
* `scripts/h2-cargo test -p wc3-render --lib --offline` → 110 passed / 0 failed.

## Open tails (named owner + exact next step)

1. **`crates/wc3-game` lib tests cannot compile right now** — owner: the packet
   that is editing `crates/wc3-game/src/mobile_hud.rs` (READ_ONLY here; 17 type
   errors at 12:17, e.g. `mobile_hud.rs:9660 expected Option<MenuPart>, found
   Option<usize>`, `E0063`). Next step for whoever owns it: make that file
   compile, then run
   `scripts/h2-cargo test -p wc3-game --lib --offline "render::tests::camera_uniform_matches_both_shaders"`
   and expect `1 passed`. Not a blocker for this packet: the same assertion runs
   at runtime on the device (`GAME_CAMERA_UNIFORM … terrain_wgsl=176`).
2. **`crates/wc3-render/src/mesh.rs` camera layout still has
   `min_binding_size: None`** — READ_ONLY here; exact patch in
   `READONLY_PATCHES.md` §1.
3. **`crates/wc3-game/src/terrain_render.rs` has no local constant for its
   `Camera` size** — READ_ONLY here; defence-in-depth patch in
   `READONLY_PATCHES.md` §2 (already covered by the `render.rs` cross-check).
4. **Separate defect, not this packet**: `FORTIFY: pthread_mutex_lock called on
   a destroyed mutex` on `hwuiTask0`, one occurrence at 12:03:58 (pid 29989,
   `analysis/agent-crash/start-4.log`), after frames had rendered and with no
   wgpu error. Outside `crates/wc3-render` / `crates/wc3-game/src/render.rs`;
   needs the Android/SDL lifecycle owner (`apps/**`). Repro hint: switch apps
   while the game is resuming (`am start` from a su shell while another app holds
   focus, then bring it forward).

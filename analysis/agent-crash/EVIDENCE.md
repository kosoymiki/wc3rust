# EVIDENCE — packet A / ledger 480 (crash-hunter)

All timestamps device-local (MSK, `h2oc now` 2026-11-17). Every claim below is
tied to a command output or a file:line; nothing here is "should".

## 0. Environment / authority (observed)

```
$ su -c 'id'
uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0
$ command -v rtk r2 su / rtk --version / node --version
rtk      /data/data/com.termux/files/usr/bin/rtk
r2       /data/data/com.termux/files/usr/bin/r2
su       /data/data/com.termux/files/usr/bin/su
rtk 1.1.0
v24.22.2
$ rtk rewrite "ls -la /data/data/com.termux"
rtk ls -la /data/data/com.termux        (rc=0)
```

Repo is not a git repo — no `git` command was run (packet pitfall).

## 1. The abort (observed, three independent occurrences)

Source of truth: `analysis/crash-20261117.txt` (tombstones) and
`analysis/device-qa-20261117/logs/launch-01.log` (live, installed build).

```
11-17 11:13:52.850 24928 24959 F libc : Fatal signal 6 (SIGABRT) ... tid 24959 (SDLThread) pid 24928
11-17 11:13:54.084 ... Abort message: 'wgpu error: Validation Error
11-17 11:13:54.084 ...   In a CommandEncoder, label = 'game-encoder'
11-17 11:13:54.084 ...     In a draw command, kind: Draw
11-17 11:13:54.084 ...       In bind group index 0, the buffer bound at binding index 0
                              is bound with size 112 where the shader expects 176.'
11-17 11:20:35.147 30635 30669 F libc : Fatal signal 6 (SIGABRT) ... pid 30635
11-17 11:20:35.469 ... same message, same 112 vs 176
11-17 11:39:12.423 13189 13240 F libc : Fatal signal 6 (SIGABRT) ... pid 13189   <- live run
```

Tombstone backtrace (`analysis/crash-20261117.txt:45-48`) locates the raise:

```
#11 wgpu::backend::wgpu_core::default_error_handler
#12 wgpu_core::ContextWgpuCore::handle_error_inner
#13 wgpu_core::CoreCommandEncoder ... CommandEncoderInterface::finish
#14 wc3_game::render::Renderer::render_with+5836
```

i.e. wgpu's default handler (an unconditional abort) fired at `encoder.finish()`
inside `render_with` — the message carries the encoder label, never the pass.

## 2. Which build each abort belongs to (observed)

| abort | pid | APK build | source mtimes at that time |
|---|---|---|---|
| 11:13:54 | 24928 | pre-11:13 | `render.rs` < 11:13 |
| 11:20:35 | 30635 | 11:18 (`build/android-p0/herofolends-p0.apk`, 11:18) | `terrain_render.rs` 11:20, `render.rs` 11:25 |
| 11:39:12 | 13189 | same 11:18 APK (still installed) | unchanged since 11:28 |

```
$ ls -la --time-style=+%m-%d\ %H:%M crates/wc3-game/src/render.rs crates/wc3-render/src/mesh.rs ...
-rw------- 11-17 11:25 crates/wc3-game/src/render.rs
-rw------- 11-17 11:28 crates/wc3-render/src/mesh.rs
-rw------- 11-17 11:28 crates/wc3-render/src/fog.rs
-rw------- 11-17 11:20 crates/wc3-game/src/terrain_render.rs
-rw------- 11-18 11:18 build/android-p0/herofolends-p0.apk
```

### "intermittency" is not run-to-run

`launch-01.log` (the APK built at 11:18, installed now) aborts on its **first
rendered frame**, every launch:

```
11:39:03.816  P0_SURFACE configured adapter Adreno (TM) 730 ... present=Mailbox
11:39:09.607  GAME_READY ... models=93 textures=800          (assets decoded)
11:39:09.652  GAME_SCENE_CENSUS ... unit_placed=88            (sim ready)
11:39:12.411  GAME_TIME day=1 hour=00.00                      (first frame)
11:39:12.412  GAME_SKIN mode=cpu ... draws=49                 (first prepared frame)
11:39:12.423  Fatal signal 6 (SIGABRT)
```

`GAME_CAMERA_UNIFORM` appears **0×** in that 2.6 MB log — the line was added at
11:25 with the 176-byte fix, so the build predates it. The 2.7 s between
`GAME_READY` and `GAME_TIME` is `Renderer::new` uploading 800 textures, not
frames. `GAME_FIRST_FRAME` (`apps/android-native/src/lib.rs:1733`, printed after
the first successful present) appears **0×** in both `launch-01.log` and
`attempt-03-game.log`, so the frame never reached the swapchain. Hence the abort
is **build-dependent and deterministic**, not intermittent; the "same APK ran
fine with `tick=684`" run belongs to an earlier build (before `TERRAIN_SHADER`'s
`Camera` grew to 176 B).

## 3. The 112-byte producer (observed + inference, labelled)

*observed*: `crates/wc3-game/src/render.rs:55` `struct CameraUniform` today is
`mat4x4 + 7 × vec4` = 176 B, pinned by
`render.rs:78` (`assert!(CAMERA_UNIFORM_BYTES == mesh::CAMERA_UNIFORM_SIZE)`) and
`render.rs:82` (`assert!(== 176)`); `mesh.rs:313` and `mesh.rs:2164` pin the mesh
side; `TERRAIN_SHADER`'s `Camera` (`terrain_render.rs:818-833`) is `mat4x4 + 7 ×
vec4` = 176 B.

*observed*: the only group-0 uniform shapes in the tree are 176 (game/mesh/
terrain camera), 128 (particles camera), 96 (fog, draw params), 16 (UI viewport,
terrain params). None is 112 (`analysis/agent-crash/AUDIT.md`, table).

*inference (high confidence)*: 112 = `mat4x4 (64) + 3 × vec4 (48)` = the
pre-lighting game camera `{view_proj, light, sun, ambient}`. That struct is the
only 176-byte consumer in the tree whose Rust side is a plain `Pod` copy of the
WGSL struct; the WGSL side had already grown (`day`, `fog`, `fog_range`, `eye`)
in the same refactor window, so the 11:18 build bound 112 B to a 176-byte
shader.

### Byte-level proof in the shipped binary (observed, r2)

The APK installed at 11:20:23 was still on the device, so its `libmain.so`
(11:18) could be inspected directly:

```
$ su -c 'ls -la /data/app/*/org.herofolends.p0-*/lib/arm64/libmain.so'
-rwxr-xr-x 1 system system 12362288 2026-11-17 11:18 .../lib/arm64/libmain.so
$ strings -a libmain-1118-stale.so | grep -c GAME_CAMERA_UNIFORM
0                                   <- the 176-byte log line is not in this build
$ rabin2 -s libmain-1118-stale.so | grep Renderer3new
22299 0x005070ac 0x005070ac LOCAL FUNC 7088 _RNvMs_..._8wc3_game6renderNtB4_8Renderer3new
```

Disassembly of `Renderer::new` (`r2 -c 'pd 46 @ 0x5070d8'`), the camera buffer
right after the two bind-group layouts:

```
0x0050710c  bl <DeviceInterface>::create_bind_group_layout     ; game-camera-layout
0x00507120  bl <DeviceInterface>::create_bind_group_layout     ; texture layout
0x00507124  adrp x8, 0x177000
0x00507128  add  x8, x8, 0x532        ; 0x177532 = "game-camera-buffer"
0x00507130  str  x8, [sp, 0xbc0]      ; BufferDescriptor.label.ptr
0x0050713c  ldr  q0, [x8, 0xb80]      ; 16 B constant from 0x1b8b80
0x00507148  str  w22, [sp, 0xbd8]     ; w22 = 0x48 = UNIFORM|COPY_DST
0x00507150  stur q0, [r23, 0x30]      ; -> [sp, 0xbc8]
0x00507158  bl   <DeviceInterface>::create_buffer

$ r2 -c 'pxq 32 @ 0x1b8b80'   libmain-1118-stale.so
0x001b8b80  0x0000000000000012  0x0000000000000070     ; 18 (= len), 112 (= size)
$ r2 -c 'pxq 16 @ 0x1bbcc0'   libmain-1118-stale.so
0x001bbcc0  0x0000000000000070  0x0000000000000000     ; BufferBinding size = 112
$ r2 -c 'ps @ 0x177532'       libmain-1118-stale.so
game-camera-buffer
```

So in the shipped build the `game-camera-buffer` descriptor is
`{label: "game-camera-buffer", size: 112, usage: 0x48}` and the bind group entry
binds `BufferBinding { offset: 0, size: 112 }`. That is the 112 in
`bound with size 112 where the shader expects 176`, measured, not inferred. The
`Camera` it is bound against is the terrain/mesh 176-byte one (section 3), so the
first group-0 draw of the frame fails.

*inference (high confidence) — offending pipeline*: the **terrain** pipelines
(`draw_terrain`/`draw_cliffs`/`draw_water`/`draw_shore`), which are the first
draws in the frame that consume `@group(0) @binding(0)` with the game camera
(`render.rs:708` sets it, `render.rs:711-712` draw). The mesh pipelines bind
their own 176-byte `mesh-camera-bind` (`mesh.rs:984`), so they cannot produce
the 112-byte message; particles (128) and fog (96) report their own sizes.

## 4. Static verification of the fixed tree

```
$ scripts/h2-cargo check -p herofolends-android --offline
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.57s     (rc=0)
$ scripts/h2-cargo test -p wc3-render --lib --offline
test result: ok. 109 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    (includes ui::tests::viewport_uniform_matches_the_wgsl_struct,
             fog::tests::fog_uniform_matches_the_wgsl_struct,
             particles::tests::camera_uniform_matches_the_wgsl_struct)
```

## 5. Device verification (5 cold starts)

Build under test: `target/release/libmain.so` 12:00 (12 387 408 B), APK
`build/android-p0/herofolends-p0.apk` `82a8b67211846b7e0e588a4cc1c4b27f96459cd7c93002cfe7a37241c1d2eb4c`.
Install + identity check (the tech-lead's requirement — same sha or the test is
meaningless):

```
$ su -c 'pm install -r -d build/android-p0/herofolends-p0.apk'
Success
$ sha256sum /data/app/*/org.herofolends.p0-*/base.apk
82a8b67211846b7e0e588a4cc1c4b27f96459cd7c93002cfe7a37241c1d2eb4c  .../base.apk
$ sha256sum build/android-p0/herofolends-p0.apk
82a8b67211846b7e0e588a4cc1c4b27f96459cd7c93002cfe7a37241c1d2eb4c  build/android-p0/herofolends-p0.apk
```

`analysis/agent-crash/cold-starts.sh` (round 1) and `cold-starts2.sh` (round 2/3,
first-frame acceptance: `GAME_FIRST_FRAME` + `GAME_SKIN` + alive + no
`FATAL`/`P0_PASS_ERROR`) produced **five cold starts with a presented first
frame and no abort**:

| # | start | focus | `GAME_FIRST_FRAME` | `GAME_SKIN` | `melee-ready` | `P0_PASS_ERROR` | FATAL |
|---|---|---|---|---|---|---|---|
| 1 | 12:03:16 (`start-3.log`) | game | yes | 1 | 1 | 0 | 0 |
| 2 | 12:04:12 (`start-5.log`) | game | yes | 1 | 1 | 0 | 0 |
| 3 | 12:05:56 (`start-v2-1.log`) | game | yes | 1 | 2 | 0 | 0 |
| 4 | 12:06:23 (`start-v2-2.log`) | Instagram | yes | 1 | 2 | 0 | 0 |
| 5 | 12:11:38 (`start-v3-3-1.log`) | game | yes | 1 | 2 | 0 | 0 |

Every one of the five logged the startup contract line (runtime cross-check of
the terrain shader, parsed on the device):

```
I Herofolends: GAME_CAMERA_UNIFORM bytes=176 buffer_bytes=176 mesh_wgsl=176 terrain_wgsl=176 terrain_bind=game upholds=176
```

Compare with the shipped-binary 112 in section 3: the same buffer is now created
and bound at 176 B, and the same code that used to abort at the first world frame
now presents frames.

### Runs that were not usable as evidence (recorded, not hidden)

The operator was using the phone throughout (Telegram, Instagram, launcher), so
`am start` sometimes created the Activity without it being RESUMED — SDL pauses
(`GAME_LIFECYCLE ... block_on_pause=1`) and `render_with` is never called:

| start | state | why not counted |
|---|---|---|
| 12:02:22 (`start-1.log`) | alive 20 s, camera=176, `melee-ready` ×2, `skin=0` | never resumed → no frame |
| 12:02:50 (`start-2.log`) | died after 1 s | process gone before the first frame, no crash record |
| 12:06:40 (`start-v2-3.log`) | alive, camera=176, `skin=0` | Instagram held focus → paused |
| 12:07:38 (`start-v2-4.log`) | alive, camera=176, `skin=0` | created but not resumed |
| 12:08:31 (`start-v2-5.log`) | alive, camera=176, `skin=0` | Instagram held focus → paused |
| 12:10:40, 12:11:38 (`v3-1`, `v3-2`) | alive, camera=176, `skin=0` | launcher/Instagram held focus |

### One separate defect seen once (not the wgpu crash)

Round 1, start 4 (12:03:44, pid 29989) reached the world (`camera=176`,
`GAME_SKIN=1`, `melee-ready=1`, `P0_PASS_ERROR=0`) and then died 14 s later on a
**different** thread and a **different** failure:

```
11-17 12:03:58.498 29989 30040 F libc : FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x783e01f498)
11-17 12:03:58.499 29989 30040 F libc : Fatal signal 6 (SIGABRT) ... tid 30040 (hwuiTask0)
```

No wgpu validation error, no `bound with size`, no `P0_PASS_ERROR` — this is an
Android HWUI/lifecycle mutex use-after-destroy, outside the renderer and outside
this packet's OWNER paths (`apps/**` is READ_ONLY for me). It occurred while the
operator was switching apps around the launches. Routed to the tech-lead; not
fixed here and not claimed as fixed.

### Mirror check on the new binary (observed, r2)

Same symbol, same call site as the shipped 11:18 build in section 3:

```
$ rabin2 -s target/release/libmain.so | grep Renderer3new
22206 0x004b0194 0x004b0194 LOCAL FUNC 7688 _RNvMs1_..._8wc3_game6renderNtB5_8Renderer3new
$ r2 -c 'pd 600 @ 0x4b0194' ...           ; camera buffer creation
0x004b032c  adrp x8, 0x176000
0x004b0330  add  x8, x8, 0x659             ; 0x176659 = "game-camera-buffer"
0x004b0344  ldr  q0, [x8, 0x470]           ; constant from 0x1ba470
0x004b0354  str  w22, [sp, 0x358]          ; 0x48 = UNIFORM|COPY_DST
0x004b0360  bl <DeviceInterface>::create_buffer
$ r2 -c 'ps @ 0x176659' / -c 'pxq 16 @ 0x1ba470' target/release/libmain.so
game-camera-buffer
0x001ba470  0x0000000000000012  0x00000000000000b0   ; len 18, size 176
```

| build | `game-camera-buffer` descriptor | first world frame |
|---|---|---|
| installed 11:18 | `{len 18, size 0x70 = 112, usage 0x48}` | SIGABRT (112 vs 176) |
| new 12:00 | `{len 18, size 0xb0 = 176, usage 0x48}` | presented ×5 |

## 6. Commands run (rc evidence)

| command | rc | output |
|---|---|---|
| `scripts/h2-cargo check -p herofolends-android --offline` | 0 | `Finished dev profile … 10.57s` |
| `scripts/h2-cargo build -p herofolends-android --release --offline` | 0 | `Finished release profile … 3m 25s` (`CARGO_RC=0`) |
| `bash scripts/build-apk.sh` | 0 | `APK: …/herofolends-p0.apk`, `apksigner` v2+v3 verified |
| `su -c 'pm install -r -d …'` | 0 | `Success` (sha256 match above) |
| `scripts/h2-cargo test -p wc3-render --lib --offline` | 0 | `110 passed; 0 failed` (incl. the three new WGSL-agreement tests) |
| `scripts/h2-cargo test -p wc3-game --lib --offline render::tests::camera_uniform_matches_both_shaders` | 101 | **blocked**: `crates/wc3-game/src/mobile_hud.rs` (READ_ONLY, another packet's in-flight edit at 12:09-12:14) has 17 type errors — `expected Option<MenuPart>, found Option<usize>` at `mobile_hud.rs:9660`, `E0063`/`E0308`. Not this packet's code; `cargo check -p herofolends-android` and the release build were green at 11:47/12:00. |

The terrain half of the camera contract is nevertheless proven on the device:
`GAME_CAMERA_UNIFORM … terrain_wgsl=176 …` is produced by
`wc3_render::mesh::wgsl_struct_size(TERRAIN_SHADER, "Camera")` at `Renderer::new`,
i.e. the exact value the unit test asserts, computed on the shipped binary.

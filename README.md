# wc3rust

wc3rust is a clean-room Rust/Vulkan real-time-strategy engine that runs on the operator's own
Warcraft III: Reign of Chaos / The Frozen Throne data. It is not a fork, a repack or a wrapper of
another engine: the simulation, renderer, formats, HUD and touch layer are written here, in Rust,
against the user's legally owned game files that the app reads from its private data directory at
runtime. Android (Termux, aarch64, SDL3 + wgpu/Vulkan) is the shipping target; a host build runs the
same crates for tests, tools and headless proofs.

The repository contains no Blizzard code, models, textures, maps or sounds. Warcraft is a trademark
of Blizzard Entertainment; this project is not affiliated with Blizzard. See `docs/LEGAL.md`.

## Requirements and toolchain

| Item | Value |
| --- | --- |
| Language | Rust 1.99.1, edition 2024 |
| Workspace | 30 members (`crates/`, `apps/`, `tools/`) |
| Platform | Termux/Android aarch64 (primary), host Linux for tests/tools |
| Graphics | SDL3 + wgpu 30 / Vulkan; Turnip through `crates/wc3-adrenotools` |
| Android SDK | `aapt2`, `javac`, `d8`, `zipalign`, `apksigner`, `keytool`, `android.jar` (auto-detected at `$PREFIX/tmp/opencode/android-sdk/android-36/android.jar`) |
| Root | `su` (KernelSU) for install, data migration and logcat |
| SDL3 native | prebuilt at `third_party/sdl3-android-build/libSDL3.so` |

`scripts/h2-cargo` is the only cargo entry point on this device. It serialises every cargo
invocation through one global flock slot, waits for a memory floor before starting, sets
`CARGO_BUILD_JOBS=2` and caps test runs with `RUST_TEST_THREADS=2`; the log is
`$PREFIX/tmp/h2-cargo.log`. Two concurrent heavy test binaries exhaust the 11 GB phone and the
kernel kills a process mid-run, so bare `cargo` is not used.

## Build, install, run

Host tests (the two gates that cover the renderer and the terrain path):

```sh
scripts/h2-cargo test -p wc3-render --lib --offline
scripts/h2-cargo test -p wc3-game --lib --offline terrain_render::
```

Workspace-wide gate (derives its member list from `cargo metadata`, one job at a time):

```sh
scripts/test-all.sh --check-only     # cargo check --workspace
scripts/test-all.sh                  # check + Android target checks + per-member tests
```

Android APK — two steps, release APK lands in `build/android-p0/release/`:

```sh
scripts/h2-cargo build --release -p wc3rust-android
scripts/build-apk.sh --release
# -> build/android-p0/release/wc3rust-p0.apk
```

Install and launch on the device:

```sh
su -c 'pm install -r -d build/android-p0/release/wc3rust-p0.apk'
su -c 'am start -n org.wc3rust.p0/org.libsdl.app.SDLActivity'
su -c 'logcat -d -v brief | grep wc3rust'
```

### Application ID and data migration

The application ID is **`org.wc3rust.p0`** (`apps/android/app/src/main/AndroidManifest.xml`). It is
the only package the build produces; every runtime path in the engine points at
`/data/data/org.wc3rust.p0/files`.

An install that predates the package rename (`org.herofolends.p0`) is migrated by copying its data
directory into the new package, never by moving or deleting it. The old package and its data stay
untouched, and the old APK keeps working:

```sh
su -c 'pm install -r -d build/android-p0/release/wc3rust-p0.apk'
su -c 'mkdir -p /data/data/org.wc3rust.p0/files'
su -c 'cp -a /data/data/org.herofolends.p0/files/. /data/data/org.wc3rust.p0/files/'
su -c 'chown -R u0_a360:u0_a360 /data/data/org.wc3rust.p0/files'   # uid of org.wc3rust.p0
su -c 'restorecon -R /data/data/org.wc3rust.p0/files'
```

Take the uid from `su -c 'stat -c %u /data/data/org.wc3rust.p0'` — it is allocated per install. The
first launch then finds the game archives, the melee map and the HUD pack in the new directory. A
checkpoint written before the rename is still read when `wc3rust-resume.sav` is absent, and the next
save writes the new name (`apps/android-native/src/lib.rs`, `pre_rename_checkpoint`).

### Data files in the app directory

`/data/data/org.wc3rust.p0/files` is the operator's data root:

```
/data/data/org.wc3rust.p0/files/
  War3.mpq  War3x.mpq  War3xlocal.mpq      the user's own game archives
  Melee.w3x  Orc01.w3m  Human04.w3m  Maps/ maps
  hud_pack/                               the installed HUD asset pack (see below)
  turnip/                                 the Turnip ICD drop (scripts/setup-turnip.sh)
  ui_atlas.json  ui_atlas.png  ui_atlas_insets.json
  wc3rust-resume.sav  wc3rust-resume.sav.meta
```

The Turnip ICD is shipped disabled (it sits in that directory as `freedreno_icd.disabled`), so the
default path is the vendor Vulkan driver. The resume checkpoint is written when the app is
backgrounded.

## Repository contents

### Engine crates

| Crate | Role |
| --- | --- |
| `crates/wc3-core` | Deterministic core types and fixed-point math |
| `crates/wc3-mpq` | MPQ archive reader (StormLib-compatible core) |
| `crates/wc3-formats` | MDX, BLP, SLK, W3E/W3I, object data, map containers |
| `crates/wc3-data` | Typed game-data database (units, abilities, items, upgrades) |
| `crates/wc3-path` | Terrain grid pathfinding: A*, flow fields, collision |
| `crates/wc3-sim` | Fixed-tick simulation: ECS, orders, combat, economy, abilities |
| `crates/wc3-jass` | JASS2 lexer, parser, interpreter, triggers, native registry |
| `crates/wc3-anim` | MDX animation sampling, geoset animation, particles, ribbons |
| `crates/wc3-input` | Touch router, gestures, RTS camera math, minimap mapping |
| `crates/wc3-audio` | WAV/MP3 decoding, spatial mixer, music and 3D pan |
| `crates/wc3-bink` | Bink 1 video / BinkAudio decoder for the campaign movies |
| `crates/wc3-replay` | Replay format, command log, checksums, lockstep session |
| `crates/wc3-assetdb` | Bounded caches, asset providers, catalogs, dependency graph |
| `crates/wc3-render` | wgpu/Vulkan renderer: static geometry, GPU skinning, state-sorted draws, UI |
| `crates/wc3-game` | Playable slice: commands, AI, HUD, menus, campaign, touch front end |
| `crates/wc3-adrenotools` | Clean-room Rust port of libadrenotools: namespace driver loading |
| `crates/wc3-adrenotools-main-hook` | `dlopen` interposer (libmain_hook.so) |
| `crates/wc3-adrenotools-fopen-hook` | `fopen` redirect interposer (libfile_redirect_hook.so) |
| `crates/wc3-adrenotools-gsl-hook` | GSL allocator interposer (libgsl_alloc_hook.so) |
| `crates/wc3-vulkan-loader` | Loader shim: AOSP loader plus a Turnip ICD through the driver loader |
| `crates/wc3-turnip-vendor` | Vendor-driver shim exposing a full Vulkan API over the Turnip ICD |
| `crates/wc3-gpu` | GPU feature probing and adapter selection |
| `crates/wc3-platform` | Platform services shared by Android and host |
| `crates/wc3-re` | Resource/VM/object-data layer used by the reverse-engineering tools |

`crates/wc3-guest` (x86-32 guest model for the static-recompilation research path) is on disk but not
a workspace member.

### Apps and tools

| Target | Role |
| --- | --- |
| `apps/android-native` | Android entry point (libmain.so): SDL3 window, wgpu surface, checkpointing |
| `apps/android` | Java SDLActivity shell, manifest, resources and the APK packaging tree |
| `apps/bench` | `wc3rust-bench` and `wc3rust-gpu-bench` host benchmarks |
| `apps/headless` | `wc3rust-headless` — run a map for N ticks without a window |
| `apps/smoke` | `p0-smoke` boot smoke test |
| `apps/driver-probe` | Vulkan/Turnip driver probe |
| `tools/mdx-gltf` | `mdx-gltf` MDX -> glTF 2.0 exporter for visual verification |

### HUD art production pipeline

The manifest-driven multi-agent asset generator is documented in [tools/hud_artisan/README.md](tools/hud_artisan/README.md).
It compiles per-asset image jobs, supports local ComfyUI and custom CLI adapters, verifies provenance,
and checks output against the 92-asset engine contract. It does not fabricate a missing pack manifest
or claim artistic approval from file-generation success.

### HUD asset pack

The HUD is manifest-driven; no pixel offset is hand-tuned. The pack layout is:

```
manifest.json          # 92 assets: w/h/insets/states, atlas, safe areas, design height, fonts
atlas/  commandcard/  console/  controlgroups/  cursor/  fonts/  icons/  menus/
minimap/  portrait/  resources/  selection/  tooltip/  topbar/  docs/  source/  build/
SHA256SUMS.txt
```

The source of truth is `analysis/hudskin/final/wc3rust-hud-final/manifest.json`; the installed copy
is the flat `hud_pack` directory in the app's files dir. `crates/wc3-game/src/hudskin/pack.rs`
searches the app dir, the operator's Download drop and the analysis tree, and fails loudly
(`HUD_PACK missing=…`) instead of substituting art. The mobile relayout spec, device metrics and
measurements live in `HUD_LAYOUT_QA.md`.

### Audit, testing and device evidence

- `docs/audit/WC3RUST_CODE_AUDIT.md` — the single audit registry: every item carries `FIXED`,
  `REPRODUCED_NOT_A_BUG` or `BLOCKED`, with a regression test and the commit that closed it.
- `docs/TESTING.md` — device test procedure and the log contract (`GAME_FIRST_FRAME`, `GAME_DATA`,
  `GAME_RESUME`, `GAME_CHECKPOINT`).
- `scripts/qa/device_evidence.sh` and `scripts/qa/device_p0_cycle.py` — install, launch, screenshot
  and log capture with one JSON report per run.
- `scripts/game-selftest.sh` — launch, wait for the first frame, collect logcat into one log.

### Backups

`~/bin/full-backup.sh` and `~/bin/full-restore.sh` snapshot and restore the whole working set (repo,
`~/.config/opencode`, H2 state, databases); the plan and retention rules are in `~/BACKUP_PLAN.md`.

### Device automation (CN kit)

The DeepTesting/CN device automation lives outside the repo: `~/deeptesting_proxy` with the
`~/bin/deeptest-cn` launcher (`~/bin/deeptest-stop`, `~/bin/deeptest-unblock` for control).

## Controls on a phone

Touch-first, no mouse required: tap selects, drag pans the camera, pinch zooms, a two-finger tap is
the right-click (smart order), long-press shows tooltips and confirms destructive orders. The
keyboard is replaced by on-screen controls: a console band with menu/idle-worker/control-group
buttons, a 3×3 command card, the minimap (tap to jump, drag to pan) and an army chip. Targets stay
at 44–48 dp minimum. The full gesture contract and layout zones are in `docs/TOUCH.md`.

## Known limits and reporting a defect

- The Android UI thread can abort when the activity is backgrounded (`FATAL … SIGABRT … hwuiTask0`)
  while the native surface is being torn down. The surface guard is covered by
  `crates/wc3-render/tests/surface_guard.rs`; the acquire-freeze fix is the next device build — see
  `HUD_LAYOUT_QA.md`.
- Turnip ships disabled (`freedreno_icd.disabled`): the default is the vendor Vulkan driver.
  `docs/GPU_DRIVERS.md` describes enabling the ICD.
- Campaign movies and some campaign maps need the operator's extra archives; the engine skips them
  with a log line instead of inventing data.

To report a defect, attach: the logcat slice (`su -c 'logcat -d -v brief | grep wc3rust'`), a
screenshot from `scripts/qa/device_evidence.sh`, the output of `su -c 'pm dump org.wc3rust.p0'` and
the exact commit (`git rev-parse HEAD`). `docs/TESTING.md` lists the lines that make a report
actionable.

# P0 EVIDENCE — 2026-11-12

Все команды выполнены на RMX3709 (Android 16, aarch64) в Termux. Никакие Blizzard-ассеты
не добавляются в git; `analysis/war3_samples/` и `third_party/` исключены через `.gitignore`.

## 1. Toolchain

| компонент | observed |
|---|---|
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) |
| cargo | 1.99.1 |
| SDL3 | **3.5.0** (source build, static `libSDL3.a` 2.7 MB) |
| wgpu | 30.0.1 (Vulkan backend) |
| Vulkan instance | 1.4.368 (Termux loader) |
| GPU | **Turnip Adreno (TM) 725**, Mesa 26.2.6, Vulkan 1.4.354; llvmpipe fallback |
| StormLib | bundled libtommath/libtomcrypt static `libstorm.a` |

## 2. SDL3 + wgpu headless triangle

Command: `SDL3_PREFIX="$PWD/third_party/sdl3-min" ./target/release/p0-smoke`

```
SDL3_VERSION=3.5.0
GPU_ADAPTER name="Turnip Adreno (TM) 725" backend=Vulkan device_type=IntegratedGpu driver="turnip Mesa driver" driver_info="Mesa 26.2.6"
P0_GPU_TRIANGLE_OK lit_pixels=20808 ppm=target/p0_triangle.ppm
```

- Real Vulkan path: `wgpu::Instance` VULKAN → high-performance adapter → device → WGSL render
  pipeline → 256×256 `Rgba8Unorm` target → `copy_texture_to_buffer` → map/read.
- `lit_pixels=20808` (triangle pixels with g>100 && b>100); background cleared to (0.02,0.03,0.06).
- PPM: `target/p0_triangle.ppm`, 192 KiB, sha256
  `f3657bad4ab15c8f6e16010b6ca9f957e77a33c21edebe54aaca28feddffc5ac`.
- SDL3 initialized with `SDL_INIT_VIDEO|SDL_INIT_EVENTS`; version query corrected to SDL3's
  packed-int `SDL_GetVersion()` (initial struct FFI returned 0.0.0; fixed).
- Host offscreen `SDL_CreateWindow`/event-pump probes were tried and **removed after a segfault**
  on the Termux offscreen driver. The real SDLActivity window/surface path belongs to the Android
  APK slice (P0 remaining); P0 keeps the stable SDL3 init/version + wgpu headless triangle.

## 3. StormLib / War3.mpq oracle

- `War3.mpq` extracted from `Warcraft III.iso` (438 529 036 B; ISO built from MDF via `mdf2iso`).
- `tools/storm_probe` (StormLib API) opened the archive **without errors** and listed
  **10 525 files** (`analysis/war3_mpq_listing.txt`, 10 527 lines incl. header/summary).
- Extracted samples (`analysis/war3_samples/`):
  - `Units/Human/Footman/Footman.mdx` 105 022 B, magic `MDLX`, `VERS` version **800**
  - `Textures/Footman.blp` 98 795 B, magic `BLP1`, version **0**
  - `Units/UnitData.slk` 197 885 B, magic `ID;PWXL`
  - plus 2 command-button BLP, portrait/campaign textures, Footman blood MDX.
- SHA256: see `analysis/war3_samples/SHA256SUMS`.
- Rust `wc3-formats` tests parse these real fixtures: MDX version, BLP1, SLK detection.

## 4. Tests / quality

- `SDL3_PREFIX="$PWD/third_party/sdl3-min" cargo test --workspace` → **8 passed, 0 failed**
  (4 core fixed-point/ids + 4 real-fixture format detection/MDX/BLP/SLK).
- `cargo fmt --all -- --check` → clean.
- `cargo clippy --workspace --all-targets -- -D warnings` → **No issues found**.
- Reproducer: `./scripts/p0-smoke.sh` (builds, tests, runs; writes `analysis/p0-smoke.out`).

## 5. Provided Turnip (Turnip_Gen8_V39_Patched.zip)

- `meta.json`: name `Turnip Gen8 V39-patched`, description **Turnip A8xx Android/Bionic**,
  driverVersion Vulkan 1.4.367, minApi 28, packageVersion 34.
- `libvulkan_freedreno.so`: 8 460 728 B, sha256
  `2db3c4601bc3f046596f86d9d189cb5cad693812f8c8c55fe1817ec90ea879ac`, ELF aarch64 Android 34,
  NDK r29, stripped.
- Standard Vulkan loader cannot use it as ICD:
  `vk_icdGetInstanceProcAddr`/`vkGetInstanceProcAddr` symbols absent; loader logs
  "Attempt to retrieve ... failed" → no drivers. `ctypes.CDLL` loads the ELF but symbols are
  undefined.
- Device reports **Adreno 725 (Gen7 A7xx)**, while the pack is **A8xx (Gen8)**; this pack is not
  the right driver for this device. It stays in `third_party/turnip/` as a user-provided pack for
  future Gen8 devices / libadrenotools-in-app loading, **not** the default ICD.
- Working P0 driver: Termux Turnip **Mesa 26.2.6** (`freedreno_icd.aarch64.json`), used by wgpu.

## 6. P0 gate status

| gate | status |
|---|---|
| donor matrix | done (21 repos + local, `docs/DONORS_MATRIX.md`) |
| local artifacts RE | done (`docs/REVERSE_ENGINEERING.md`) |
| data path | done: ISO MPQ opens with StormLib; samples extracted |
| SDL3 platform smoke | done: 3.5.0 init/version (host offscreen window/event probe unstable, removed) |
| SDL3 window + surface | pending: Android SDLActivity/NDK slice (host offscreen window crashed; no NDK/APK yet) |
| wgpu Vulkan triangle | done: Turnip adapter, 20 808 lit pixels |
| format header tests | done: real MDX/BLP/SLK fixtures |
| provided Turnip | tested: incompatible with Gen7/standard ICD; documented |
| legal gate | docs complete; operator/legal sign-off still required before release |
| W3AndroidEngine baseline benchmark | pending (P0 remaining) |

## 6.1 P0 deliverable summary

**Done:** Rust workspace (`wc3-core`, `wc3-formats`, `wc3-platform`, `wc3-gpu`, `p0-smoke`),
SDL3 3.5.0 static build + init/version, wgpu 30.0.1 Vulkan headless triangle on Turnip
(Mesa 26.2.6), real `War3.mpq` StormLib oracle + MDX/BLP/SLK fixtures + Rust header tests,
Turnip pack provenance/compatibility evidence, docs/plan/diagrams.
**P0 remaining:** SDL3 window + wgpu surface in Android APK (needs NDK/APK slice), W3AndroidEngine
baseline benchmark, legal sign-off. These are explicit next steps, not claimed done.

## 7. Reproduce

```bash
cd ~/src/herofolends-rs
SDL3_PREFIX="$PWD/third_party/sdl3-min" cargo test --workspace
SDL3_PREFIX="$PWD/third_party/sdl3-min" cargo run --release -p p0-smoke
./tools/storm_probe /path/to/War3.mpq --limit 20
```


## 8. Android Vulkan driver loading — Rust adrenotools port (2026-11-12)

- `crates/wc3-adrenotools` is a complete Rust port of libadrenotools (linker
  namespace bypass, ELF soname patcher, dlopen/fopen/GSL hooks, KGSL mapping, BCn
  enabler). No C/C++ donor code is built or linked.
- `cargo test -p wc3-adrenotools` → **27 passed**; BCn patch bytes are identical to
  the donor blob; all scans/ELF offsets are bounds checked.
- `./target/debug/driver-probe` → **0 failures**:
  - hidden linker symbols resolve; private namespace loads a unique
    `libvulkan.so` (vendor `Adreno (TM) 730`);
  - hook redirects `vulkan.adreno.so` to Mesa Turnip; direct ICD query returns
    `Turnip Adreno (TM) 725`, Vulkan 1.4.354;
  - the provided Gen8 pack loads but has no ICD entry points (0 defined functions
    in `.dynsym`);
  - a generated ICD JSON + Khronos loader enumerates Turnip (recommended native
    path).
- `/system/lib64/libvulkan.so` is the AOSP shim, not an ICD loader: substituting
  Turnip there yields 0 devices. The game must bundle a Khronos loader (or use the
  ICD entry points directly).
- Docs: `docs/ADRENOTOOLS_RUST_PORT.md`, `docs/WINLATOR_TURNIP_RESEARCH.md`.


## 9. Full engine expansion (2026-11-12)

Implemented crates (all Rust): `wc3-mpq`, `wc3-formats` (MDX v800, BLP1/BLP2,
SLK, W3E, W3I, WPM, object data, INI, map), `wc3-assetdb`, `wc3-path`, `wc3-sim`,
`wc3-jass`, `wc3-replay`, `wc3-input`, `wc3-audio`, `wc3-anim`, `wc3-render`,
`apps/headless`, `apps/bench`; plus the earlier `wc3-core`, `wc3-platform`,
`wc3-gpu`, `wc3-adrenotools*` and the Android APK.

Real-fixture validation:
- `wc3-mpq`: 3/3 tests on `War3.mpq` (10,525 blocks; Footman.mdx 105,022 B,
  Footman.blp 98,795 B, UnitData.slk 197,885 B, listfile 526,563 B).
- `wc3-formats`: 20/20 tests on MDX/BLP/SLK/W3E/W3I/WPM/map/object data; campaign
  map `Human04.w3m` opens (129x129 terrain, 512x512 pathing, JASS script).
- `wc3-sim`: deterministic moves/combat/production/ability/state-hash tests.
- `wc3-jass`: loops/functions/arrays/triggers/unit natives.
- `wc3-replay`: `H2RP` roundtrip + re-simulation checksum + lockstep delay/desync.

OOM incident and guard (root cause + fix):
- dmesg: `Out of memory: Killed process 9843 (wc3_assetdb-986) anon-rss:7749064kB`.
  Causes: (1) uncompressed MPQ files were routed through the sector table;
  (2) MDX counts/inclusive sizes were not validated before allocation, so a
  misread could loop/allocate without bound; (3) tests were run uncapped in the
  interactive session.
- Fixes: direct raw path for uncompressed MPQ; per-sector codec tag 0x02/0x10;
  MDX count/stride checks, node/geoset/material size guards, EVTS/ATCH layouts;
  BLP1 JPEG header reconstruction (header_size + mip data) and BLP1 paletted
  alpha bits; W3M HM3W header → MPQ base offset; SLK stateful X/Y cells; object
  data v1 (no variation field).
- Test guard: `scripts/safe-test.sh <crate>` (one crate, detached,
  `CARGO_BUILD_JOBS=1`, `CARGO_PROFILE_TEST_DEBUG=0`, `--test-threads=1`,
  `ulimit -v 4.5GB`, log + `SAFE_TEST_DONE`).

r2 evidence (small fixtures, `r2 -n`): MDX `MDLXVERS` header, BLP `BLP1` header,
W3M `HM3W` header; MPQ sector header `02 78 9c` (zlib tag).

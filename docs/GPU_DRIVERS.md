# GPU DRIVERS — Turnip, libadrenotools, fallback

## 1. Why

Android Adreno drivers ship with the OS; Turnip (Mesa Freedreno Vulkan) is an open-source
alternative, freely distributed (Mesa core MIT; community builds e.g. K11MCH1/AdrenoToolsDrivers).
Turnip can improve compatibility/performance on some devices, but it is device/version-sensitive.
Our engine must work on **system Vulkan** first, with Turnip as an optional, user-selectable driver.

## 2. Components

| component | license | role |
|---|---|---|
| Mesa / Turnip | MIT (core; individual files may differ) | Vulkan driver source |
| libadrenotools | BSD-2-Clause | rootless loading of custom Adreno drivers |
| SDL3 | zlib | window/surface/lifecycle |
| wgpu | Apache-2.0/MIT | Vulkan renderer |
| system Adreno Vulkan | vendor | default fallback |

## 3. libadrenotools requirements (observed from `driver.h`)

- `adrenotools_open_libvulkan(dlopenMode, featureFlags, tmpLibDir, hookLibDir, customDriverDir,
  customDriverName, fileRedirectDir, userMappingHandle)`.
- `hookLibDir` must equal Android `ApplicationInfo.nativeLibraryDir`.
- `customDriverDir` must be in app internal storage (`internalDataPath`), **not** sdcard/storage
  (dlopen security).
- App must use `useLegacyPackaging = true` so native libs are extracted to `nativeLibraryDir`.
- Android 9+ arm64.
- Optional `adrenotools_set_turbo(true)` (thermal still applies), file redirect, BCn enable,
  shader dump, user memory mapping.

## 4. Driver manager design

```
Settings → GPU driver
  ├─ System Vulkan (default; always available)
  ├─ Import Turnip .so (SAF file picker)
  ├─ Download Turnip build (trusted URL; optional)
  └─ Per-profile: driver = system | turnip:<sha8>
```

- Verify SHA-256; display name/version/source/license.
- Copy to `internalDataPath/drivers/<sha>/libvulkan_freedreno.so`.
- Load via libadrenotools; if `vkEnumeratePhysicalDevices` returns 0 or init fails → fallback to
  system Vulkan and show error.
- No root; no `/sdcard` loading; no unknown binaries bundled in APK.
- Rollback button; clear driver cache; export logs (GPU name, driver version, features).
- License notices for Mesa/libadrenotools; user opt-in warning that custom drivers are experimental.

## 5. Compatibility matrix (observed 2026-11-12)

| device | SoC | Android | Vulkan | driver | triangle | notes |
|---|---|---|---|---|---|---|
| RMX3709 | Adreno 725/730 (Gen7) | 16 | 1.4.354 | **Termux Turnip Mesa 26.2.6** | **PASS** (`p0-smoke`, 20 808 lit px) | primary P0 path |
| RMX3709 | same | 16 | 1.4.368 loader | provided `Turnip_Gen8_V39_Patched` | **FAIL** | A8xx pack; no `vk_icd*` symbols; Gen7 device → not used |
| low-end | to pick | 12+ | to test | system/Turnip | pending | low profile |

Test protocol:
1. `vkEnumeratePhysicalDevices`, `vkGetPhysicalDeviceProperties`, `vkGetPhysicalDeviceFeatures2`.
2. Create device/queue/surface; render triangle; readback pixel.
3. Run 10-min scene; measure FPS/frame time/thermal.
4. Compare system vs Turnip on same build/map.
5. Log driver name/version/SHA; report unsupported.

## 6. SDL3 + wgpu surface

- SDL3 window flags `SDL_WINDOW_VULKAN`; get native handle via `SDL_GetWindowProperties`
  (Win32 HWND, X11/Wayland, Android `ANativeWindow`).
- wgpu `Instance::create_surface_unsafe(SurfaceTargetUnsafe::RawHandle { .. })`; configure
  sRGB/linear; recreate on resize/lifecycle.
- Optional SDL3 GPU backend: `SDL_CreateGPUDevice` + `SDL_ClaimWindowForGPUDevice`; SPIR-V
  shaders; use for A/B and driver debugging, not primary.

## 6.1 Provided pack — exact evidence

`Turnip_Gen8_V39_Patched.zip` (user-provided):
- `meta.json`: name **Turnip Gen8 V39-patched**, description **Turnip A8xx Android/Bionic**,
  driverVersion Vulkan 1.4.367, minApi 28, packageVersion 34.
- `libvulkan_freedreno.so` 8 460 728 B, sha256
  `2db3c4601bc3f046596f86d9d189cb5cad693812f8c8c55fe1817ec90ea879ac`.
- ELF64 aarch64 Android 34, NDK r29, stripped; `ctypes.CDLL` loads but
  `vk_icdGetInstanceProcAddr` / `vkGetInstanceProcAddr` are undefined.
- Standard Vulkan loader: `Attempt to retrieve ... failed` → no drivers. It is an A8xx pack and
  the device reports Adreno 725 (Gen7 A7xx); it is retained for future Gen8 devices /
  libadrenotools in-app loading, not used as the P0 ICD.
- P0 uses Termux Turnip Mesa 26.2.6 (`freedreno_icd.aarch64.json`), adapter
  `Turnip Adreno (TM) 725`, Vulkan 1.4.354.

## 7. Security / legal

- Custom driver is native code; only load from trusted sources and verify SHA-256.
- Do not bundle Blizzard assets; driver notices (Mesa MIT, libadrenotools BSD-2) preserved.
- No root requirement; no system partition modification.
- Thermal/battery warnings; fallback if device overheats/crashes.


## 8. Android loader compatibility — observed 2026-11-12

`/system/lib64/libvulkan.so` on this device is the **AOSP Vulkan shim**, not a
Khronos ICD loader. It loads the vendor HAL (`/vendor/lib64/hw/vulkan.adreno.so`)
and enumerates the Adreno blob, but when `libadrenotools` substitutes Mesa Turnip
for the ICD it returns **0 physical devices** — the shim does not use
`vk_icdGetInstanceProcAddr`.

Evidence (`apps/driver-probe`, Rust adrenotools port):

- test 2: private namespace + unique `libvulkan.so` → `Adreno (TM) 730`,
  api 1.1.128, vendor 0x5143 (vendor blob, no hook);
- test 3: hook intercepts `vulkan.adreno.so` and loads Turnip → the loader sees
  0 devices, while a direct ICD query through `vk_icdGetInstanceProcAddr` returns
  `Turnip Adreno (TM) 725`, Vulkan 1.4.354;
- test 6b: a **generated ICD JSON** + the Termux Khronos loader enumerates
  `Turnip Adreno (TM) 725` without any hook.

Consequences for wc3rust:

1. In a native app, load Turnip through a **bundled Khronos loader** (or any loader
   that uses the ICD interface) with a generated ICD JSON, or call
   `vk_icdGetInstanceProcAddr` directly.
2. The AOSP shim is only sufficient for vendor Qualcomm packs (ADPKG).
3. Do not build on `dlopen("/system/lib64/libvulkan.so")` + Turnip; it cannot work
   on a stock Android loader.

Winlator 11 reaches the same conclusion differently: guest Turnip is a **rootfs
ICD** (`usr/lib/libvulkan_freedreno.so` + `usr/share/vulkan/icd.d/*.json`), and
libadrenotools is used only by its host Vortek renderer for vendor packs. Details
and source URLs: `docs/WINLATOR_TURNIP_RESEARCH.md`.

The C++ `libadrenotools` is no longer used by this project; `crates/wc3-adrenotools`
is a full Rust port (27 tests, on-device probe passing). See
`docs/ADRENOTOOLS_RUST_PORT.md`.

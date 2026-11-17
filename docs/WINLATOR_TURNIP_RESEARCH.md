# How Winlator loads Turnip — web research (2026-11-12)

Scope: determine, from primary sources, how Winlator (brunodev85) makes Vulkan
Turnip available to games, and what part `libadrenotools` plays. This matters for
Herofolends-RS because our Rust adrenotools port must load a driver the same way a
real app does.

Method:
- `stealth-browser` MCP (headless Chromium) for GitHub API trees and raw source.
- `git clone --depth 1 --filter=blob:none --sparse` of `winlator-app` (only
  `app/src/main/java`, `app/src/main/cpp/{vortekrenderer,winlator}`) to grep the
  full Java/native source locally instead of guessing URLs.
- `curl` for the public Turnip component archive (binary artifact), because a
  browser cannot usefully render a 2.6 MB `.tzst`.
- Access date for every URL: **2026-11-12**. Winlator app commit:
  `2c33741d50c5daab0521a098be5c969f63b9e887`.

## 1. Sources (all observed)

| # | URL / artifact | What it establishes |
|---|---|---|
| S1 | `https://github.com/brunodev85/winlator` (tree API) | Winlator 11 is a thin repo: `app`, `vortek`, `gladio` submodules; `installable_components/turnip/{turnip-24.1.0,25.0.0,26.0.3}.tzst` |
| S2 | `https://github.com/brunodev85/winlator-app` (tree API, 1046 files) | Android app source; vendored `app/src/main/cpp/libadrenotools`; `assets/graphics_driver/turnip-26.2.0.tzst`; `vortekrenderer` |
| S3 | `.../app/src/main/java/com/winlator/core/DefaultVersion.java` | `TURNIP = "26.2.0"`, `VORTEK = "2.1"`, `ZINK = "22.2.5"`, `GLADIO = "1.1"` |
| S4 | `.../container/GraphicsDrivers.java` | `TURNIP` and `VORTEK` are the two Vulkan drivers; default is `TURNIP` on Adreno, `VORTEK` otherwise |
| S5 | `.../XServerDisplayActivity.java:726-787` | `extractGraphicsDriverFiles()`: deletes `rootfs/usr/lib/libvulkan_freedreno.so`, `libvulkan_vortek.so`, `libGL.so.1.7.0` and the whole `rootfs/usr/share/vulkan/icd.d`, then extracts the selected package |
| S6 | `.../contentdialog/TurnipConfigDialog.java:59-76` | Turnip guest env: `TU_OVERRIDE_HEAP_SIZE`, `MESA_VK_WSI_NATIVE_MEM_IMPORTED=1`, `MESA_VK_WSI_PRESENT_MODE`, `TU_DEBUG=sysmem/gmem/deck_emu` (no `VK_ICD_FILENAMES`) |
| S7 | `.../core/GeneralComponents.java:31,87-101,205-224` | Component types incl. `TURNIP` and `ADRENOTOOLS_DRIVER`; `TURNIP` destination is the **rootfs root**; extraction is `TarCompressorUtils.extract(ZSTD, assets/graphics_driver/turnip-<ver>.tzst, rootfs)` |
| S8 | `.../xenvironment/RootFS.java:26-88` | rootfs = `files/rootfs`; `getLibDir() = rootfs/usr/lib` |
| S9 | `.../xenvironment/components/GuestProgramLauncherComponent.java:86-116` | guest launched as `rootfs/usr/local/bin/box64 <exe>` with `cwd = rootfs`, `LD_LIBRARY_PATH = rootfs/usr/lib`; host-absolute paths (no chroot visible here) |
| S10 | `.../xenvironment/components/VortekRendererComponent.java:29-78,170` | host renderer; `adrenotoolsDriver` option → `GeneralComponents.getDefinitivePath(ADRENOTOOLS_DRIVER, ...)` → `initVulkanWrapper(nativeLibraryDir, libvulkanPath)` |
| S11 | `.../cpp/vortekrenderer/src/main.c:11,16-39,59-64` | exact adrenotools call (below) |
| S12 | `.../cpp/vortekrenderer/CMakeLists.txt` + `.../cpp/CMakeLists.txt` | `libvortekrenderer` links `adrenotools`, `winlator`, `log`, `android`; libadrenotools is a CMake subdirectory |
| S13 | `.../cpp/winlator/include/winlator.h:15` | `#define LIBVULKAN_PATH "/system/lib64/libvulkan.so"` |
| S14 | `https://github.com/brunodev85/vortek` (README + tree + `vortek_icd.aarch64.json`) | Vortek = "compatibility layer on top of the Vulkan host driver", client/server over a socket; ICD `api_version 1.1.128`, `library_path=/data/data/com.winlator/files/rootfs/lib/libvulkan_vortek.so` |
| S15 | Turnip component archive, downloaded 2026-11-12, sha256 `1e5bd643d64b450960ea3208cc96599a4de577f178e9ba0843b1c37083ce50d2`, 2 659 862 B | Contents below |
| S16 | `donors/libadrenotools/tools/ADPKG.md` (vendored donor doc) | ADPKG format: `meta.json` + driver `.so` + patched dependency libs; canonical Qualcomm pack uses `vulkan.ad0615.so` (name contains `vulkan.`) |

## 2. Turnip package contents (observed, S15)

```
./usr/share/vulkan/icd.d/freedreno_icd.aarch64.json     212 B
./usr/lib/libvulkan_freedreno.so                      16 728 760 B
```

`freedreno_icd.aarch64.json`:

```json
{
    "ICD": {
        "api_version": "1.4.354",
        "library_arch": "64",
        "library_path": "/data/data/com.winlator/files/rootfs/lib/libvulkan_freedreno.so"
    },
    "file_format_version": "1.0.1"
}
```

Notes:
- The JSON lives in the **rootfs standard ICD directory** (`/usr/share/vulkan/icd.d`).
- `library_path` is an **absolute Android-host path** inside the app's rootfs, not a
  guest-relative path.

## 3. Guest path: how a game gets Turnip

Observed chain (S3–S9):

1. User selects Vulkan driver = Turnip (`GraphicsDrivers.TURNIP`).
2. `XServerDisplayActivity.extractGraphicsDriverFiles()` computes a cache id
   `<driver>-<version>-<gl-driver>-<gl-version>`. When it changes, it deletes the
   previous Vulkan libs and the whole `rootfs/usr/share/vulkan/icd.d` directory and
   re-creates it.
3. `GeneralComponents.extractFile(Type.TURNIP, ...)` decompresses
   `assets/graphics_driver/turnip-26.2.0.tzst` (or a downloaded
   `installable_components/turnip/turnip-<ver>.tzst`) into the rootfs root.
4. Result: `rootfs/usr/lib/libvulkan_freedreno.so` and
   `rootfs/usr/share/vulkan/icd.d/freedreno_icd.aarch64.json`.
5. `TurnipConfigDialog.setEnvVars()` adds Mesa/Turnip tuning env vars to the guest
   environment (heap override, native WSI memory import, present mode, `TU_DEBUG`).
6. The guest is started as `rootfs/usr/local/bin/box64 <exe>` with
   `LD_LIBRARY_PATH=rootfs/usr/lib` (S9). Wine's `winevulkan` loads the guest
   Vulkan loader from there; the loader sees the ICD JSON in the standard
   `.../usr/share/vulkan/icd.d` directory and loads the Turnip `.so`.

For Vortek the same mechanism is used with `graphics_driver/vortek-2.1.tzst`:
`libvulkan_vortek.so` + `vortek_icd.aarch64.json` are placed in the same ICD
directory. In other words, **Turnip and Vortek are mutually exclusive ICDs in the
container, selected by files on disk, not by env vars**.

No `VK_ICD_FILENAMES`/`VK_DRIVER_FILES` appears anywhere in the app's Java or
native sources (S2 local grep over all 276 Java files + vortekrenderer/winlator
natives). How the rootfs `libvulkan` loader resolves `usr/share/vulkan/icd.d`
(patched loader compiled for the fixed app path vs. a wrapper inside the rootfs)
is **not in the app source and was not verified** — see §6.

## 4. Host path: where libadrenotools is actually used

Observed (S10–S13):

`VortekRendererComponent` (Java) reads the Vortek config option `adrenotoolsDriver`
and resolves it to a `.so` path:

```java
// VortekRendererComponent.java:66-67
String adrenotoolsDriver = config.get("adrenotoolsDriver");
options.libvulkanPath = GeneralComponents.getDefinitivePath(
        GeneralComponents.Type.ADRENOTOOLS_DRIVER, context, adrenotoolsDriver);
```

`GeneralComponents.Type.ADRENOTOOLS_DRIVER` (S7):
- install mode `FILE` (user-supplied pack, not a versioned download);
- stored under `files/installed_components/adrenotools_driver/<name>/`;
- `getDefinitivePath()` reads the pack's `*.json` manifest, takes `libraryName`,
  and returns `<pack>/<libraryName>`;
- the built-in identifier is `"System"`, which resolves to `null`.

The JNI constructor then calls (S10):

```java
// VortekRendererComponent.java:78
initVulkanWrapper(nativeLibraryDir, options.libvulkanPath);
```

Native side (S11):

```c
static void* openVulkanLibrary(JNIEnv* env, jstring nativeLibraryDir, jstring libvulkanPath) {
    if (libvulkanPath) {
        const char* nativeLibraryDirC = ...;
        const char* libvulkanPathC = ...;
        const char* libvulkanName = basename(libvulkanPathC);
        char libvulkanDir[PATH_MAX] = {0};
        strcpy(libvulkanDir, dirname(libvulkanPathC));
        strcat(libvulkanDir, "/");
        asprintf(&tmpDir, "%s%s", libvulkanDir, "tmp");
        libvulkan = adrenotools_open_libvulkan(
            RTLD_NOW | RTLD_LOCAL,
            ADRENOTOOLS_DRIVER_CUSTOM,
            tmpDir,             // writable memfd fallback dir (API < 29)
            nativeLibraryDirC,  // hookLibDir = APK nativeLibraryDir
            libvulkanDir,       // customDriverDir
            libvulkanName,      // customDriverName
            NULL, NULL);
    } else {
        libvulkan = dlopen(LIBVULKAN_PATH, RTLD_NOW | RTLD_LOCAL); // "/system/lib64/libvulkan.so"
    }
    ...
}
```

So the host renderer:
- with **no** custom driver → uses the stock Android `libvulkan.so` (AOSP shim);
- with a **custom ADPKG pack** → uses libadrenotools with `ADRENOTOOLS_DRIVER_CUSTOM`,
  exactly the API our Rust port implements.

This host renderer is the server side of Vortek (S14): the guest `libvulkan_vortek.so`
is an ICD client that talks over the socket
`/data/data/com.winlator/files/rootfs/tmp/.vortek/V0`.

## 5. What this means (inference, labelled)

- Winlator 11 does **not** use libadrenotools to give games Turnip. Games get Turnip
  from the **Linux rootfs ICD** (direct Mesa driver, direct `/dev/kgsl` access,
  native WSI memory import).
- libadrenotools is used for the **host-side Vortek renderer** and only when the
  user installs an `ADRENOTOOLS_DRIVER` pack. The canonical packs (ADPKG, S16) are
  **Qualcomm blobs** with a `vulkan.*.so` name and the vendor HAL entry points the
  AOSP shim expects; the system loader path is `/system/lib64/libvulkan.so`.
- This matches our on-device probe: the AOSP system loader can load the vendor
  `vulkan.adreno.so` (it enumerated "Adreno (TM) 730"), but when adrenotools
  substitutes the custom Mesa Turnip it enumerates **0 devices**, because the AOSP
  shim does not use the ICD `vk_icdGetInstanceProcAddr` interface. Loading Turnip
  through the ICD interface directly **works** (Turnip Adreno 725, Vulkan 1.4.354).

## 6. Unknowns / limits

- The rootfs `libvulkan.so` implementation was not inspected (rootfs.tzst is an
  APK asset, not in the git tree; not downloaded). Therefore the exact reason the
  guest loader finds `rootfs/usr/share/vulkan/icd.d` — patched loader default path,
  wrapper script, or another mechanism — is **unknown**. The app source contains no
  ICD env var.
- Winlator 7.x/8.x may have used a different mechanism; this research covers the
  current `main` (Winlator 11.2 app, commit `2c33741…`).
- GitHub API trees and raw files are point-in-time (`2026-11-12`); the app repo can
  change.

## 7. Consequences for Herofolends-RS

1. Our Rust port is API-compatible with Winlator's host call: same flags,
   `hookLibDir = nativeLibraryDir`, `customDriverDir = dirname(driver)`,
   `customDriverName = basename(driver)`.
2. For a **native Android game** (no container), loading Mesa Turnip requires a
   loader that speaks the ICD interface. Options:
   - bundle the Khronos Vulkan loader and point `open_libvulkan_with_loader()` at
     it (`libvulkan_loader.so`), or
   - use the driver's `vk_icdGetInstanceProcAddr` directly (what our probe does),
     or
   - ship a small `vkGetInstanceProcAddr` compatibility shim in front of the ICD
     (not recommended: the AOSP shim also expects vendor HAL symbols).
   The stock `/system/lib64/libvulkan.so` is only sufficient for Qualcomm blobs.
3. Do **not** copy Winlator's rootfs approach into the game: it exists because the
   guest is a Linux/box64 container. A native Rust/Vulkan game should own the
   loader choice explicitly.
4. The provided `Turnip_Gen8_V39_Patched.zip` cannot be used as an ICD by any of
   these paths: its `.dynsym` has 0 defined function symbols (no
   `vk_icdGetInstanceProcAddr`, no `vkGetInstanceProcAddr`). It is a raw blob for a
   loader that uses fixed offsets or an internal dispatch, not the Vulkan ICD ABI.

## 8. Reproduce

```bash
# component contents
curl -L -o turnip-26.2.0.tzst \
  https://raw.githubusercontent.com/brunodev85/winlator-app/main/app/src/main/assets/graphics_driver/turnip-26.2.0.tzst
sha256sum turnip-26.2.0.tzst   # 1e5bd643d64b450960ea3208cc96599a4de577f178e9ba0843b1c37083ce50d2
tar --zstd -tf turnip-26.2.0.tzst
tar --zstd -xOf turnip-26.2.0.tzst ./usr/share/vulkan/icd.d/freedreno_icd.aarch64.json

# source
git clone --depth 1 --filter=blob:none --sparse https://github.com/brunodev85/winlator-app
cd winlator-app
git sparse-checkout set app/src/main/java app/src/main/cpp/vortekrenderer app/src/main/cpp/winlator
rg -n 'adrenotools_open_libvulkan|LIBVULKAN_PATH|TYPE.TURNIP|rootfs/usr/share/vulkan' app
```

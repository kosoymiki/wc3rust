# REVERSE ENGINEERING — Herofolends APK, installer, data, ISO

Дата: 2026-11-12. Всё ниже — `observed` на локальных файлах; донорский код не исполнялся.

## 1. Inputs

```
/storage/emulated/0/Download/wr3/
  Herofolends+v0.0.16.apk      7 649 350 B
  setup.exe                    2 292 669 B
  data1.bin                1 302 143 004 B
  data2.bin                  263 263 718 B
  data3.bin                    4 030 894 B
  Warcraft_install/Warcraft_install/
    Warcraft III.mdf           796 185 072 B
    THEFROZENTHRONE.mdf        579 031 824 B
    *.mds, warcraft-sn.txt, frozentron-sn.txt, prepatch.log
```

`setup.exe` + data bins are a **Warcraft 3 repack** (Inno Setup + FreeArc); the MDF/MDS are
Alcohol 120% images of the original install CDs. The APK is a separate **Herofolends** launcher.

## 2. APK

### 2.1 Manifest / package
- package `not.have.page.herofolends`, versionName `0.0.16`, minSdk 10, targetSdk 12.
- Activities: `SampleLauncher`, `Urho3D` (extends `SDLActivity`), `ScriptPicker`, `AboutView`.
- Permissions: READ_EXTERNAL_STORAGE, INTERNET, READ_PHONE_STATE.
- Native ABI: **armeabi-v7a only**; `lib/armeabi-v7a/libHerofolends.so` 10 050 079 B.

### 2.2 Native library
- ELF32 ARM EABI5, stripped, 20 855 symbols; compiler GCC 4.9/Clang 3.8; JNI.
- Urho3D C++ symbols; Lua 5.1.5; AngelScript; SDL2; OpenGL ES.
- `rabin2 -I`/`strings` evidence; `r2 -A` starts analysis but full function recovery is not needed
  for the plan (no source-level game logic in APK).

### 2.3 Assets (recovered tree)
```
assets/CoreData/Shaders/GLSL/        45 files (PBR, FXAA, shadows, IBL, ...)
assets/CoreData/RenderPaths/         10 XML
assets/CoreData/Techniques/          many XML
assets/CoreData/Materials/           DefaultGrey.xml
assets/CoreData/Textures/            engine textures
assets/Data/Fonts/                   Anonymous Pro, BlueHighway
assets/Data/Models/Plane.mdl         336 B (placeholder)
assets/Data/Textures/                UI, UIRed, UIYellow, selection, minimap_marker, ...
assets/Data/UI/                      MainMenu, SelectRaceScreen, SelectmapScreen, InGameMenu, ChangeResources, styles
assets/Data/html/                    about.htm, logo.png, Urho3D/StormLib/BLPConverter licenses
assets/Data/Scripts/                 0 files
assets/Data/LuaScripts/              0 files
```
- `Urho3D.java` scans `Data/Scripts` (AngelScript) and `Data/LuaScripts`; if scripts are absent it
  cannot start a game. The APK is a **front-end/launcher**, not the full engine content.
- `about.htm`: “RTS game engine without its own game resources. It can use the content from
  Warcraft 3 Reign of Chaos / The Frozen Throne”; public alpha, not ready.
- UI flow: MainMenu (Play, Change resources, About, Exit) → SelectRaceScreen → SelectmapScreen →
  InGameMenu; ChangeResources picks the data folder. This is a useful UX reference.
- Libraries credited: Urho3D, StormLib, BLPConverter (MIT), plus MDX/BLP/W3M/W3X specs.

**Conclusion:** Herofolends is an Urho3D-based RTS launcher/engine skeleton. It contains
WC3-specific UI and format libraries, but no game scripts/assets in the APK. The actual game
logic/data is either in the encrypted FreeArc volumes or was never published.

## 3. setup.exe (Inno Setup repack)

- PE32, Inno Setup 5.5, title **Warcraft 3**, RU/EN; `innoextract --info`: **not passworded**.
- Temp payload (extracted): `unarc.dll`, `ISDone.dll`, `facompress.dll`, `facompress_mt.dll`,
  `precomp.exe`, `CLS-MSC.dll`, `CLS-precomp.dll`, `CLS-srep.dll`, `packjpg_dll*.dll`, `zlib1.dll`,
  `botva2.dll`, `fontinst.dll`, UI images, `records.inf`, `english.ini`, `russian.ini`.
- `unarc.dll` supports `-p{Pwd}` and `arc.ini`; `ISDone.dll` has `ERROR_ISARC_BAD_PASSWORD` and
  `password?` — the FreeArc archive password/key is supplied by the installer script or prompted.
- `innoextract` lists/extracts only temp files; main game files are the external `data*.bin`.

## 4. data*.bin (FreeArc AES)

- `file`: FreeArc archive; header `ArC\x01`.
- `data1.bin`: `lzma:mfbt4:d1m+aes-256/ctr:n1000:r0:s...:c...:i...`
- `data2.bin`, `data3.bin`: same format; sizes 263 MB / 4 MB.
- Old `unarc` (xredor) → `unsupported compression method` (no AES).
- `freearc-c` (built native aarch64) → `archive is encrypted, use -pPassword`; empty/wrong
  password rejected.
- **We do not crack** the archive. Options: user runs original installer on Windows/VM; user
  supplies the password; or use the ISO MPQs below.

## 5. ISO install media (clean MPQ source)

- MDF is 2352-byte-sector ISO9660 Mode 1. `mdf2iso` converts to ISO.
- `Warcraft III.iso` 635.2 MB:
  - `War3.mpq` 438 529 036 B (classic game data)
  - `Movies/*.mpq` (cinematics)
  - `setup.mpq` 17 130 959 B
- `THEFROZENTHRONE.iso` 480.8 MB:
  - `setup.mpq` 459 222 362 B (TFT installer payload)
- MPQ header `MPQ\x1a`; 7z 17.05 cannot open MPQ; `mpyq` fails on listfile/encryption.
- **StormLib oracle built and verified:** `tools/storm_probe` opened `War3.mpq` and listed
  **10 525 files** (`analysis/war3_mpq_listing.txt`). Extracted real fixtures:
  `Units/Human/Footman/Footman.mdx` (MDLX, VERS 800), `Textures/Footman.blp` (BLP1 v0),
  `Units/UnitData.slk` (ID;PWXL), plus command-button BLP/portrait/campaign textures.
  `wc3-formats` Rust header tests pass on these files. Evidence: `analysis/p0-evidence.md`.
- Need **CascLib** for Reforged CASC; StormLib covers classic MPQ. Both are cloned, MIT.

## 6. Recovered tree / what to restore

```
Herofolends (abandoned)
├─ Android launcher (Urho3D/SDL, Java)      [APK, no source]
├─ UI (MainMenu/Race/Map/InGame/ChangeRes)  [APK XML, usable as UX reference]
├─ native engine (Urho3D + Lua/AngelScript) [.so, no source]
├─ game scripts/assets                      [missing/encrypted; not in APK]
└─ repack data (FreeArc AES)                [password/user-assisted; no crack]
Warcraft III install media
├─ War3.mpq (classic)                       [ISO, clean]
├─ TFT setup.mpq                             [ISO, clean]
└─ movies                                    [ISO, clean]
```

## 7. What this means for the Rust project

1. We cannot “decompile Herofolends” into a complete game: the APK is a launcher, and the engine
   source is absent. We can recover its **UI flow, libraries, asset paths, and requirements**.
2. We can test format parsers on **War3.mpq** and MDX/BLP files from the ISO immediately.
3. We should not depend on the encrypted FreeArc repack; use clean ISO MPQ/CASC + user data.
4. The clean-room Rust engine is justified: no usable source, and the existing Java/TS/C# donors
   provide behavior/spec references, not a Rust/Vulkan codebase.

## 8. Toolchain

| tool | use |
|---|---|
| `jadx` | DEX decompile (48 KB; launcher activities) |
| `r2`/`rabin2` | ELF/PE analysis, symbols, strings, xrefs |
| `aapt`/`unzip` | APK manifest/assets |
| `innoextract` | Inno Setup list/extract (not passworded) |
| `mdf2iso` | MDF→ISO (2352→2048) |
| `7z` | ISO listing/extraction (no MPQ) |
| `unarc` (xredor) | FreeArc without AES (fails on these archives) |
| `freearc-c` | FreeArc with AES support (needs password) |
| `StormLib`/`CascLib` | MPQ/CASC reference; build/test oracle |

## 9. Next RE actions

- Build StormLib `storm` CLI or a small C++ test program; list/extract `War3.mpq`.
- Extract representative MDX/BLP/SLK/W3M samples; add to `wc3-testkit/fixtures`.
- Build CascLib test program; use user Reforged CASC if available.
- Continue APK UI XML analysis; map each screen to our touch UI.
- If user supplies FreeArc password/Windows extraction, inventory the abandoned port's scripts
  and compare with Warsmash/W3AndroidEngine; otherwise mark `unavailable` with evidence.

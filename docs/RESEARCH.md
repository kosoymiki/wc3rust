# RESEARCH — WC3-compatible Rust/Vulkan port

Дата доступа: **2026-11-12**. Метод: stealth-browser MCP (DuckDuckGo HTML + прямые страницы),
GitHub API/репозитории. Все утверждения — `observed` (страница/файл прочитан), `inferred`
(вывод), `reported` (автор заявляет, не проверяли) или `unknown`.

## 1. Заброшенный Herofolends (локальные файлы)

| ID | Источник | observed |
|---|---|---|
| H1 | `Herofolends+v0.0.16.apk` → `assets/Data/html/about.htm` | «RTS game engine without its own game resources. It can use the content from Warcraft 3 Reign of Chaos / The Frozen Throne»; public alpha, not ready; support `herofolends@gmail.com` |
| H2 | APK assets | `CoreData` 45 GLSL shaders + 10 render paths; `Data/UI`: MainMenu, SelectRaceScreen, SelectmapScreen, InGameMenu, ChangeResources; `Data/Models/Plane.mdl`; 9 UI textures; `StormLib` + `BLPConverter` license pages; **0** `Data/Scripts`, **0** `Data/LuaScripts` |
| H3 | `lib/armeabi-v7a/libHerofolends.so` | ELF32 ARM EABI5, stripped, 10 050 079 B, 20 855 symbols, GCC/Clang 3.8; Urho3D C++ symbols; Lua 5.1; AngelScript; SDL2; `rabin2 -I`/`strings` |
| H4 | `classes.dex` | package `not.have.page.herofolends`; activities `SampleLauncher`, `Urho3D` (extends SDLActivity), `ScriptPicker`, `AboutView`; scans `Data/Scripts` (AngelScript) and `Data/LuaScripts`; if no library extra, lists libraries; `Urho3D` splits picked script by `:` |
| H5 | APK UI XML | MainMenu: Play, Change resources, About Herofolends, Exit; SelectRaceScreen: race list + preview; SelectmapScreen: map list; InGameMenu: Resume, Other map; ChangeResources: path picker |
| H6 | `setup.exe` (`innoextract -l/--info`) | Inno Setup 5.5 «Warcraft 3», RU/EN, **not passworded**; temp files: `unarc.dll`, `ISDone.dll`, `facompress*.dll`, `precomp.exe`, `CLS-*.dll`, UI images, `records.inf`, language INI; no game data inside |
| H7 | `data1.bin` 1 302 143 004 B, `data2.bin` 263 263 718 B, `data3.bin` 4 030 894 B | FreeArc `ArC\x01`; compression method `lzma:mfbt4:d1m+aes-256/ctr...`; `unarc` (old) → unsupported compression; `freearc-c/unarc` → `archive is encrypted, use -pPassword`; empty/wrong password rejected |
| H8 | `Warcraft_install/Warcraft_install/` | `Warcraft III.mdf` 796 185 072 B, `THEFROZENTHRONE.mdf` 579 031 824 B, `.mds`, serial txt, `prepatch.log`; MDF 2352-byte sectors, ISO9660 Mode 1 |
| H9 | converted ISO via `mdf2iso` | `Warcraft III.iso` 635.2 MB contains `War3.mpq` 438 529 036 B, `Movies/*.mpq`, `setup.mpq` 17 130 959 B; TFT ISO 480.8 MB contains `setup.mpq` 459 222 362 B; MPQ header `MPQ\x1a` |
| H10 | MPQ tooling | 7z/17.05 does **not** open MPQ; `mpyq` fails (listfile absent / encrypted); StormLib/CascLib are the robust path; `War3.mpq` is the classic data source; TFT `setup.mpq` contains the TFT installer payload |

**Inferred:** The APK is a launcher/front-end for an RTS engine; it contains WC3-specific UI but no
game data/scripts. The abandoned engine source is not in the APK; game data/scripts are either in the
encrypted FreeArc volumes or were never published. The ISO install media is the clean, unencrypted
source of MPQ data for parser tests.

## 2. Existing WC3 engines / donors

| ID | Source | observed |
|---|---|---|
| D1 | https://github.com/Retera/WarsmashModEngine | Java/libGDX WC3 emulation engine; Hive thread: «mostly clean-room engineered rewrite of Warcraft III», uses LibGDX + `mdx-m3-viewer`; 592 stars (GitHub search 2026-11-12) |
| D2 | https://github.com/nikoltu/W3AndroidEngine | «Warsmash Reforged. Windows & Android»; Java 17/libGDX; reads **CASC install or extracted files**; own **JASS2 interpreter**; **OpenGL ES shaders**; adds touch/S Pen, campaign, save/load, cinematics, graphics sets Classic/HD/Reforged, determinism for replays; targets 30 FPS weak devices → 120 FPS fast; no game content, user-owned data only |
| D3 | https://github.com/Retera/WarsmashAndroidPort2022 | 2022 Android port/parody; LibGDX/gdx-liftoff; **not under active development**; source available; useful as Android packaging/touch experiment, not architecture |
| D4 | https://github.com/linsmod/WarsmashModEngine-Android | Android fork of Warsmash; LibGDX; «does not include Warcraft III assets»; alternative Android integration |
| D5 | https://github.com/Drake53/War3Net | «complete .NET toolkit for Warcraft III modding»; C#; formats, maps, JASS, MPQ; good format/API reference |
| D6 | https://github.com/stijnherfst/HiveWE | Warcraft III world editor, C++/Qt; 490 stars; editor/data model reference; GPL implications |
| D7 | https://github.com/flowtsohg/mdx-m3-viewer | WebGL viewer; MDX extensive, M3 partial, W3M/W3X partial, BLP1 extensive, DDS partial; **standalone parsers** for MDX/MDL, BLP1, INI, SLK, MPQ1, W3M/W3X/W3N; JASS context |
| D8 | https://github.com/4eb0da/war3-model | TS MDL/MDX converter/renderer; claims **100% of classic + Reforged models parse**, 99.7% (8753/8773) byte-identical round-trip; supports versions 800/1000/1100; BLP1 decoder only; DDS dxt1/3/5/ati2; MIT |
| D9 | https://github.com/ladislav-zezula/StormLib | MIT; reference MPQ implementation; needed for classic `War3.mpq` and map MPQs |
| D10 | https://github.com/ladislav-zezula/CascLib | CASC reader; «Blizzard games since 2014»; Warcraft III uses CASC since 1.30 build 9655 PTR; needed for Reforged/current data |
| D11 | https://github.com/OpenRA/OpenRA | C# open-source RTS engine for early Westwood games; 17 773 stars; architecture/UX reference, not WC3 data |
| D12 | https://github.com/rust-mobile/android-activity | Rust glue for Android NativeActivity/GameActivity; critical for Rust Android integration |
| D13 | https://github.com/bylaws/libadrenotools | rootless Adreno driver mods/replacements; loads custom drivers such as **Turnip**, BCn textures, shader dumps, driver config without root; Android 9+ arm64 |
| D14 | https://github.com/hedge-dev/XenosRecomp | Xbox 360 shader binaries → HLSL → DXC → DXIL/SPIR-V; designed around **Unleashed Recompiled**; translation layer, not GPU emulation; incomplete features documented |
| D15 | https://github.com/gligli/libxemit | runtime Xenos shader emitter library; low-level X360 shader reference |

## 3. Format specs and tooling

| ID | Source | observed |
|---|---|---|
| F1 | https://github.com/flowtsohg/mdx-m3-viewer | MDX/MDL read/write; BLP1 read; INI/SLK; MPQ1 read/write; W3M/W3X/W3N read/write; DDS DXT1/3/5/RGTC; TGA; unit tests render comparisons |
| F2 | https://github.com/4eb0da/war3-model | MDX versions 800/900/1000/1100; BLP1; team color, replaceable IDs, global sequences, layers, billboard, ribbon/particle emitters; Reforged PBR (ORM, specular, normal, env); no Light nodes/render priority; MIT |
| F3 | https://github.com/FernandoS27/WhiteoutLib/blob/master/docs/MDX_FILE_FORMAT_SPECIFICATION.md | MDX is binary 3D model format for WC3/Reforged: geometry, materials, skeletal animation, particles, cameras, collision volumes |
| F4 | https://www.hiveworkshop.com/threads/blp-specifications-wc3.279306/ | BLP spec for WC3 (BLP1); WoW BLP not fully covered |
| F5 | https://github.com/ladislav-zezula/StormLib | MPQ reference; used by Herofolends APK (license page) |
| F6 | https://github.com/ladislav-zezula/CascLib | CASC reference; WC3 1.30+ |
| F7 | http://www.zezula.net/en/casc/main.html | CASC storage info; Warcraft III since 1.30 build 9655 PTR |
| F8 | https://wowdev.wiki/CASC | CASC technical wiki: content-addressable storage, encoding keys (truncated MD5) |
| F9 | https://github.com/4eb0da/war3-model (README) | BLP1 only, no BLP0/BLP2; DDS BC1/BC3/BC5; useful for texture pipeline |
| F10 | https://github.com/ladislav-zezula/StormLib (README/MPQ) | MPQ archive format; listfile/encryption support needed for WC3 maps |

## 4. GPU / Android / Turnip

| ID | Source | observed |
|---|---|---|
| G1 | https://github.com/bylaws/libadrenotools | rootless custom Adreno driver loading (Turnip), BCn textures, shader dumps, driver config; Android 9+ arm64 |
| G2 | https://www.ppsspp.org/docs/reference/custom-drivers/ | modern Android can load alternative Vulkan drivers for Adreno via libadrenotools; Mesa Turnip vs Qualcomm driver |
| G3 | https://docs.mesa3d.org/drivers/freedreno.html | Mesa Freedreno/Turnip docs (page loaded; body content limited by JS/format) |
| G4 | https://wgpu.rs/ | wgpu = safe portable Rust graphics based on WebGPU; native Vulkan/Metal/DX12/GLES; browsers WebGPU/WebGL2 |
| G5 | https://github.com/rust-mobile/android-activity | Rust Android NativeActivity/GameActivity glue |
| G6 | https://github.com/K11MCH1/AdrenoToolsDrivers/releases?q=turnip | Turnip driver builds for AdrenoTools-compatible apps (Skyline/Strato/Vita3K/Yuzu Android) |
| G7 | https://github.com/K11MCH1/AdrenoToolsDrivers | «Turnip drivers are compiled from Mesa», freely distributed; repository disclaimer/source |
| G8 | https://github.com/libsdl-org/SDL | SDL3: cross-platform multimedia; zlib license |
| G9 | https://wiki.libsdl.org/SDL3/CategoryGPU | SDL3 GPU API: cross-platform 3D/compute in style of Metal/Vulkan/D3D12; `SDL_CreateGPUDevice`, `SDL_ClaimWindowForGPUDevice`, command buffers, render passes |
| G10 | https://github.com/libsdl-org/SDL/blob/main/docs/README-android.md | Android: SDK 36+, NDK r28c+, min API 21; Java `SDLActivity` shim + JNI; Gradle project |
| G11 | https://docs.rs/sdl3/latest/sdl3/ | Rust `sdl3` crate: SDL3 bindings, `build-from-source`, `ash` feature for Vulkan types, image/mixer/ttf features; some SDL3 features still partial |
| G12 | https://docs.mesa3d.org/license.html | «The core Mesa library is licensed according to the terms of the MIT license»; individual files may differ |
| G13 | https://github.com/gfx-rs/wgpu | wgpu Apache-2.0/MIT; Rust graphics API based on WebGPU; Vulkan/Metal/DX12/GLES |

**Inferred:** For Herofolends-RS, use **SDL3** as platform layer (window/input/touch/pen/gamepad/
audio/Android) and **wgpu Vulkan** as primary renderer; optional SDL3 GPU backend; optional custom
Turnip through `libadrenotools`; system Adreno Vulkan as fallback. SDL3 is zlib; Turnip/Mesa core
is MIT; libadrenotools BSD-2; wgpu Apache/MIT. Do not require root.

## 5. Touch RTS UX

| ID | Source | observed |
|---|---|---|
| T1 | https://fadi-labib.github.io/Generals-Android/port/TOUCH_CONTROLS/ | 2003 mouse-only RTS → touch: 1-finger tap = hover + left click; drag = selection box; long-press 600 ms = right click/deselect; 2-finger drag = camera scroll; pinch = zoom; two-finger pan/pinch mutually exclusive; DPI-scaled thresholds; deferred commitment; hover-before-click; exactly one source of mouse events; cancelled touches never click; first-tap trap |
| T2 | https://github.com/molotovgit/CnC-Generals-ZeroHour-Android/blob/main/docs/discoveries/touch-rts-control-scheme.md | thin adapter synthesizes precise mouse/keyboard from multi-touch; engine untouched |
| T3 | https://docs.gamedevspice.com/rtsengine/manual/08_Modules/11_Mobile_Controls.html | Unity RTS mobile controls: one-finger pan, two-finger zoom, touch selection/commands |
| T4 | https://github.com/nikoltu/W3AndroidEngine | touch + S Pen controls; campaign/launcher; Android native views; 30→120 FPS target |

**Design takeaway:** implement a **touch input translator** that emits the same command events as
mouse/keyboard, with a state machine and DPI-scaled thresholds; never scatter touch handling across
game logic.

## 6. Shader/texture recompilation

| ID | Source | observed |
|---|---|---|
| S1 | https://github.com/hedge-dev/XenosRecomp | X360 shader container reverse-engineered (constant buffers, interpolators, vertex decls, instructions); ALU/vector converted; control flow via while/switch because HLSL has no goto; constants little-endian; output HLSL → DXC → DXIL/SPIR-V; dynamic register indexing and integer constants incomplete; designed for Unleashed Recompiled, not turnkey |
| S2 | https://github.com/xenia-project/xenia | Xbox 360 emulator; GPU/shader translation reference; experimental, legal reverse engineering of purchased games |
| S3 | https://github.com/gligli/libxemit | runtime Xenos shader emitter; low-level reference |
| S4 | https://github.com/xenosworks/vulkan4xenon | Vulkan 1.0 subset for Xbox 360 homebrew; not relevant to WC3 PC but useful for X360 graphics architecture |

**Important:** Warcraft III (PC) is **not** an Xbox 360 title. Its assets are MDX/BLP/MPQ/CASC, not
Xenos uCode/tiled X360 textures. XenosRecomp is a **separate R&D track** for future native ports of
Xbox 360 games; do not force WC3 through it. For WC3, the “recompilation” work is:
MDX/MDL → glTF, BLP → KTX2/ASTC/BC, SLK/TXT → typed data, shaders → WGSL/SPIR-V. Xbox 360 track:
Xenos uCode → HLSL → SPIR-V, X360 tiled textures → linear, endianness.

## 7. Legal / IP

| ID | Source | observed |
|---|---|---|
| L1 | https://www.blizzard.com/en-us/legal | Blizzard legal hub: Warcraft III Art Tools EULA, Custom Game Acceptable Use Policy, fan art terms |
| L2 | https://us.forums.blizzard.com/en/warcraft3/t/help-me-understand-the-new-legal-terms/21133 | Reforged legal terms discussion: custom games copyright assignment language (reported/contested; not legal advice) |
| L3 | https://www.techspot.com/news/83796-blizzard-now-owns-copyrights-all-custom-warcraft-3.html | News report: Blizzard owns copyrights to custom games under Reforged AUP (reported) |
| L4 | https://github.com/nikoltu/W3AndroidEngine | Clean-room-ish project explicitly: no game content, user-owned copy, not affiliated/endorsed by Blizzard |
| L5 | https://github.com/Retera/WarsmashModEngine | Hive: «mostly clean-room engineered rewrite»; no WC3 assets |

**Policy for this project:** no Blizzard code/assets in the repo; user supplies data; no distribution
of extracted MPQ/CASC; no Battle.net emulation; no DRM/password cracking; legal review before public
release; comply with Blizzard AUP/EULA; preserve third-party licenses (Urho3D MIT, StormLib MIT,
BLPConverter MIT, wgpu Apache/MIT, etc.).

## 8. RTS architecture references

| ID | Source | observed |
|---|---|---|
| A1 | https://github.com/OpenRA/OpenRA | C# RTS engine; modular traits, world/renderer separation, replay/observer, map formats; good architecture comparison |
| A2 | https://github.com/spring/spring | C++ RTS engine (Spring); large-scale simulation/rendering separation; reference only |
| A3 | https://github.com/Wargus/stratagus | C++ RTS engine; data-driven units/spells; reference |
| A4 | https://gafferongames.com/post/what_every_programmer_needs_to_know_about_game_networking/ | deterministic lockstep vs authoritative; network simulation approximations |
| A5 | https://github.com/Retera/WarsmashModEngine | Java WC3 sim/JASS/renderer split; closest domain reference |

## 9. Unknowns / research gaps

1. **Herofolends source**: no public repo found; APK launcher only; encrypted data archives; exact
   engine scripts unknown.
2. **FreeArc password/key**: not in APK; installer not passworded but archive encrypted; we will not
   crack. Need user-provided password or Windows installer run. ISO MPQs are the clean fallback.
3. **Turnip on Adreno 730**: driver build/version support and stability must be tested on device;
   system Adreno Vulkan fallback required.
4. **Reforged HD asset licensing**: user-owned only; cannot redistribute; feature parity must be
   tested with user data.
5. **JASS2 completeness**: Warsmash/W3AndroidEngine have JASS interpreters; our Rust interpreter
   must cover triggers, natives, handles, opcodes; donor behavior matrix needed.
6. **Multiplayer**: WC3 Battle.net protocol is not public/authorized; plan LAN/direct lockstep only.
7. **Performance**: 30/60/120 FPS targets on Adreno 730 with Turnip vs system driver; thermal
   throttling and memory bandwidth need measurement.

## 10. Reference backlog (to fetch/verify next)

- Warsmash `WarsmashModEngine` source: renderer/sim/JASS/CASC classes, build, tests.
- W3AndroidEngine source: Android launcher, touch, determinism, graphics sets.
- War3Net: map/JASS/MPQ APIs.
- HiveWE: editor data model, SLK/INI.
- StormLib/CascLib: API and extraction CLI.
- XenosRecomp: shader container/instruction coverage.
- OpenRA/Spring/Stratagus: module boundaries and replay.
- Blizzard AUP/EULA exact text (legal review).

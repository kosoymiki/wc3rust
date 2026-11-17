# DONORS MATRIX — byte inventory and mechanism comparison

Дата: 2026-11-12. Метод: shallow clone + file/byte inventory; donor code not executed. `files`/`bytes` exclude `.git`, `node_modules`, `build`, `target`, `dist`. `mechanisms` are file:line evidence from top source files (bounded, not exhaustive).

| ID | donor | lang | files | bytes | source | license | commit | mechanisms |
|---|---|---|---:|---:|---:|---|---|---|
| OpenRA | https://github.com/OpenRA/OpenRA | .cs | 4163 | 64118593 | 1772 | MIT | `b4d95a5671` | lockstep, opengl, pathfinding, renderer, shader, simulation, texture, touch |
| W3AndroidEngine | https://github.com/nikoltu/W3AndroidEngine | .java | 3133 | 16145369 | 3067 | MIT | `a865a38905` | blp, casc, jass, lockstep, mdx, mpq, opengl, pathfinding, renderer, shader, simulation, texture, touch |
| wgpu | https://github.com/gfx-rs/wgpu | .rs | 2710 | 38273680 | 1959 | MIT | `c92258fcae` | lockstep, opengl, renderer, shader, texture, touch, vulkan |
| SDL | https://github.com/libsdl-org/SDL | .c | 2385 | 54260557 | 1668 | MIT | `4ca1b1c2d0` | lockstep, opengl, renderer, shader, simulation, texture, touch, vulkan |
| WarsmashModEngine | https://github.com/Retera/WarsmashModEngine | .java | 2177 | 9098644 | 2123 | MIT | `f9e0aeed4b` | blp, casc, jass, lockstep, mdx, mpq, opengl, pathfinding, renderer, shader, simulation, texture, touch |
| War3Net | https://github.com/Drake53/War3Net | .cs | 1881 | 95055226 | 1586 | MIT | `5f6f7bac26` | blp, jass, lockstep, mdx, mpq, renderer, shader, texture |
| sdl3-sys-rs | https://github.com/revmischa/sdl3-sys-rs | .c | 1493 | 82483828 | 1122 | MIT | `a04bef22c5` | lockstep, opengl, renderer, shader, simulation, texture, touch, vulkan |
| WarsmashModEngine-Android | https://github.com/linsmod/WarsmashModEngine-Android | .java | 1453 | 10824822 | 1396 | MIT | `8b69cc7a13` | blp, casc, jass, lockstep, mdx, mpq, opengl, pathfinding, renderer, shader, simulation, texture, touch |
| WarsmashAndroidPort2022 | https://github.com/Retera/WarsmashAndroidPort2022 | .java | 1018 | 4526290 | 993 | MIT | `d1788bdca3` | blp, casc, jass, lockstep, mdx, mpq, opengl, pathfinding, renderer, shader, simulation, texture, touch |
| mdx-m3-viewer | https://github.com/flowtsohg/mdx-m3-viewer | .ts | 555 | 5187221 | 461 | MIT | `2ff0bc00c6` | blp, jass, lockstep, mdx, mpq, opengl, shader, simulation, texture, touch |
| parry | https://github.com/dimforge/parry | .rs | 501 | 3507237 | 482 |  | `b3c3584ce9` | lockstep, renderer, shader, simulation, touch |
| HiveWE | https://github.com/stijnherfst/HiveWE | .ixx | 494 | 10682820 | 142 | MIT | `cd6d61a8d3` | blp, casc, jass, lockstep, mdx, mpq, opengl, pathfinding, renderer, shader, simulation, texture, touch, vulkan |
| StormLib | https://github.com/ladislav-zezula/StormLib | .c | 379 | 3655073 | 318 | MIT | `116135c27e` | blp, lockstep, mpq, pathfinding, texture, touch |
| kira | https://github.com/tesselode/kira | .rs | 160 | 4939976 | 142 | MIT | `b0325713c2` | lockstep, renderer, simulation, touch |
| android-activity | https://github.com/rust-mobile/android-activity | .webp | 128 | 2264791 | 65 | MIT | `b4ddf059b7` | lockstep, simulation, touch, vulkan |
| CascLib | https://github.com/ladislav-zezula/CascLib | .h | 125 | 97054555 | 90 | MIT | `6b889ec8f4` | casc, lockstep, mpq, touch |
| war3-model | https://github.com/4eb0da/war3-model | .ts | 84 | 821411 | 63 | MIT | `52d7f421a5` | blp, mdx, opengl, pathfinding, renderer, shader, simulation, texture |
| libadrenotools | https://github.com/bylaws/libadrenotools | .h | 43 | 541293 | 29 | MIT | `8fae8ce254` | lockstep, renderer, texture, vulkan |
| libxemit | https://github.com/gligli/libxemit | .c | 22 | 7533321 | 10 |  | `3660b70443` | shader |
| XenosRecomp | https://github.com/hedge-dev/XenosRecomp | .h | 18 | 113378 | 11 | MIT | `990d03b28a` | shader, texture, vulkan |
| WarsmashWindowsWrapper | https://github.com/Retera/WarsmashWindowsWrapper | <none> | 11 | 27740 | 5 | MIT | `f78ef00a2e` |  |
| herofolends-local | local /storage/emulated/0/Download/wr3 | .bin | 5 | 1579379635 | 0 | mixed: Urho3D MIT + StormLib MIT + BLPConverter MIT; game assets proprietary | `` | blp, jass, mdx, mpq, renderer, touch |

## Per-donor narrative
### CascLib

- URL: https://github.com/ladislav-zezula/CascLib
- commit: `6b889ec8f421cbc9baccec296c582362ab917887` (2026-10-25), branch `master`
- size: 125 files, 97054555 bytes, 90 source files
- license: MIT (LICENSE)
- extensions: {'.h': 36, '.cpp': 34, '.c': 11, '<none>': 9, '.json': 4, '.txt': 3, '.vcxproj': 3, '.filters': 3, '.vcproj': 3, '.py': 3, '.sln': 2, '.bat': 2}
- README head: # CascLib  **CascLib** is an open-source implementation of library for reading Blizzard's CASC storages since 2014.  For API documentation, refer to http://www.zezula.net/en/casc/casclib.html .  ## Using CascLib on Linux (generic cmake build) - cmake -L CMakeLists.txt - make  ## Using CascLib as shared library in Debian/Ubuntu (build packages) - dpkg-buildpackage -us -uc - cd .. - sudo dpkg -i libcasc1_3.2_amd64.deb libcasc-dev_3.2_amd64.deb  ## Using CascLib as static library in Windows 1. Clone the CascLib repository into a local folder:  	`git clone https://github.com/ladislav-zezula/CascLib.git`  2. Open one of the solution files in Microsoft Visual Studio  	- `CascLib_vs17.sln` for Visu
- mechanism evidence:
  - **casc**: src/CascRootFile_MNDX.cpp:82: // Root file entry for CASC storages with MNDX root file (Heroes of the Storm) | src/common/FileStream.cpp:240: // However, Blizzard's CASC handlers read encoded data so that if less than expected
  - **lockstep**: src/CascReadFile.cpp:158: // Note the "Checksum" value probably won't match with older game versions. | src/CascRootFile_TVFS.cpp:211: // Determine size of file table offsets
  - **mpq**: src/common/FileStream.cpp:13: /* 28.04.14  1.00  Lad  Copied from StormLib                                 */ | test/TLogHelper.cpp:4: /* Helper class for reporting StormLib and CascLib tests                     */
  - **touch**: src/zlib/deflate.c:19: *      is preferable to spend more time in matches to allow very fast string | src/zlib/inffast.c:21: inflate execution time is spent in this routine.

### HiveWE

- URL: https://github.com/stijnherfst/HiveWE
- commit: `cd6d61a8d361e67621e469b2234d5359570122d0` (2026-10-11), branch `main`
- size: 494 files, 10682820 bytes, 142 source files
- license: MIT (LICENSE)
- extensions: {'.ixx': 95, '.png': 77, '.cpp': 76, '.mdl': 61, '.h': 52, '.ui': 16, '.frag': 14, '.vert': 13, '.json': 10, '.ico': 9, '.mdx': 9, '.dds': 8}
- README head: # HiveWE HiveWE is a Warcraft III World Editor (WE) that focuses on speed and ease of use.  It improves massively on the vanilla WE, especially for large maps where the regular World Editor is often too slow and clunky. The aim is to be a lightweight tool that only does what it needs to and relegates other tasks to high-quality apps.  [Thread on the Hiveworkshop](https://www.hiveworkshop.com/threads/introducing-hivewe.303183/)  Some of the benefits over the vanilla WE: - Way faster loading times (32s -> 4s on a sample map) - Renders your whole map at 120 fps - Modern UI/UX - Edit the pathing map directly - Edit global tile pathing - Import heightmaps - Improved editing palettes   - Directly 
- mechanism evidence:
  - **blp**: src/file_formats/mdx/validator.cpp:236: if (ext != ".blp" && ext != ".tga" && ext != ".tif" && ext != ".dds") { | src/asset_manager/asset_tree_model.cpp:83: if (ext == ".blp" || ext == ".tga" || ext == ".dds" || ext == ".png" || ext == ".jpg" || ext == ".jpeg") {
  - **casc**: src/base/map/unused_files.cpp:27: // CASC lookup is case-insensitive | overlay-ports/casclib/vcpkg.json:4: "description": "An open-source implementation of library for reading CASC storage from Blizzard games since 2014",
  - **jass**: src/base/triggers/map_script.cpp:24: if (script.mode == ScriptMode::jass) { | src/trigger_editor/trigger_editor.cpp:87: ScriptMode mode = ScriptMode::jass;
  - **lockstep**: data/warcraft/ability_insights.json:28: "raw_text": "* The TownHall version only dictates the range from which to search peasants.\n   * It’s defined in “Area of Effect”.\n   * Effectively it just causes all nearby peasa | src/brush/terrain_operators.cpp:293: // Determine if cliff
  - **mdx**: src/file_formats/mdx/mdl_reader.cpp:1: module MDX; | src/file_formats/mdx/mdx_reader.cpp:1: module MDX;
  - **mpq**: src/main_window/hivewe.cpp:2: #define __STORMLIB_NO_STATIC_LINK__ | src/main_window/main_ribbon.cpp:186: open_map_mpq->setText("Open Map (MPQ)");
  - **opengl**: overlay-ports/soil2/vcpkg.json:4: "description": "Simple OpenGL Image Library 2",
  - **pathfinding**: data/warcraft/ability_insights.json:28: "raw_text": "* The TownHall version only dictates the range from which to search peasants.\n   * It’s defined in “Area of Effect”.\n   * Effectively it just causes all nearby peasa
  - **renderer**: src/render/vulkan_viewport.cpp:268: // vk-bootstrap falls back to another format when the desired one is missing, which no pipeline is built for | src/object_editor/model_grid_widget.cpp:301: EditableMeshRenderer renderer;
  - **shader**: src/file_formats/mdx/mdx_reader.cpp:100: void read_MTLS_texs_pre_v1100(BinaryReader& reader, ShaderType shader, uint32_t version, Material& material, int& unique_tracks) { | src/file_formats/mdx/validator.cpp:307: && materials[geoset.material_id].layers.front().shader == ShaderType::HD;
  - **simulation**: tests/particle_emitter2_test.cpp:5: // and that the simulation survives the rest. | src/render/vulkan_viewport.h:103: /// A GPU timestamp and the host tick it was taken at, renewed every second as the two clocks drift apart
  - **texture**: src/file_formats/mdx/mdl_writer.cpp:275: // The `<= slot` designator only applies to HD layers that bind a texture per PBR slot; SD | src/file_formats/mdx/validator.cpp:9: /// A particle emitter texture grid dimension is only usable if it is a non-zero power of two.
  - **touch**: data/warcraft/ability_insights.json:48: "raw_text": "(Notes below by RoyGBV)\n* “Data - Charge for Current Life” doesn’t do anything.\n* “Data - Mana per Summoned Hitpoint” controls the mana spent by the caster upon cast
  - **vulkan**: src/render/vulkan_viewport.cpp:33: std::println("Vulkan unavailable: {}", result.error()); | src/object_editor/model_grid_widget.cpp:78: /// The Vulkan window behind ModelGridWidget. Holds the grid state; the widget forwards its API here.

### OpenRA

- URL: https://github.com/OpenRA/OpenRA
- commit: `b4d95a56717d8d0070f195e343f6c2728ce87900` (2026-11-08), branch `bleed`
- size: 4163 files, 64118593 bytes, 1772 source files
- license: MIT (COPYING)
- extensions: {'.cs': 1517, '.yaml': 759, '.png': 416, '.lua': 234, '.shp': 213, '.oramap': 189, '.bin': 184, '.ftl': 180, '.des': 171, '.tem': 85, '.aud': 46, '.sno': 38}
- README head: # OpenRA  A Libre/Free Real Time Strategy game engine supporting early Westwood classics.  * Website: [https://www.openra.net](https://www.openra.net) * Chat: [#openra on Libera](ircs://irc.libera.chat:6697/openra) ([web](https://web.libera.chat/#openra)) or [Discord](https://discord.openra.net) ![Discord Badge](https://discordapp.com/api/guilds/153649279762694144/widget.png) * Repository: [https://github.com/OpenRA/OpenRA](https://github.com/OpenRA/OpenRA) ![Continuous Integration](https://github.com/OpenRA/OpenRA/workflows/Continuous%20Integration/badge.svg)  Please read the [FAQ](https://github.com/OpenRA/OpenRA/wiki/FAQ) in our [Wiki](https://github.com/OpenRA/OpenRA/wiki) and report pro
- mechanism evidence:
  - **lockstep**: OpenRA.Mods.Common/Pathfinder/HierarchicalPathFinder.cs:58: /// created by dividing the map up into a series of grids, of say 10x10 nodes. Within each grid, we determine the | OpenRA.Game/Server/Server.cs:150: ReplayRecorder recorder;
  - **opengl**: OpenRA.Platforms.Default/OpenGL.cs:28: static class OpenGL | OpenRA.Mods.Common/Graphics/DefaultSpriteSequence.cs:159: [Desc("Apply an OpenGL/Photoshop inspired blend mode.")]
  - **pathfinding**: OpenRA.Mods.Common/Pathfinder/HierarchicalPathFinder.cs:19: namespace OpenRA.Mods.Common.Pathfinder | OpenRA.Mods.Common/Traits/Mobile.cs:18: using OpenRA.Mods.Common.Pathfinder;
  - **renderer**: OpenRA.Game/Game.cs:55: public static Renderer Renderer; | OpenRA.Mods.Common/Widgets/Logic/Lobby/LobbyUtils.cs:217: var labelText = WidgetUtils.TruncateText(FluentProvider.GetMessage(faction.Name), label.Bounds.Width, Game.Renderer.Fonts[label.Font]);
  - **shader**: OpenRA.Platforms.Default/OpenGL.cs:172: { GL_DEBUG_SOURCE_SHADER_COMPILER, "Shader Compiler" }, | OpenRA.Game/Renderer.cs:229: // Invalidate cached state to force a shader update
  - **simulation**: OpenRA.Game/Server/Server.cs:369: t.Tick(this); | OpenRA.Mods.Common/Traits/Air/Aircraft.cs:73: [Desc("Pitch steps to apply each tick when starting/stopping.")]
  - **texture**: OpenRA.Game/Game.cs:77: lastConnectionState = ConnectionState.PreConnecting; | OpenRA.Platforms.Default/OpenGL.cs:95: // Texture mapping
  - **touch**: OpenRA.Mods.Common/Traits/BotModules/SquadManagerBotModule.cs:130: readonly Stack<Squad> squadsPendingUpdate = []; | OpenRA.Platforms.Default/Sdl2PlatformWindow.cs:102: public bool IsSuspended { get; internal set; }

### SDL

- URL: https://github.com/libsdl-org/SDL
- commit: `4ca1b1c2d084b59ca72c87f36a18fc1a3898f424` (2026-11-11), branch `main`
- size: 2385 files, 54260557 bytes, 1668 source files
- license: MIT (LICENSE.txt, src/hidapi/LICENSE-bsd.txt, src/hidapi/LICENSE-gpl3.txt, src/hidapi/LICENSE-orig.txt, src/hidapi/LICENSE.txt)
- extensions: {'.c': 752, '.h': 739, '.png': 97, '.txt': 76, '.vcxproj': 73, '.md': 64, '.rpt_desc': 48, '.m': 47, '.cmake': 44, '.xml': 41, '.hlsl': 34, '.webp': 30}
- README head:  Simple DirectMedia Layer (SDL for short) is a cross-platform library designed to make it easy to write multi-media software, such as games and emulators.  You can find the latest release and additional information at: https://www.libsdl.org/  Installation instructions and a quick introduction is available in [INSTALL.md](INSTALL.md)  This library is distributed under the terms of the zlib license, available in [LICENSE.txt](LICENSE.txt).  Enjoy!  Sam Lantinga (slouken@libsdl.org) 
- mechanism evidence:
  - **lockstep**: include/SDL3/SDL_video.h:196: SDL_ORIENTATION_UNKNOWN,            /**< The display orientation can't be determined */ | include/SDL3/SDL_audio.h:283: * Determine if an SDL_AudioFormat represents floating point data.
  - **opengl**: include/SDL3/SDL_opengl_glext.h:16: ** This header is generated from the Khronos OpenGL / OpenGL ES XML | src/video/khronos/GLES2/gl2ext.h:12: ** This header is generated from the Khronos OpenGL / OpenGL ES XML
  - **renderer**: src/video/directx/d3d12.h:139: #ifndef __ID3D12PipelineState_FWD_DEFINED__ | src/video/khronos/vulkan/vulkan_core.h:107: VK_DEFINE_NON_DISPATCHABLE_HANDLE(VkPipelineCache)
  - **shader**: src/render/vulkan/SDL_render_vulkan.c:188: // Vertex shader, common values | include/SDL3/SDL_gpu.h:107: * a compute shader. Then it sets states it needs for the compute dispatches:
  - **simulation**: src/video/wayland/SDL_waylandevents.c:77: // Weston uses a ratio of 10 units per scroll tick | src/video/windows/SDL_windowsevents.c:152: static void WIN_SetMessageTick(DWORD tick)
  - **texture**: src/gpu/vulkan/SDL_gpu_vulkan.c:69: // Only required for decoding HDR ASTC textures | src/video/khronos/GLES2/gl2ext.h:197: #ifndef GL_KHR_texture_compression_astc_hdr
  - **touch**: src/video/khronos/openxr/openxr.h:150: XR_SESSION_LOSS_PENDING = 3, | src/video/khronos/openxr/openxr_reflection.h:36: _(XR_SESSION_LOSS_PENDING, 3) \
  - **vulkan**: src/video/khronos/vulkan/vulkan_core.h:11: ** This header is generated from the Khronos Vulkan XML API Registry. | src/gpu/vulkan/SDL_gpu_vulkan.c:30: #include "../../video/khronos/vulkan/vulkan.h"

### StormLib

- URL: https://github.com/ladislav-zezula/StormLib
- commit: `116135c27e8c0dcf8ea0a0c549a6d737ce10fc73` (2026-10-12), branch `master`
- size: 379 files, 3655073 bytes, 318 source files
- license: MIT (LICENSE)
- extensions: {'.c': 234, '.h': 47, '.txt': 32, '.cpp': 28, '.vcxproj': 5, '.filters': 5, '.json': 4, '<none>': 3, '.sln': 3, '.vcproj': 3, '.bat': 2, '.yml': 2}
- README head: # StormLib  This is official repository for the StormLib library, an open-source project that can work with Blizzard MPQ archives.  ## Installation and basic usage  ### Windows (Visual Studio 2022) 0. Make sure you have the toolset for Visual Studio 2017 - Windows XP installed 1. Download the latest release of StormLib 2. Open the solution file `StormLib.sln` in Visual Studio 2017/2019/2022 3. Choose "Build / Batch Build" and select every build of "StormLib" 4. Choose "Rebuild" 5. The result libraries are in `.\bin\Win32` and `.\bin\x64`  Note that you can also build the library using newer toolset, such as v143. To do that, you need to retarget the projects. Right-click on the solution, the
- mechanism evidence:
  - **blp**: src/SFileReadFile.cpp:198: // Special case (MPQ_2024_v1_300TK2.09p.w3x, file File00010254.blp): | src/SFileOpenFileEx.cpp:38: // ReplaceableTextures\CommandButtons\BTNHaboss79.blp
  - **lockstep**: test/StormTest.cpp:45: #define ERROR_UNDETERMINED_RESULT 0xC000FFFF | src/SBaseFileTable.cpp:288: static ULONGLONG DetermineArchiveSize_V1(
  - **mpq**: test/StormTest.cpp:4: /* Test module for StormLib                                                  */ | src/SBaseFileTable.cpp:11: #define __STORMLIB_SELF__
  - **pathfinding**: src/libtommath/tommath.h:269: /* b = a*2 */ | src/libtomcrypt/src/headers/tomcrypt_math.h:265: @param  d     The destination (a*b mod c)
  - **texture**: src/SFileReadFile.cpp:198: // Special case (MPQ_2024_v1_300TK2.09p.w3x, file File00010254.blp): | src/SFileOpenFileEx.cpp:38: // ReplaceableTextures\CommandButtons\BTNHaboss79.blp
  - **touch**: src/zlib/deflate.c:19: *      is preferable to spend more time in matches to allow very fast string | src/zlib/inffast.c:38: inflate execution time is spent in this routine.

### W3AndroidEngine

- URL: https://github.com/nikoltu/W3AndroidEngine
- commit: `a865a38905581166cc960113e1c21602efc96193` (2026-11-10), branch `main`
- size: 3133 files, 16145369 bytes, 3067 source files
- license: MIT (LICENSE)
- extensions: {'.java': 3017, '.json': 35, '.j': 17, '.png': 11, '.gradle': 9, '.jpg': 7, '.fdf': 7, '.xml': 6, '<none>': 3, '.jar': 3, '.md': 2, '.properties': 2}
- README head: ![Warsmash Reforged](docs/images/banner.png)  <p align="center">   <b>W3 on Windows and on Android phones and tablets: an engine and a launcher that play from your own copy of the game.</b> </p>  <p align="center">   <img alt="Platform: Windows and Android" src="https://img.shields.io/badge/platform-Windows%20%7C%20Android%208.0%2B-3DDC84?logo=android&logoColor=white">   <img alt="Version 0.3.6" src="https://img.shields.io/badge/version-0.3.6-blue">   <img alt="Licence: AGPL-3.0" src="https://img.shields.io/badge/licence-AGPL--3.0-orange">   <img alt="Engine: Warsmash" src="https://img.shields.io/badge/engine-Warsmash-8a2be2">   <img alt="Languages: Java 17, JASS2, GLSL" src="https://img.shi
- mechanism evidence:
  - **blp**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:176: final String texturesExt = ".blp"; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUICommands.java:152: placeholderPreview.setTexture("ReplaceableTextures\\TeamColor\\TeamColor06.blp", this.ui.rootFrame);
  - **casc**: desktop/src/com/etheller/warsmash/desktop/launcher/datasources/DataSourceChooserPanel.java:68: import com.hiveworkshop.blizzard.casc.io.WarcraftIIICASC; | core/src/com/hiveworkshop/blizzard/casc/vfs/VirtualFileSystem.java:1: package com.hiveworkshop.blizzard.casc.vfs;
  - **jass**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CUnit.java:22: import com.etheller.warsmash.parsers.jass.scope.CommonTriggerExecutionScope; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:13: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.abilities.jass.RecordingAbilityTargetCheckReceiver;
  - **lockstep**: desktop/src/com/etheller/warsmash/desktop/launcher/LauncherScreen.java:61: import com.etheller.warsmash.viewer5.handlers.w3x.replay.GameReplays; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:6: import com.etheller.warsmash.viewer5.handlers.w3x.replay.ReplayPlayback;
  - **mdx**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:125: import com.etheller.warsmash.viewer5.handlers.mdx.Attachment; | core/src/com/etheller/warsmash/parsers/fdf/GameUI.java:80: import com.etheller.warsmash.viewer5.handlers.mdx.MdxModel;
  - **mpq**: core/src/mpq/compression/huffman/Huffman.java:1: package mpq.compression.huffman;
  - **opengl**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:123: import com.etheller.warsmash.viewer5.gl.WebGL; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:55: import com.etheller.warsmash.viewer5.gl.WebGL;
  - **pathfinding**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:86: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.pathing.CPathfindingProcessor; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:24: public class CPathfindingProcessor {
  - **renderer**: desktop/src/com/etheller/warsmash/desktop/launcher/LauncherScreen.java:147: private GlueBackdropRenderer renderer; | core/src/com/etheller/warsmash/WarsmashGdxMenuScreen.java:242: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER);
  - **shader**: android/src/com/etheller/warsmash/android/LauncherKit.java:18: import android.graphics.Shader; | core/src/com/etheller/warsmash/viewer5/handlers/mdx/GeometryEmitterFuncs.java:179: public static void bindParticleEmitter2Shader(final ParticleEmitter2 emitter, final ShaderProgram shader) {
  - **simulation**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CUnit.java:1: package com.etheller.warsmash.viewer5.handlers.w3x.simulation; | desktop/src/com/etheller/warsmash/desktop/launcher/LauncherScreen.java:63: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.campaign.CCampaignProgress;
  - **texture**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:120: import com.etheller.warsmash.viewer5.Texture; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/abilitybuilder/core/AbilityBuilderGsonBuilder.java:99: .registerSubtype(ABCallbackGetLastCreatedAbility.class, "getLastCreatedAbility")
  - **touch**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:39: import com.etheller.warsmash.input.touch.VirtualCursor; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUICommands.java:19: import com.etheller.warsmash.input.touch.TouchInputRouter;

### War3Net

- URL: https://github.com/Drake53/War3Net
- commit: `5f6f7bac26ed7ce7d0db83ceb4fb9ce59f918ff8` (2026-10-17), branch `master`
- size: 1881 files, 95055226 bytes, 1586 source files
- license: MIT (LICENSE)
- extensions: {'.cs': 1536, '.w3m': 54, '.csproj': 45, '.w3x': 45, '.md': 26, '.w3i': 25, '.j': 25, '.w3e': 23, '.doo': 11, '.txt': 9, '.props': 7, '.png': 7}
- README head: # War3Net Modding Library ## The complete .NET toolkit for Warcraft III modding  [![GitHub stars](https://img.shields.io/github/stars/Drake53/War3Net.svg)](https://github.com/Drake53/War3Net/stargazers) [![GitHub forks](https://img.shields.io/github/forks/Drake53/War3Net.svg)](https://github.com/Drake53/War3Net/network/members) [![GitHub issues](https://img.shields.io/github/issues/Drake53/War3Net.svg)](https://github.com/Drake53/War3Net/issues) [![GitHub pull requests](https://img.shields.io/github/issues-pr/Drake53/War3Net.svg)](https://github.com/Drake53/War3Net/pulls) [![GitHub license](https://img.shields.io/github/license/Drake53/War3Net.svg)](https://github.com/Drake53/War3Net/blob/ma
- mechanism evidence:
  - **blp**: src/War3Net.Drawing.Blp/BlpFile.cs:6: namespace War3Net.Drawing.Blp | src/War3Net.Drawing.Blp/BlpEncoder.cs:1: ﻿namespace War3Net.Drawing.Blp
  - **jass**: src/War3Net.Build.Core/Map.cs:42: ScriptLanguage.Jass => JassMapScript.FileName, | src/War3Net.CodeAnalysis.Jass/SyntaxFactory/NativeFunctionDeclarationFactory.cs:1: ﻿namespace War3Net.CodeAnalysis.Jass
  - **lockstep**: src/War3Net.IO.Mpq/MpqArchive.cs:220: crc32 = Crc32Checksum.Compute(mpqFile.MpqStream); | src/War3Net.IO.Mpq/MpqStreamFactory.cs:121: // Files with metadata have an extra block containing block checksums.
  - **mdx**: src/War3Net.Modeling/BinaryConstants.cs:3: // https://www.hiveworkshop.com/threads/mdx-specifications.240487/ | src/War3Net.Build.Core/Providers/LightEnvironmentProvider.cs:9: case Tileset.Ashenvale: return @"Environment\DNC\DNCAshenvale\DNCAshenvaleTerrain\DNCAshenvaleTerrain.mdl";
  - **mpq**: src/War3Net.IO.Mpq/MpqArchive.cs:1: ﻿namespace War3Net.IO.Mpq | src/War3Net.IO.Mpq/MpqStream.cs:1: ﻿namespace War3Net.IO.Mpq
  - **renderer**: src/War3Net.Rendering/ModelInstance.cs:27: commandList.SetPipeline(model._instanceResources.Pipelines[geoset][layer]);
  - **shader**: src/War3Net.Rendering/Factories/ShaderFactory.cs:47: var shaders = resourceFactory.CreateFromSpirv(
  - **texture**: src/War3Net.Modeling/BinaryModelParser.cs:243: private static IEnumerable<Texture> ParseTextures(BinaryReader reader) | src/War3Net.Rendering/ModelInstance.cs:104: Textures = mesh.TexturePaths.Select(texture => texture ?? string.Empty).ToArray(),

### WarsmashAndroidPort2022

- URL: https://github.com/Retera/WarsmashAndroidPort2022
- commit: `d1788bdca3858c57f271aac1ad1749213928a833` (2023-08-20), branch `master`
- size: 1018 files, 4526290 bytes, 993 source files
- license: MIT (LICENSE)
- extensions: {'.java': 985, '.png': 9, '.gradle': 5, '.tokens': 4, '<none>': 3, '.properties': 3, '.xml': 3, '.jar': 2, '.md': 1, '.bat': 1, '.pro': 1, '.ini': 1}
- README head: # Warsmash3Phone  This project is NOT UNDER ACTIVE DEVELOPMENT and will NOT be supported!  This project is a 2022 parody of Warsmash created by Retera originally in private that is capable of running on some Android devices. In the summer of 2022, after the Arclight Rumble Mobile for Warcraft was announced, as a form of self-parody or perhaps dark humor I ported the work in progress Java based Warcraft 3 emulator "Warsmash" so that it would run on Android devices. This allowed me to create YouTube videos of myself holding a phone with what would appear to be Warcraft 3 on the phone in private, mostly to make parody videos about Arclight Rumble such as the following (Warning: video contains e
- mechanism evidence:
  - **blp**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:146: final String texturesExt = ".blp"; | core/src/main/java/com/etheller/warsmash/WarsmashGdxMenuScreen.java:162: "ReplaceableTextures\\TeamColor\\TeamColor06.blp");
  - **casc**: core/src/main/java/com/hiveworkshop/blizzard/casc/vfs/VirtualFileSystem.java:1: package com.hiveworkshop.blizzard.casc.vfs; | core/src/main/java/com/etheller/warsmash/WarsmashGdxMapScreen.java:232: case "CASC": {
  - **jass**: core/src/main/java/com/etheller/warsmash/parsers/jass/Jass2.java:1: package com.etheller.warsmash.parsers.jass; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:59: import com.etheller.warsmash.parsers.jass.Jass2.RootFrameListener;
  - **lockstep**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/ui/MenuUI.java:152: private GlueTextButtonFrame viewReplayButton; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/mdx/MdxComplexInstance.java:293: * determined to not be visible will not be updated, nor will any of their
  - **mdx**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:73: import com.etheller.warsmash.viewer5.handlers.mdx.Attachment; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:77: import com.etheller.warsmash.viewer5.handlers.mdx.Attachment;
  - **mpq**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:127: import mpq.MPQArchive; | core/src/main/java/mpq/compression/huffman/Huffman.java:1: package mpq.compression.huffman;
  - **opengl**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:75: import com.etheller.warsmash.viewer5.gl.WebGL; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:49: import com.etheller.warsmash.viewer5.gl.WebGL;
  - **pathfinding**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:45: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.pathing.CPathfindingProcessor; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:20: public class CPathfindingProcessor {
  - **renderer**: core/src/main/java/com/etheller/warsmash/WarsmashGdxMenuScreen.java:115: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER); | core/src/main/java/com/etheller/warsmash/WarsmashGdxGame.java:81: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER);
  - **shader**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/SplatModel.java:18: * Shouldn't this just be a geomtry shader that takes X/Y/Texture as input and | core/src/main/java/com/etheller/warsmash/viewer5/handlers/mdx/GeometryEmitterFuncs.java:155: public static void bindParticleEmitter2Shader(final ParticleEmitter2 emitter, final ShaderProgram shader) {
  - **simulation**: core/src/main/java/com/etheller/warsmash/parsers/jass/Jass2.java:75: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.CDestructable; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:105: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.CDestructable;
  - **texture**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:26: import com.badlogic.gdx.graphics.Texture; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:73: import com.etheller.warsmash.viewer5.Texture;
  - **touch**: core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:93: private boolean timeOfDaySuspended; | core/src/main/java/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:148: private void touch(final int pathfindJobId) {

### WarsmashModEngine

- URL: https://github.com/Retera/WarsmashModEngine
- commit: `f9e0aeed4be372d6016519d0e97b384aa873f374` (2025-12-08), branch `main`
- size: 2177 files, 9098644 bytes, 2123 source files
- license: MIT (LICENSE)
- extensions: {'.java': 2100, '.j': 17, '.json': 15, '.gradle': 8, '.ini': 8, '.fdf': 5, '.png': 4, '<none>': 3, '.jar': 3, '.md': 2, '.properties': 2, '.bat': 2}
- README head: # Warsmash: Mod Engine This is a Warcraft III emulator made using LibGDX game engine. For obvious reasons, the Warsmash project does not include Warcraft III assets. Instead, in order to run the Warcraft III emulator you need to purchase a valid copy of Warcraft III from Activision Blizzard and then configure Warsmash to use those assets.  ### In the News Some news websites and social media claimed that Warsmash was taken down in 2022 via a Cease and Desist letter from Activision Blizzard, but that did not happen. The confusion started from a parody video on YouTube that was almost immediately taken down. If you have more concerns on that topic, feel free to join the Warsmash discord server 
- mechanism evidence:
  - **blp**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:156: final String texturesExt = ".blp"; | core/src/com/etheller/warsmash/WarsmashGdxFDFTestRenderScreen.java:145: "ReplaceableTextures\\TeamColor\\TeamColor06.blp");
  - **casc**: core/src/com/hiveworkshop/blizzard/casc/vfs/VirtualFileSystem.java:1: package com.hiveworkshop.blizzard.casc.vfs; | core/src/com/etheller/warsmash/WarsmashGdxMapScreen.java:241: case "CASC": {
  - **jass**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:1: package com.etheller.warsmash.parsers.jass; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:64: import com.etheller.warsmash.parsers.jass.Jass2.RootFrameListener;
  - **lockstep**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MenuUI.java:176: private GlueTextButtonFrame viewReplayButton; | server/src/com/etheller/warsmash/networking/uberserver/GamingNetworkServerBusinessLogicImpl.java:182: connectionContext.joinedGame(gameName, game.mapName, game.mapChecksum);
  - **mdx**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:87: import com.etheller.warsmash.viewer5.handlers.mdx.Sequence; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:77: import com.etheller.warsmash.viewer5.handlers.mdx.Attachment;
  - **mpq**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:159: import mpq.MPQArchive; | core/src/mpq/compression/huffman/Huffman.java:1: package mpq.compression.huffman;
  - **opengl**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:82: import com.etheller.warsmash.viewer5.gl.WebGL; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:48: import com.etheller.warsmash.viewer5.gl.WebGL;
  - **pathfinding**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:60: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.pathing.CPathfindingProcessor; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:19: public class CPathfindingProcessor {
  - **renderer**: core/src/com/etheller/warsmash/WarsmashGdxMenuScreen.java:98: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER); | core/src/com/etheller/warsmash/WarsmashGdxFDFTestRenderScreen.java:100: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER);
  - **shader**: core/src/com/etheller/warsmash/viewer5/handlers/mdx/GeometryEmitterFuncs.java:178: public static void bindParticleEmitter2Shader(final ParticleEmitter2 emitter, final ShaderProgram shader) { | core/src/com/etheller/warsmash/viewer5/handlers/w3x/SplatModel.java:18: * Shouldn't this just be a geomtry shader that takes X/Y/Texture as input and
  - **simulation**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:102: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.CDestructable; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:112: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.CDestructable;
  - **texture**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:28: import com.badlogic.gdx.graphics.Texture; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:80: import com.etheller.warsmash.viewer5.Texture;
  - **touch**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:128: private boolean timeOfDaySuspended; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:154: private void touch(final int pathfindJobId) {

### WarsmashModEngine-Android

- URL: https://github.com/linsmod/WarsmashModEngine-Android
- commit: `8b69cc7a13c0247b5e1d5015cd79c1f4ead808c5` (2023-07-23), branch `main`
- size: 1453 files, 10824822 bytes, 1396 source files
- license: MIT (LICENSE)
- extensions: {'.java': 1350, '.glsl': 30, '.png': 10, '.fdf': 10, '.gradle': 9, '.ini': 8, '.xml': 7, '.pf': 4, '.jui': 4, '<none>': 3, '.properties': 3, '.md': 2}
- README head: # Warsmash: Mod Engine This is a Warcraft III emulator made using LibGDX game engine. For obvious reasons, the Warsmash project does not include Warcraft III assets. Instead, in order to run the Warcraft III emulator you need to purchase a valid copy of Warcraft III from Activision Blizzard and then configure Warsmash to use those assets.  ### In the News Some news websites and social media claimed that Warsmash was taken down in 2022 via a Cease and Desist letter from Activision Blizzard, but that did not happen. The confusion started from a parody video on YouTube that was almost immediately taken down. If you have more concerns on that topic, feel free to join the Warsmash discord server 
- mechanism evidence:
  - **blp**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:24: import com.etheller.warsmash.viewer5.handlers.blp.BlpGdxTexture; | core/src/com/etheller/warsmash/WarsmashGdxFDFTestRenderScreen.java:144: "ReplaceableTextures\\TeamColor\\TeamColor06.blp");
  - **casc**: core/src/com/hiveworkshop/blizzard/casc/vfs/VirtualFileSystem.java:1: package com.hiveworkshop.blizzard.casc.vfs; | core/src/com/etheller/warsmash/WarsmashGdxMapScreen.java:232: case "CASC": {
  - **jass**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:1: package com.etheller.warsmash.parsers.jass; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:60: import com.etheller.warsmash.parsers.jass.Jass2.RootFrameListener;
  - **lockstep**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MenuUI.java:170: private GlueTextButtonFrame viewReplayButton; | core/src/com/etheller/warsmash/util/JAVACrc32C.java:23: import java.util.zip.Checksum;
  - **mdx**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:73: import com.etheller.warsmash.viewer5.handlers.mdx.Sequence; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:74: import com.etheller.warsmash.viewer5.handlers.mdx.Attachment;
  - **mpq**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:130: import mpq.MPQArchive; | core/src/mpq/compression/huffman/Huffman.java:1: package mpq.compression.huffman;
  - **opengl**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:81: import com.etheller.warsmash.viewer5.gl.WebGL; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/environment/Terrain.java:44: import com.etheller.warsmash.viewer5.gl.WebGL;
  - **pathfinding**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:47: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.pathing.CPathfindingProcessor; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java:19: public class CPathfindingProcessor {
  - **renderer**: core/src/com/etheller/warsmash/WarsmashGdxMenuScreen.java:87: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER); | core/src/com/etheller/warsmash/WarsmashGdxFDFTestRenderScreen.java:99: final String renderer = Gdx.gl.glGetString(GL20.GL_RENDERER);
  - **shader**: core/src/com/etheller/warsmash/viewer5/handlers/mdx/MdxShaders.java:9: return new ShaderShellCodes("MdxShaders.vsHd.glsl","MdxShaders.fsHd.glsl"); | core/src/com/etheller/warsmash/viewer5/handlers/w3x/SplatModel.java:18: * Shouldn't this just be a geomtry shader that takes X/Y/Texture as input and
  - **simulation**: core/src/com/etheller/warsmash/parsers/jass/Jass2.java:10: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.util.*; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:106: import com.etheller.warsmash.viewer5.handlers.w3x.simulation.CDestructable;
  - **texture**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/ui/MeleeUI.java:26: import com.badlogic.gdx.graphics.Texture; | core/src/com/etheller/warsmash/viewer5/handlers/w3x/War3MapViewer.java:79: import com.etheller.warsmash.viewer5.Texture;
  - **touch**: core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/CSimulation.java:104: private boolean timeOfDaySuspended; | core/src/com/etheller/warsmash/WarsmashGdxMenuScreen.java:15: import com.badlogic.gdx.input.GestureDetector;

### WarsmashWindowsWrapper

- URL: https://github.com/Retera/WarsmashWindowsWrapper
- commit: `f78ef00a2ecb05d5f1331d4b4ad163caf66369a0` (2021-04-04), branch `master`
- size: 11 files, 27740 bytes, 5 source files
- license: MIT (LICENSE)
- extensions: {'<none>': 2, '.cpp': 2, '.h': 2, '.md': 1, '.sln': 1, '.rc': 1, '.vcxproj': 1, '.filters': 1}
- README head: # Warsmash Windows Wrapper This repository is a simple Visual Studio project to create an EXE that will launch the Warsmash mod engine on Windows 10. Warsmash is made in Java so for the average user if we did not have an EXE like this, we would then require the user to install Java and learn how to do all of that, and that might annoy some users, since most common Warcraft 3 players are likely to associate Java with the annoying popup on their computer from 2005-2015 that asked for updates despite not doing anything, rather than to associate it with a programming language. 

### XenosRecomp

- URL: https://github.com/hedge-dev/XenosRecomp
- commit: `990d03b28a27b50277ee5d8d942e1c5f873869d1` (2025-08-03), branch `main`
- size: 18 files, 113378 bytes, 11 source files
- license: MIT (LICENSE.md)
- extensions: {'.h': 7, '.txt': 3, '.cpp': 3, '<none>': 2, '.md': 2, '.json': 1}
- README head: # XenosRecomp  XenosRecomp is a tool that converts Xbox 360 shader binaries to HLSL. The resulting files can be recompiled to DXIL and SPIR-V using the DirectX Shader Compiler (DXC) for use in Direct3D 12 (D3D12) and Vulkan.  The current implementation is designed around [Unleashed Recompiled](https://github.com/hedge-dev/UnleashedRecomp), a recompilation project that implements a translation layer for the renderer rather than emulating the Xbox 360 GPU. Unleashed Recompiled specific implementations are placed under the `UNLEASHED_RECOMP` preprocessor macro.  Users are expected to modify the recompiler to fit their needs. **Do not expect the recompiler to work out of the box.**  ## Implement
- mechanism evidence:
  - **shader**: XenosRecomp/main.cpp:1: #include "shader.h" | XenosRecomp/shader_common.h:18: #ifdef __spirv__
  - **texture**: XenosRecomp/shader_common.h:55: uint2 getTexture2DDimensions(Texture2D<float4> texture) | XenosRecomp/constant_table.h:20: Texture,
  - **vulkan**: XenosRecomp/shader_recompiler.cpp:77: // NOTE: These are specialized Vulkan locations for Unleashed Recompiled. Change as necessary. Likely not going to work with other games.

### android-activity

- URL: https://github.com/rust-mobile/android-activity
- commit: `b4ddf059b77be12cdb955d394e65a92e7568d936` (2026-03-24), branch `main`
- size: 128 files, 2264791 bytes, 65 source files
- license: MIT (android-activity/LICENSE, android-activity/LICENSE-APACHE, android-activity/LICENSE-MIT)
- extensions: {'.webp': 30, '.rs': 24, '.xml': 11, '<none>': 10, '.h': 10, '.gradle': 6, '.md': 5, '.toml': 4, '.json': 4, '.cpp': 4, '.properties': 4, '.yml': 2}
- README head: # `android-activity`  [![ci](https://github.com/rust-mobile/android-activity/actions/workflows/ci.yml/badge.svg)](https://github.com/rust-mobile/android-activity/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/android-activity.svg)](https://crates.io/crates/android-activity) [![Docs](https://docs.rs/android-activity/badge.svg)](https://docs.rs/android-activity) [![MSRV](https://img.shields.io/badge/rustc-1.85.0+-ab6000.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)  ## Overview  `android-activity` provides a "glue" layer for building native Rust applications on Android, supporting multiple [`Activity`] base classes. It's comparable to [`android_native_app_g
- mechanism evidence:
  - **lockstep**: android-activity/src/game_activity/ffi_aarch64.rs:3: #[doc = r" If Bindgen could only determine the size and alignment of a"] | android-activity/src/input.rs:35: /// Pretty much any device with buttons. Query the keyboard type to determine
  - **simulation**: android-activity/android-games-sdk/game-activity/prefab-src/modules/game-activity/include/game-activity/native_app_glue/android_native_app_glue.h:21: * The glue library to interface your game loop with GameActivity.
  - **touch**: android-activity/android-games-sdk/game-activity/prefab-src/modules/game-activity/src/game-activity/native_app_glue/android_native_app_glue.c:76: LOGV("Config: mcc=%d mnc=%d lang=%c%c cnt=%c%c orien=%d touch=%d dens=%d " | android-activity/android-games-sdk/game-activity/prefab-src/modules/game-activity/include/game-activity/GameActivity.h:62: GAMECOMMON_INSETS_TYPE_MANDATORY_SYSTEM_GESTURES,
  - **vulkan**: examples/na-mainloop/Cargo.toml:67: name = "android.hardware.vulkan.level"

### herofolends-local

- URL: local /storage/emulated/0/Download/wr3
- commit: `` (2026-11-12), branch ``
- size: 5 files, 1579379635 bytes, 0 source files
- license: mixed: Urho3D MIT + StormLib MIT + BLPConverter MIT; game assets proprietary (APK assets/Data/html/Urho3D/LICENSE.html, StormLib/LICENSE.html, BLPConverter/LICENSE.html)
- extensions: {'.apk': 1, '.exe': 1, '.bin': 3}
- sha256: {'Herofolends+v0.0.16.apk': 'ae90e817023230b50c13635ed4468ebeb0bb1032a3bdfb73ba40590da2cd0838', 'setup.exe': '657f876ea7a279dc134bc7c5c6771e27c1890c7b79b9739d78787766b138f4ba', 'data1.bin': 'a327e5cc5121b4f02381e715b8c2bd1e45d047a04ad16c7fb2ddc882de47db80', 'data2.bin': 'bd1433e2332d6905d1e2125a8269c16ceb1127798ed5928165d1f73cb74c85af', 'data3.bin': '798290ebb89ebdd53f5d924b3b3e313bb322cfc3799694420908c28d74eb6f93'}
- README head: Herofolends public alpha. RTS engine without own resources; uses WC3 RoC/TFT content. Urho3D + Lua/AngelScript + StormLib + BLPConverter. APK is a launcher; data*.bin are encrypted FreeArc; ISO install media available.
- mechanism evidence:
  - **blp**: BLPConverter license page
  - **jass**: Data/Scripts and Data/LuaScripts scanned by Urho3D.java; 0 in APK
  - **mdx**: assets/Data/Models/Plane.mdl
  - **mpq**: about.htm + StormLib license
  - **renderer**: CoreData/Shaders/GLSL 45 files; RenderPaths 10
  - **touch**: UI XML: MainMenu/SelectRace/SelectMap/InGameMenu/ChangeResources

### kira

- URL: https://github.com/tesselode/kira
- commit: `b0325713c2cf090f980b46246505509f838d48f5` (2026-10-27), branch `main`
- size: 160 files, 4939976 bytes, 142 source files
- license: MIT (LICENSE-APACHE, LICENSE-MIT)
- extensions: {'.rs': 130, '.toml': 11, '.ogg': 8, '<none>': 4, '.md': 3, '.yml': 2, '.json': 1, '.wav': 1}
- README head: # Kira  #### [crates.io](https://crates.io/crates/kira) | [docs](https://docs.rs/kira/)  Kira is a backend-agnostic library to create expressive audio for games. It provides tweens for smoothly adjusting properties of sounds, a flexible mixer for applying effects to audio, a clock system for precisely timing audio events, and spatial audio support.  ## Examples  Playing a sound multiple times simultaneously:  ```rust use kira::{ 	AudioManager, AudioManagerSettings, DefaultBackend, 	sound::static_sound::StaticSoundData, };  // Create an audio manager. This plays sounds and manages resources. let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())?; let sound_dat
- mechanism evidence:
  - **lockstep**: crates/kira/src/modulator.rs:16: - You want to control multiple properties of objects in lockstep | crates/kira/src/modulator/lfo/builder.rs:28: /// This determines when in the oscillation the modulator will start.
  - **renderer**: crates/kira/src/manager.rs:17: Backend, DefaultBackend, Renderer, RendererShared, | crates/kira/src/backend/cpal/desktop/stream_manager.rs:27: renderer: RendererWithCpuUsage,
  - **simulation**: crates/kira/src/sound/streaming/sound/test.rs:370: // the sound is set to start at tick 2, so it should not | crates/kira/src/sound/static_sound/sound/test.rs:305: // the sound is set to start at tick 2, so it should not
  - **touch**: crates/kira/src/modulator/lfo.rs:97: /// The ratio between how much time the oscillator spends on one value vs. the other.

### libadrenotools

- URL: https://github.com/bylaws/libadrenotools
- commit: `8fae8ce254dfc1344527e05301e43f37dea2df80` (2024-09-10), branch `master`
- size: 43 files, 541293 bytes, 29 source files
- license: MIT (LICENSE)
- extensions: {'.h': 18, '.cpp': 7, '.md': 5, '<none>': 3, '.c': 3, '.txt': 2, '.in': 1, '.sh': 1, '.s': 1, '.py': 1, '.bp': 1}
- README head: ### Adreno Tools  A library for applying rootless Adreno GPU driver modifications/replacements. Currently supports loading custom GPU drivers such as [turnip](https://docs.mesa3d.org/android.html#building-using-the-android-ndk),  enabling BCn textures and redirecting file operations to allow accessing shader dumps and modification of the [driver config file](https://gist.github.com/bylaws/04130932e2634d1c6a2a9729e3940d60) without root.  #### Documentation API is documented in the `include/adrenotools` headers.  #### Support Android 9+ Arm64  Please create an issue if support for anything else is desired.  ### FAQ  #### Is there an example project?  There is a simple bare-bones project [Adren
- mechanism evidence:
  - **lockstep**: tools/qtimapper-shim/ext/gralloc_priv.h:96: /* Flag to determine interlaced content
  - **renderer**: src/hook/kgsl.h:285: * @soptimestamp: Start of pipeline timestamp written by GPU before the
  - **texture**: tools/qtimapper-shim/ext/gr_utils.cpp:36: #define ASTC_BLOCK_SIZE 16 | tools/qtimapper-shim/ext/gr_adreno_info.cpp:238: case HAL_PIXEL_FORMAT_COMPRESSED_RGBA_ASTC_4x4_KHR:
  - **vulkan**: src/hook/kgsl.h:274: * This field contains the adreno revision | tools/qtimapper-shim/ext/gr_utils.cpp:34: #include "gr_adreno_info.h"

### libxemit

- URL: https://github.com/gligli/libxemit
- commit: `3660b70443f4f5c94abccd1810d94127d42bbd20` (2021-05-27), branch `master`
- size: 22 files, 7533321 bytes, 10 source files
- license:  ()
- extensions: {'.c': 3, '.xml': 3, '.mk': 3, '<none>': 2, '.h': 2, '.properties': 2, '.py': 1, '.bash': 1, '.exe': 1, '.sln': 1, '.vcxproj': 1, '.filters': 1}
- README head: 
- mechanism evidence:
  - **shader**: xemitops.c:1: // generated with Xenos shader emitter generator by GliGli 0.06 (C:\x360dev\libxemit\gen\xemitgen.py) | gen/xemitgen.py:5: SCRIPT_NAME = "Xenos shader emitter generator by GliGli"

### mdx-m3-viewer

- URL: https://github.com/flowtsohg/mdx-m3-viewer
- commit: `2ff0bc00c6363f425016e23d88c0fb2929d3b3cc` (2025-08-27), branch `master`
- size: 555 files, 5187221 bytes, 461 source files
- license: MIT (LICENSE)
- extensions: {'.ts': 413, '.png': 55, '.js': 45, '.html': 12, '.md': 11, '<none>': 4, '.txt': 4, '.json': 3, '.css': 2, '.jpg': 2, '.code-workspace': 1, '.sublime-project': 1}
- README head: **NO LONGER ACTIVELY MAINTAINED.**  mdx-m3-viewer =============  At the core of it, a 3D model viewer for MDX and M3 models used by the games Warcraft 3 and Starcraft 2 respectively.  The viewer part handles the following formats: * MDX (Warcraft 3 model): extensive support, almost everything should work. * M3 (Starcraft 2 model): partial support. * W3M/W3X (Warcraft 3 map): partial support. * BLP1 (Warcraft 3 texture): extensive support, almost everything should work. * TGA (image): extensive support, almost everything should work. * DDS (compressed texture): partial support - DXT1/DXT3/DXT5/RGTC. * PNG/JPG/GIF/WebP: supported by the browser.  There are file parsers that the viewer depends 
- mechanism evidence:
  - **blp**: src/viewer/handlers/w3x/map.ts:179: const texturesExt = this.solverParams.reforged ? '.dds' : '.blp'; | src/parsers/blp/jpg.js:18: // NOTICE: This file was edited to support loading JPEG data stored in BLP files, which use a non-standard RGBA pixel format.
  - **jass**: src/utils/w3x/weu/conversions.ts:141: // Finally, return the whole trigger as Jass. | src/parsers/w3x/wtg/triggerdata.ts:13: * A standard object mapping strings to function signatures and an optional Jass name.
  - **lockstep**: src/viewer/handlers/mdx/modelinstance.ts:263: * Nodes that are determined to not be visible will not be updated, nor will any of their children down the hierarchy. | src/viewer/handlers/mdx/geometryemitterfuncs.ts:166: // Determine where this texture is coming from.
  - **mdx**: src/viewer/handlers/w3x/map.ts:15: import MdxModel from '../mdx/model'; | src/parsers/mdlx/model.ts:35: * Supports loading from and saving to both the binary MDX and text MDL file formats.
  - **mpq**: src/viewer/handlers/w3x/map.ts:88: // MPQ paths have backwards slashes...always? Don't know. | src/parsers/mpq/file.ts:13: * A MPQ file.
  - **opengl**: src/viewer/handlers/w3x/map.ts:63: vertexBuffer: WebGLBuffer | null = null; | src/viewer/handlers/mdx/geometryemitterfuncs.ts:195: viewer.webgl.bindTextureAndWrap(texture, 0, mdxTexture.wrapS, mdxTexture.wrapT);
  - **shader**: src/viewer/handlers/mdx/geometryemitterfuncs.ts:3: import Shader from '../../gl/shader'; | src/viewer/handlers/mdx/handler.ts:8: import Shader from '../../gl/shader';
  - **simulation**: src/viewer/handlers/mdx/modelinstance.ts:304: // If this is a forced update, or this node's local data was updated, or the parent node was updated, do a full world update. | clients/thirdparty/fpsmeter.min.js:29: } "object" === H(a) && a.nodeType === j && (e = a, a = document.body); a || (a = document.body); var b = this, d = I({}, D.defaults, e || {}), f = {}, u = [], h, y, J = 100, w = []
  - **texture**: src/viewer/handlers/w3x/map.ts:14: import Texture from '../../texture'; | src/parsers/blp/jpg.js:18: // NOTICE: This file was edited to support loading JPEG data stored in BLP files, which use a non-standard RGBA pixel format.
  - **touch**: clients/shared/camera.js:21: // Touch modes. | src/viewer/scene.ts:77: * It may stay in suspended state indefinitly until the user interacts with the page, due to browser policies.

### parry

- URL: https://github.com/dimforge/parry
- commit: `b3c3584ce9be9bf6edb821fd7548b817087131ff` (2026-10-06), branch `master`
- size: 501 files, 3507237 bytes, 482 source files
- license:  (LICENSE)
- extensions: {'.rs': 476, '.md': 6, '.toml': 6, '.obj': 6, '.sh': 3, '<none>': 2, '.yml': 2}
- README head: <p align="center">   <img src="https://parry.rs/img/logo_parry.svg" alt="crates.io" height="200px"> </p> <p align="center">     <a href="https://discord.gg/vt9DJSW">         <img src="https://img.shields.io/discord/507548572338880513.svg?logo=discord&colorB=7289DA">     </a>     <a href="https://crates.io/crates/parry2d">          <img src="https://img.shields.io/crates/v/parry2d.svg?style=flat-square" alt="crates.io">     </a>     <a href="https://crates.io/crates/parry3d">          <img src="https://img.shields.io/crates/v/parry3d.svg?style=flat-square" alt="crates.io">     </a> </p>  -----  <p align = "center">     <strong>         <a href="http://docs.rs/parry2d">2D Documentation</a> | <
- mechanism evidence:
  - **lockstep**: src/transformation/voxelization/voxelized_volume.rs:28: /// Controls how voxelization determines which voxels are filled vs empty. | src/transformation/polygon_intersection.rs:13: /// issues when determining geometric relationships between points, lines, and polygons.
  - **renderer**: src/query/default_query_dispatcher.rs:86: /// - Building a custom collision detection pipeline | src/transformation/volume_mesh/isosurface_stuffing.rs:17: /// Everything the cover pipeline hands this is a pure per-item computation, and rayon's
  - **shader**: src/math/vector_ext.rs:21: /// This is a SPIR-V compatible replacement for bracket indexing (`v[i]`),
  - **simulation**: src/transformation/vhacd/vhacd.rs:48: /// for use in collision detection and physics simulation. | src/shape/heightfield3.rs:48: /// or fix contacts normals that could lead to incorrect bumps in physics simulation (especially
  - **touch**: src/query/contact_manifolds/contact_manifold.rs:300: /// Contact prediction allows detecting contacts before shapes actually touch, | src/query/epa/epa3.rs:218: /// // After GJK determines penetration and fills a simplex:

### sdl3-sys-rs

- URL: https://github.com/revmischa/sdl3-sys-rs
- commit: `a04bef22c5c81cc130bbcc1d019b9e1ed0318bf6` (2023-01-23), branch `main`
- size: 1493 files, 82483828 bytes, 1122 source files
- license: MIT (c-sdl3/LICENSE.txt, c-sdl3/Xcode/SDL/pkg-support/resources/License.txt, c-sdl3/src/hidapi/LICENSE-bsd.txt, c-sdl3/src/hidapi/LICENSE-gpl3.txt, c-sdl3/src/hidapi/LICENSE-orig.txt, c-sdl3/src/hidapi/LICENSE.txt)
- extensions: {'.c': 511, '.h': 489, '.bmp': 57, '.cpp': 41, '.m': 39, '.md': 33, '.vcxproj': 33, '.txt': 25, '<none>': 24, '.xml': 22, '.sh': 18, '.png': 18}
- README head: # sdl3-sys-rs  Rust bindings to version 3 of the [SDL library](https://github.com/libsdl-org/SDL).  **Status:** This is just a minimal test of the building/linking process. I'm waiting for a stable version of SDL3 to be released before filling out the crate properly. Until then, an unknown amount of SDL might change at any time, and tracking that closely wouldn't be much use. 
- mechanism evidence:
  - **lockstep**: c-sdl3/include/SDL3/SDL_video.h:159: SDL_ORIENTATION_UNKNOWN,            /**< The display orientation can't be determined */ | c-sdl3/wayland-protocols/xdg-shell.xml:106: Compositors can use this to determine if the client is still
  - **opengl**: c-sdl3/include/SDL3/SDL_opengl_glext.h:12: ** This header is generated from the Khronos OpenGL / OpenGL ES XML | c-sdl3/src/video/khronos/GLES2/gl2ext.h:12: ** This header is generated from the Khronos OpenGL / OpenGL ES XML
  - **renderer**: c-sdl3/src/video/khronos/vulkan/vulkan_handles.hpp:31: struct PipelineCacheHeaderVersionOne; | c-sdl3/src/video/khronos/vulkan/vulkan_core.h:113: VK_DEFINE_NON_DISPATCHABLE_HANDLE(VkPipelineCache)
  - **shader**: c-sdl3/src/render/direct3d12/SDL_shaders_d3d12.c:41: - <FUNC>: the function. This is main for the pixel shader, and one of | c-sdl3/src/render/direct3d12/SDL_render_d3d12.c:85: /* Vertex shader, common values */
  - **simulation**: c-sdl3/src/video/wayland/SDL_waylandevents.c:72: /* Weston uses a ratio of 10 units per scroll tick */ | c-sdl3/src/video/windows/SDL_windowsevents.c:118: static void WIN_SetMessageTick(DWORD tick)
  - **texture**: c-sdl3/src/video/khronos/vulkan/vulkan_format_traits.hpp:170: case VULKAN_HPP_NAMESPACE::Format::eEtc2R8G8B8UnormBlock: return 8; | c-sdl3/src/video/khronos/GLES2/gl2ext.h:197: #ifndef GL_KHR_texture_compression_astc_hdr
  - **touch**: c-sdl3/wayland-protocols/wayland.xml:263: by the server and does not touch the file corresponding to the | c-sdl3/src/video/SDL_blit_N.c:108: * Data Stream Touch...Altivec cache prefetching.
  - **vulkan**: c-sdl3/src/video/khronos/vulkan/vulkan_structs.hpp:6: // This header is generated from the Khronos Vulkan XML API Registry. | c-sdl3/src/video/khronos/vulkan/vulkan_funcs.hpp:6: // This header is generated from the Khronos Vulkan XML API Registry.

### war3-model

- URL: https://github.com/4eb0da/war3-model
- commit: `52d7f421a5505a0434d764294057f2009ce3987b` (2026-04-03), branch `master`
- size: 84 files, 821411 bytes, 63 source files
- license: MIT (LICENSE)
- extensions: {'.ts': 23, '.glsl': 23, '.wgsl': 12, '.md': 6, '<none>': 4, '.css': 4, '.html': 4, '.json': 3, '.js': 2, '.yml': 2, '.png': 1}
- README head: # war3-model TypeScript-based mdl/mdx (Warcraft 3 model formats) converter/renderer  ## Demo * [WebGL/WebGPU model previewer](https://4eb0da.ru/war3-model/dist/docs/preview/preview.html) * [MDL/MDX converter (also json-like structure-previewer)](https://4eb0da.ru/war3-model/dist/docs/convert/convert.html) * [BLP previewer (BLP1 decoder only)](https://4eb0da.ru/war3-model/dist/docs/decodeblp/decodeblp.html) * [Simple model optimizer](https://4eb0da.ru/war3-model/dist/docs/optframes/optframes.html)  ## Usage  * [Usage in Node.js or with bundler (e.g. webpack)](docs/node.md) * [Usage in browser as external script](docs/browser-global.md) * [Usage in browser as ES Module directly](docs/browser-e
- mechanism evidence:
  - **blp**: docs/preview/preview.ts:11: import { decode, getImageData } from '../../blp/decode'; | blp/decode.ts:65: throw new Error('Not a blp image');
  - **mdx**: docs/preview/preview.ts:6: import { parse as parseMDL } from '../../mdl/parse'; | model.ts:255: // Not actually mapped to mdx flags (0: Head, 1: Tail, 2: Both)
  - **opengl**: renderer/modelRenderer.ts:10: import {mat4fromRotationOrigin, getShader, isWebGL2} from './util'; | renderer/particles.ts:11: import vertexShader from './shaders/webgl/particles.vs.glsl?raw';
  - **pathfinding**: renderer/shaders/webgpu/integrateBRDF.wgsl:39: let cosTheta: f32 = sqrt((1.0 - Xi.y) / (1.0 + (a*a - 1.0) * Xi.y)); | renderer/shaders/webgpu/prefilterEnv.wgsl:54: let cosTheta: f32 = sqrt((1.0 - Xi.y) / (1.0 + (a*a - 1.0) * Xi.y));
  - **renderer**: renderer/modelRenderer.ts:230: private gpuPipelines: Record<string, GPURenderPipeline> = {}; | mdx/generate.ts:6: import { LAYER_TEXTURE_ID_MAP } from '../renderer/util';
  - **shader**: renderer/modelRenderer.ts:15: import vertexShaderHardwareSkinningSource from './shaders/webgl/sdHardwareSkinning.vs.glsl?raw'; | mdx/generate.ts:331: (model.Version >= 900 && model.Version < 1100 ? 80 : 0) /* Shader */ +
  - **simulation**: docs/preview/preview.ts:308: function tick(timestamp: number) {
  - **texture**: package-lock.json:20: "dds-parser": "1.0.1", | renderer/modelRenderer.ts:4: import type {DdsInfo} from 'dds-parser';

### wgpu

- URL: https://github.com/gfx-rs/wgpu
- commit: `c92258fcae03572ced056fa9162370d3ebcf8143` (2026-11-11), branch `trunk`
- size: 2710 files, 38273680 bytes, 1959 source files
- license: MIT (LICENSE.APACHE, LICENSE.MIT, .github/actions/ai-commit-check/LICENSE.MIT, deno_webgpu/LICENSE.md)
- extensions: {'.rs': 877, '.wgsl': 466, '.glsl': 253, '.toml': 244, '.spvasm': 175, '.ron': 159, '.metal': 136, '.hlsl': 108, '.md': 71, '.frag': 41, '.png': 37, '.mit': 19}
- README head: # wgpu  <img align="right" width="20%" src="logo.png">  [![Build Status](https://img.shields.io/github/actions/workflow/status/gfx-rs/wgpu/ci.yml?branch=trunk&logo=github&label=CI)](https://github.com/gfx-rs/wgpu/actions) [![codecov.io](https://img.shields.io/codecov/c/github/gfx-rs/wgpu?logo=codecov&logoColor=fff&label=codecov&token=84qJTesmeS)](https://codecov.io/gh/gfx-rs/wgpu)  `wgpu` is a cross-platform, safe, pure-Rust graphics API. It runs natively on Vulkan, Metal, D3D12, and OpenGL; and on top of WebGL2 and WebGPU on wasm.  The API is based on the [WebGPU standard][webgpu], but is a fully native Rust library. It serves as the core of the WebGPU integration in Firefox, Servo, and Den
- mechanism evidence:
  - **lockstep**: naga/src/back/msl/writer.rs:75: /// The `sizes` slice determines whether this function writes a | wgpu-core/src/device/resource.rs:268: /// determine if it can be dropped. If our internal accesses of devices were
  - **opengl**: naga/src/back/glsl/writer.rs:204: if self.entry_point.stage == ShaderStage::Vertex && self.options.version.is_webgl() { | tests/tests/wgpu-gpu/zero_init.rs:107: .parameters(TestParameters::default().expect_fail(FailureCase::webgl2()))
  - **renderer**: naga/src/back/msl/writer.rs:17: ray::RT_NAMESPACE, sampler as sm, Error, LocationMode, Options, PipelineOptions, | wgpu-core/src/device/resource.rs:48: pipeline::{
  - **shader**: wgpu-core/src/device/resource.rs:159: /// This is the value that should be returned by size queries in shader | naga/src/front/wgsl/lower/mod.rs:10: use crate::front::wgsl::error::{Error, ExpectedToken, InvalidAssignmentType};
  - **texture**: naga/src/back/msl/writer.rs:57: /// For some reason, Metal does not let you have `metal::texture<..>*` as a buffer argument. | wgpu-core/src/device/resource.rs:54: QuerySet, QuerySetState, RawResourceAccess, ResourceState, Sampler, StagingBuffer, Texture,
  - **touch**: wgpu-core/src/resource.rs:363: #[error("Buffer map is pending")] | wgpu-hal/src/lib.rs:171: //! separately for each backend is infeasible; effort would be better spent
  - **vulkan**: wgpu-hal/src/vulkan/adapter.rs:7: use crate::{vulkan::semaphore_list::SemaphoreList, AllocationSizes}; | wgpu-hal/src/vulkan/device.rs:35: /// [`vkSetDebugUtilsObjectName`]: https://registry.khronos.org/vulkan/specs/latest/man/html/vkSetDebugUtilsObjectNameEXT.html


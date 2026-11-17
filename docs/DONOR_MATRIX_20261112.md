# DONOR MATRIX — Herofolends-RS (2026-11-12)

Five external references were requested for functional extraction. All five were
cloned shallow (`--depth 1`) to
`/data/data/com.termux/files/usr/tmp/donors-herofolends/` and inspected file by
file. This matrix records what exists, what was adopted, what was rejected, and
where the adopted mechanism lives in our tree.

**Law note:** WargusHD-android (Stratagus, GPL-2.0) and pc_decomp_homm5 (GPL-3.0)
were read as *design references only*. No source text was copied into this
MIT/Apache-2.0 project. BOTWRecompAndroid and d2r-mobile are MIT; their
mechanisms were also reimplemented in Rust rather than copied. Every adopted item
below is an algorithm/UX decision, implemented independently.

## Inventory (observed)

| donor | size | files | dominant languages | what it is |
|---|---|---|---|---|
| WargusHD-android | 20 MB | 292 cpp, 182 h, 84 java, 41 glsl | C++/Java | Stratagus/Warcraft II engine on Android, GPL-2.0 |
| StarDroid | 37 MB | 492 json, 171 py, 127 java, 65 cpp | Java/C++/JS/WASM | StarCraft + Brood War Android port (OpenBW), touch-first |
| BOTWRecompAndroid | 46 MB | 78 h, 77 c, 52 cpp, 33 py | C/C++/Java | Switch game recompilation with NVN→Vulkan, Android-only |
| pc_decomp_homm5 | 15 MB | 259 py, 120 c, 61 h, 32 cpp | Python/C | HoMM5 decompilation + Android D3D9→DXVK host |
| d2r-mobile | 47 MB | 352 cs, 349 h, 226 cpp, 203 c | C/C++/C# | Diablo II Resurrected mobile port, custom touch layer, MIT |

## Adopted mechanisms (ported)

| # | donor evidence | mechanism | our implementation | verification |
|---|---|---|---|---|
| 1 | `StarDroid/app/src/main/java/org/openbw/androidtest/NativeSurface.java:3463-3760` | touch state machine: button / command / mini / tap / box / aim / place / pinch, release only inside the pressed control | `wc3-game/src/game.rs` `Gesture`, `pointer_down/move/up` | `cargo test -p wc3-game` + on-device taps |
| 2 | `StarDroid/.../ControlSettings.java:78-80` | drag shorter than `holdMs` pans, longer becomes box select | same: `Gesture::Pan` vs `Gesture::Box` after 0.30 s | device drag/tap |
| 3 | `StarDroid/.../NativeSurface.java:3736-3745` | double-tap selects all units of the same type | `Game::tap_action` double-tap branch → `select_same_kind` | unit test + device |
| 4 | `StarDroid/.../NativeSurface.java:3521-3550` | two-finger pinch zooms around midpoint, fingers also pan | `pinch_begin/update/end` + `camera.zoom_by` | device pinch |
| 5 | `StarDroid/.../NativeSurface.java:3867-3878` | minimap tap/drag moves the camera, long press issues an order | `Gesture::Mini`, 0.45 s → `Command::Move` | device |
| 6 | `StarDroid/.../CameraPan.java:12-40` | sub-pixel pan accumulation so slow drags do not jitter | `Camera::pan_pixels` `pan_residual` | unit test `camera` |
| 7 | `WargusHD-android/src/include/touch_drag.h:50-75`, `src/video/sdl.cpp:1193-1220` | drag starts only after drift; the press is synthesised at the landing point so selection begins under the finger | `pointer_move`: 12 px threshold, origin kept in `Pointer::down`; box uses the origin | device drag |
| 8 | `WargusHD-android/src/include/selection_cells.h:50-85` | selection cells keep a unit's place while it remains selected | `keep_selection_cells` in `game.rs` | unit test `selection_cells_keep_positions` |
| 9 | `d2r-mobile/dm/android/touch_android.h:1-30` | stable touch slots, dp coordinates, game-view rectangle for controls | one pointer state + HUD rects computed from the real viewport (`HudLayout::new`) | device |
| 10 | `BOTWRecompAndroid/runtime/hle/framepace.c:1-40` | fixed simulation step with clamped catch-up, present paced by the display | `Game::update` accumulator, max 5 steps/frame, `dt` clamped to 0.1 s; Fifo present | `GAME_STATE` FPS log on device |
| 11 | `StarDroid/.../NativeHud.java:8,37,211-216,593` | command card grid + minimap + slot hotkeys; HUD laid out relative to the world viewport | `HudLayout`, 6×2 command grid, minimap at bottom-left, hotkey line in the label | screenshots |
| 12 | `StarDroid/.../NativeSurface.java:3700` (production cancel) | tapping a queued production slot cancels it | `Command::CancelTrain` + refund (sim + test) | `cancelling_production_refunds` |
| 13 | `pc_decomp_homm5/android/host/input.c:1-30` | latch a press until the next poll so a short tap is never lost | SDL event loop drains every queued event each frame before `update` | device |

## Rejected / out of scope (honest)

| donor evidence | why not |
|---|---|
| `WargusHD` full Stratagus data/Lua layer | we have our own deterministic Rust sim and JASS subset; merging a GPL engine is a license and architecture conflict |
| `StarDroid` OpenBW WASM/JS bridge | different engine and asset pipeline; only the touch UX transfers |
| `BOTWRecompAndroid` NVN→Vulkan translator, guest VFR patching | bound to one retail binary's addresses; no counterpart in an original RTS |
| `pc_decomp_homm5` D3D9→DXVK host, guest memory | Windows-ABI emulation layer; our renderer is native wgpu |
| `d2r-mobile` virtual stick / 12-slot skill bar | action-RPG control scheme, not RTS; only the touch-slot discipline was adopted |
| All donors' copyrighted game assets | never copied; the APK contains no Blizzard/other retail art |

## Consequences across layers (H2 donor gate)

- **Code:** new `wc3-game` crate (assets, camera, game, hud, render) and extended
  `wc3-sim` commands (`MoveAttack`, `Hold`, `Harvest`, `CancelTrain`).
- **Tests:** `wc3-sim` 9/9 (4 new gameplay tests), `wc3-game` 6/6.
- **Docs:** this matrix; `docs/ENGINE.md` game-layer section.
- **Evidence:** H2 capture with clone paths, file:line table and test logs.
- **Memory:** brain `Patterns.md` entry with the adopted touch decisions.
- **No bypass:** every donor was cloned and read; no README-only claims.

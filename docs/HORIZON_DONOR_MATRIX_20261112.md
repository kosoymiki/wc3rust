# HORIZON DONOR MATRIX — full intake (2026-11-12)

Donor: `https://github.com/horizon-rust/horizon` (MIT).
Clone: `git clone --depth 1 --single-branch`, commit
`eb084c515286ea0599ddaa968da9fdc1345f4452` (2026-11-12), 193 MB.
License: MIT, Copyright (c) 2026 Ludovic Marie and the Horizon contributors
(notice in `third_party/horizon/NOTICE`).

This is a full Minecraft Java 26.3 client rewritten in Rust: 49 workspace
packages (plus 3 excluded helper crates), ~1.28 M lines of Rust in the client
tree alone. It is the closest architectural sibling to our own Rust/wgpu client,
so every crate and every top-level module family was inspected and classified.
It is **not** a source of Warcraft formats; the value is client architecture,
texture/mip handling, touch layout and renderer discipline.

Method (all observed):
1. Shallow clone + `cargo metadata --no-deps` (49 packages, dependency counts).
2. Per-crate file/line inventory (`find`, `wc`); per-module inventory for
   `horizon-client-render` (148 files in `gfx/`, 70 383 lines).
3. Targeted source reads of the mechanisms below (raw file reads, not shell
   summaries), with file:line anchors.
4. Triage: PORT (mechanism reimplemented here), CROSS (design adopted),
   REFERENCE (read, no code), OOS (Minecraft/desktop-specific).
5. Every adoption has a test and a named verification; no donor code was
   executed.

## 1. Crate matrix (all 49 packages)

| crate | responsibility (from its manifest) | decision |
|---|---|---|
| horizon-client-render | world data, renderer, screens, meshing, HUD, touch | **PORT/CROSS** |
| horizon-client | native client orchestration and gameplay screens | CROSS (architecture) |
| horizon-client-lib | reusable client systems, control tool, parity probes | REFERENCE |
| horizon-client-core | text/translation, NBT, scoreboard, boss bars | REFERENCE |
| horizon-client-input | keybinds, mouse, controllers, clipboard | CROSS (input arbitration) |
| horizon-assets | model/resource decoding and baking | REFERENCE (bake/transform ideas) |
| horizon-core | simulation/worldgen math parity | OOS |
| horizon-world | NBT, regions, chunk storage | OOS (WC3 maps) |
| horizon-protocol | Minecraft 26.3 network protocol | OOS |
| horizon-command | Brigadier command tree | OOS |
| horizon-items | blocks/items/components | OOS |
| horizon-server | dedicated server supervisor | OOS |
| horizon-server-files | server.properties/whitelist/bans | OOS |
| horizon-shaders / shader-api | shader pack format and WGSL stages | REFERENCE |
| horizon-optifine / polytone | OptiFine/Polytone pack formats | OOS |
| horizon-theme | design tokens, fonts, icons | REFERENCE |
| horizon-threads | worker count, parallel chunks, allocator hints | CROSS |
| horizon-system | OS/CPU/GPU/memory facts | REFERENCE |
| horizon-hdr | HDR swapchain metadata | OOS (SDR game) |
| horizon-platform | public API of the platform layer | REFERENCE |
| horizon-hooks | extension points/registries | REFERENCE |
| horizon-game | platform entry (35 lines) | REFERENCE |
| horizon-official / official-servers | official executable/server list | OOS |
| horizon-mods / mod-api-gen / mod-deps / mod-registry | mod system | OOS |
| horizon-port-pack / prepare | modpack import, asset preparation | REFERENCE (prepare/validate) |
| horizon-cosmetics | cosmetic item rendering | OOS |
| horizon-discord / friends / relay / heartbeat | social/telemetry | OOS |
| horizon-dualsense / narrator | HID/TTS | REFERENCE (accessibility later) |
| horizon-bench / telemetry / telemetry-server | benchmarks/telemetry | REFERENCE (bench method) |
| horizon-anticheat / chat-filter / bots | server-side checks | OOS |
| horizon-mcp / addresses / release-sign | tooling/endpoints/signing | OOS |
| cargo-horizon-mod / xtask | build tooling | OOS |

## 2. Renderer module families (`horizon-client-render/src`)

| family | files/lines | what it is | decision |
|---|---:|---|---|
| `gfx/` | 148 / 70 383 | Vulkan/wgpu pipelines, passes, atlas, culling, frame policy | **PORT mipmap; REFERENCE culling** |
| `mesher/` | 12 / 5 143 | section meshing, face culling, light/AO, fluid | REFERENCE |
| `particles/` | 7 / 4 088 | particle pipelines | REFERENCE |
| `lod/` | 6 / 3 520 | distant terrain LOD merging | REFERENCE |
| `hud/`, `hud_kit/` | 16 / 4 660 | HUD, widgets, fit/hint/motion/plate | REFERENCE |
| `touch/` | 4 / 1 420 | normalized touch layout + aim | **CROSS/PORT layout math** |
| `models/`, `chunk/`, `streaming/` | 8 / 2 427 | model catalog, sections, streaming | REFERENCE |
| `options_ui/`, `settings_shell/`, `social/` | 5 / 733 | screens | OOS |
| `android_runtime.rs`, `android_gamepad.rs` | 2 / 1 421 | Android input arbitration | **CROSS** |
| `core_shader/`, `pack_formats/`, `packs/`, `trims/` | 9 / 2 969 | pack/shader plumbing | OOS |

## 3. Mechanisms read with anchors

| mechanism | donor evidence | what it does |
|---|---|---|
| linear-light mip chain | `crates/horizon-client-render/src/mipmap.rs:243-278` (`mean_linear`), `:280` (`generate`) | averages RGB through sRGB↔linear tables, alpha linearly; keeps cutout coverage |
| atlas stitching/padding | `gfx/pack_atlas/stitch.rs:1-70` | `Stitcher` regions, power-of-two, padded holders, mip-aligned `fit` |
| touch placement math | `touch/layout.rs:105-145` | normalized centre, height-relative size, aspect-correct square `rect()`, `sanitize()` |
| touch gesture/aim | `touch/aim.rs:1-40` | camera turn, tap ray, hotbar slot, tap-vs-hold item rules |
| input arbitration | `android_runtime.rs:22-70` | one active device (touch/gamepad/keypad), clearing held inputs on switch |
| frame loop idle policy | `horizon-client/src/game/event_loop.rs:14-21` | `ControlFlow::Poll` only after a frame; otherwise wait ~16 ms |
| GPU culling | `gfx/gpu_culling.rs:1-60` | compute pass, indirect multi-draw, frustum+occlusion, readback stats |
| section meshing | `mesher.rs:1-45` | packed vertex (pos/uv/layer/color/light/normal), face and AO rules |
| LOD merging | `lod/mesh.rs:1-40` | exposed voxel-column faces merged without changing UVs |
| frame format/sRGB | `gfx/frame_policy.rs:13-27` | choose sRGB/linear/HDR frame, never double-tonemap |
| model baking | `horizon-assets/src/bake.rs:1-30` | cuboid geometry, blockstate rotation, UV locking |
| worker sizing | `horizon-threads/src/lib.rs:20-50` | one `available_parallelism`, `HORIZON_WORKER_THREADS`, serial reference mode |
| theme tokens | `horizon-theme/src/lib.rs:1-40` | one source for launcher/site/game tokens and bundled fonts |

## 4. Ported / cross-adopted in Herofolends-RS

| # | donor mechanism | our implementation | verification |
|---|---|---|---|
| 1 | `mipmap.rs` `mean_linear`/`generate` | `crates/wc3-game/src/mipmap.rs` (full chain, sRGB tables, linear-light colour, linear alpha), used by `render.rs` texture upload with `mip_level_count` and `MipmapFilterMode::Linear` | `cargo test -p wc3-game mipmap` (solid colour, black/white linear average, alpha) — 3/3 pass; device FPS/quality screenshot |
| 2 | touch placement (normalized, height-relative, aspect-correct) | `HudLayout`/`Rect` already size from viewport height; adopted the principle for 6×2 RTS buttons, minimap and panels; no layout editor (not needed for six commands) | device self-test taps the real button centres |
| 3 | input arbitration (one active device, clear held state) | app tracks touch vs mouse; SDL touch-mouse ID ignored; touch updates `last_touch`; mouse down/move/up suppressed for 700 ms after touch | `scripts/game-selftest.sh` (tap-driven) |
| 4 | idle frame policy (poll only after a frame, short sleep otherwise) | surface `Timeout`/`Occluded` sleeps 2 ms instead of busy-spinning | device FPS log |
| 5 | atlas padding/mip safety | our terrain atlas uses a 1-texel inset and no mips; unit textures get per-texture mips (no cross-atlas bleed). The full stitcher is **REFERENCE**, not copied | asset test + visual test |
| 6 | `ab_glyph` text atlas | already used independently; Horizon confirmed glyph atlas + UV layout. Our font bug (atlas coordinate used as bearing) is fixed and locked by HUD text on screenshots | visual HUD metrics |
| 7 | fixed-step clamp | our `Game::update` accumulator with max 5 steps and `dt<=0.1` matches the same discipline; Horizon’s guest-specific VFR is OOS | sim tick progression in self-test |

## 5. Deliberately not ported

| item | reason |
|---|---|
| full chunk/section renderer, GPU culling, LOD | our maps are a single 128×128 terrain mesh; the cost/benefit is negative at this scale. Revisit when maps >512² or multiple maps stream. |
| texture-array block atlas | our 8 ground tiles + 20 unit textures fit current bind groups; the array design is recorded for the next renderer revision. |
| Minecraft world/NBT/registry/protocol/commands/mods | different game and data formats. |
| HDR frame policy | the Android target is SDR sRGB; our blend/output path is already sRGB-correct. |
| winit desktop loop | Android SDL3 is the product target; the event-loop idle rule was adopted, not winit. |
| vendored `wgpu-hal` patches | Horizon patches Intel/Metal issues; our Adreno Vulkan path uses stock wgpu 30.0.1 and works. |
| theme fonts/assets | SIL OFL fonts are not needed; the system font + fallback is sufficient and smaller. |

## 6. H2 consequences

- **Code:** new `wc3-game/src/mipmap.rs`; texture upload now builds full mip chains;
  visual/asset/gameplay integration tests; device self-test scripts.
- **Tests:** `wc3-game` lib 9/9, `tests/assets` 1/1, `tests/gameplay` 4/4,
  `tests/visual` 1/1 (real map render: green 0.371, black 0.160, white 0.000,
  colours 231), device selftest scripts.
- **Docs:** this matrix; `docs/ENGINE.md` game layer; `docs/DONOR_MATRIX_20261112.md`
  for the other five donors.
- **License:** MIT notice added; no GPL code copied. Mipmap algorithm is a
  reimplementation with attribution, not a source copy.
- **Evidence:** clone path/commit, `cargo metadata` 49 packages, module LOC table,
  file:line anchors, test logs, screenshots.
- **No bypass:** every crate classified; key modules read; no README-only claims.

# TOOLING + FINAL PRODUCT (P0 world) — ledger 348

**Date:** 2026-11-13 22:21:02 MSK · session ses_e30595423ffdGkcohq9P8MIxiL · agent-world
**Device slot:** `H2_DEVICE_SLOT` unset → no Android app launch this round (device gate respected).

## 1. Toolchain audit (command -v / --version)
| tool | path | version |
|---|---|---|
| python3 | /data/data/com.termux/files/usr/bin/python3 | 3.14.10 |
| pip3 | …/pip3 | 26.3 |
| node | …/node | v24.22.2 |
| npm | …/npm | 12.2.2 |
| bun | …/bun | 1.4.4 |
| cargo / rustc | …/cargo, …/rustc | 1.99.1 |
| git | …/git | 2.56.0 |
| jq | …/jq | 1.8.2 |
| sqlite3 | …/sqlite3 | 3.54.1 |
| ffmpeg | …/ffmpeg | 9.0.2 |
| sox | …/sox | SoX_ng 14.8.1 |
| ImageMagick (`magick`) | …/magick | 7.1.2-35 Q16-HDRI |
| optipng | …/optipng | 7.9.1 |
| pngquant | …/pngquant | 3.0.3 |
| gltf-transform | …/gltf-transform | 4.5.1 (`@gltf-transform/cli`) |
| three | global npm | 0.187.0 |
| trimesh (py) | site-packages | 5.2.1 |
| pygltflib (py) | site-packages | 1.16.5 |
| radare2 | …/r2 | 6.2.6 (abi 174, aarch64) |
| 7z | …/7z | 17.05 |
| chromium | …/chromium-browser | present (GPU process crashes headless: exit_code=11) |
| wc3re storm | target/release/wc3re | StormLib-compatible extractor (Human01.w3m sha256 match) |

**Gaps (recorded, with mitigation):**
- `assimp` — no Termux package (`pkg install -y assimp` → rc=1, 0 new packages). Mitigation: `trimesh` + `pygltflib` + `gltf-transform validate` cover import/validation; our own `mdx-gltf` exporter replaces the missing MDX reader.
- `stormlib` CLI — no package. Mitigation: `wc3re storm/extract` proven (extracted `Maps\Campaign\Human01.w3m` from War3.mpq, sha256 `28c05722…` == extracted tree).
- Headless WebGL — Chromium GPU process exits 11 on this device; three.js viewer page exists but cannot be screenshotted headlessly. Mitigation: wgpu Vulkan render (`melee-scene.png`) + numpy software rasterizer (`gltf_preview.py`).
- `blender` absent (not needed; no procedural final art used).

## 2. H2 base used (IDs/paths)
- `harness2_corpus_pack` ok (ae_corpus_discipline+evidence), `harness2_corpus_search` → item **34084** (Blender 3D lesson; checked, not blindly applied).
- `harness2_experience` → **4421** (WC3 `.mdl`→`.mdx` resolution, meshless missile MDX, adapter logging) reused for the MDX path work.
- Skills present: `~/.config/opencode/skills/{stealth-web-h2,deep-donor,engineering-reflection,fullstack-primer}`; `stealth_browser_doctor` ok (chromium found, Termux args correct), but spawn/navigate has an instance-registry mismatch (`list_instances` empty) — recorded, not worked around with blind retries.
- Ledger heartbeat/checkpoints: `harness2_monitor` id **348**.

## 3. Tools delivered/adapted (official deps only)
- `tools/mdx-gltf` (new workspace bin, Rust): MDX → glTF 2.0 (Z-up→Y-up `x,z,-y`, POSITION/NORMAL/TEXCOORD_0/indices, materials, BLP→PNG textures, `--root` case-insensitive resolver). Build rc=0.
  - `gltf-transform validate TownHall.gltf` → **rc=0** “No hints found”; `GreatHall.gltf` → **rc=0**.
  - `trimesh.load` → TownHall 36 geoms/6388 verts bounds `[-457.23,-227.26,-746.35]..[228.33,643.74,431.4]`; GreatHall 19 geoms/3654 verts `[-327.0,-325.59,-588.23]..[259.97,676.3,524.99]`.
- `tools/gltf_preview.py` (numpy z-buffer + texture sampling + ground line at y=0) → `gltf/viewer-TownHall.png`, `gltf/viewer-GreatHall.png`.
- `analysis/agent-world/viewer/index.html` — three.js GLTFLoader/OrbitControls viewer (runs where WebGL exists; blocked headlessly here).
- Existing tools validated: `ffmpeg` PPM→PNG, ImageMagick/optipng/pngquant available for final texture polish, `sox` for audio, `r2` for RE.

## 4. P0 world fixes (root causes)
**(a) Units/buildings sink into terrain — model pivot added.**
- `crates/wc3-game/src/assets.rs:160` `ModelAsset.pivot`; computed from the visible Stand geosets at `:648` (`visible_min`), scaled at `:669`; fallback `:1363`.
- `crates/wc3-game/src/game.rs:2379` scene base = `height_at(x,y) - model.pivot` (previously just `height_at`).
- Measured: TownHall/GreatHall authored base extends to **-156.4** below origin → halls previously sat 156 units inside the ground. Now ground 187.9/445.0 → base_y 344.3/601.4 (`MELEE_PLACEMENT` in `melee-scene.log`).

**(b) Spawn walkability + pathing around obstacles.**
- `game.rs:810` `find_spot_footprint` — halls require their whole footprint on walkable cells (not just the centre).
- `game.rs:622` `spawn_melee_scenario` — every unit is spawned through the walkable-snapped sim path; buildings block their footprint (`wc3-sim` set_walkable).
- `crates/wc3-game/src/sim_bridge.rs:178` `spawn_map_destructables` — the real map's `war3map.doo` trees are spawned first, so their cells block pathing before bases are chosen.
- `crates/wc3-sim/src/lib.rs:1716/1784` stuck handling: blocked movement now finishes the order after 30 ticks + one retry (was 150 ticks + two retries ≈ 30 s of “running in place”).

**(c) Real 1v1 melee scenario (no Human04 3-hall campaign script).**
- `game.rs:607` `MELEE_MAP_STEM = "Orc01"`; `:269` melee detection skips `attach_map_runtime` (script not booted); `:622` spawns **player Orc** (Great Hall `ogre`, 2 peons) vs **Human** (Town Hall `htow`, 2 peasants), **one gold mine + five neutral creeps per side** (10 creeps total), real map terrain/pathing/trees on `Maps/Campaign/Orc01.w3m`.
- `apps/android-native/src/lib.rs:889` app now prefers `Orc01.w3m`; map copied to `/data/data/org.herofolends.p0/files/Orc01.w3m` (311106 B, no launch — slot unset).

## 5. Gates (rc + evidence)
- `cargo test -p wc3-game --lib map_units_spawn --offline -- --nocapture` → **rc=0**, 2 passed (`analysis/agent-world/p0-world.log`). Asserts: `melee=true`, no engine, exactly **2 main halls**, **2 gold mines**, **10 creeps**, every non-building on a walkable cell, no hall walled in by blocked cells, finite terrain height, melee scenario logged.
- `cargo check -p wc3-game --lib --offline` → **rc=0** (`p0-check.log`).
- `cargo test -p wc3-game --lib melee_scene_render --offline -- --ignored --nocapture` → **rc=0**: `MELEE_SCENE_RENDER halls=2 units=16 non_bg=896500`, adapter `Turnip Adreno (TM) 725` Vulkan (`melee-scene.log`, `gltf/melee-scene.png` 1280x720, 36578 sampled colours).

## 6. Artifacts (sha256)
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/TownHall.gltf` — 29237 B — `16ab04656653021083a4acad8ec1de84f395601c86a789f545776194f73a5876`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/TownHall.bin` — 243056 B — `2842599bb916cd89a37ea16e982a744704ede2b8096bae0c4ae9134b59fa2ece`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/GreatHall.gltf` — 16427 B — `ebcc86be39afc78732b5c2c6b03d767e45f18c6c95466bcd6dfb756759d6af76`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/GreatHall.bin` — 149412 B — `19702c44440f2c3b2d2fb66e621daeed950d496a5b9e831cd8655ecde95c46f4`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/viewer-TownHall.png` — 20255 B — `e4e41bd073ddec921ee60040757035c7c4024067c13e3bc67d0c8aa3cc2ba852`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/viewer-GreatHall.png` — 34087 B — `8ecb30a84d0f8fd7b15c6b4feb66adf5564df1b5f87d3762b111d493dd8df9f9`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/gltf/melee-scene.png` — 1937142 B — `17c7e78745e5c6905b0af943cfdeb36a2f94f94fbee8e336806275f2f85f25e1`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/p0-world.log` — 18972 B — `cd3519bf62462d6ecfe5cb3e3d980ec7f3c0438a6ac1ffd65d375663ab3b451f`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/p0-check.log` — 206 B — `a97dbe88c071536f375de270084147c321046d03d089096b055d9ad9e57dd4c9`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/melee-scene.log` — 21222 B — `90def638aba338b2e325b3c10965cf2e47d3056944150524bf6b1d88d0ede2b7`
- `/data/data/com.termux/files/home/src/herofolends-rs/tools/mdx-gltf/src/main.rs` — 13477 B — `e3676695804ade028798781cbf049406218db802f5134e579e6b30d140b6237c`
- `/data/data/com.termux/files/home/src/herofolends-rs/tools/gltf_preview.py` — 6490 B — `efebef93a61ec74d64730910c0608a2332dfe99bd6dc7ba74ec60003eb685782`

## 7. Result
**DONE (tooling + P0 product) rc=0.** Toolchain audited and gaps recorded; the 3D chain is real (MDX→glTF exporter + validators + software preview + wgpu scene render) without assimp/stormlib CLI; units/halls now use the authored model pivot on `height_at`, spawns and paths are walkable/obstacle-aware, and the scenario is a real Orc-vs-Human melee arena with exactly two halls, two mines and ten creeps.

## 8. Risks / Next
- Both halls still render the shared `UnitKind::TownHall` model (Assets holds 4 canonical models); per-definition wiring (`ModelRegistry` → `Assets`) remains the next increment (ledger 348 tail `assets_models_min`).
- Device launch + screenshot pending `H2_DEVICE_SLOT=1`; the map is already staged in the app files dir.
- glTF preview is geometry-shaded (trimesh PBR texture sampling not wired); textures are exported as PNG in the glTF and validated by gltf-transform.
- `assimp`/`stormlib` CLI absent on Termux; `wc3re storm` + `trimesh`/`gltf-transform` are the working substitutes.

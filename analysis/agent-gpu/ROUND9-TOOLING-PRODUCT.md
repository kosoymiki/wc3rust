# ROUND 9 — TOOLING + FINAL PRODUCT (ledger 352, GPU, 2026-11-13)

## 1. Toolchain audit (observed: `command -v` + `--version`)

| tool | version | state |
|---|---|---|
| python3 | 3.14.10 | present |
| node / npm / bun | 24.22.2 / 12.2.2 / 1.4.4 | present |
| cargo / rustc | 1.99.1 | present |
| clang / clang++ | 21.1.8 | present |
| cmake / make | 4.4.5 / 4.4.1 | present |
| r2 / radare2 | 6.2.6 | present |
| rtk | 1.1.0 | present |
| ffmpeg / sox | 9.0.2 / SoX_ng 14.8.1 | present |
| ImageMagick / optipng / pngquant | 7.1.2-35 / 7.9.1 / 3.0.3 | present |
| gltf-transform | 4.5.1 | present |
| trimesh / pygltflib | 5.2.1 / 1.16.5 | present (pip) |
| three | 0.187.0 | present (npm global, ESM) |
| SPIRV-Tools (`spirv-val/as/dis`) | v2026.4 | present |
| glslangValidator | 11:16.6.0 | present |
| assimp | libassimp 6.0.6 (`$PREFIX/lib/libassimp.so`) | **CLI absent**; pyassimp 5.2.5 installed and loads via `LD_LIBRARY_PATH=$PREFIX/lib` |
| naga | 30.0.2 (`~/.cargo/bin/naga`) | **installed this round** (`cargo install naga-cli --locked` rc=0) |
| storm/stormlib | n/a | **not packaged**; built from the official repo (see §2) |
| mdxview | n/a | replaced by the new `mdx2gltf` exporter (§3) |

## 2. Install/adapt (official sources, versions + rc)

- `cargo install naga-cli --locked` → rc=0, `naga 30.0.2` (`~/.cargo/bin/naga`). WGSL/SPIR-V validation path closed.
- `pip install pyassimp` → pyassimp **5.2.5**, imports with `LD_LIBRARY_PATH=$PREFIX/lib` against the Termux `libassimp.so` **6.0.6**.
- `gltf-transform` **4.5.1**, `trimesh` **5.2.1**, `pygltflib` **1.16.5**, `three` **0.187.0** already installed and verified.
- StormLib: official repo `github.com/ladislav-zezula/StormLib` cloned (rc=0); CMake 4.4 required
  `-DCMAKE_POLICY_VERSION_MINIMUM=3.5` and bundled tommath/tomcrypt/bzip2 (`libtommath` pkg-config absent).
  `cmake --build build3 -j2` → **rc=0**, `libstorm.so.9.40` (shared, `$PREFIX/tmp/StormLib-r9/build3/libstorm.so.9.40`);
  log `$PREFIX/tmp/StormLib-r9/build3.log`. The product itself keeps using `wc3_re::storm::StormFs`.
- No untrusted donor code was executed; only official package managers/repos were used.

## 3. MDX → glTF tool (visual checks)

`crates/wc3-render/examples/mdx2gltf.rs` (new, ~330 lines, safe code):

```text
cargo run -p wc3-render --release --example mdx2gltf -- analysis/TownHall.mdx out/TownHall.gltf
MDX2GLTF input=analysis/TownHall.mdx ... geosets=36 vertices=6388 triangles=3220 bytes=243056
```

Coordinates: WC3 Z-up → glTF Y-up `(x,y,z) -> (x,z,-y)` (pure rotation, winding preserved). Each geoset becomes a primitive (POSITION/NORMAL/TEXCOORD_0 + u32 indices) with placeholder PBR materials (BLP textures are not referenced).

Validated by four independent consumers (observed):

| consumer | result |
|---|---|
| `gltf-transform inspect` | version 2.0, generator `wc3rust mdx2gltf`, no extensions required |
| `pygltflib` | meshes 1, accessors 144, materials 9, buffers 1 |
| `pyassimp` (libassimp 6.0.6) | meshes 36, vertices 6388 |
| `trimesh` | geometry 36, faces 3220 (matches the exporter) |

## 4. Terrain quality fix (P0 continuation, root)

Files:
- `crates/wc3-render/src/terrain.rs` (**new**): `tile_from_sheet` — picks the variation with the highest alpha coverage, copies it at native resolution (no more averaging all 128px variations), reuses the **authored BLP mip chain** by extracting the same grid cell at each authored level, fills alpha holes with the sheet base, generates only the missing mip tail with wrapped linear-light box filtering, and applies a **2px** wrap-edge blend (was 10px). Unit tests: detail preserved, opaque-variation choice, authored mips reused, full pyramid to 1×1, hole fill, seamless base.
- `crates/wc3-game/src/terrain_render.rs`: `TileSet.mips`; `load_tile_set` builds every layer through `tile_from_sheet` (RE path + assetdb fallback); `upload_tile_array` uploads the authored levels per layer (generating only when absent); `anisotropy_clamp: 4 → 8`; sampler already linear + linear mip + Repeat; array format `Rgba8UnormSrgb` (correct sRGB).
- `crates/wc3-game/tests/terrain_tiles.rs` (**new gate**): parses the real archive. Observed: combined install `rows=177 unique_files=141 tilesets=18 missing=0`; RoC only `98/98/13` (matches PARITY_MATRIX R4 "98 tiles"); every referenced BLP opens.
- `apps/android-native/src/lib.rs`: added `GAME_RENDER_SCALE swapchain=WxH scene=WxH native=<bool>` on frame 1 (native-resolution diagnostic).

Render scale: the app already uses `SDL_GetWindowSizeInPixels` (physical 2414×1080) for `config.width/height`, resizes the game to the same size, and renders into the swapchain view directly (no offscreen downscale). Prior device logs: `GAME_SURFACE 2414x1080`, `GAME_FIRST_FRAME surface=2414x1080`. This round `H2_DEVICE_SLOT` was **unset**, so no device launch/screenshot was made (constraint honoured); the diagnostic is ready for the next granted run.

## 4b. Shader / SPIR-V toolchain evidence

- Extracted the real terrain WGSL from `crates/wc3-game/src/terrain_render.rs` (`TERRAIN_SHADER`, 6 830 B).
- `naga terrain.wgsl terrain.spv` → **rc=0** (module validates, SPIR-V emitted, 53 248 B).
- `spirv-val terrain.spv` → **rc=0**.
- `glslangValidator 11:16.6.0` available for GLSL sources; `spirv-as/dis` for round-trips.

## 5. Gates (rc)

| command | rc | result |
|---|---|---|
| `cargo check -p wc3-render --all-targets` | 0 | compiles (lib, tests, examples incl. mdx2gltf) |
| `cargo test -p wc3-render` | 0 | **76 passed** (53 lib incl. 6 terrain + 4 astc/3 fx/10 fx/3 parity/3 cache) |
| `cargo check -p wc3-game --lib` | 0 | downstream green |
| `cargo test -p wc3-game --test terrain_tiles` | 0 | 1 passed (177 rows / 0 missing; RoC 98/98/13) |
| `cargo check -p herofolends-android` | 0 | app compiles |
| `naga` + `spirv-val` on terrain WGSL | 0 / 0 | shader validates, SPIR-V valid |

**External note (observed, active writer):** at 22:23 `crates/wc3-sim/src/lib.rs` was mid-edit
(duplicate `use wc3_re::gameplay::GameplayConfig` at :27/:42, duplicate `gameplay` field at
:562/:593 and :650/:674), so `cargo test -p wc3-game --lib` currently fails to compile in a
**foreign crate**. Not touched: the file was written by the sim layer seconds before this check;
owner is the sim agent. The GPU-layer terrain edits compiled before that write
(`cargo check -p wc3-game --lib` rc=0) and the dedicated `terrain_tiles` gate passed.

## 6. Gaps / next

- `assimp` CLI is not shipped by the Termux package (only `libassimp.so`); pyassimp covers import. A CLI wrapper can be added later.
- StormLib configure/build result pending (bundled-deps flags); not a product dependency (`wc3-re::storm` is canonical).
- Device screenshot/visual confirmation of the sharper terrain needs a granted run (`H2_DEVICE_SLOT=1`), then `GAME_RENDER_SCALE` + a terrain close-up capture.
- `mesh.rs` resident-set integration and frozen `assets.rs` model count remain outside this round's owner scope.

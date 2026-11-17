# Toolchain audit — final product round (ledger 356, touch)

Date: 2026-11-13 21:56–22:40 MSK. Environment: Termux aarch64, Android 16 (SDK 36), KernelSU root (`su -c 'id'` → uid=0), Python 3.14.10, Node 24.22.2, npm 12.2.2, bun 1.4.4, rustc/cargo 1.99.1, pip 26.3, cmake 4.4.5.
Method: `command -v` + `--version` per tool; every install records rc; every capability has a smoke command. H2 base: corpus pack + `harness2_corpus_search` (IDs 34084, 60124, 60122), `harness2_experience` search (IDs 4342, 4424, 4286).

## Present at audit (observed)

| Tool | Version | Note |
|---|---|---|
| python3 / pip | 3.14.10 / 26.3 | |
| node / npm / bun | 24.22.2 / 12.2.2 / 1.4.4 | |
| rustc / cargo | 1.99.1 / 1.99.1 | |
| cmake / make / git | 4.4.5 / present / present | assimp source build |
| r2 | 6.2.6 | RE |
| rtk | 1.1.0 | command rewrite |
| adb | present (`$PREFIX/bin/adb`) | no device transport this round |
| trimesh (CLI + module) | 5.2.1 | GLB export/load |
| pygltflib | 1.16.5 | module only (no CLI by design) |
| gltf-transform | 4.5.1 | **was broken**, fixed below |
| ImageMagick (`magick`/`convert`/`identify`) | 7.1.2-35 Q16-HDRI | |
| optipng / pngquant | 7.9.1 / 3.0.3 | |
| ffmpeg / ffprobe | 9.0.2 | |
| sox | 14.8.1 (SoX_ng) | |
| StormLib (adapted in-repo) | donors/StormLib + `third_party/stormlib-build2/libstorm.a` (1.03 MB) + `tools/storm_probe` (NDK r30) | |

## Gaps found and closed

### 1. assimp — missing
`command -v assimp` → MISSING at 21:56. Built from the official repo, minimal importer set (glTF/glTF2/OBJ/FBX), static lib:

```text
git clone --depth 1 https://github.com/assimp/assimp        # commit d355998, 2026-11-13
cmake -DCMAKE_BUILD_TYPE=Release -DASSIMP_BUILD_TESTS=OFF -DASSIMP_BUILD_SAMPLES=OFF \
      -DASSIMP_BUILD_ASSIMP_TOOLS=ON -DASSIMP_BUILD_ALL_IMPORTERS_BY_DEFAULT=OFF \
      -DASSIMP_BUILD_GLTF_IMPORTER=ON -DASSIMP_BUILD_GLTF2_IMPORTER=ON \
      -DASSIMP_BUILD_OBJ_IMPORTER=ON -DASSIMP_BUILD_FBX_IMPORTER=ON -DBUILD_SHARED_LIBS=OFF
cmake --build build -j2 && cmake --install build --prefix "$PREFIX"
```

`CONFIG_RC=0`, `BUILD_RC=0`, `INSTALL_RC=0`; `$PREFIX/bin/assimp` 3.71 MB (22:11), `$PREFIX/lib/libassimp.a` 11.4 MB; `assimp version` → **6.0-st (GIT d3559989)**; `assimp info cube.glb` → rc 0 (1 mesh, 1 material).
Note: the Termux DB also reports `assimp 6.0.6` (a package installed by another agent during the round; `$PREFIX/lib/libassimp.so` is dated Nov 10). The binary on PATH is this verified source build; `pkg install --reinstall assimp` restores the distro binary if the package is preferred.

### 2. gltf-transform — installed but aborts on Termux
Exact symptom: `Error: Could not load the "sharp" module using the android-arm64 runtime`. Fix (the sharp error itself lists the WebAssembly option):

```text
npm install -g --cpu=wasm32 sharp @img/sharp-wasm32      # rc 0
gltf-transform --version                                  # 4.5.1, rc 0
gltf-transform inspect cube.glb                           # rc 0
```

### 3. three — missing
`npm install -g three` → rc 0, **0.187.0**.

### 4. StormLib — already adapted, now verified
`./tools/storm_probe /data/data/org.herofolends.p0/files/War3.mpq --limit 6` → `SUMMARY listed=10525 extracted=0`, rc 0 (read-only list of the operator's own archive). Experience 4286 documents this probe as an independent oracle for extraction audits.

### 5. pygltflib — module present, no CLI
`pygltflib.__version__` = 1.16.5; loaded the smoke GLB (1 mesh, 2 accessors). No CLI exists upstream; validation is Python-side (documented, not a gap to "fix").

## End-to-end 3D smoke (rc 0)

```text
trimesh 5.2.1  -> cube.glb (8 verts, 12 faces)        rc 0
pygltflib      -> load cube.glb (1 mesh, 2 accessors) rc 0
gltf-transform -> inspect cube.glb (glTF 2.0)         rc 0
assimp 6.0-st  -> info cube.glb (1 mesh, 1 material)  rc 0
```

## Remaining gaps (named owner + next step)

1. **MDX → glTF exporter absent** (owner: formats/render). `crates/wc3-formats/src/mdx.rs` is the one MDX reader; a bin there (or in wc3-re) should emit `.glb`, then validate with the verified pipeline above. Visual checks today use the PNG galleries (`analysis/agent-anim/gallery/`), not a second MDX parser in the touch layer.
2. **r2 MPQ plugin absent** — StormLib/`storm_probe` covers MPQ; r2 remains the binary RE tool.
3. **sharp on Termux** requires the wasm fallback (`--cpu=wasm32`), otherwise every `gltf-transform` run aborts at import time.

## H2 base usage this round

- Corpus: `harness2_corpus_pack` (task = final product round); `harness2_corpus_search` IDs **34084** (Blender 3D book), **60124** (Herofolends APK is a launcher; ISO/MPQ is the clean data path), **60122** (W3AndroidEngine touch translator: router→recognizer→RTS adapter — the pipeline `wc3-input` implements).
- Experience: **4342** (one reader/one byte path), **4424** (platform must forward events — the round-8 keyboard lesson), **4286** (`storm_probe` as independent MPQ oracle — used above).
- Stealth-browser docs attempt: `spawn_browser` ok (instance `ee22757c…`), `navigate`/`get_page_content` returned `null`/empty in both single and batch modes → docs fetch **observed/unavailable**; the sharp fix is grounded in the local error text (which lists `npm install sharp @img/sharp-wasm32`) and verified by rc 0, not by a fetched page.

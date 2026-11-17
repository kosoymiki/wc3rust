# Tooling audit — QA layer (2026-11-13)

Generated 2026-11-13T19:23:10Z from `command -v` + version probes.

| Tool | Path | Version |
|---|---|---|
| `python3` | `/data/data/com.termux/files/usr/bin/python3` | Python 3.14.10 |
| `pip` | `/data/data/com.termux/files/usr/bin/pip` | pip 26.3 from /data/data/com.termux/files/usr/lib/python3.14/site-packages/pip (python 3.14) |
| `node` | `/data/data/com.termux/files/usr/bin/node` | v24.22.2 |
| `npm` | `/data/data/com.termux/files/usr/bin/npm` | 12.2.2 |
| `bun` | `/data/data/com.termux/files/usr/bin/bun` | 1.4.4 |
| `cargo` | `/data/data/com.termux/files/usr/bin/cargo` | cargo 1.99.1 (5f94df478 2026-08-27) (built from a source tarball) |
| `rustc` | `/data/data/com.termux/files/usr/bin/rustc` | rustc 1.99.1 (58cf8f9c3 2026-10-13) (built from a source tarball) |
| `cmake` | `/data/data/com.termux/files/usr/bin/cmake` | cmake version 4.4.5 |
| `make` | `/data/data/com.termux/files/usr/bin/make` | GNU Make 4.4.1 |
| `clang` | `/data/data/com.termux/files/usr/bin/clang` | clang version 21.1.8 |
| `ffmpeg` | `/data/data/com.termux/files/usr/bin/ffmpeg` | ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers |
| `sox` | `/data/data/com.termux/files/usr/bin/sox` | sox:      SoX_ng v14.8.1 |
| `magick` | `/data/data/com.termux/files/usr/bin/magick` | Version: ImageMagick 7.1.2-35 Q16-HDRI aarch64 acaf7e418:20261031 https://imagemagick.org |
| `identify` | `/data/data/com.termux/files/usr/bin/identify` | Version: ImageMagick 7.1.2-35 Q16-HDRI aarch64 acaf7e418:20261031 https://imagemagick.org |
| `optipng` | `/data/data/com.termux/files/usr/bin/optipng` | OptiPNG version 7.9.1 |
| `pngquant` | `/data/data/com.termux/files/usr/bin/pngquant` | 3.0.3 |
| `assimp` | `/data/data/com.termux/files/usr/bin/assimp` | Unrecognized command. Use 'assimp help' for a detailed command list |
| `gltf-transform` | `/data/data/com.termux/files/usr/bin/gltf-transform` | 4.5.1 |
| `r2` | `/data/data/com.termux/files/usr/bin/r2` | [31mERROR:[0m Missing file to open |
| `rabin2` | `/data/data/com.termux/files/usr/bin/rabin2` | Usage: rabin2 [-AcdeEghHiIjJlLMqrRsSuvVxzZ] [-@ at] [-a arch] [-b bits] [-B addr] |
| `7z` | `/data/data/com.termux/files/usr/bin/7z` | Command Line Error: |
| `sqlite3` | `/data/data/com.termux/files/usr/bin/sqlite3` | 3.54.1 2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6alt1 (64-bit) |
| `rg` | `/data/data/com.termux/files/usr/bin/rg` | ripgrep 15.2.0 |
| `fd` | `/data/data/com.termux/files/usr/bin/fd` | fd 10.5.2 |
| `git` | `/data/data/com.termux/files/usr/bin/git` | git version 2.56.0 |

## Python modules

| Module | State |
|---|---|
| `trimesh` | 5.2.1 |
| `pygltflib` | 1.16.5 |
| `numpy` | 2.5.4 |
| `PIL` | 12.3.0 |
| `pyassimp` | unavailable (import fails) |

## Gaps and decisions

- **assimp**: Termux `assimp` 6.0.6 ships `libassimp.so` **without a CLI**. Decision: keep the library and validate through the QA probe `scripts/qa/assimp_check.c` (`clang -lassimp`), which imports the glTF and reports meshes/vertices/faces/materials. `pyassimp` cannot be imported here (`ctypes.util.find_library('assimp')` fails on the unversioned Termux soname), so the C probe replaces it.

- **stormlib / storm / mpqextract**: absent from the Termux repositories (`apt-cache search stormlib` empty). Decision: MPQ stays on the canonical `wc3-mpq` crate; `r2`/`rabin2` handle binary inspection. No third-party MPQ binary installed.

- **glTF tooling**: `@gltf-transform/cli` 4.5.1, `three` 0.187.0, `gltf-validator` 2.0.0-dev.3.10, `gltfpack` 1.3.0 installed globally. Global bare-specifier `import`/`require` fails from an arbitrary cwd; QA harnesses import them by absolute path under `/data/data/com.termux/files/usr/lib/node_modules/`.

- **Image**: ImageMagick 7.1.2-35 Q16-HDRI, OptiPNG 7.9.1, pngquant 3.0.3 — screenshot crops, PNG checks, palette experiments.

- **Audio**: ffmpeg 9.0.2, SoX_ng 14.8.1 — WAV decode/inspect for audio spot checks.

- **RE**: radare2 present (`r2` needs a file argument; bare `r2` prints 'Missing file'), `rabin2` for headers/sections.

## Adopted pipeline (working, evidence in `analysis/agent-qa/tooling/`)

1. `scripts/qa/mdx_gltf` (Rust, depends only on `wc3-formats`) converts MDX -> glTF 2.0 with PNG textures decoded by the canonical `wc3_formats::blp` (PNG written by a built-in stored-deflate encoder; no image crate, no second MDX/BLP parser).

2. `scripts/qa/gltf_check.py` validates each glTF with four independent consumers: Khronos `gltf-validator`, `gltf-transform inspect`, `trimesh`, and the assimp C probe.

3. `scripts/qa/gltf_preview.py` renders a flat-shaded three-quarter view for human eyes; output is labelled `geometry preview v1 (procedural raster)` — a QA aid, never a product asset.

## Measured results (current tree)

- 5 models across 4 races + a building convert and validate with **0 Khronos errors/warnings**; trimesh and assimp agree on face counts:
  Footman 372, Archer 449, Ghoul 1035, Grunt 688, TownHall 3220 (`analysis/agent-qa/tooling/gltf-run4/check.json`).

- Textures decoded with 0 failures for all five models (e.g. Footman 2, TownHall 6, Ghoul 3).

- Broader sweep: 16 four-race unit/building MDX converted into 14 unique output dirs, **14/14 validated, 0 Khronos errors** (`analysis/agent-qa/tooling/gltf-sweep/check.json`; duplicate parent names reuse a directory, so two conversions overwrote an existing output name — recorded, not hidden).


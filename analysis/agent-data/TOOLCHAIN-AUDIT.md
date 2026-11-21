# TOOLCHAIN AUDIT — r2-data (ledger 354), 2026-11-13

Device: Termux/Android aarch64 · Model: opencode-go/deepseek-v4.1-flash#max
H2 base consulted: `harness2_corpus_search` ids 44496 (full-surface audit mandate),
61478 (MDX per-sequence extents), `harness2_experience` ids 4438, 4426, 4413.

## 1. Present (command -v, --version)

| Tool | Version | Path |
|---|---|---|
| python3 | 3.14.10 | `$PREFIX/bin/python3` |
| pip | 26.3 | `$PREFIX/bin/pip` |
| node | v24.22.2 | `$PREFIX/bin/node` |
| npm | 12.2.2 | `$PREFIX/bin/npm` |
| bun | 1.4.4 | `$PREFIX/bin/bun` |
| cargo / rustc | 1.99.1 | `$PREFIX/bin` |
| go | 1.27.3 android/arm64 | `$PREFIX/bin/go` |
| clang / gcc alias | 21.1.8 | `$PREFIX/bin` |
| cmake / make | 4.4.5 / 4.4.1 | `$PREFIX/bin` |
| git / rg | 2.56.0 / 15.2.0 | `$PREFIX/bin` |
| sqlite3 | 3.54.1 | `$PREFIX/bin/sqlite3` |
| jq | 1.8.2 | `$PREFIX/bin/jq` |
| ffmpeg / sox | 9.0.2 / SoX_ng 14.8.1 | `$PREFIX/bin` |
| ImageMagick | 7.1.2-35 Q16-HDRI | `magick`/`convert` |
| optipng / pngquant | 7.9.1 / 3.0.3 | `$PREFIX/bin` |
| radare2 / rabin2 | 6.2.6 | `$PREFIX/bin` |
| 7-Zip (p7zip) | 17.05 | `$PREFIX/bin/7z` |

Python libs: numpy 2.5.4, trimesh 5.2.1, pygltflib 1.16.5, **pyassimp 5.2.5** (pip, wraps system `libassimp.so` 6.0.6).
Node libs (local tool dir `~/.local/share/h2-tools/gltf`): `@gltf-transform/core|functions|extensions` **4.5.1**, `three` **0.187.0**.

## 2. Installed / adapted this round (with rc)

| Step | Command | rc | Evidence |
|---|---|---|---|
| pyassimp bindings | `pip install pyassimp` | 0 | `pip show pyassimp` 5.2.5 |
| assimp load probe | `LD_LIBRARY_PATH=$PREFIX/lib python3` → `pyassimp.load(box.glb)` | 0 | `loaded meshes 1 verts 8` |
| glTF toolchain (pure JS) | `npm install @gltf-transform/core @gltf-transform/extensions @gltf-transform/functions three` | 0 | versions above |
| glTF inspect probe | `node gltf_inspect.mjs box.glb` | 0 | `{"meshes":1,"nodes":1,"accessors":2,"materials":0}` |

`gltf_inspect.mjs` (tool dir, not the repo):
`~/.local/share/h2-tools/gltf/gltf_inspect.mjs` — reads a GLB through
`NodeIO`/`Document` and reports mesh/node/accessor/material counts.

## 3. Gaps (and the sanctioned alternative)

| Gap | Symptom | Alternative in use |
|---|---|---|
| `assimp` CLI | Termux package ships only `libassimp.so` (no `assimp` binary) | `pyassimp` (ctypes on the same lib) + `trimesh`/`gltf-transform` for GLB |
| `gltf-transform` CLI | global CLI crashes importing `sharp` native module on aarch64/Android | `@gltf-transform/core` (pure JS) via `gltf_inspect.mjs`; `sharp` needed only for texture resize |
| `stormlib` / `smpq` | no Termux package; no pip wheel | project `wc3-mpq` crate + `r2`/`rabin2` for MPQ/format RE (read-only, operator's own files) |
| headless WebGL for `three` | Node on Termux has no WebGL context | `three` installed for viewer code; visual device checks only with `H2_DEVICE_SLOT=1` |

No untrusted donor code was executed; every probe ran on a locally generated GLB
(`trimesh.creation.box()`).

# PHASE 0 TOOLCHAIN AUDIT — Termux/Android aarch64

Date: 2026-11-13 · session ledger **350** (RE) · root verified `su -c id` →
`uid=0(root) gid=0(root) context=u:r:ksu:s0`.

## Installed and verified

| Tool | Path | Version (observed) | Use |
|---|---|---|---|
| assimp | `$PREFIX/bin/assimp` | **4.5.1** (CLI banner; pkg metadata 6.0.6) | MDX/OBJ/glTF import + export |
| gltf-transform | `$PREFIX/bin/gltf-transform` | **4.5.1** | glTF validation/compression/inspection |
| trimesh | `$PREFIX/bin/trimesh` | **5.2.1** (`python3 -c`) | mesh loading/GLB/OBJ checks |
| pygltflib | Python site-packages | **1.16.5** | glTF/GLB read/write in Python |
| three | `npm root -g` = `$PREFIX/lib/node_modules` | **r187** | glTF scene graph/renderer (Node needs `NODE_PATH`) |
| ImageMagick | `magick`, `convert` | **7.1.2-35 Q16-HDRI aarch64** | image convert/compare |
| OptiPNG | `optipng` | **7.9.1** | lossless PNG optimize |
| pngquant | `pngquant` | **3.0.3** | palette PNG quantization |
| ffmpeg | `ffmpeg` | **9.0.2** | audio/video transcode, waveform |
| sox | `sox` | **SoX_ng 14.8.1** | audio conversion/trim |
| radare2 | `r2`, `radare2` | **6.2.6** | binary RE (Game.dll/Storm) |
| StormLib | `third_party/stormlib-build2/libstorm.a` | **9.40** (`STORMLIB_VERSION_STRING`) | MPQ oracle |
| Storm probe | `tools/storm_probe` | ELF aarch64, NDK r30, 552 304 B | trusted local MPQ listing/read probe |
| node/npm/bun | `$PREFIX/bin` | 24.22.2 / 12.2.2 / (bun present) | JS tooling |
| python3/pip3 | `$PREFIX/bin` | 3.14.10 / 26.3 | Python tooling |
| cargo/rustc | `$PREFIX/bin` | 1.99.1 | Rust engine |
| 7z / unzip | `$PREFIX/bin` | present | archive inspection |

## Installs performed this round (rc recorded)

- `pkg install -y assimp` → **rc=0** (`assimp is already the newest version`).
- `pip3 install --upgrade pygltflib` → **rc=0** (`pygltflib 1.16.5` already present).
- `npm install -g three` → **rc=0** (`changed 1 package`).
- Log: `analysis/agent-re/toolchain-install.log`.

## Gaps and honest status

- **No `storm`/`stormlib` CLI package**: StormLib is available as the trusted
  local static library (9.40) and `tools/storm_probe`; the Rust `wc3-mpq`
  implementation is the primary manifest reader. Missing CLI is not a blocker.
- **`assimp` CLI reports 4.5.1 while `pkg` metadata says 6.0.6**: the observed
  binary version is recorded; do not claim 6.x features.
- **`three` is a library, not a headless viewer**: Node needs
  `NODE_PATH=$(npm root -g)`; no GPU/WebGL renderer is configured in Node.
  Offline mesh validation uses `gltf-transform` + `trimesh`/`pygltflib` +
  `assimp`; visual device checks remain gated by `H2_DEVICE_SLOT=1`.
- **No `pyassimp`** (Python bindings) — use the `assimp` CLI or `trimesh`.
- Donor code is never executed: StormLib is a trusted already-built local copy;
  no arbitrary donor scripts were run.

## H2 evidence cited

- `harness2_corpus_search` `evidence_id=ev-43fc87d2b84ecc99`:
  corpus item **61472** (StormLib MPQ oracle: 10 525 entries in War3.mpq;
  real MDX/BLP/SLK fixtures), **61463** (setup.mpq pairing), **60124**
  (ISO MPQ clean data path), **4337** (MPQ per-sector tags).
- `harness2_experience` `evidence_id=ev-c743dae3c7724524`:
  experience **4285** (encrypted-uncompressed MPQ sectors), **4253**
  (PKWARE setup.mpq), **4283/4280** (measure existing crates before porting),
  **4337** (single-base/MPQ sector-tag discipline).

## MDX → glTF → visual-check pipeline (verified)

- Extracted `Units\Human\Footman\footman.mdx` (101 969 B) from
  `analysis/war3_full3/War3_med.mpq` via `wc3re extract` (rc 0).
- `cargo run -p wc3-render --example mdx2gltf -- footman.mdx footman.gltf`
  → **rc 0**, `geosets=5 vertices=505 triangles=304`, 4 476 B glTF + 19 808 B
  bin (real MDX geometry, no procedural substitute).
- `trimesh` loads the glTF as a Scene with 5 geometries / 505 vertices.
- `assimp info footman.gltf` rc 0 (1 mesh, 1 material).
- `gltf-transform inspect` OOMs with the default Node heap on this device;
  `NODE_OPTIONS=--max-old-space-size=2048` makes it pass (rc 0).
  This is a tooling constraint, recorded rather than hidden.

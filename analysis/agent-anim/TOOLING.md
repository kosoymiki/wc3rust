# Anim layer toolchain + MDX→glTF verification (round 9, 2026-11-13)

Owner: `analysis/agent-anim/**`, `crates/wc3-anim/**`. No device launch
(`H2_DEVICE_SLOT` not set). Original game data only; the exporter reads the
operator's extracted tree at runtime.

## 1. Toolchain audit (command -v / --version)

| tool | version | path | status |
| --- | --- | --- | --- |
| python3 | 3.14.10 | `$PREFIX/bin/python3` | present |
| pip3 | 26.3 | `$PREFIX/bin/pip3` | present |
| node | 24.22.2 | `$PREFIX/bin/node` | present |
| npm | 12.2.2 | `$PREFIX/bin/npm` | present |
| bun | 1.4.4 | `$PREFIX/bin/bun` | present |
| ffmpeg | 9.0.2 | `$PREFIX/bin/ffmpeg` | present |
| sox | SoX_ng 14.8.1 | `$PREFIX/bin/sox` | present |
| ImageMagick | 7.1.2-35 Q16-HDRI | `$PREFIX/bin/magick` | present (`convert` is the deprecated alias) |
| optipng | 7.9.1 | `$PREFIX/bin/optipng` | present |
| pngquant | 3.0.3 | `$PREFIX/bin/pngquant` | present |
| gltf-transform | 4.5.1 | `$PREFIX/lib/node_modules/@gltf-transform/cli` | present (fixed, see §2) |
| gltf-validator (Khronos) | 2.0.0-dev.3.10 | global npm | installed, rc=0 |
| three | 0.187.0 (r187) | `$PREFIX/lib/node_modules/three` | installed, rc=0 |
| trimesh | 5.2.1 | python site-packages | present |
| pygltflib | 1.16.5 | python site-packages | present |
| numpy | 2.5.4 | python site-packages | present |
| scipy | 1.18.1 | python site-packages | present |
| assimp | 6.0.6 **library only** | `$PREFIX/lib/libassimp.so` + headers | present, no CLI, **no MDX importer** |
| radare2 | 6.2.6 | `$PREFIX/bin/r2` | present |
| rtk | 1.1.0 | `$PREFIX/bin/rtk` | present |
| su (KernelSU) | 4.2.0-rc3 | `$PREFIX/bin/su` | present |
| cargo / rustc | 1.99.1 | `$PREFIX/bin/cargo` | present |
| h2oc | 0.2.0 (Rust) | `$PREFIX/bin/h2oc` | present |
| stormlib | — | — | **missing**: not in the Termux repo (gap; MPQ access already covered by the extracted tree + r2) |

## 2. Gaps found and how they were closed

1. **gltf-transform crashed at startup** — its bundled `sharp` native module has
   no `android-arm64` binary:
   `Error: Could not load the "sharp" module using the android-arm64 runtime`.
   Fix: install the WebAssembly fallback next to the CLI
   (`npm install --prefix $(npm root -g)/@gltf-transform/cli @img/sharp-wasm32`,
   rc=0). `gltf-transform --version` → 4.5.1; `gltf-transform inspect` rc=0.
2. **assimp Termux package is a library, not a tool**, and it cannot read
   Warcraft MDX: a C probe against `libassimp.so` builds rc=0 and reports
   version 6.0.6, and `strings libassimp.so | grep -ci mdx` → **0**. The
   MDX→glTF bridge is therefore our exporter, not an assimp import.
3. **three r187 splits `three.module.js` + `three.core.js`** — copying only the
   module produced a silent 404 and a dead module graph. Both files plus
   `examples/jsm/{loaders,utils}` are vendored under
   `analysis/agent-anim/gltf-viewer/vendor/`.
4. **Headless Chromium WebGL readback is empty on this device**:
   `gl.readPixels` returns `0,0,0,0` and `canvas.toDataURL` is transparent even
   though the context reports `WebGL 2.0`. Tried `--enable-unsafe-swiftshader`,
   `--use-angle=swiftshader`, `--use-gl=angle --use-angle=swiftshader`,
   `--disable-gpu --enable-unsafe-swiftshader` — identical result. three.js
   still validates the file at the loader level (`document.title =
   ready:Spell`, 2 clips); pixel evidence comes from the independent Python
   glTF rasterizer (§4).
5. `sweep::pick_sequence` matches lower-cased names: the `shot` mode passed
   `MDX_SEQUENCE` verbatim, so `MDX_SEQUENCE=Spell` silently fell back to Stand
   (fixed: needle is lower-cased; `mdxaudit shot` then reports `seq=Spell`).

## 3. MDX→glTF exporter (new: `mdxaudit gltf`)

`analysis/agent-anim/mdxaudit/src/gltf.rs` (~450 lines, no new crates) writes a
GLB 2.0: one skinned mesh (one primitive per geoset, JOINTS_0/WEIGHTS_0 from the
canonical `GeosetSkin`, ≤4 influences), one joint per BONE under an `armature`
root, identity inverse-bind matrices (MDX bind pose is identity), and one glTF
animation per requested clip sampled from `Skeleton::bone_worlds_in_sequence`
(the same runtime the game renders).

CLI:

```bash
mdxaudit gltf <model.mdx...> --assets analysis/war3_full --gallery <out> \
  --ability cast --sequences Stand --projectile flight,impact --fps 15 --frames 60
```

Sequence selection uses the round-8 registry alias tables, so `--ability cast`
picks `Spell`/`Spell Throw`/`Spell - 1`/`Attack Walk Stand Spin` and
`--projectile flight,impact` picks `Stand`/`Death` per missile.

Special cases: models with **zero geosets** (particle-only missiles/effects) are
reported as `skipped: particle-only`, not as failures.

## 4. Verification evidence

### Khronos glTF validator (`gltf-validator` 2.0.0-dev.3.10, WASM)

| set | files | errors | warnings |
| --- | ---: | ---: | ---: |
| curated (`gallery/gltf/*.glb`) | 4 | **0** | **0** |
| batch (`gallery/gltf-batch/*.glb`) | 76 | **0** | **0** |

### Structural (pygltflib + trimesh), batch CSV

`analysis/agent-anim/gallery/gltf-batch/gltf-verify.csv` — **76/76 PASS**:
145 clips, 10 476 animation channels, 1 768 joints, 16 990 vertices, 8 427 368
bytes. Every file has ≥1 skin with joints, ≥1 primitive and finite vertices.

Export coverage: 19 hero models (`--ability cast --sequences Stand`; one stem is
shared by two folders, `HeroKeeperoftheGroveGhost`), 58 missile/effect models
(`--projectile flight,impact`), 9 particle-only models skipped (no geometry).

### gltf-transform (4.5.1)

`gltf-transform inspect HeroArchMage.glb` rc=0 — version 2.0, generator
`h2 mdxaudit gltf`, scene bbox `-162.77..162.26 / -149.02..149.16 / -0.63..176.66`,
672 upload vertices (matches the MDX geoset count).

### three.js (r187) loader

Headless Chromium page (`gallery/gltf-viewer/`, served over local HTTP) loads
the GLB with `GLTFLoader` and reports `ready:Spell` and 2 clips for
`HeroArchMage.glb` (title/HUD evidence). Pixel readback gap documented in §2.4.

### Independent glTF skinning render (`gltf-verify/render_glb.py`)

Pure numpy/PIL: reads the GLB, evaluates the file's own samplers, applies
`jointWorld × inverseBind`, rasterizes two frames (z-buffer). No Rust code is
reused, so this proves the file carries correct weights and animation:

| model | clip | duration | channels | frame diff |
| --- | --- | ---: | ---: | ---: |
| HeroArchMage | Spell | 2.700 s | 132 | 9.91 %  |
| HeroBladeMaster | Attack Walk Stand Spin | 0.433 s | 156 | 6.43 % |
| FireBallMissile | Stand | 0.500 s | 15 | 3.03 % |

Artifacts: `gallery/gltf-verify/<Model>-<Clip>-{t0,tmid}.png` (900×700).

## 5. H2 citations

- corpus `61480` — MDX animation must be sampled per sequence (canonical rule
  the exporter consumes through `bone_worlds_in_sequence`).
- corpus pack `ae_corpus_discipline+evidence` (this round), skill pack
  `h2-skill-pack/v1` (3D verification task).
- experience `4274` — per-sequence sampling/loop rule (no cross-sequence leak).
- experience `4418` — gameplay animation alias tables with flagged fallbacks
  (round 8), reused by `--ability/--projectile` selection.

## 6. Reproduce

```bash
cd ~/src/herofolends-rs
# curated exports
mdxaudit gltf analysis/war3_full/Units/Human/HeroArchMage/HeroArchMage.mdx \
  --assets analysis/war3_full --gallery analysis/agent-anim/gallery/gltf \
  --ability cast --sequences Stand
# batch: heroes
mdxaudit gltf $(find analysis/war3_full/Units -path "*Hero*" -name "*.mdx" ! -iname "*portrait*") \
  --assets analysis/war3_full --gallery analysis/agent-anim/gallery/gltf-batch \
  --ability cast --sequences Stand --fps 15 --frames 60
# batch: missiles
mdxaudit gltf analysis/war3_full/Abilities/Weapons \
  --assets analysis/war3_full --gallery analysis/agent-anim/gallery/gltf-batch \
  --projectile flight,impact --fps 15 --frames 60
# validate
node ~/tmp/gltf_validate.cjs analysis/agent-anim/gallery/gltf-batch/*.glb
python3 analysis/agent-anim/gltf-verify/render_glb.py \
  analysis/agent-anim/gallery/gltf/HeroArchMage.glb analysis/agent-anim/gallery/gltf-verify Spell
```

## 7. Remaining gaps (documented, not blocking this layer)

- Headless Chromium WebGL pixels: no composited/readable framebuffer on this
  Termux build; three.js loader-level validation works, pixel evidence comes
  from the independent rasterizer.
- stormlib absent from the Termux repo; not required (the exporter reads the
  extracted tree).
- Textures are exported as per-material base colours sampled from the first BLP
  texel, not as embedded PNGs (visual verification is about skeleton/skin
  correctness; team-colour textures are replaceable and render grey).

# Tooling + final-product round — QA layer (2026-11-13)

**Ledger:** 349 (`r2-qa`) · **Owner:** `scripts/qa/**`, `analysis/agent-qa/**`
**Device:** not launched — operator directive in force and no `H2_DEVICE_SLOT=1` grant.

## 1. Toolchain audit

Full table with paths/versions: `analysis/agent-qa/tooling/AUDIT.md`.

Present (observed): python 3.14.10 · pip 26.3 · node 24.22.2 · npm 12.2.2 · bun 1.4.4 ·
cargo/rustc 1.99.1 · cmake 4.4.5 · make 4.4.1 · clang 21.1.8 · ffmpeg 9.0.2 · SoX_ng 14.8.1 ·
ImageMagick 7.1.2-35 · OptiPNG 7.9.1 · pngquant 3.0.3 · gltf-transform 4.5.1 · three 0.187.0 ·
gltf-validator 2.0.0-dev.3.10 · gltfpack 1.3.0 · radare2 + rabin2 · 7z · sqlite3 3.54.1 · rg 15.2.0 · fd 10.5.2 · git 2.56.0.

Installed this round (rc 0):

| Install | rc | Result |
|---|---:|---|
| `pip install trimesh pygltflib` | 0 | trimesh 5.2.1, pygltflib 1.16.5, numpy 2.5.4 |
| `npm install -g three gltf-validator` | 0 | three 0.187.0, gltf-validator 2.0.0-dev.3.10 |
| `pkg install -y assimp` | 0 | assimp 6.0.6 (**library only**: `libassimp.so`, no CLI) |
| `pip install pyassimp` | 0 | installed but **unusable**: `ctypes.util.find_library('assimp')` fails on Termux's unversioned soname |

Gaps and decisions:

- **assimp CLI absent** → QA probe `scripts/qa/assimp_check.c` compiled with `clang -lassimp`
  (`analysis/agent-qa/tooling/assimp_check`, 10.4 KiB). It imports a glTF and reports
  meshes/vertices/faces/materials — the same library the desktop ecosystem uses.
- **stormlib / storm / mpqextract absent** from Termux repositories (`apt-cache search stormlib` empty).
  MPQ access stays on the canonical `wc3-mpq` crate; `r2`/`rabin2` handle binary inspection.
- **Global npm resolution**: bare `import`/`require` of global packages fails from an arbitrary cwd;
  harnesses import by absolute path under `/data/data/com.termux/files/usr/lib/node_modules/`.

## 2. MDX → glTF pipeline (new, working)

| Piece | Path | Notes |
|---|---|---|
| Converter | `scripts/qa/mdx_gltf/src/main.rs` (~470 lines) | canonical `wc3_formats::mdx` parser + `wc3_formats::blp` decoder; no second parser; PNG written by a built-in stored-deflate encoder (no image crate) |
| Validator harness | `scripts/qa/gltf_check.py` | Khronos `gltf-validator` + `gltf-transform inspect` + `trimesh` + assimp C probe; writes `check.json` |
| Preview | `scripts/qa/gltf_preview.py` | flat-shaded three-quarter raster for human eyes, labelled `geometry preview v1 (procedural raster)` — QA aid, not a product asset |
| assimp probe | `scripts/qa/assimp_check.c` | libassimp consumer (CLI substitute) |

Measured (current tree):

- Core set: 5 models (Footman, Archer, Ghoul, Grunt, TownHall) — **0 Khronos errors/warnings**,
  trimesh and assimp agree on faces: 372 / 449 / 1035 / 688 / 3220; textures decoded with 0 failures.
- Sweep: 16 four-race unit/building MDX converted, 14 unique output dirs validated, **14/14 pass**,
  0 Khronos errors (`analysis/agent-qa/tooling/gltf-sweep/check.json`).
- Previews non-blank: `preview-Footman.png` std 81.9 / 188 colours; `preview-TownHall.png` std 99.3 / 190 colours.
- Combined gate `tooling.mdx_gltf`: **19/19 models pass**.

Commands:

```
cargo build --offline            (in scripts/qa/mdx_gltf)                     rc 0
./target/debug/mdx-gltf --model analysis/war3_full/... --out <dir> --texture-root analysis/war3_full
python3 scripts/qa/gltf_check.py --root analysis/agent-qa/tooling/gltf-run4   rc 0 (5/5)
python3 scripts/qa/gltf_check.py --root analysis/agent-qa/tooling/gltf-sweep  rc 0 (14/14)
python3 scripts/qa/gltf_preview.py --gltf <model.gltf> --out <png>            rc 0
```

## 3. Product gates on the current tree

`python3 scripts/qa/parity.py --skip-build` → **overall PASS — 32 PASS / 0 FAIL / 0 GAP / 1 SKIP**
(probe rebuilt 22:28 after the sim `Cargo.toml` edit landed; first build attempt failed with
`failed to load manifest for dependency wc3-sim` while the file was mid-write — recorded, not retried blindly).

All former gaps now measure PASS with numbers: `cast.coverage` 154/154 data-backed
(115 active castable, hand-written 11), `hero.leveling` 12/12, `items.gaps` 4/4,
`triggers.jass` 45/45 + return 3/3 + elseif 1/1, `train.menu_linkage` 121/121, `audio.wired` PASS,
`models.per_def` 216/216, `models.budget` 73.4/1536 MB with 98 LRU evictions.

## 4. Report

`analysis/agent-qa/final/report.json` (`run_id=tooling-and-parity`): **16 PASS / 0 FAIL / 1 NOT_RUN**
(device), overall **PASS**. Tooling gates are part of the report; `parity_observed` carries the raw
32/0/0/1 table; `device.status = NOT_RUN` with the four pre-directive attempts and their evidence.

## 5. Device (unchanged)

No launch this round: the operator-stop directive is still in force and `H2_DEVICE_SLOT` was not granted.
Unproven on device: control-group recall after a different selection, tooltip band, training from a newly
placed hall. Harness fixes are in `scripts/qa/device_p0_cycle.py`; one granted cycle on an idle phone closes them.

## 6. Risks / next

| Item | Owner | Evidence |
|---|---|---|
| Device P0 evidence for the current APK | tech-lead grant → QA | `analysis/agent-qa/parity/p0-device/DEVICE-NOT-RUN.md` |
| MDX→glTF covers static geometry only (no animation/skinning/particles/team colour) | QA decision | converter header; summary reports skipped features |
| Preview renderer is a flat raster, not render parity | QA | labelled output + `AUDIT.md` |

# TOOLING + FINAL PRODUCT AUDIT — r2-fdf / ledger 353 (2026-11-13)

Layer: fdf (HUD fonts / Cyrillic / font tooling). Model: opencode-go/deepseek-v4.1-flash#max.

## 1. Toolchain audit (command -v + --version, observed)

| Tool | Version / path | Status |
|---|---|---|
| rustc / cargo | 1.99.1 (`$PREFIX/bin`) | OK |
| python3 / pip | 3.14.10 / 26.3 | OK |
| node / npm / bun | v24.22.2 / 12.2.2 / 1.4.4 | OK |
| ffmpeg / sox | 9.0.2 / SoX_ng 14.8.1 | OK |
| ImageMagick / optipng / pngquant | 7.1.2-35 / 7.9.1 / 3.0.3 | OK |
| tesseract | 5.5.4 (eng+rus) | OK |
| radare2 | 6.2.6 (abi 174, linux-arm_64) | OK |
| git / jq | 2.56.0 / 1.8.2 | OK |
| @gltf-transform/cli | 4.5.1 (npm global) | **CLI broken on Termux** — `sharp` native module throws at startup |
| three | 0.187.0 (npm global) | OK (module only) |
| trimesh / pygltflib | 5.2.1 / 1.16.5 (pip, present) | OK |
| fonttools / mpyq | 4.68.0 / 0.2.5 (pip, installed this round) | OK |
| assimp | pkg 6.0.6 installed (`libassimp.so`, headers) | **CLI missing** (`$PREFIX/bin/assimp` absent) |
| stormlib | — | **not packaged** (`pkg install stormlib` rc=100 `E: Unable to locate package`) |

Installs this round (rc evidence):
- `pip install fonttools mpyq` → **rc=0** (fontTools 4.68.0, mpyq 0.2.5).
- `pkg install -y assimp` → **rc=0**, package 6.0.6 installed; CLI not shipped.
- `pkg install -y stormlib` → **rc=100** (unavailable in Termux mirror).
- `@gltf-transform/cli@4.5.1`, `three@0.187.0` already global; gltf-transform CLI fails on `sharp`.

### Gaps and alternatives (deliberate, not stubs)
- **assimp CLI**: use Python `trimesh` + `pygltflib` for mesh IO/validation; if the CLI is needed,
  build from source later (`pkg install cmake ninja` + upstream `assimp`), or call `libassimp.so`
  via `pyassimp` with `LD_LIBRARY_PATH=$PREFIX/lib`.
- **stormlib**: MPQ access via `mpyq 0.2.5` (read-only) and the project's canonical Rust readers;
  no new dependency required for current work.
- **gltf-transform CLI**: blocked by `sharp` on Termux aarch64; `pygltflib`/`trimesh` cover
  validation and conversion for the visual-check pipeline; `three` available for JS side.

## 2. H2 base usage (cited)

- `harness2_corpus_search` → **61936** `dream.acceptance-qa-must-follow-the-canonical-hud-not-4316`
  (canonical console HUD gates: 64 px cells, 11/13/20/22 px Tokens, console 288, 4×3 card, no joystick;
  old 48dp/24px rules are advisory) — applied to the device-evidence reading from R8.
- `harness2_corpus_search` → **44493** `build.update-is-upstream-only` (never `/update` the custom H2
  build) — respected: no self-update run.
- `harness2_experience search` → **4393** (wc3-data: measure parity from data, not doc baselines) —
  applied to the font audit: usage measured from the operator's 71 FDFs and font files, not from docs.
- Skill `engineering-reflection` loaded and applied (Goal/invariants → mechanism → root cause →
  blast radius → falsification → named verification → persist).
- Corpus/experience gates for ROUND8/9 runs: `harness2_corpus_preflight`, `harness2_monitor`
  ledger 353.

## 3. Font tooling delivered (final-product work)

`analysis/agent-fdf/tools/font_audit.py` (fontTools + operator data, audit-only; runtime parsing
stays in Rust `wc3-formats`/`wc3-game`). Output `analysis/agent-fdf/evidence/font-audit.{json,md}`.

Observed results (rc=0):

| Metric | Value |
|---|---|
| FDF documents scanned | **71** |
| Font keys in `war3skins.txt` | 6 (all → `Fonts\FRIZQT__.TTF`) |
| FDF font keys used | `MasterFont`, `EscMenuTextFont`, `InfoPanelTextFont` |
| FDF sizes | 0.009 / 0.011 / 0.013 / 0.015 / 0.024 (0.011 → 36 frames, 0.013 → 17, 0.015 → 12) |
| Unique UI chars | 143; **Cyrillic used: 62** |
| Chars missing from FRIZQT__ | **0** |
| FRIZQT__ coverage | ASCII 95/84 range-ok, **Cyrillic 92/96** (4 absent are non-Russian range slots) |
| Overflow candidates (>50% of 1920 px at exact FDF size) | **0** |
| Other fonts | NIM_____ 95/92, dfst-m3u 95/66, tt5500m 95/66, DFHeiMd/dffn_b31 cmap unreadable by fontTools (noted, not fatal) |

Interpretation: the operator's UI strings (including 62 Cyrillic characters) are fully covered by the
game font at every FDF size in use; no string in the audited corpus overflows half the 1920 px line at
its own FDF size. `analysis/agent-fdf/evidence/font-audit.md` is the human-readable report.

## 4. 3D pipeline (MDX → glTF visual checks)

Repo tool `scripts/qa/mdx_gltf` (canonical `wc3-formats` MDX/BLP readers) had two defects blocking use;
validated fixes in a scratch copy and delivered as a patch (owner: QA, outside this layer's write scope):

- `texture_png`/texture-count symbol mismatch → summary uses `images_json.len()`.
- Windows `\` kept in texture URI → invalid JSON escape (`"textures/Textures\Arthas_256x256.png"`) and
  an odd filename; normalise to `/` before deriving the stem.

Patch: `analysis/agent-fdf/patches/0001-mdx-gltf-texture-count-and-uri-escape.patch`
(applies with `patch -p4` inside `scripts/qa/mdx_gltf`, verified on the current tree: `patch_apply_rc=0`,
`cargo build --release rc=0`).

Evidence (current code + patch, scratch build):

```
MDX_GLTF_OK model="Arthas" primitives=4 vertices=650 triangles=531 materials=4 textures=1
  texture_failures=0 bounds_min=[-148.9,0.0,-139.2] bounds_max=[130.1,121.0,139.9]
MDX_GLTF_OK model="HumanMage" primitives=4 vertices=441 triangles=438 materials=3 textures=2 ...
{"json_valid":true,"meshes":1,"images":1,"materials":4,"geoms":4,"textures":"textures/Arthas_256x256.png"}
```

- pygltflib loads the JSON; trimesh builds the scene (4 geometries); bounds match the converter summary.
- Image pipeline on the exported texture (256×256 PNG 262 488 B): `optipng -o1` rc=0 → 130 916 B;
  `pngquant --quality 60-90` rc=0 → 43 676 B; `magick identify` OK.
- Assets: `analysis/agent-fdf/evidence/gltf3/arthas/`, `gltf2/{arthas,human_mage}/`,
  `gltf-validate.json`; toolchain self-test `evidence/toolchain-selftest-reference-v1.glb`
  (procedural reference v1, toolchain check only — not product art).

## 5. Final-product status for this layer

- Fonts/overflow/Cyrillic: measured (this audit) and enforced by tests already in the layer
  (`exact_size_px`, locale-aware face resolution, Cyrillic round-trip, console/tooltip faces).
- Rust gate re-run this round is **blocked by external in-flight edits** (not fdf):
  `wc3-data/src/balance.rs` (`min_damage`/`max_damage` missing on `UnitView`, then E0499 borrow) —
  `cargo check -p wc3-game --lib` rc=101 at 21:5x; `wc3-formats` fdf rc=0 (13 passed).
  Re-run plan once data settles: `cargo check -p wc3-game --lib`; `ui::text::tests`; `ui::tests`;
  `ui_fdf`; `formats fdf`.
- No device launch this round (H2_DEVICE_SLOT not set).

## 6. Next steps
1. QA owner applies `patches/0001-mdx-gltf-*.patch`; then batch-convert unit galleries to glTF for
   visual checks (script exists, patch unblocks it).
2. Optional: `pyassimp` + `LD_LIBRARY_PATH` to use the installed `libassimp.so`, or build assimp CLI
   from source if the CLI is required.
3. Re-run fdf layer gates when `wc3-data` compiles; keep `ulimit -v 8000000` for wc3-game test builds.
4. Wire the audit into periodic checks (`font_audit.py` is deterministic; JSON diff on changes).

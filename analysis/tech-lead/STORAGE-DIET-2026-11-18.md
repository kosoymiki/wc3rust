# Storage diet — 2026-11-18 (ledger 560)

Scope: `~/src/wc3rust/analysis/**` (artifacts only), plus a disk survey of `~/tmp/**`.
No source game data and no `.psd` sources were touched. `rm` was not used:
old logs were moved to `analysis/trash-2026-11-18/` (DELETE_GUARD intact).

## Result

| Metric | Before | After | Delta |
|---|---|---|---|
| `df -h /data` available | 4.8 G (98%) | **5.4 G (98%)** | **+0.6 G** free |
| `du -sh analysis` | 4.3 G | **3.3 G** | **−1.0 G** |
| PNG ≥ 2 MB | 284 files / 1368.4 MB | **284 files / 339.9 MB** | **−1028.5 MB (−75.2%)** |
| Logs older than 24 h | 689 files / 33.0 MB | 0 in tree | **−33.0 MB** (moved) |
| **Total artifact bytes removed** | | | **1061.5 MB** |

The `df` delta is smaller than the artifact delta because lanes 537–555 wrote new
build artifacts during the run; the repo-level `du` drop (−1.0 G) matches the PNG
saving.

## PNG method

- 281 files: `pngquant --quality=70-90 --speed 1 --strip`, dimensions unchanged,
  palette quantized. Sample RMSE on `design/shots/before-hud-2414x1080.png` =
  **1.95/255** (visually identical).
- 3 files rejected by the quality floor (`pngquant rc=99`): re-saved losslessly with
  Pillow (`optimize=True, compress_level=9`), pixel bytes verified identical:
  `agent-hud/icons/commandbuttons_atlas.png` 11 538 100 → 6 720 017,
  `agent-hud/icons/contact_sheet.png` 11 536 692 → 6 668 734,
  `agent-world/icons-contact-sheet.png` 3 386 480 → 3 376 405.
- Every one of the 284 outputs re-opened with Pillow: **0 corrupt**.
- Notes: `analysis/design/shots/README-STORAGE.md`,
  `analysis/agent-hud/README-STORAGE.md` (both say “scaled for storage”).
- Per-file old/new bytes: `analysis/tech-lead/STORAGE-DIET-2026-11-18.csv` (284 rows).

## Logs moved (not deleted)

689 files, 33.0 MB, relative paths preserved under
`analysis/trash-2026-11-18/`; index `analysis/trash-2026-11-18/MOVED-LOGS.csv`.
Largest: `device-build-probe.log` 5.7 MB, `agent-qa/final/focus-gates.log` 4.2 MB,
`device-qa-20261117/logs/launch-01.log` 2.6 MB. Four **zero-byte** `*.log` files
were left in place (moving them frees nothing).

## Top-10 largest artifacts found (pre-cleanup)

`analysis/**`:

| MB | Path |
|---:|---|
| 287.0 | `analysis/tft_files_Files100_War3x.mpq` (source data — untouched) |
| 67.6 | `analysis/agent-hud/redraw-v0.1/wc3rust_hud_components.psd` (source — untouched) |
| 51.5 | `analysis/tft_files_Files100_War3xlocal.mpq` (source data — untouched) |
| 24.3 | `analysis/agent-tft/probe/target/release/deps/libwide-…rlib` |
| 23.9 | `analysis/agent-tft/probe/target/release/deps/libwide-…rmeta` |
| 22.2 | `analysis/agent-hud/redraw-v0.1/wc3rust_hud_landscape.psd` (source — untouched) |
| 21.2 | `analysis/agent-anim/mdxaudit/target/debug/deps/libglam-…rlib` |
| 21.0 | `analysis/agent-anim/mdxaudit/target/debug/deps/libglam-…rmeta` |
| 20.7 | `analysis/agent-tft/probe/target/release/deps/libglam-…rlib` |
| 20.6 | `analysis/agent-tft/probe/target/release/deps/libglam-…rmeta` |

`~/tmp/**`:

| MB | Path |
|---:|---|
| 7233.7 | `~/tmp/cn/payload.bin` |
| 2033.3 | `~/tmp/cn/my_stock.img` |
| 1775.9 | `~/tmp/cn/odm.img` |
| 1250.9 | `~/tmp/cn/my_heytap.img` |
| 1018.2 | `~/tmp/cn/system_ext.img` |
| 869.7 | `~/tmp/cn/my_bigball.img` |
| 735.5 | `~/tmp/cn/my_product.img` |
| 657.6 | `~/tmp/cn/system.img` |
| 110.0 | `~/tmp/ubuntu/opt/python/lib/python3.12/site-packages/frida/_frida.abi3.so` |
| 109.5 | `~/tmp/ubuntu/opt/node/bin/node` |

`~/tmp/cn/*` (≈15.3 GB) is outside the repo and not touched: it is an operator
decision (see `OPEN-TAILS.md` row k). Build `target/` directories are also left
alone — they belong to running lanes.

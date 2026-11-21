# Design lane 544 — index and integration contract

This directory is the art-source and visual-direction deliverable for the WC3 mobile HUD/menu redesign. The implementation lane is 553; the asset pipeline/validator/atlas lane is 555. This lane delivers the evidence, the direction, and the source art pack.

## Files

| file | what it is |
|---|---|
| `REFERENCES-50.md` | 50 GitHub file-level references: repo, path, commit, sha256, pattern, exact numbers taken. GitHub only; access date 2026-11-18. |
| `PROVENANCE.json` | machine-readable provenance of those 50 files (full commit + sha256 + bytes). |
| `HUD-DIRECTION.md` | the design spec: palette sampled from the shipped BLPs, Nimrod type scale at 3×, dp rhythm, 9-slice inset table, states, motion, composition, measured critique, acceptance. |
| `ASSETS.md` | asset-by-asset provenance: source BLP, source sha256, transform, output sha256, label, 9-slice checks. |
| `MEASUREMENTS.md` | before/after table with the exact numbers and the design-qa outputs (rc). |
| `assets/` (repo root `assets/hud/`) | delivered 9-slice PNGs + resource icons + `manifest.json` (format 2). |
| `shots/` | runtime `before-*` PNGs and direction `direction-*` PNGs at 2414×1080 and 1080×2414. |
| `qa/` | layout dumps + palette files + `REPORT.txt` (all four design-qa runs with rc). |
| `tools/` | `build_assets.py` (8-segment slicing), `build_pack.py` (pack + manifest), `render_direction.py` (direction shots). |
| `src_rgba/`, `src_icons/` | decoded shipped BLPs used by the build. |

## Integration contract for line 553 (UI Core / runtime)

1. **One layout source.** The three current console heights (456 / 367 / 384 px) must collapse to the 456 px = 152 dp source: `3 × 48 + 2 × 4 = 152` for the command grid, `portrait 80`, `card 376–440`, `minimap 112`.
2. **Safe area and density are inputs.** 70/40/30/32 dp landscape, 8/8/32/21 dp portrait; `px = dp × density`.
3. **Menu targets.** Rows 50 dp, tabs 48 dp, nav 48 dp, gaps 8 dp, title 32 dp, panel 660 × 282 (list) / 440 × 282 (main), content inset 10 dp. The campaign page shows 2 rows/page in landscape (page label + PREV/NEXT); portrait shows 6 rows + preview.
4. **Theme tokens.** Use `HUD-DIRECTION.md` YAML; no per-widget colours. The current tokens' contrast already passes; keep them.
5. **States.** Every plate exposes normal/hover/pressed/disabled/locked/selected; locked rows use the disabled atlas + padlock glyph, never a flat colour square.
6. **No silent fallback.** A missing themed part is a load error with the asset id, not an FDF substitute.
7. **Font.** Nimrod from the operator's `War3.mpq`; 13 dp label floor, 15 dp body, 22 dp title, tabular numerals.
8. **Runtime proof.** After wiring, `cargo run --example hud_mock` at both orientations + `layout_audit.py` rc=0 on the published layout dump. The direction shots here are design composites, not runtime proof.

## Integration contract for line 555 (pipeline/validator/atlas)

- Pack root: `assets/hud/`; manifest `assets/hud/manifest.json`, `format_version: 2`.
- IDs: `panel.menu`, `button.{normal,hover,pressed,disabled}`, `tab.{normal,active}`, `tooltip.panel`, `command.{normal,pressed,disabled}`, `minimap.{frame,arch}`, `portrait.frame`, `resource.{gold,lumber,supply,upkeep,mana}`, `infocard.{gold,supply}`.
- Insets are source pixels; validate `l + r < w`, `t + b < h`, centre free of border art, corners not scaled, border ≥ 1 px at target.
- `minimap.frame` centre is transparent by design (it overlays the map); other plates bake their fill.
- `minimap.arch` (232 × 312) and `portrait.frame` (300 × 284) are fixed images, not 9-slices.
- Atlases: none in this pack; 555 owns packing/UVs. Do not re-encode the PNGs lossily.
- Font: record `nimrod` sha256; verify Cyrillic coverage before load.

## Notes / limits

- The direction shots are PIL composites of the delivered PNGs and Nimrod, labelled `procedural reference v1`; they are not hand-drawn final art and not a runtime render.
- `crates/wc3-game/examples/ui_assets.rs` is an additive decode utility used to produce `src_rgba/`; it does not touch the runtime HUD path. The mp3 compile fix (`frame.offset as u64`) was a tree blocker met during the run and is disclosed in the report.
- No device commands were used; no `tests/` or `#[test]` were touched.

# Agent H — UI screens / menus / dialogs (all-layers push, 2026-11-21)

Ledger 604 · packet `analysis/tech-lead/all-layers-20261121/PACKET_H_ui-screens.md`
Owner paths (only writer): `crates/wc3-game/src/ui/**`, `crates/wc3-game/src/ui.rs`,
`crates/wc3-game/src/hudskin/**`.

Headless artifacts only: every PNG in this report is an **offline soft-raster mock**
(`hudskin::soft_raster`, CPU, nearest sampling). No device/GPU capture was taken; the
device slot is owned by agent G.

## Goal

Main menu and glue screens fit every viewport class (2414x1080, 1080x2414, 1280x720,
2340x1080, 320x240) without clipping, labels stay readable, dialogs use the design
tokens and stay inside the safe frame. Root causes fixed in the UI layer, with named
tests; the packet gate `menu_layout_report` must print 5/5 PASS.

## Baseline (observed 2026-11-21 14:02)

`menu_layout_report` was already 5/5 PASS *geometrically*, but on the compact class the
rows were **6.8 px high** (`MENU_BUTTON name=compact-320x240 ... h=6.8`): containment was
green while every label would have been clipped. That was the real defect class this
layer had to fix.

## Actions (file:line)

### 1. `crates/wc3-game/src/ui/layout/screens.rs` (new, pure dp)

- `plan_menu(window, theme, title, subtitle, entries, nav, page)` — safe frame from
  `Window::available()` (sanitised insets), centred plate (`PANEL_MAX_W_DP` 420), title
  band, status band, entry rows and a nav row. Row height is the largest lane the page
  has, capped at `ROW_MAX_DP` (64 dp) and floored by the **compact token line + label
  inset** (`row_floor`).
- When the entries cannot keep that floor the plan **paginates** (`per_page`,
  `page_count`, clamped `page`) instead of shrinking lanes; every entry is reachable
  exactly once.
- Labels are fitted through the theme's type ladder (`LABEL_LADDER`: Title→Heading→
  Body→Label→Compact) using `text_fit::advance`; a string that still does not fit is
  ellipsised with a **bounded** descent (one glyph per step, terminates at `…`).
- Short frames (< 420 dp) drop the subtitle and the status line *before* the rows: that
  recovered 6 pages of 1 row → 2 pages of 3 rows on 320x240.
- `plan_dialog(...)` — proportional plate, title cartouche, footer buttons whose count,
  columns and height come from the token floor, never from `height / count`.
- `shipped_windows()` — the five viewport classes with their platform densities
  (phone class density 3.0 per `mobile_hud::REFERENCE_HEIGHT` 1080 px) and safe insets.

Tests (all rc=0): `ui_screens_menu_fits_the_five_shipped_viewports`,
`ui_screens_dialog_fits_banner_and_buttons`,
`ui_screens_compact_viewport_paginates_instead_of_shrinking_rows`,
`ui_screens_safe_area_and_degenerate_inputs_stay_finite`,
`ui_screens_plan_is_deterministic`, `ui_screens_ellipsis_is_measured_not_clipped`.

### 2. `crates/wc3-game/src/ui/menu.rs`

`ui_menu_screens_plan_without_clipping_on_every_viewport` plans **every shipped screen**
(Main, Campaign, CustomGame, SinglePlayer, RaceSelect, OpponentSelect, MapSelect,
Difficulty, Options, SaveLoad, Paused, Loading, InGame + Victory/Defeat hosts) at all
five viewports: entries, list rows, tabs and nav. Each label is either drawn whole or
honestly ellipsised inside its box; every page of a paginated list is visited and the
page count covers the model exactly.

### 3. `crates/wc3-game/src/hudskin/dialogs.rs`

- `panel_art_scale` — a stretched nine-patch may not keep an atlas-proportional border:
  the border is capped at half of each axis. Before: the 320x240 dialog had 66/56 px
  insets on a 220 px plate, a 58 px content box and 14 px buttons.
- `panel_content` caps each side at a quarter, so the content is never under half of the
  plate (new test `panel_border_never_eats_half_of_a_stretched_plate`).
- `menu_lane_floor`, `token_px_for_height`, `fitted_px`, `label_fits`,
  `ellipsise_to_fit` — label sizes come from `Theme::classic_fantasy()` type roles at a
  viewport density (`type_density`), with a sub-compact step solved from the box itself
  (a 41 px label box at density 3 vs a 41.25 px compact line).
- `draw_menu`/`draw_menu_paged` **paginate** by `menu_lane_floor`; `draw_menu_with_labels`
  and `draw_dialog_with_labels` return `label_text` / `label_px` / `label_truncated`, so
  the caller cannot clip a label even when it ignores the fit flags.
- Dialog footer rows are counted (`rows * cell + gaps`); columns fall back to 1 when a
  two-column cell cannot host the longest button at the minimum size (3 buttons on
  320x240 with `QUIT TO MAIN MENU`).

Tests (all rc=0): `hudskin_dialogs_menu_fits_five_viewports_with_token_labels`,
`hudskin_dialogs_dialog_fits_five_viewports_with_token_labels`,
`hudskin_dialogs_compact_menu_paginates_instead_of_clipping`,
`hudskin_dialogs_use_the_design_token_floors`, plus the 18 existing dialog tests.

### 4. QA defect — 0-byte `portrait-1080x2340-tooltip-1080x2340.png`

- Regenerated with the shipped `hud_layout` example (binary 2026-11-21 12:56, newer than
  its source): 43 `SCREEN` lines, **0 `SCREEN FAILED`**; the file is now
  **1 014.4 KB** with IHDR 1080x2340 (verified by the new audit test).
- Root cause of the 0-byte state: `hudskin/soft_raster.rs::write_png` created the final
  path and then wrote — a killed render left an empty file. It now writes a hidden
  sibling scratch file, `sync_all`s it, and `rename`s it into place; a failed write
  removes the scratch and leaves the previous artifact untouched.
  (`soft_raster_write_png_is_atomic_and_leaves_no_scratch_file`,
  `soft_raster_write_png_failure_keeps_the_previous_artifact`,
  `soft_raster_encode_png_states_the_exact_dimensions` — 3/3 rc=0.)
- `crates/wc3-game/src/hudskin/screens_audit.rs` (new, test-only module): every top-level
  PNG in `analysis/hudskin/screens` must be non-empty, carry the PNG signature, have
  `IEND`, and match the `<w>x<h>` suffix in its name. `hudskin_screens_matrix_has_no_
  zero_byte_or_mislabelled_png` scans the real matrix (2/2 rc=0); the synthetic case
  proves the detector rejects empty, wrong-size and non-PNG files.

## Evidence (command → rc)

| Command (repo root) | Result |
|---|---|
| `scripts/h2-cargo test -p wc3-game --lib --offline ui::layout::screens` | 6 passed, 0 failed, **rc=0** |
| `scripts/h2-cargo test -p wc3-game --lib --offline hud::menu` | 6 passed, 0 failed, **rc=0** |
| `scripts/h2-cargo test -p wc3-game --lib --offline hudskin::dialogs` | 22 passed, 0 failed, **rc=0** |
| `scripts/h2-cargo test -p wc3-game --lib --offline hudskin::soft_raster` | 3 passed, **rc=0** |
| `scripts/h2-cargo test -p wc3-game --lib --offline hudskin::screens_audit` | 2 passed, **rc=0** |
| `scripts/h2-cargo run -q -p wc3-game --example menu_layout_report --offline` | **5/5 PASS**, rc=0 |
| `hud_layout` example (regeneration, current sources) | 43 `SCREEN`, 0 `SCREEN FAILED`; layout gate rc=1 — see Risks (B-owned). Logs: `$PREFIX/tmp/opencode/hud_layout_live_1795286591.log`, `$PREFIX/tmp/opencode/hud_layout_regen.log` |

Full `hudskin::` sweep (sandbox copy of the same sources, rc=101):
127 passed, 3 failed — all three are B-owned `mobile_hud` geometry (below), none in code
changed here.

## Result

- Packet gate: **5/5 PASS** in the live tree (`menu_layout_report`, rc=0).
- UI has a real screen planner (`ui/layout/screens.rs`): containment, token label sizes,
  pagination at a readable floor, bounded ellipsis, safe-frame clamps, degenerate-input
  safety — 6 named tests.
- Menu model proved across every screen × five viewports — 1 named test (`hud::menu`).
- Dialogs on design tokens with label-aware composition and pagination — 22 named tests.
- QA 0-byte artifact fixed at the root (atomic PNG write) and guarded by a named audit
  test; the specific PNG is regenerated and valid.
- Headless evidence only; no device run was made by this agent.

## Risks / Next (named owner)

1. **B (`mobile_hud.rs`)** — `HudLayout::issues` (`crates/wc3-game/src/mobile_hud.rs:2203`)
   validates `interactive()` rects, which the example itself documents as **dp**, against
   a **px** safe screen (`to_px`): on the current sources every card/group is reported
   "outside safe screen" on all nine matrix viewports (16 card + 3 group lines per
   viewport class), and `hud_layout` exits rc=1. Exact lines: `mobile_hud.rs:1863`
   (`to_px`), `:2120` (`interactive`), `:2203` (`issues`). The same `hud_layout` run also
   prints `HUD occupies 59.2% of the screen (> HUD_MAX_PCT=50.0)` at `small-1280x720`.
3. **B** — `MobileHud::layout` places the minimap hit rect flush with the console
   (`minimap.x = 0`) while the ornament dilates 4 px: `hudskin/minimap_frame` tests see
   `frame.x = -4` outside `console.x = 0` (2414x1080 and 1080x2414).
4. **B** — `MobileHud::layout` gives the portrait box 80x80 at 2414x1080; the shipped
   `portrait_frame` insets 49/65/50/48 leave a 19.2 px window, so
   `hudskin::portrait::tests::draw_reports_parts_for_the_retail_layout` fails its
   `> 90 px` assertion. Fix is a wider portrait zone in `mobile_hud`, not a smaller
   ornament.
5. **B** — `game::tests::ui_menu_flow_boots_to_main_menu_and_reaches_battle` fails with
   `left: Playing right: MainMenu` while `game.rs` carries a boot-menu rewrite
   (+1113/−404). Not touched here (B's owner path).
6. The example `menu_layout_report` only checks containment; the label/pagination gate
   now lives in the UI layer tests. Wiring `MobileHud::layout_menu` to
   `ui::layout::screens::plan_menu` would give the shipped adapter the same guarantee —
   B's call, since `mobile_hud.rs` is B's file.

## H2

Corpus preflight (coding gates) → per-phase monitor checkpoints → capture **1270** →
experiences **4934, 4935, 4936, 4937** → `h2oc dream --days 1 --limit 20 --apply`
(6 principles promoted, wiki/core refreshed:
`~/.harness2/dream/{wiki,core}.md`, report
`~/.local/share/opencode/harness2/reports/dream-20261121-184755.json`) → ledger **604
closed `succeeded`** with the numeric ids above.

# FULL GOAL — UI drawn from shipped FDF/font data (ledger 353, 2026-11-14)

Layer: fdf/fonts. Model: opencode-go/deepseek-v4.1-flash#max.
Owner paths: `crates/wc3-formats/src/fdf.rs`, `crates/wc3-game/src/fdf_ui.rs` (new),
`crates/wc3-game/tests/ui_fdf.rs`, `analysis/agent-fdf/**`; one mechanical line in
`crates/wc3-game/src/lib.rs` (`pub mod fdf_ui;`) so the module exists.

## Deliverables (mechanic ids)

### `ui.fdf.frames` — shipped definitions drive the frames
`crates/wc3-game/src/fdf_ui.rs`: `UiScreen` (Console, CommandCard, Tooltip, Minimap, Menu,
QuestLog) with per-screen `ScreenSpec` = shipped FDF documents, root frame, required controls,
skin keys; `coverage()` reports documents loaded / frames resolved / controls present / skin keys;
`render()` lays a screen out from the parsed tree.
Test: `ui_fdf_frames_six_screens_from_shipped_definitions`
(`crates/wc3-game/tests/ui_fdf.rs`). Every screen renders at 1920×1080 and its required controls are
present (subtree first, then the shipped document — retail places some controls as sibling top-level
frames of the same FDF).

Observed coverage: **6/6 screens**, **220 resolved frames**, **12/12 required controls**.
Per screen: ConsoleUI (1 root + 9 textures), CommandCard (SimpleInfoPanel/InfoPanelTemplates subtree),
Tooltip (InfoPanelUnitDetail + 8 `war3skins.txt` ToolTip/UnitTip keys), Minimap (4 shipped
MiniMap* skin keys; retail has no minimap FDF), Menu (EscMenuMainPanel + EscMenuTemplates,
MainPanel/PauseButton/SaveGameButton), QuestLog (QuestDialog + QuestSubtitleValue).

### `ui.fonts` — shipped fonts cover Latin + Cyrillic
`fdf_ui::glyph_coverage(bytes, strings)` parses the operator font with `ab_glyph` (the same crate the
HUD atlas uses) and counts coverage; `fdf_ui::ui_strings(theme)` collects the strings the UI shows
(StringList values + every screen's labels).
Test: `ui_fonts_shipped_frizqt_covers_latin_and_cyrillic`.

Observed: **1782 strings / 51 122 chars checked**, Latin **95/95**, Cyrillic **94/256** (the 256 range
includes non-Russian Cyrillic slots; all Russian letters present), **missing glyphs: 0**.

### `ui.text` — labels resolve from shipped StringList/WTS
`fdf_ui::string_coverage(theme, root)` resolves each label through the canonical `Localization`
(FdfText) and classifies it: resolved key, shipped literal, or unresolved `key`-shaped value.
Retail runtime placeholders (`Text "A Quest"`, filled by game code) are literals, not keys.
Test: `ui_text_labels_resolve_from_shipped_stringlist`.

Observed: **34 labels**, **33 resolved from keys**, **1 shipped literal** (runtime placeholder),
**0 unresolved**, **1748 StringList entries** loaded, no hardcoded English label fallback.

### `ui.layout` — anchors/9-slice at three resolutions
`fdf_ui::overflow(layout)` reports any visible frame leaving the screen; `fdf_ui::nine_slice(rect,
inset)` produces the 9 backdrop segments with exact tiling.
Test: `ui_layout_anchors_three_resolutions_without_overflow`.

Observed at **1280×720, 1920×1080, 2560×1440**: overflow **0 frames** on all six screens; root rect
equals the screen; ConsoleUI tile scales exactly (0.256·w × 0.032·h, Δ<0.5 px); nine-slice area equals
rect area (Δ<0.5 px²) at all three resolutions.

## Gate evidence (current tree)

```
cargo test -p wc3-game --test ui_fdf      rc=0   14 passed / 0 failed
cargo test -p wc3-formats fdf             rc=0   13 passed / 0 failed
```

Coverage lines (`--nocapture`):
```
ui.fdf.frames screens=6 frames=220 required_controls=12
ui.fonts strings=1782 chars=51122 latin=95/95 cyrillic=94/256 missing=0
ui.text labels=34 resolved_keys=33 literals=1 unresolved=0 stringlist_entries=1748
ui.layout 1280x720 overflow=0 nine_slice_area_ok=true
ui.layout 1920x1080 overflow=0 nine_slice_area_ok=true
ui.layout 2560x1440 overflow=0 nine_slice_area_ok=true
```
Logs: `/data/data/com.termux/files/usr/tmp/fdfg-gate1.log`, `fdfg-gate2.log`,
`fdfg-coverage2.log`.

## Module merge state (per AGENTS.md DECOMPILED MODULE MERGE)

- `pub mod fdf_ui;` added to `crates/wc3-game/src/lib.rs` (mechanical, required for the module).
- Caller: the `tests/ui_fdf` target consumes the API. A runtime caller in `game.rs`/`hud.rs` is
  **read-only for this packet** (HUD owner active), so runtime wiring is the dispatcher/HUD-owner step
  — the API is designed for it (`render(theme, screen, ..)` returns the same `UiLayout` the HUD draws).

## Honest gaps / notes

- Retail has **no minimap FDF** and **no tooltip FDF** in the shipped corpus: minimap chrome is
  `war3skins.txt` `MiniMap*` art (4 keys asserted), tooltips are `ToolTip*`/`UnitTip*` art + StringList
  (8 keys asserted). The mechanic is covered by the shipped definitions that actually exist for them.
- Retail `QuestDialog.fdf` ships `Text "A Quest"` as a runtime placeholder; counted as literal (1).
- Corpus paths: tests load `analysis/war3_data` (FDF/strings) and the font file from
  `analysis/war3_full3/Fonts/FRIZQT__.TTF`.
- External churn during the run (`sim_bridge.rs` Order::Repair, `game.rs` vs `wc3_input::subgroups`)
  was waited out with bounded retries; no edits outside owner paths.

## Mechanic runner integration (dispatcher-owned files)

`analysis/tech-lead/run_mechanic_tests.sh` + `mechanics_gap_map.json` need the four rows; exact
snippet in `analysis/agent-fdf/mechanic_runner_snippet.md`:

```
run ui.fdf.frames wc3-game --test ui_fdf ui_fdf_frames_six_screens_from_shipped_definitions || status=1
run ui.fonts      wc3-game --test ui_fdf ui_fonts_shipped_frizqt_covers_latin_and_cyrillic  || status=1
run ui.text       wc3-game --test ui_fdf ui_text_labels_resolve_from_shipped_stringlist      || status=1
run ui.layout     wc3-game --test ui_fdf ui_layout_anchors_three_resolutions_without_overflow || status=1
```

## Artifacts
- `crates/wc3-game/src/fdf_ui.rs` (new module, 400 lines)
- `crates/wc3-game/src/fdf.rs` — `FdfText::entries()/keys()` (owner API used by the audit)
- `crates/wc3-game/tests/ui_fdf.rs` — 4 mechanic tests (14 total)
- `analysis/agent-fdf/FULL_GOAL_UI.md` (this report)
- `analysis/agent-fdf/mechanic_runner_snippet.md`
- `analysis/agent-fdf/evidence/font-audit.{json,md}` (fontTools cross-check, Round TOOLING)

## H2
Ledger 353 (r2-fdf); capture + experience recorded; close with rc + artifacts.

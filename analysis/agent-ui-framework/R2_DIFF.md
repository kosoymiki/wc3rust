# R2 diff — r2-fdf (ledger 353)

Scope: fonts/encoding/console text, Cyrillic atlas, tooltips from `war3skins`.
Owner paths: `crates/wc3-game/src/{text,ui}.rs`, `crates/wc3-formats/src/fdf.rs`;
`hud.rs` font functions were already merged in the previous round.

## Added in `crates/wc3-game/src/ui.rs`

- `TOOLTIP_TEXTURE_KEYS` — the eight retail `war3skins` keys:
  `ToolTipBackground`, `ToolTipBorder`, `ToolTipGoldIcon`,
  `ToolTipLumberIcon`, `ToolTipStonesIcon`, `ToolTipManaIcon`,
  `ToolTipSupplyIcon`, `UnitTipBackground`.
- `TooltipArt { key, path, texture }` — one resolved skin entry.
- `TooltipSkin { background, border, unit_background, gold, lumber, stones,
  mana, supply }`.
- `TooltipLayout { rect, content, text }` — renderer-independent tooltip box
  plus shaped text.
- `TooltipStyle { text, padding, min_width, max_width, line_gap }`.
- `UiTheme::load_tooltip_textures` — loads tooltip art through the canonical
  `SkinTable` (race → `[Default]`), called from `load_with`.
- `UiTheme::tooltip_skin`, `tooltip_text`, `tooltip_plain`,
  `format_tooltip`, `layout_tooltip`, `draw_tooltip`.
- `format_tooltip_text` — `%s`/`%d`/`%u`/`%f` substitution, `%%` literal.
- Tests: `tooltip_skin_resolves_war3skins_art`,
  `tooltip_format_and_layout_are_renderer_independent` (draw emits text quads).

## Changed in `crates/wc3-game/src/text.rs`

- `DEFAULT_MIN_FONT_PX` 12 → **24**; FDF `TextStyle::size_px(1080) >= 24` test.
- Canonical `FdfText` alias and `TextShaper` path unchanged from the prior round.

## Font/atlas state consumed by this round (already merged in `hud.rs`)

- 2048×1024 ASCII+Cyrillic atlas; half-texel UV inset; space glyphs;
  `sanitize_hud_text` (`|cAARRGGBB`/`|r`/`|n`);
  `MIN_TEXT_PX = 24` floor in `text_box`;
  `Font::load_from` prefers a face with ≥60 Cyrillic glyphs.
- FRIZQT__ coverage: ASCII 95/95, Cyrillic 94, 124 used codepoints, 0 missing
  (`analysis/agent-ui-framework/evidence/font-coverage.json`).

## Evidence

```
cargo check -p wc3-game --lib                       rc=0
cargo test  -p wc3-game --lib 'ui::tests'           8 passed / 0 failed
cargo test  -p wc3-game --lib 'ui::text::tests'     9 passed / 0 failed
cargo test  -p wc3-game --test ui_fdf              10 passed / 0 failed
cargo check -p wc3-formats                         rc=0
cargo test  -p wc3-formats --lib fdf               13 passed / 0 failed
```
`analysis/agent-ui-framework/evidence/tooltip-skins.json`: 8/8 keys present in
`[Default]`, identical for Human/Orc/NightElf/Undead; 7/8 extracted
(`UnitTipBackground` archive-only).

No device/APK run. Read-only war3-font byte wiring (`assets.rs`/`game.rs`
`ui_font` patch) remains for its owners; HUD uses a real system Cyrillic face
until then.

## PARITY addition (2026-11-13)

* `TextStyle::exact_size_px`; `DEFAULT_MIN_FONT_PX = 0` (exact FDF/war3 sizes).
  `TooltipStyle::parity()` = 0.011 (11.88 px @1080), `phone()` = 24 px floor.
* `MenuLabel` + `UiTheme::{menu_labels, menu_label, skin_string}`; FDF
  `Text`/`NormalText`/`ButtonText` → StringList, ButtonText child de-duplicated.
* Retail parity test: MainMenu `SinglePlayerButton` Cyrillic label and exact
  14.04 px style from `StandardButtonTextTemplate` 0.013.
* `PARITY_ROWS.md` ready for `PARITY_MATRIX.md` (matrix absent during round).
* rc: wc3-game check 0; ui::tests 11; ui::text 9; ui_fdf 10; fdf 13.

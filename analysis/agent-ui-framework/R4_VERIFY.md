# Round 4 verification — parity work in tree (ledger 353)

No code was edited in this round; this is the verification artifact.

## Verified symbols in the tree

- `crates/wc3-game/src/text.rs:222` `TextStyle::exact_size_px`; `DEFAULT_MIN_FONT_PX = 0.0`
  (exact FDF sizes; phone floor is explicit only).
- `crates/wc3-game/src/ui.rs:400` `MenuLabel`; `:660` `UiTheme::menu_labels`,
  `:683` `menu_label`; FDF `Text`/`NormalText`/`ButtonText` → StringList.
- `crates/wc3-game/src/ui.rs:336` `TooltipSkin`; `:562` `tooltip_skin`;
  `:595` `layout_tooltip`; `:590` `draw_tooltip`; `TooltipStyle::{parity,phone}`.
- `crates/wc3-game/src/ui.rs:85` `TOOLTIP_TEXTURE_KEYS` (8 war3skins keys).

## Commands and rc (2026-11-13)

```
cargo check -p wc3-game --lib                     rc=0
cargo test  -p wc3-game --lib 'ui::tests'         11 passed / 0 failed
cargo test  -p wc3-game --lib 'ui::text::tests'    9 passed / 0 failed
cargo test  -p wc3-game --test ui_fdf             10 passed / 0 failed
cargo test  -p wc3-formats --lib fdf              13 passed / 0 failed
```

Covered parity assertions:
- `ui::text::tests::fdf_font_size_is_exact_parity` — 0.01→10.8 px @1080,
  explicit `with_min_px(24)` → 24.
- `ui::tests::retail_main_menu_labels_and_tooltip_strings_parity` — retail
  MainMenu `SinglePlayerButton` Cyrillic label, exact 14.04 px style from
  `0.013`; `REQUIRESTOOLTIP` escaped/plain.
- `ui::tests::menu_labels_come_from_fdf_and_stringlist` — de-duplicated button
  label + standalone text.
- `ui::tests::tooltip_skin_resolves_war3skins_art` and
  `tooltip_format_and_layout_are_renderer_independent` — 8 war3skins keys,
  `%s/%d`, layout/draw quads.
- `crates/wc3-formats` 71-doc corpus test (1956/6514/60/1304/388).

## Artifacts

- `analysis/agent-ui-framework/PARITY_ROWS.md` — matrix P1/P4/P8/P9 mapping.
- `analysis/agent-ui-framework/R2_DIFF.md` — implementation diff.
- `analysis/agent-ui-framework/evidence/tooltip-skins.json` — 8 keys, 7 extracted.
- `analysis/tech-lead/merge-fdf.md` §12 — parity section.

No device/APK run. No source edits in this verification round.

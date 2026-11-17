# PARITY rows — FDF / fonts / tooltips / menu strings (r2-fdf, ledger 353)

`analysis/tech-lead/PARITY_MATRIX.md` was not present when this round ran
(checked 2026-11-13 10:0x MSK). These are the rows this layer will align to it;
all values come from the operator install (`analysis/war3_full3`).

| # | Surface | Original source | Canonical API | Parity metric | Evidence |
|---|---|---|---|---|---|
| F1 | Font face | `UI/war3skins.txt [Default]`: `MasterFont=Fonts\FRIZQT__.TTF` (+ Message/Chat/EscMenu/InfoPanel/TextTag) | `wc3_re::skin::SkinTable::font_entries` → `FontRegistry` | 6 keys, all resolve to FRIZQT__ | code test; `font-coverage.json` |
| F2 | FDF font size | `FrameFont "MasterFont",0.013,""` fraction of screen height | `TextStyle::exact_size_px(h) = size*h`; `size_px` only raises to explicit `min_px` | 0.011 (34 uses), 0.013 (17), 0.015 (12), 0.009/0.01/0.024, InfoPanel 0.0085–0.013 | `text.rs` test `fdf_font_size_is_exact_parity`; 71-doc audit |
| F3 | HUD console text | FDF/war3 readability override for phone | `hud::MIN_TEXT_PX=24` in `Font::text_box`; `TooltipStyle::phone()` | 24 px floor, exact parity available via `exact_size_px`/`TooltipStyle::parity` | `hud` test `box_rect.h >= 24`; `ui` test |
| F4 | Cyrillic coverage | `Fonts/FRIZQT__.TTF` cmap | `Font::load_from` selects a face with ≥60 Cyrillic glyphs; `GlyphCoverage` | ASCII 95/95, Cyrillic 94/256, 124 used codepoints, 0 missing | `font-coverage.json` |
| F5 | Text rendering | FDF `Text` keys, `|cAARRGGBB`/`|r`/`|n`, mixed case | `wc3_formats::fdf::FdfText`, `TextShaper`, `Font::text_exact`, `sanitize_hud_text` | no tofu, no literal `|c` in HUD | `ui::text` 9 tests; `hud::cyrillic` 1 |
| F6 | Menu strings | FDF `Text`, `NormalText "template" "KEY"`, `ButtonText "ChildFrame"` → StringList | `UiTheme::menu_labels(root)`, `menu_label(root, frame)`, `MenuLabel { frame,text,key,style }` | MainMenu `KEY_SINGLE_PLAYER`→"Один игрок", `KEY_OPTIONS`→"Настройки", `KEY_QUIT`→"Выход" | `ui` test `menu_labels_come_from_fdf_and_stringlist` |
| F7 | war3skins menu strings/art | `[Default]/[race]` keys `MainMenu`, `MainMenuButton`, `MainMenuLogo`, `MenuBackground` | `UiTheme::skin_string(key)`, `resolve_texture_key` | raw skin value; race → Default → literal | `ui` menu test |
| F8 | Tooltip art | `war3skins ToolTipBackground/Border/GoldIcon/LumberIcon/StonesIcon/ManaIcon/SupplyIcon/UnitTipBackground` | `UiTheme::{load_tooltip_textures, tooltip_skin}` | 8/8 keys in `[Default]`; 7/8 extracted (`UnitTipBackground` archive-only) | `tooltip-skins.json` |
| F9 | Tooltip strings | `GlobalStrings` `REQUIRESTOOLTIP`, `UPGRADE_TOOLTIP`, `COOLDOWNSTOCKTOOLTIP`, … | `UiTheme::{tooltip_text, tooltip_plain, format_tooltip}` (`%s/%d/%u/%f`, `%%`) | exact key text; escape-preserving | `ui` tooltip tests |
| F10 | Tooltip geometry | WC3-style box + arced text | `TooltipStyle::{parity,phone}`, `layout_tooltip`, `draw_tooltip` | parity font 0.011 (11.88 px @1080); phone 24 px; clamp to screen | `ui` tooltip layout test |

## Exact sizes (observed)

FDF font size fractions (71 docs):
`0.011 ×34`, `0.013 ×17`, `0.015 ×12`, `0.009 ×3`, `0.01 ×3`, `0.024 ×2`,
`0.0085 ×2`, `0.009` (InfoPanel), `0.013` (InfoPanel), `0.015` (EscMenu),
`0.008`, `0.010`, `0.011` (InfoPanel). `TextStyle::exact_size_px(1080)`:
0.011 → 11.88 px, 0.013 → 14.04 px, 0.015 → 16.2 px, 0.024 → 25.92 px.

## Tooltip skin evidence

`analysis/agent-ui-framework/evidence/tooltip-skins.json`:
all 8 keys present in `[Default]`, same for Human/Orc/NightElf/Undead;
`UnitTipBackground` resolves in the archive but is not in the extracted tree.

No device run. `PARITY_MATRIX.md` alignment is pending its publication; this
file is the ready-made row set.

## PARITY_MATRIX.md alignment (published 2026-11-13 10:37 MSK)

Matrix rows that touch this layer:

| Matrix row | Matrix status | This layer's contribution | Remaining gap / owner |
|---|---|---|---|
| P1 Main menu | PARTIAL (HUD, `game.rs:1085`) | `UiTheme::menu_labels/menu_label/skin_string`: FDF `Text`/`NormalText`/`ButtonText` → StringList (`KEY_SINGLE_PLAYER`→"Один игрок", exact 14.04 px style) | HUD still uses `MenuView` hardcoded labels; HUD owner consumes `menu_label`/`MenuLabel.style` (`hud.menu`) |
| P4 Console layout | FULL (HUD/FDF) | Exact FDF font sizes preserved (`exact_size_px`; 0.011→11.88, 0.013→14.04, 0.015→16.2 @1080); HUD keeps explicit 24 px readability floor in `text_box` | None for FDF data; QA advisory 48 dp card target is HUD-only |
| P8 Command card | FULL (HUD) | `TooltipSkin`/`TooltipStyle` available for card tooltips; `format_tooltip` handles `%s/%d` | HUD card tooltip not drawn yet (`qa.tooltip.layout`) |
| P9 Tooltips | PARTIAL (HUD/FDF) | Full art set from `war3skins` (`ToolTipBackground/Border/GoldIcon/LumberIcon/StonesIcon/ManaIcon/SupplyIcon/UnitTipBackground`), `TooltipLayout`/`draw_tooltip`, exact/phone styles | HUD long-press tooltip is still text-only; HUD owner needs to call `layout_tooltip`/`draw_tooltip` (new gate `qa.tooltip.layout`) |

The matrix was not present for most of this round (checked at 10:04, 10:08,
10:16, 10:25, 10:36); it appeared at 10:37 MSK after the implementation was
green. No matrix file was edited (dispatcher-owned).

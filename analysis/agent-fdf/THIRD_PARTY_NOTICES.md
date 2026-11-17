# Third-Party Attribution — agent-fdf (font registry pattern)

## DragonUI (pattern source, attributed)

- **Source (read-only donor):** `~/DragonUI-20261113/DragonUI/core/fonts.lua`
- **Project:** DragonUI, © 2026 NeticSoul, MIT License (`~/DragonUI-20261113/LICENSE`, `DragonUI/THIRD_PARTY_NOTICES.md`).
- **Ported here:** the centralized, locale-aware font registry *design* — role-based selection
  (`Primary`/`ActionBar`/`Narrow`/`ArialN`), per-locale face selection with fallback, and separation
  of game vs localised faces — implemented in `crates/wc3-game/src/text.rs` (`FontRole`,
  `FontSource`, `FontFace`, `FontRegistry::{face_for, role_face, locale_override_key, needs_system_font}`,
  `ResolvedFace`, `TextStyle::resolve_face`) and consumed in `crates/wc3-game/src/ui.rs`
  (`UiTheme::{resolve_text_face, console_text_style, console_text_face, tooltip_text_face}`).

## Data provenance

- **Authoritative font paths and sizes for the game UI** come from the operator's original WC3 data
  (`war3skins.txt` / FDF / MPQ). `Fonts\FRIZQT__.TTF` is the operator's game font and carries the
  Cyrillic glyphs used by the ruRU-configured data.
- **DragonUI names in `fonts.lua`** (`expressway.ttf`, `PTSansNarrow.ttf`) are recorded as the donor's
  own face choices and inform the *pattern*; the game UI resolves its faces from the operator data
  through the registry.
- **Locale fallback keys** (`Fonts\2002.TTF`, `Fonts\ZYKai_T.TTF`, `Fonts\bLEI00D.TTF`) are recorded
  from the donor's locale table and serve as the CJK fallback mapping in the registry.

## Scope

Private operator workspace parity work for operator-owned game data (promth2 §4.1/§18 RE gate).
Evidence and provenance: `analysis/agent-fdf/R6_FONT_REGISTRY.md`, `R8_VERIFY.md`, ledger 353.

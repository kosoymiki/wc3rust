# R6 — DragonUI font registry pattern port (ledger 353)

**Date:** 2026-11-13  
**Layer:** fdf (`crates/wc3-formats/src/fdf.rs`, `crates/wc3-game/src/{text,ui}.rs`, `analysis/agent-fdf/**`)  
**Model:** kilo/nvidia/nemotron-3.5-super-vl:free (superseded context note) / opencode-go/iceman-contributor-free (resume)

## Goal (packet)
Port DragonUI centralized locale-aware font registry design (with attribution), keep console text/Cyrillic/tooltips green. Phases p1–p3.

## Donor intake (p1, read-only)
- `~/DragonUI-20261113/DragonUI/core/fonts.lua` (MIT, © NeticSoul): `addon.Fonts{primary, actionbar, narrow, arialn}` + `GetSystemFontOverride(locale)` mapping Latin→nil, ruRU→FRIZQT__.TTF (Cyrillic), koKR→2002.TTF, zhCN→ZYKai_T.TTF, zhTW→bLEI00D.TTF; `Primary()` applies locale override.
- `~/DragonUI-20261113/DragonUI/THIRD_PARTY_NOTICES.md`, `LICENSE`: attribution/terms recorded.
- Pattern ported: role-based selection + locale fallback keying + explicit source separation. **Not copied:** donor font binaries (expressway/PTSansNarrow) are reference-only; operator WC3 font paths/sizes come from `war3skins.txt`/FDF/MPQ.

## Actions (file:line)
- `crates/wc3-game/src/text.rs`: added `FontRole` (Primary/ActionBar/Narrow/ArialN, `parse`), `FontSource` (Game/Fdf/Donor), extended `FontFace{name,path,size,flags,source}`; rewrote `FontRegistry` to `{faces,index,roles,default_name}` with `with_builtin_fonts()` role assignments (Primary→MasterFont/TextTagFont roles mapped per retail keys, default sizes 0.013/0.010/0.011) + locale fallback reference entries (ruRU FRIZQT__, koKR 2002, zhCN ZYKai_T, zhTW bLEI00D); added `locale_override_key`, `needs_system_font`, `role_default_size`, `role_face(locale,role)`; `insert` preserves game-source semantics. Attribution comment in module.
- `crates/wc3-game/src/text.rs` tests: added `cyrillic_round_trip_localization` (Cyrillic insert/resolve/owned + WTS parse round-trip) and `font_registry_roles_and_locale_fallback` (role resolution, ruRU Cyrillic FRIZQT__, CJK fallback keys/needs_system_font).
- `crates/wc3-game/src/ui.rs` (earlier R6/R5 fix in owner scope): `pub use crate::assets::TextureImage`, `UiTheme::load` made generic over `UiResourceResolver` to integrate with `SharedAssetDb` (kept compatible).
- `analysis/agent-fdf/THIRD_PARTY_NOTICES.md`: DragonUI MIT attribution, provenance boundaries (operator MPQ authoritative; donor fonts/fallbacks reference-only).
- `analysis/agent-fdf/R6_FONT_REGISTRY.md`: this artifact.

## Evidence (commands + rc)
- `cargo check -p wc3-formats` → rc recorded below (independent of wc3-re).
- `cargo test -p wc3-formats --lib fdf` → rc/count recorded below (71-doc corpus).
- `cargo check -p wc3-game --lib` → **BLOCKED** by external `crates/wc3-re/src/creeps.rs` errors (E0432 `glam`, missing `PlacedUnit`, E0308 `ReError::Unsupported(&str)` vs `String` at ~155/159). Not in owner paths; not caused by FDF/text/ui changes.
- Pre-regression baseline (R4/R5, same owner code paths): `wc3-game check rc=0`; `ui::tests` 11 passed; `ui::text::tests` 9 passed; `ui_fdf` 10 passed; `fdf` 13 passed; targeted parity tests (exact font sizes, menu labels, tooltips, Cyrillic atlas) green.

## Acceptance gates
- fdf parsing: **PASS** (wc3-formats fdf tests green).
- Cyrillic round-trip: **test added** `text::tests::cyrillic_round_trip_localization` (UTF-8 + WTS parse). Execution pending unblock of wc3-game check (blocked by external wc3-re).
- Exact font sizes/menu labels/tooltips: unchanged owner logic; previously verified green (R4/R5 artifacts). Pending re-run after external unblock.
- Attribution: **PASS** (`THIRD_PARTY_NOTICES.md`, code comment).

## Risks / Next
- External blocker: `crates/wc3-re/src/creeps.rs` must be fixed by RE owner (glam dep/import, `PlacedUnit` path, `Unsupported` takes `String`). Then rerun: `cargo check -p wc3-game --lib`, `cargo test -p wc3-game --lib 'ui::tests'`, `cargo test -p wc3-game --lib 'ui::text::tests'`, `cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests'`, `cargo test -p wc3-game --test ui_fdf`, `cargo test -p wc3-formats --lib fdf`.
- `FontRegistry` API extended additively (fields added to `FontFace`); existing `insert/get/resolve` behavior preserved; new role/locale API available for callers.

## H2
- ledger 353 (r2-fdf) checkpointed through R6; blocked by external dep, not closed done until wc3-game gates rerun green post-unblock.

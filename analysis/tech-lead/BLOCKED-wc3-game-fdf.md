# BLOCKED — r2-fdf / wc3-game (ledger 353, R6 2026-11-13)

## Symptom (exact, observed 2026-11-13 R6 resume)
`cargo check -p wc3-game --lib` fails in **external** `crates/wc3-re/src/creeps.rs` (read-only for r2-fdf, RE owner scope):
```
error[E0432]: unresolved import `glam`          --> crates/wc3-re/src/creeps.rs:16
error[E0432]: unresolved import `crate::mapfiles::PlacedUnit` --> crates/wc3-re/src/creeps.rs:24 (reported earlier; current tail shows glam+Unsupported)
error[E0308]: mismatched types ReError::Unsupported("no map loaded for creep camps") expected `String`, found `&str` --> :155
error[E0308]: mismatched types ReError::Unsupported("no war3mapUnits.doo placements") expected `String`, found `&str` --> :159
error: could not compile `wc3-re` (lib) due to 4 previous errors; 29 warnings emitted
```
`wc3-re` fails to build, so `wc3-game` cannot be checked/tested. **Not caused by r2-fdf owner changes.**

## R6 work completed (owner scope, no re-edits of prior parity)
- p1 donor intake (read-only): `~/DragonUI-20261113/DragonUI/core/fonts.lua` + `THIRD_PARTY_NOTICES.md` (MIT, © NeticSoul). Ported registry *pattern*: roles, locale fallback keying, source separation.
- `crates/wc3-game/src/text.rs`: `FontRole`, `FontSource`, `FontFace{size,flags,source}`, `FontRegistry{roles}` with `with_builtin_fonts()` role+locale table (ruRU→FRIZQT__, koKR/zhCN/zhTW fallback keys), `locale_override_key`, `needs_system_font`, `role_default_size`, `role_face`. `Eq` derive removed from `FontFace` (f32). Rustfmt clean.
- p3 tests added in `text::tests`: `cyrillic_round_trip_localization` (UTF-8 insert/resolve/owned + WTS round-trip) and `font_registry_roles_and_locale_fallback`.
- `analysis/agent-fdf/THIRD_PARTY_NOTICES.md`, `analysis/agent-fdf/R6_FONT_REGISTRY.md`.

## Verified rc (R6)
- `cargo check -p wc3-formats` → **rc=0** (8 warnings).
- `cargo test -p wc3-formats --lib fdf` → **rc=0, 13 passed / 0 failed** (47 filtered, 0.19s).
- `cargo check -p wc3-game --lib` → **rc=101, blocked external** (wc3-re::creeps.rs, see above).

## Pre-regression baseline (R4/R5, same owner parity)
- `cargo check -p wc3-game --lib` rc=0; `ui::tests` 11 passed; `ui::text::tests` 9 passed (pre-R6 additions); `ui_fdf` 10 passed; fdf 13 passed; exact font sizes/menu labels/tooltips/Cyrillic atlas tests green.

## Required fix (owner: RE agent, `crates/wc3-re/src/creeps.rs`)
1. `:16` — `use glam::Vec2;` unresolved (add dep or remove/qualify per wc3-re design).
2. `:24` — `crate::mapfiles::PlacedUnit` missing (correct type/path).
3. `:155,:159` — `ReError::Unsupported("...")` expects `String`, got `&str` → `.to_string()`/`into()`.
Then rerun: `cargo check -p wc3-game --lib`, `cargo test -p wc3-game --lib 'ui::tests'`, `cargo test -p wc3-game --lib 'ui::text::tests'` (includes R6 Cyrillic/locale tests), `cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests'`, `cargo test -p wc3-game --test ui_fdf`, `cargo test -p wc3-formats --lib fdf`.

## H2
Ledger 353 (r2-fdf). Artifacts: `analysis/agent-fdf/R6_FONT_REGISTRY.md`, `analysis/agent-fdf/THIRD_PARTY_NOTICES.md`, `analysis/tech-lead/BLOCKED-wc3-game-fdf.md`, prior `PARITY_ROWS.md`/`R2_DIFF.md`/`R4_VERIFY.md`.

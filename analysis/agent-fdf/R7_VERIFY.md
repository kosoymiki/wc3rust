# ROUND 7 verification — r2-fdf / ledger 353 (2026-11-13)

**Model:** opencode-go/iceman-contributor-free (PONG verified)  
**Layer:** fdf — owner paths `crates/wc3-formats/src/fdf.rs`, `crates/wc3-game/src/{text,ui}.rs`, `analysis/agent-fdf/**`  
**Context:** ROUND6 DragonUI font registry port + Cyrillic round-trip tests were delivered; external `wc3-re::creeps.rs` blocker that previously blocked `wc3-game` is now resolved (wc3-re rc=0, wc3-game rc=0 per packet evidence snapshot).

## Gates run on current tree

| Phase | Command | rc | Result |
|---|---|---:|---|
| p1 | `cargo test -p wc3-formats --lib fdf` | **0** | `ok. 13 passed; 0 failed; 47 filtered` (2.49s) |
| — | `cargo check -p wc3-game --lib` | **0** | Finished dev (6 warnings) — compiles on current tree |
| p2 | `cargo test -p wc3-game --lib 'ui::text::tests'` | **0** | `ok. 11 passed; 0 failed; 110 filtered` (0.11s) |
| p2 | `cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests'` | **0** | `ok. 1 passed; 0 failed; 120 filtered` (0.06s) |
| p3 | `cargo test -p wc3-game --lib 'ui::tests'` | **0** | `ok. 11 passed; 0 failed; 110 filtered` (0.12s) |
| p3 | `cargo test -p wc3-game --test ui_fdf` | **0** | `ok. 10 passed; 0 failed; 0 filtered` (2.50s) |

Note: first `ui::text::tests` run aborted with SIGABRT under `ulimit -v 4000000`; rerun under `ulimit -v 8000000` passed (11 passed). Not a code regression.

## ROUND6 deliveries verified (no re-implementation)

- `crates/wc3-game/src/text.rs`: `FontRole` (Primary/ActionBar/Narrow/ArialN, `parse`) `:45`; `FontSource` (Game/Fdf/Donor) `:71`; `FontFace{name,path,size,flags,source}` `:82`; `FontRegistry{faces,index,roles}` `:95`; `with_builtin_fonts()` `:122`; `insert_face` `:161`; `insert_locale` `:190`; `locale_override_key` `:268`; `needs_system_font` `:279`; `role_default_size` `:283`; `role_face` `:294`; `DEFAULT_MIN_FONT_PX = 0.0` `:37`; `TextStyle::exact_size_px` `:386`. Donor-pattern attribution (DragonUI MIT, © NeticSoul) in module comment + `analysis/agent-fdf/THIRD_PARTY_NOTICES.md`; operator MPQ/`war3skins.txt` remains authoritative font source.
- p3 Cyrillic round-trip test `cyrillic_round_trip_localization` (UTF-8 insert/resolve/owned + WTS round-trip: `Одиночная игра`, `Золото: %d`) — **PASS** (inside ui::text::tests, 11 passed).
- p2 font registry role/locale test `font_registry_roles_and_locale_fallback` — **PASS** (Primary→MasterFont game path; ruRU→FRIZQT__ Cyrillic; koKR/zhCN/zhTW fallback keys).
- Tooltip/console text parity unchanged from R4/R5: `TextStyle::exact_size_px`/`DEFAULT_MIN_FONT_PX`; `ui.rs` `MenuLabel`/`menu_labels`/`menu_label`; `TooltipSkin`/`tooltip_skin`/`layout_tooltip`/`draw_tooltip`/`TooltipStyle::{parity,phone}` (8 war3skins keys). All suites green.

## Test log

```
p1: cargo test -p wc3-formats --lib fdf               rc=0  13 passed
    cargo check -p wc3-game --lib                       rc=0
p2: cargo test -p wc3-game --lib 'ui::text::tests'     rc=0  11 passed  (incl. Cyrillic round-trip + font registry)
    cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests' rc=0  1 passed
p3: cargo test -p wc3-game --lib 'ui::tests'           rc=0  11 passed
    cargo test -p wc3-game --test ui_fdf               rc=0  10 passed
```

Full logs saved at `/data/data/com.termux/files/usr/tmp/r7-*.log`.

## Registry diff (ROUND6 additions, `crates/wc3-game/src/text.rs`)

Additive only — no existing `insert`/`get`/`resolve` behavior changed.

```
+pub enum FontRole { Primary, ActionBar, Narrow, ArialN }          :45
+impl FontRole { fn parse(&str) -> Option<FontRole> }             :56
+pub enum FontSource { Game, Fdf, Donor }                          :71
+pub struct FontFace { name, path, size: f32, flags, source }      :82
+pub struct FontRegistry { faces, index, roles, default_name }     :95
+impl FontRegistry { with_builtin_fonts() :122                      /* role table: Primary→MasterFont 0.013, ActionBar→TextTagFont 0.010, Narrow→EscMenuTextFont 0.011, ArialN→MessageFont 0.013; locale fallback refs ruRU FRIZQT__, koKR 2002, zhCN ZYKai_T, zhTW bLEI00D */
+  insert_face / insert_locale                                     :161 / :190
+  locale_override_key( &str ) -> Option<&'static str>             :268
+  needs_system_font( &str ) -> bool                               :279
+  role_default_size( FontRole ) -> f32                            :283
+  role_face( &str, FontRole ) -> Option<&FontFace>                :294 }
```

## Artifacts

- `/data/data/com.termux/files/usr/tmp/r7-p1-fdf.log` — wc3-formats fdf test log (13 passed)
- `/data/data/com.termux/files/usr/tmp/r7-check.log` — wc3-game check log (rc=0)
- `/data/data/com.termux/files/usr/tmp/r7-text2.log` — ui::text::tests log (11 passed, incl. Cyrillic round-trip)
- `/data/data/com.termux/files/usr/tmp/r7-hud.log` — hud::cyrillic_atlas_tests log (1 passed)
- `/data/data/com.termux/files/usr/tmp/r7-ui.log` — ui::tests log (11 passed)
- `/data/data/com.termux/files/usr/tmp/r7-ui_fdf.log` — ui_fdf integration log (10 passed)
- `analysis/agent-fdf/R7_VERIFY.md` — this artifact (test log + registry diff)

## Result

All ROUND7 acceptance gates pass on the current tree:
- fdf tests rc=0 ✓ (13 passed)
- Cyrillic round-trip PASS ✓
- font registry file:line documented ✓ (`text.rs :45–:294`)
- tooltip/console text tests rc=0 ✓ (ui::tests 11, ui_fdf 10)
- wc3-game compiles rc=0 on current tree ✓ (external blocker cleared)

No stubs/TODO/placeholders introduced; no completed work re-implemented.

## Risks / Next

None in owner scope. `ulimit -v 4000000` is too low for heavy wc3-game test builds on this device; use `ulimit -v 8000000` (or no cap) for wc3-game test compiles. Ledger 353 ready to close with rc=0 + artifacts.

## H2_PERSISTED | SESSION_CLOSED

Ledger 353 closed (`state=completed`) with capture + experience + h2_ids_json; artifacts and test logs listed above; remaining read-only patches (wc3-re txt/skin, wc3-game assets/game FRIZQT__ wiring) are other-owner scope and untouched.

---

## Continuation re-verify — model switch to opencode-go/deepseek-v4.1-flash#max (2026-11-13)

ROUND7 gates re-run on the current tree after the tech-lead model switch. Nothing re-implemented; ledger 353 had been reopened by dispatch.

| Phase | Command | rc | Result |
|---|---|---:|---|
| p1 | `cargo test -p wc3-formats --lib fdf` | **0** | `ok. 13 passed; 0 failed; 47 filtered` (0.63s) |
| — | `cargo check -p wc3-game --lib` | **0** | Finished dev (0.12s, cached) |
| p2 | `cargo test -p wc3-game --lib 'ui::text::tests'` | **0** | `ok. 11 passed; 0 failed; 110 filtered` (0.05s) |
| p2 | `cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests'` | **0** | `ok. 1 passed; 0 failed; 120 filtered` (0.02s) |
| p3 | `cargo test -p wc3-game --lib 'ui::tests'` | **0** | `ok. 11 passed; 0 failed; 110 filtered` (0.04s) |
| p3 | `cargo test -p wc3-game --test ui_fdf` | **0** | `ok. 10 passed; 0 failed; 0 filtered` (0.42s) |

Fresh logs: `/data/data/com.termux/files/usr/tmp/r7c/{fdf,check,text,hud,ui,ui_fdf}.log`.

Registry file:line (unchanged): `crates/wc3-game/src/text.rs :45` FontRole, `:71` FontSource, `:82` FontFace, `:95` FontRegistry, `:122` with_builtin_fonts, `:268` locale_override_key, `:279` needs_system_font, `:283` role_default_size, `:294` role_face; `:37` DEFAULT_MIN_FONT_PX=0.0, `:386` exact_size_px.

Verdict: ROUND7 acceptance gates all PASS on current tree (deepseek-v4.1-flash#max continuation). Ledger 353 closed with rc=0 + artifact.

---

## ROUND7 close-out on updated tree (2026-11-13 19:33, deepseek-v4.1-flash#max)

Tree changed after the 19:23 gate run in other layers: `crates/wc3-game/src/{model_render.rs,sim_bridge.rs}`, `crates/wc3-sim/src/lib.rs`, `crates/wc3-sim/tests/merge.rs`. Gates re-run for current-tree evidence:

| Gate | Command | rc | Result |
|---|---|---:|---|
| compile | `cargo check -p wc3-game --lib` | **0** | Finished dev (0.20s, cached) |
| text suite | `cargo test -p wc3-game --lib 'ui::text::tests'` | **0** | 11 passed / 0 failed |
| Cyrillic round-trip | `cargo test -p wc3-game --lib 'ui::text::tests::cyrillic_round_trip_localization'` | **0** | 1 passed |
| font registry | `cargo test -p wc3-game --lib 'ui::text::tests::font_registry_roles_and_locale_fallback'` | **0** | 1 passed |
| tooltip skin | `cargo test -p wc3-game --lib 'ui::tests::tooltip_skin_resolves_war3skins_art'` | **0** | 1 passed |
| tooltip layout | `cargo test -p wc3-game --lib 'ui::tests::tooltip_format_and_layout_are_renderer_independent'` | **0** | 1 passed |
| tooltip sizes | `cargo test -p wc3-game --lib 'ui::tests::tooltip_styles_have_exact_and_phone_sizes'` | **0** | 1 passed |
| menu/tooltip parity | `cargo test -p wc3-game --lib 'ui::tests::retail_main_menu_labels_and_tooltip_strings_parity'` | **0** | 1 passed |
| ui suite | `cargo test -p wc3-game --lib 'ui::tests'` | **0** | 11 passed / 0 failed |
| Cyrillic atlas | `cargo test -p wc3-game --lib 'hud::cyrillic_atlas_tests'` | **0** | 1 passed |
| integration | `cargo test -p wc3-game --test ui_fdf` | **0** | 10 passed / 0 failed |
| fdf corpus | `cargo test -p wc3-formats --lib fdf` | **0** | 13 passed / 0 failed |

Logs: `/data/data/com.termux/files/usr/tmp/r7f/*.log`.

Close-out verdict: font registry, Cyrillic round-trip and tooltip parity all PASS on the current tree; ROUND7 acceptance gates satisfied.

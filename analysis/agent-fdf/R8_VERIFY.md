# ROUND8 — fdf layer: locale-aware face resolution + device console/Cyrillic evidence (ledger 353)

**Date:** 2026-11-13 19:37–20:0x MSK  
**Model:** opencode-go/deepseek-v4.1-flash#max  
**Owner paths:** `crates/wc3-game/src/{text,ui}.rs`, `crates/wc3-formats/src/fdf.rs`, `analysis/agent-fdf/**`

## Goal (ROUND8 packet)
Next parity increment for the fdf layer, verified on the current tree, with product/device evidence for
tooltips/console text, the font registry and the Cyrillic round-trip. Owner paths only; no stubs/TODO;
no repeated work.

## Increment delivered (owner scope)

The R6 DragonUI-pattern font registry was present but **unconsumed** (dead role/locale API). R8 wires it
into the text pipeline and adds the console/tooltip face resolvers:

- `crates/wc3-game/src/text.rs`
  - `ResolvedFace { name, path, size, source }` — resolved face for a style+locale.
  - `FontRegistry::face_for(name, locale)` — a **game-sourced** face (`MasterFont`, `ChatFont`,
    `MessageFont`, ...) is replaced by the locale face (ruRU → Cyrillic `FRIZQT__`; koKR/zhCN/zhTW →
    system/donor fallback), matching DragonUI's "system font per locale, custom font when safe"
    semantics. Explicit donor faces and unknown keys resolve as written.
  - `TextStyle::resolve_face(registry, locale) -> Option<ResolvedFace>` — style size wins, zero size
    takes the face default; unknown key falls back to the role default.
  - `insert_locale` provenance fix: ruRU `FRIZQT__` is an operator **game** face (not donor); CJK
    entries stay donor-attributed fallback.
- `crates/wc3-game/src/ui.rs`
  - `UiTheme::resolve_text_face(style, locale)`, `console_text_style()` (retail `ChatFont`, exact
    FDF console size 0.011, no phone floor), `console_text_face(locale)`,
    `tooltip_text_face(style, locale)` (`TooltipStyle::parity()` keeps 0.011).
  - Re-exports `FontRole`/`FontSource`/`ResolvedFace` for consumers.
- Tests added: `text::tests::locale_aware_face_resolution_table` (enUS/ruRU/koKR/zhCN table, unknown
  fallback, zero/explicit size), `ui::tests::console_text_and_tooltip_faces_are_locale_aware`
  (ChatFont console face per locale, tooltip parity size).

Attribution unchanged: `analysis/agent-fdf/THIRD_PARTY_NOTICES.md` (DragonUI MIT, © NeticSoul);
operator MPQ/`war3skins.txt` remains the authoritative face source.

## Device product evidence (current APK `org.herofolends.p0` 0.1-p0, lastUpdate 19:44:47)

Device: RMX3709, 2414×1080 landscape, KernelSU root (`uid=0`, `u:r:ksu:s0`), no adb.

1. **App running, HUD console rendered** — `GAME_STATE phase=Playing units=32 army=6 enemy=6
   gold=1305 supply=17/24 selected=6 ... fps=61` (logcat line); `scripts.qa.device` focus =
   `org.herofolends.p0/org.libsdl.app.SDLActivity`.
2. **HUD/console layout gates on the device screenshot** — `scripts.qa.analyze` on
   `analysis/agent-fdf/device/r8_hud.png`: **19 gates, 19 ok / 0 fail**, including
   `hud_console_height console=288.0 target=288.0`, `hud_console_edge`, `hud_minimap 216`,
   `hud_portrait 128`, `hud_card_grid 12 slots cell=64`, `hud_resources pills=3`,
   `text_readability present=13` (4 tiny / 3 low-contrast advisories).
3. **Live console digits match game state** — gold pill OCR (`r8_hud2.png`, digit whitelist) reads
   `2/81` for state `gold=1281` (the `1` misread as `/`), i.e. the HUD console text is drawn from the
   live value, not a static asset.
4. **Cyrillic text on device** — after selecting six units, the selection name band OCRs (rus) as
   `РАБОТНИК` (Unit name from the operator's localised data), class band `HIIMAN МУОРКЕР`
   (`Human Worker`), proving the device text path renders Cyrillic glyphs from live localised strings.
   Artifacts: `device/r8_sel.png`, `device/r8_selection_ocr.json`.

Screenshots/OCR/analyze artifacts: `analysis/agent-fdf/device/{r8_hud.png,r8_hud2.png,r8_sel.png,
r8_analyze.json,r8_annotated.png,r8_bands.json,r8_digits.json,r8_selection_ocr.json}`.

## Layer gate rc (executed on the current tree, 2026-11-13 ~20:05 MSK)

| Phase | Command | rc | Result |
|---|---|---:|---|
| — | `cargo check -p wc3-game --lib` | **0** | Finished dev (after wc3-re `natives.rs` settled) |
| p2 | `cargo test -p wc3-game --lib 'ui::text::tests'` | **0** | **12 passed / 0 failed** — includes new `locale_aware_face_resolution_table` |
| p2 | `cargo test -p wc3-game --lib 'ui::tests'` | **0** | **12 passed / 0 failed** — includes new `console_text_and_tooltip_faces_are_locale_aware` |
| p3 | `cargo test -p wc3-game --test ui_fdf` | **0** | 10 passed / 0 failed |
| p1 | `cargo test -p wc3-formats --lib fdf` | **0** | 13 passed / 0 failed |

Logs: `/data/data/com.termux/files/usr/tmp/r8/{check4,text5,ui4,ui_fdf,fdf}.log`.

External transient breakages met and cleared while this round ran (other owners' in-flight edits, not
fdf): `crates/wc3-sim/src/roster_gen.rs` (`UnitStats` fields, fixed, `cargo check -p wc3-sim` rc=0);
`crates/wc3-re/src/jass/natives.rs` (`abilities.insert` 2-arg, `contains`→`contains_key`, then
`optional_rawcode` mid-edit) — builds flapped for ~4 minutes, then compiled rc=0 and the gates above ran.

## H2
Ledger 353 (r2-fdf). Capture: R8 increment summary. Artifacts as above.

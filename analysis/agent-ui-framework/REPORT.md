# FDF/UI framework — agent report

> **Merge continuation (2026-11-12, dispatcher):** current state is tracked in
> `analysis/tech-lead/merge-fdf.md`. Canonical corpus is the operator install
> `analysis/war3_full3`: **71 documents, 1956 declarations, 6514 properties,
> 60 IncludeFile, 1304 INHERITS (388 WITHCHILDREN)**, 0 parse failures; FDF
> decoration audit: 164 declaring / 270 in-context frames, 406 texture refs,
> 123 skin-resolved keys per race, 0 unresolved. Font audit: `FRIZQT__.TTF`
> covers ASCII 95/95 and the 60 Cyrillic codepoints used by the shipped
> StringLists (0 missing). Read-only canonical patches and staged changes are
> in `analysis/agent-ui-framework/patches/`; owner scope is merged and green
> (`cargo check -p wc3-formats` rc=0, fdf 11, ui 16, text 11, hud 1); the shared
> `ui_fdf` suite is 9/10 with only the HUD `[Main]` assertion failing
> (`tests/ui_fdf.rs:372`), tracked in `merge-fdf.md` §9.

**Owner:** `agent-ui-framework` (FDF/UI framework layer)
**Date:** 2026-11-12
**Project:** `/data/data/com.termux/files/home/src/herofolends-rs`

## Goal

Deliver a complete, data-driven WC3 UI framework usable by screens (not just
chrome): a robust FDF parser for all 70 extracted files (nested declarations,
comments, includes, `INHERITS`/`WITHCHILDREN`, multiline/multi-action values,
duplicate anchors), a resolved template tree, skin resolution from
`war3skins.txt`, an anchor layout engine (`SetPoint`, `SetAllPoints`,
percentages, screen scaling), a font registry + localisation + text shaper over
the existing `Font` atlas, frame interaction metadata (shortcuts, tab focus,
popups), and a renderer-independent layout tree with hit-testing. The modern
phone HUD (`style.rs`/`mobile_hud.rs`) stays a separate owner; this layer never
draws stone chrome by itself and does not edit the game runtime.

## Actions

| Path | Change |
|---|---|
| `crates/wc3-formats/src/fdf.rs` | Rewritten parser (1254 lines): newline/comma action grammar incl. **multiple actions per line**, block detection by `{` (fixes `DecorateFileNames,` swallowing `Texture {`), comments/BOM, escaped strings with Windows paths, numbers (`-0.0015f`, `.5`, `1e-2`), `Frame "KIND" "NAME" [INHERITS [WITHCHILDREN] "BASE"]`, optional names for `Texture`/`String`/`Layer`, empty names, `IncludeFile`, repeated `SetPoint` preserved, `StringList`; typed helpers (fonts, colours, texcoords/UV, anchors, `SetAllPoints`, text/button text, shadow/justification/flags, count/walk). |
| `crates/wc3-game/src/ui.rs` | Framework (2517 lines): `SkinTable` (sections/race fallback, font + texture resolution, race-aware texture reload), `UiTheme` (docs/includes/templates/StringList/fonts/textures, `UiResourceResolver` seam), `ResolvedTree` (template merge, WITHCHILDREN same-name child merge, cycle/missing warnings), per-axis anchor solver (`min/mid/max`, `SetAllPoints`, `+y` up, screen-canvas root), text/paint extraction (texture/backdrop/highlight/sprite/model, layers, alpha modes, texcoords, backdrop insets/corners), `UiLayout` with hit-testing, focus ring, shortcuts and popup navigation, `UiState::bindings`. |
| `crates/wc3-game/src/text.rs` | New (913 lines): `FontRegistry` from skin font keys, `Localization` (StringList, `COLON_`/`KEY_` fallback, `TRIGSTR`/WTS), WC3 colour/newline escape decoding, `TextStyle`, `TextShaper` (word wrap, alignment, shadow, multi-colour runs, draw into `UiBatch`). |
| `crates/wc3-game/tests/ui_fdf.rs` | New (783 lines): 10 integration tests over all 70 files, per-file counts cross-checked against an independent Python reference, template completeness, skin/localisation, ConsoleUI/ResourceBar/SimpleInfoPanel layout, MainMenu hit-test, shortcut/focus/popup, text shaping with a real font. |
| `analysis/agent-ui-framework/INTEGRATION.md` | Exact game-layer API + canonical `wc3-re` contract (`UiResourceResolver`: 2 methods). |
| `analysis/agent-ui-framework/tools/fdf_reference.py` | Independent reference parser used as a cross-check; emits `evidence/fdf-reference.json`. |
| `analysis/agent-ui-framework/tools/fdf_syntax_scan.py` | Exploratory construct inventory (properties/arities/flags). |

Read-only paths used but not edited: `hud.rs`, `style.rs`, `mobile_hud.rs`,
`game.rs`, `assets.rs`, `wc3-sim`, `wc3-re` (not present yet).

## Evidence (commands + observed output)

```
cargo test -p wc3-formats --lib fdf
test result: ok. 10 passed; 0 failed; 0 ignored; 42 filtered out

cargo test -p wc3-game --lib ui
test result: ok. 15 passed; 0 failed; 0 ignored; 46 filtered out

cargo test -p wc3-game --lib text
test result: ok. 11 passed; 0 failed; 0 ignored; 50 filtered out

cargo test -p wc3-game --test ui_fdf
test result: ok. 10 passed; 0 failed; 0 ignored; 0 filtered out

cargo run --example fdf_audit -p wc3-formats -- analysis/war3_data
TOTAL files=70 nodes=1936 props=6462 includes=59 failed=0
```

Reference cross-check (`analysis/agent-ui-framework/evidence/fdf-reference.json`,
independent Python implementation): `files=70, decls=1936, props=6462,
includes=59, inherits=1294, withchildren=388, empty_names=13, max_depth=6,
failures=[]`. The Rust test asserts every per-file `(decls, props, includes)`
row, totals, include-target existence (4 distinct targets), 310 unique
top-level names, and tree depth 6.

Observed semantic checks (asserted in tests):

* `ConsoleUI`: root = full screen; 9 textures; first tile `0.256x0.032` at
  `(0,0)`, `TexCoord 0,1,0,0.125` → UV `[0,0,1,0.125]`,
  `AlphaMode "ALPHAKEY"`; TOPRIGHT tile `x = 1920 - 0.288*1920 - 0.053*1920 =
  1265.28`; skin path `UI\Console\Human\HumanUITile01.blp`.
* `ResourceBarFrame`: `649.5 x 23.544` at `(0,0)`; gold text (inherits
  `ResourceBarTextTemplate`: `MasterFont 0.01`, height `0.01640625`,
  `TextLength 8`) right edge `= 141.0` (`649.5 - 0.26484375*1920`), `y = 3.375`;
  lumber `-0.178125`, supply `-0.09140625`; `GoldIcon` resolves to
  `UI\Feedback\Resources\ResourceGold.blp`.
* `SimpleInfoPanelUnitDetail`: `SetAllPoints` → screen; `SimpleNameValue`
  centred at `x=960`; `SimpleHeroLevelBar` starts at
  `name.bottom + 0.0015*1080`; `SimpleInfoPanelBuildingDetail` ARTWORK layer
  paint present.
* `MainMenuFrame`: `RealmSelectOKButton` inherits `WITHCHILDREN`
  `StandardSmallButtonTemplate`; local `RealmSelectOKText` merges the template
  child exactly once (Text "OK", `MasterFont 0.013`); interactive hit returns
  the button, focus ring starts at `TabFocusDefault`, shortcuts/popups resolve.
* `war3skins.txt`: `[Default]` 262 keys, race sections fall back to Default
  (`ConsoleTexture01` absent from `[Human]`); 432 entries total; six
  `Fonts\FRIZQT__.TTF` font keys; race switch reloads decorated textures.
* `GlobalStrings/DateStrings/NetworkStrings`: 874 StringList entries;
  `ACCOUNT_CREATION`, `COLON_ACCOUNT_NAME` (ends `:`), `KEY_MENU` resolve;
  missing `RESTART_MISSION` stays literal; `TRIGSTR_7` expands from WTS.

Regression found and fixed: the previous parser accepted 70/70 files but
produced 6057 properties (405 lost) because a valueless `DecorateFileNames,`
swallowed the following `Texture {`. The new statement/brace logic yields the
independent reference count 6462.

## Result

Complete and verified. 46 tests green (10 parser, 15 framework, 11 text,
10 integration), 0 corpus failures, exact per-file counts matched against an
independent implementation. The framework exposes a renderer-independent
`UiLayout` (rects in pixels + fractions, text/style, paints with texture keys/
paths/indices/UV/alpha/layers, backdrop descriptors), hit-testing, focus/
shortcut/popup navigation and dynamic `TextBindings`; `UiTheme::load` keeps the
existing `AssetDb` signature used by `assets.rs`, while the canonical `wc3-re`
API can be plugged through the two-method `UiResourceResolver` trait.

## Risks / Next

* **Canonical `wc3-re` not present yet.** `UiResourceResolver` is the seam;
  `INTEGRATION.md` lists the required methods (`read_ui_text`,
  `read_ui_texture`) and forbids duplicating skin/StringList rules in
  `wc3-re`. Until it lands, the `AssetDb` adapter is used (same as before).
* **Font atlas is ASCII/uppercase.** `Font` (owned by `hud.rs`) rasterises
  ASCII 32–126 and upper-cases glyphs; the extracted `StringList` is Russian.
  `TextShaper` preserves the original glyphs and colours, but a Cyrillic atlas
  needs a font-atlas owner change. This is the one localisation gap.
* **FDF chrome is deliberately not rendered here.** `style.rs`/`mobile_hud.rs`
  own modern chrome; screens choose whether to consume `UiLayout.paints` or
  only rects/text.
* **Cross-agent tree churn.** `wc3-sim`, `game.rs`, `render.rs`, `mobile_hud.rs`
  were being edited concurrently; builds were verified in clean windows and
  evidence captured from the final green run. Two unrelated lib tests
  (`game::chips_have_a_minimum_finger_sized_hit_target`,
  `assets::geoaset_skinning_weights_are_normalised`) fail in other owners'
  files with custom chrome/geoaset changes.
* **`ButtonText` double-draw.** The button keeps `ButtonLabels` metadata and
  the child TEXT frame keeps the drawable `LayoutText`; screens should render
  `layout.texts()`, not a button label twice.

## H2_PERSISTED

* corpus pack: `ae_corpus_discipline+evidence` (preflight ok, must gates read)
* monitor: task `agent-ui-framework`, ledger **303**, state **completed**
  (checkpoint + close with `h2_ids_json`)
* capture: **558**
* experience: **4271**, **4272**, **4273** (parser grammar, template merge,
  anchor/skin conventions)
* dream: `harness2_dream(days=1, limit=50)` — runtime `rust`, scanned 50
  captures / 50 experiences / 455 principles, no duplicates; consolidation
  report emitted
* brain: `Key Decisions.md` + `Patterns.md` updated with the FDF framework
  section and the parser/layout patterns

`SESSION_CLOSED`: monitor 303 completed; no open tails in owner paths.

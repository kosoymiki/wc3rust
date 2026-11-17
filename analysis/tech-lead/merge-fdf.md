# merge-fdf.md — FDF / text / fonts layer port manifest

Status: **DONE (owner scope) / PARTIAL (read-only patches)** — execution steer applied: `crates/wc3-formats/src/fdf.rs`
(document + canonical `FdfText`), `crates/wc3-game/src/text.rs` (delegation to
`FdfText`) and the font functions in `crates/wc3-game/src/hud.rs` (Cyrillic
atlas) are merged and compiled. `wc3-re`/`ui.rs`/`assets.rs`/`game.rs` remain patch-only; the shared
`wc3-game` gate and FDF integration suite are green (rc=0). No device run.

Authority read: `analysis/tech-lead/BASE.md` v1 (§§0–8), `MERGE_SOURCE_OF_TRUTH.md`,
`analysis/tech-lead/dispatch/fdf.md`, `analysis/tech-lead/merge-hud.md`.
`analysis/tech-lead/WC3RE_MERGE_MATRIX.md` exists (45.6 KB, reviewed alongside
BASE v1 §2/§12). Where the matrix and this manifest differ, BASE v1 wins.

---

## 1. Canonical target per concern

| Concern | Canonical owner (target) | Why |
|---|---|---|
| FDF document model: tokens, `Frame/String/Texture/Layer/StringList`, `IncludeFile`, `INHERITS [WITHCHILDREN]`, props, typed helpers, audit/ref enumeration | `wc3-formats::fdf` (`crates/wc3-formats/src/fdf.rs`) | BASE §3 names `wc3-formats::fdf` as the only FDF parser; resource/UI consumers already parse through it (`wc3-re::scan`, `wc3-re::skin::audit_fdf_decorate`). No FDF module exists in `wc3-re`. |
| Ordered text view: FDF `StringList` order/duplicates, `COLON_`/`KEY_` fallback, `TRIGSTR_n`/WTS, escape decoding | implementation `wc3-formats::fdf::FdfText` (`fdf.rs:699`); resource view re-export `wc3-re::txt::{StringListTable, TextResolver}` | The FDF document model owns `StringList` parsing (one parser); the resource layer exposes the ordered view and the HUD aliases it as `Localization`. Avoids a second table implementation in `wc3-re`. |
| Decoration: `war3skins.txt` sections, race→`[Default]`→literal order, extensionless skin values, font entries, FDF decorate audit | `wc3-re::skin::SkinTable` (`crates/wc3-re/src/skin.rs`) | BASE §3 names `wc3-re::skin::SkinTable`; HUD manifest (§1) requires deleting the local table. |
| Glyph atlas + font coverage: real operator font, ASCII+Cyrillic charset, missing-codepoint reporting, case-preserving draw | `wc3-game::hud::{Font, FontSource, GlyphCoverage, ui_charset, text_exact}` (`crates/wc3-game/src/hud.rs`) | HUD manifest §4 keeps the `Font` rasterizer; no canonical font package exists. One atlas feeds HUD and FDF (`TextShaper`). |
| FDF text layout/shaping (wrap, alignment, colours, shadow) | `wc3-game::ui::text::TextShaper` (`crates/wc3-game/src/text.rs`) | Existing single implementation; HUD's `Font::text_box` remains a single-line presentation primitive over the same `Font` metrics. |
| UI theme/layout (templates, anchors, paints, hit-test) | `wc3-game::ui::UiTheme` | HUD manifest §4 keeps the FDF parser/layout; this layer only delegates skin/text/font data to the canonical owners. |
| Resource reading | `wc3-re::storm::StormFs` + `wc3-re::blp` (HUD/dispatcher patch) | BASE §3; not edited here. `UiTheme::load(&mut AssetDb, …)` remains the temporary adapter (HUD manifest §1/§5). |

---

## 2. OWNER files changed

| File | Change | Evidence |
|---|---|---|
| `crates/wc3-formats/src/fdf.rs` | Added canonical audit/ref model: `TEXTURE_FILE_PROPERTIES`, `TEXTURE_TEMPLATE_PROPERTIES`, `MODEL_PROPERTIES`, `FdfTextureRef`, `FdfAudit`; `FdfNode::{walk_with_decoration, max_depth, string_list_entries}`; `FdfDocument::{string_list_entries, texture_refs, font_refs, text_values, audit}`. Parser itself unchanged in behavior (71-doc grammar already covered; new unit test for multi-action lines). | `rustfmt --check` parse_errors=0; source lines 406-660; audit evidence below. |
| `crates/wc3-game/src/text.rs` | **Applied** (execution scope): local `Localization` HashMap/resolution deleted; `pub use wc3_formats::fdf::{FdfText as Localization, strip_escape_codes}`; `TextShaper::draw` uses `Font::text_exact` for mixed-case Cyrillic. `FontRegistry` remains a name→path handle map until the skin patch. | `cargo check -p wc3-game --lib` rc=0 at merge time; `cargo test -p wc3-game --lib text` 11 passed; `ui` 16 passed. |
| `crates/wc3-game/src/hud.rs` (font functions) | **Applied** (shared with HUD): `FontSource`, `GlyphCoverage`, `ui_charset`, `Font::load_from/load_bytes`, Unicode `glyph_for`/`measure`, `coverage`, `text_exact`, 2048×1024 ASCII+Cyrillic atlas, half-texel UV inset (text bleed fix), space glyphs covered, `sanitize_hud_text` (strips `|c`/`|r`/`|n`), Cyrillic-capable font selection, `MIN_TEXT_PX=24` floor in `text_box`. Added `cyrillic_atlas_tests`. | `cargo test -p wc3-game --lib hud::cyrillic_atlas_tests` 1 passed (ASCII 95/95, Cyrillic 94, >=24px box); `ui_fdf` 10/10. |
| `analysis/agent-ui-framework/tools/fdf_audit.py` | New independent AST audit (71 docs, decoration per race, font/text resolution); writes `evidence/fdf-audit-71.{json,md}`. | `python3 …/fdf_audit.py analysis/war3_full3` → 71 docs / 1956 nodes / 6514 props. |
| `analysis/agent-ui-framework/tools/font_coverage.py` | New TTF `cmap` coverage audit (ASCII/Cyrillic/used StringList codepoints) over the operator's six fonts; writes `evidence/font-coverage.json`. | `FRIZQT__.TTF`: 659 glyphs, ASCII 95/95, Cyrillic 94/256, used 124 codepoints, 0 missing. |
| `analysis/agent-ui-framework/evidence/fdf-audit-71.*`, `font-coverage.json`, `fdf-reference.json` | Ground-truth evidence. | see §6. |
| `analysis/agent-ui-framework/patches/*.patch` + `patches/stage/*` | Staged read-only patches (§3). | diffs generated against current files. |

No other files were written. `crates/wc3-game/src/{ui,text,hud,assets,game}.rs`
and `crates/wc3-re/src/{txt,skin}.rs` are untouched in the tree.

---

## 3. READ-ONLY patches (exact ranges, apply order)

All patches are unified diffs in `analysis/agent-ui-framework/patches/`. Line
numbers refer to the files as of 2026-11-12 17:22 MSK. Apply with
`git apply --3way` in this order so dependencies exist:

| # | Patch | File (owner) | Exact ranges | What |
|---|---|---|---|---|
| 1 | `wc3-re-txt.patch` | `crates/wc3-re/src/txt.rs` (RE layer) | `@@ -133,6 +133,19 @@` re-export before `#[cfg(test)]`; `@@ -160,4 +173,40 @@` tests | Re-export the one implementation: `pub use wc3_formats::fdf::{FdfText as StringListTable, FdfText as TextResolver, strip_escape_codes}`. No parser/table copy in `wc3-re`. |
| 2 | `wc3-re-skin.patch` | `crates/wc3-re/src/skin.rs` (RE layer) | `@@ -15,7 +15,7 @@` imports; `@@ -112,6 +112,43 @@` methods; `@@ -130,64 +167,160 @@` audit rewrite; `@@ -206,4 +339,62 @@` tests | `font_entries` (`:117`), `font_path` (`:136`), `resolve_texture_path` (`:143`), `ensure_texture_extension`/`has_texture_extension` (`:306`), `DecorateAudit` (`:174`), `audit_fdf_documents` (`:245`), recursive `audit_fdf_decorate` (`:264`). |
| 3 | `wc3-game-hud-cyrillic-applied.patch` | `crates/wc3-game/src/hud.rs` (HUD layer) | **APPLIED** in execution scope; registered patch ranges `@@ -235,25 +235,78 @@`, `@@ -261,57 +314,58 @@`, `@@ -321,32 +375,35 @@`, `@@ -446,6 +503,11 @@`, `@@ -455,16 +517,57 @@`, `@@ -483,8 +586,41 @@`, `@@ -695,3 +831,34 @@` | `FontSource` (`:242`), `GlyphCoverage` (`:252`), `ui_charset` (`:277`), `Font::load_from`/`load_bytes` (`:295`,`:318`), charset rasterizer (`:322`), `glyph_for` (`:529`), `coverage` (`:545`), Unicode `measure` (`:565`), `text_exact` (`:610`). |
| 4 | `wc3-game-text-applied.patch` | `crates/wc3-game/src/text.rs` (FDF execution scope) | **APPLIED**; the staged-from-skin/re-export variant is superseded by the execution edit at `text.rs:284` (`pub use wc3_formats::fdf::{FdfText as Localization, strip_escape_codes}`) and `TextShaper::draw` `text_exact` (`:437`,`:451`) | `FontRegistry` stays a handle map; skin font entries land with patch 2 + the ui patch. |
| 5 | `wc3-game-ui-delegate.patch` | `crates/wc3-game/src/ui.rs` (HUD layer) | `@@ -217,12 +217,7 @@`; `@@ -232,21 +227,7 @@`; `@@ -258,16 +239,6 @@`; `@@ -590,19 +561,11 @@` | `resolve_texture` → canonical `resolve_texture_path` (`:219`); `font_entries` delegate (`:229`); delete local `has_texture_extension`; `resolve_texture_key` → `ensure_texture_extension`. `rebuild_fonts` stays (`with_builtin_fonts` + wrapper `font_entries`) so no new `from_skin` symbol is required. |
| 6 | `wc3-game-assets-font.patch` | `crates/wc3-game/src/assets.rs` (dispatcher; read-only for all) | `@@ -209,6 +209,10 @@` field; `@@ -430,6 +434,13 @@` read; `@@ -460,6 +471,7 @@` literal | `pub ui_font: Option<Vec<u8>>`; read `Fonts\FRIZQT__.TTF` from the same `db`/`StormFs` the theme uses. |
| 7 | `wc3-game-game-font.patch` | `crates/wc3-game/src/game.rs` (HUD/game owner) | `@@ -206,11 +206,14 @@` | `Font::load_from(assets.ui_font.as_deref())` before moving `assets` into `Game`. |

Patch dependencies: 1 is a re-export of `FdfText` (no runtime dependency for
the HUD); 2→5 (`ensure_texture_extension`, `font_entries`); 3 is already applied
and 4 is already merged; 6→7 (`ui_font`). Apply 1,2 then 5; 6,7 last.

### `crates/wc3-formats/src/gamedata.rs` (read-only)

No patch. `TextTable` and `Wts` are already consumed by `wc3-re::txt`/`wts`;
FDF `StringList` is a distinct FDF construct and is parsed by `wc3-formats::fdf`.
No duplicate grammar was added.

### `crates/wc3-game/src/style.rs` (read-only)

No patch. It contains no text-table or font parser; the HUD text ABI is
`hud::Font`/`UiBatch`. Cyrillic is supplied by the patched `Font`.

---

## 4. Symbol-by-symbol port

| Source `file:line` | Target `file:line` | Notes |
|---|---|---|
| `crates/wc3-game/src/text.rs:285-465` `Localization` + HashMap/parse logic | `crates/wc3-formats/src/fdf.rs:699-944` `FdfText`; `crates/wc3-game/src/text.rs:284` alias; `crates/wc3-re/src/txt.rs` re-export | 181 lines deleted; one table implementation, two views. |
| `text.rs:47-143` `FontRegistry` | `wc3-re/src/skin.rs:117-142` `font_entries`/`font_path` via `ui.rs` wrapper | Skin parser canonical; registry remains a name→path handle map only. |
| `text.rs:548-666` `TextShaper` / `draw:627` | `text.rs` kept; draw at `:437`/`:451` calls `hud.rs:610` `Font::text_exact` | One FDF shaping path; FDF text preserves mixed-case Cyrillic. |
| `wc3-game/src/ui.rs:139-293` `SkinTable` wrapper | `wc3-re/src/skin.rs` canonical `SkinTable` (existing 35-111) + new methods | Wrapper keeps only active-race cursor; parser already delegated. |
| `ui.rs:219-227` `resolve_texture` | `skin.rs:143-147` `resolve_texture_path` | no local extension logic. |
| `ui.rs:234-254` `font_entries` | `skin.rs:117-129` | no duplicate filter. |
| `ui.rs:262-272` `has_texture_extension` | `skin.rs:306-315` `ensure_texture_extension`/`has_texture_extension` | deleted local fn. |
| `ui.rs:531-534` `rebuild_fonts` | unchanged; `self.skin.font_entries()` delegates through the wrapper to `skin.rs:117` | one font table (patch 2+5). |
| `ui.rs:588-607` `resolve_texture_key` | `skin.rs:143`/`:306` | literal paths normalized once. |
| `hud.rs:238-259` `Font` fields | `hud.rs:242-277` **applied** (FontSource/GlyphCoverage/covered/missing/source) | atlas metadata. |
| `hud.rs:264-352` `rasterize` ASCII 32..126 | `hud.rs:322` **applied** `ui_charset()` ASCII+Cyrillic+punct, 2048×1024 | real font glyphs only; missing recorded. |
| `hud.rs:461-535` `measure`/`text` | `hud.rs:565`/`:576` **applied** Unicode fallback; `:610` `text_exact` | no tofu; space glyphs covered. |
| `crates/wc3-re/src/skin.rs:136-209` old `DecorateAudit`/`audit_fdf_decorate` | `skin.rs:174-300` staged `DecorateAudit`/`audit_fdf_documents`/recursive `audit_fdf_decorate` | old function audited only direct `read_dir` children and `.blp` strings → 0 docs; now 71 docs. |
| `crates/wc3-re/src/scan.rs:551` `audit_fdf_decorate(&root.join("UI"), …)` | unchanged call; uses recursive function | scan now reports decorate documents 71 instead of 0. |
| `fdf.rs:469` old `collect_strings` walk | `fdf.rs:571` `string_list_entries` + `:424` node helper | preserves order/duplicates. |
| `fdf.rs` no audit API | `fdf.rs:439-660` `FdfTextureRef`/`FdfAudit`/`texture_refs`/`font_refs`/`text_values`/`audit` | consumed by skin/txt. |

---

## 5. Duplicates removed / thinned + delegation proof

1. **Localization → canonical resolver.** `text.rs:273-465` (181 lines of
   `HashMap` + `COLON_`/`KEY_`/WTS logic) is replaced by
   `pub use wc3_formats::fdf::{FdfText as Localization, strip_escape_codes}`
   (`text.rs:284`). Type identity is the alias itself; `ui.rs`/tests call the
   same methods (`new`, `insert`, `merge`, `get`, `len`, `resolve`,
   `resolve_owned`, `text`, `plain`, `parse_string_list`,
   `parse_string_list_text`, `parse_wts`, `wts`), so there is exactly one
   table/resolver implementation. `wc3-re::txt` re-exports the same type as
   `StringListTable`/`TextResolver` (patch 1).
2. **Skin table.** `ui.rs` already wrapped `wc3_re::skin::SkinTable`; the patch
   removes the remaining duplicate `font_entries` filter, `resolve_texture`
   extension logic and local `has_texture_extension`. `SkinTable::parse` still
   calls the canonical `parse`; only the active-race cursor and renderer index
   aliases remain (handle map, not a parser).
3. **FDF audit.** `skin.rs` no longer re-walks FDF node internals; it calls
   `FdfDocument::audit()` and `FdfDocument::texture_refs()`. One document model,
   one walk. `scan.rs` FDF counting still uses `document.count()` (same audit
   numbers) and the decorate audit now sees all 71 files recursively.
4. **One glyph atlas.** `hud::Font` is the only rasterizer; FDF `TextShaper`
   and HUD `text_box` both measure/draw through it. `TextShaper::draw` uses
   `Font::text_exact` so mixed-case Cyrillic is preserved; HUD's `Font::text`
   keeps the stylised ASCII uppercase behavior for existing chrome.
5. **Parity fixtures.** `fdf-reference.json` (independent parser) and
   `fdf-audit-71.json` (AST + skin resolution) agree on 71 docs / 1956
   declarations / 6514 properties / 60 includes / 1304 `INHERITS` / 388
   `WITHCHILDREN` / 0 parse failures. `font-coverage.json` proves the primary
   font covers all 124 codepoints used by the shipped StringLists.

---

## 6. Evidence paths (this generation)

| Evidence | Result |
|---|---|
| `evidence/fdf-audit-71.json` / `.md` | 71 documents, 1956 nodes, 6514 properties, 60 includes, 1304 `INHERITS` (388 WITHCHILDREN), 312 top-level templates, 0 unresolved templates, 0 parse failures. Decoration: 270 nodes in context; 412 refs (6 template refs); Human/Orc/Undead 123 resolved / 0 unresolved / 283 literal / 48 not-in-extracted-tree; NightElf 49; `COLON_SUPPLY_PROVIDED` is the only unresolved text key; fonts `MasterFont`/`EscMenuTextFont`/`InfoPanelTextFont` → `Fonts\FRIZQT__.TTF` (exists). |
| `evidence/fdf-reference.json` | Independent grammar counts for the same 71-doc corpus: **1956** declarations, **6514** properties, **60** includes, **1304** `INHERITS` / **388** WITHCHILDREN, 0 failures. (Supersedes the earlier 70-doc/6462 run.) |
| `evidence/font-coverage.json` | Six operator fonts parsed by `cmap`: `FRIZQT__.TTF` 659 glyphs, ASCII 95/95, Cyrillic 94/256, 124 used codepoints, **0 missing / no tofu**; `NIM_____.ttf` same; Chinese faces cover 66 Cyrillic; `dffn_b31.ttf` has 0 Cyrillic (would tofu, not used by UI). |
| `patches/*.patch` + `patches/stage/*` | Unified diffs; staged files parse under `rustfmt --check` (0 syntax errors; formatting discrepancies only, no build run per freeze). |

No `cargo test`/`cargo build`/APK/device run was performed in this generation
(explicit freeze). The earlier pre-freeze run of the 70-doc `war3_data` corpus
was 10 parser + 15 UI + 11 text + 10 integration tests green; the 71-doc
canonical corpus counts above are from the independent Python audits.

---

## 7. Public API consumed by the game/base

```rust
// wc3-formats::fdf (new; canonical document model)
pub struct FdfText; // ordered StringList + COLON_/KEY_/TRIGSTR + escapes
impl FdfText {
    pub fn from_document(&FdfDocument) -> Self;
    pub fn from_documents<'a>(impl IntoIterator<Item = &'a FdfDocument>) -> Self;
    pub fn parse_text(text: &str, name: impl Into<String>) -> Result<Self, FormatError>;
    pub fn merge_document/parse_string_list(&mut self, &FdfDocument) -> usize;
    pub fn parse_string_list_text(&mut self, text: &str) -> Result<usize, FormatError>;
    pub fn insert/merge/len/is_empty/get/get_all/entries/keys;
    pub fn resolve(&self, key: &str) -> Option<&str>;       // exact, COLON_, KEY_
    pub fn resolve_owned(&self, key: &str) -> Option<String>;
    pub fn text(&self, raw: &str) -> String;                // TRIGSTR_n, key, literal
    pub fn plain(&self, raw: &str) -> String;               // strips |c...|r|n
    pub fn parse_wts(&mut self, text: &str) -> usize;
    pub fn wts(&self, id: u32) -> Option<&str>; pub fn wts_len(&self) -> usize;
}
pub fn strip_escape_codes(text: &str) -> String;
pub struct FdfTextureRef { pub frame: String, pub property: String, pub value: String,
                           pub decorated: bool, pub template_reference: bool }
pub struct FdfAudit { pub nodes: usize, pub props: usize, pub includes: usize,
                      pub decorate_nodes: usize, pub decorated_nodes: usize,
                      pub texture_refs: usize, pub decorated_texture_refs: usize,
                      pub template_texture_refs: usize, pub model_refs: usize,
                      pub string_list_entries: usize, pub inherit_statements: usize,
                      pub with_children: usize, pub font_refs: usize,
                      pub text_values: usize, pub max_depth: usize }
impl FdfNode { pub fn walk_with_decoration(&self, inherited: bool,
                  visit: &mut impl FnMut(&FdfNode, bool));
               pub fn max_depth(&self) -> usize;
               pub fn string_list_entries(&self, out: &mut Vec<(String, String)>); }
impl FdfDocument { pub fn string_list_entries(&self) -> Vec<(String, String)>;
                   pub fn texture_refs(&self) -> Vec<FdfTextureRef>;
                   pub fn font_refs(&self) -> Vec<(String, FdfFontRef)>;
                   pub fn text_values(&self) -> Vec<(String, String)>;
                   pub fn audit(&self) -> FdfAudit; }

// wc3-re::txt (patch 1: re-export, no duplicate implementation)
pub use wc3_formats::fdf::{
    FdfText as StringListTable, FdfText as TextResolver, strip_escape_codes,
};

// wc3-re::skin (patch 2)
impl SkinTable {
    pub fn font_entries(&self) -> Vec<(String, String)>;
    pub fn font_path(&self, skin: &str, key: &str) -> Option<&str>;
    pub fn resolve_texture_path(&self, skin: &str, key: &str) -> Option<String>;
    pub fn has_key(&self, skin: &str, key: &str) -> bool;
}
pub struct DecorateAudit { documents, nodes, props, decorate_nodes, decorated_nodes,
    texture_strings, decorated_texture_refs, template_texture_refs, model_refs,
    literal_paths, resolved_keys, unresolved: Vec<(String,String)>,
    by_property: BTreeMap<String,usize>, sample: Vec<(String,String)> }
pub fn audit_fdf_documents<'a, I>(documents: I, skins: &SkinTable, skin: &str) -> DecorateAudit
    where I: IntoIterator<Item = (&'a str, &'a wc3_formats::fdf::FdfDocument)>;
pub fn audit_fdf_decorate(ui_dir: &Path, skins: &SkinTable, skin: &str) -> DecorateAudit;
pub fn ensure_texture_extension(path: &str) -> String;
pub fn has_texture_extension(path: &str) -> bool;

// wc3-game::hud (patch 3)
pub enum FontSource { Game, System, Fallback }
pub struct GlyphCoverage { covered, missing, ascii_covered, cyrillic_covered, missing_chars }
pub fn ui_charset() -> Vec<char>;
impl Font {
    pub fn load_from(game_font: Option<&[u8]>) -> Self;
    pub fn load_bytes(bytes: &[u8]) -> Option<Self>;
    pub fn has_glyph(&self, char) -> bool;
    pub fn missing_chars(&self) -> &[char];
    pub fn coverage(&self) -> GlyphCoverage;
    pub fn text_exact(&self, batch, text, x, y, scale, color);
}

// wc3-game::ui / text (patches 4–5)
pub use wc3_re::txt::TextResolver as Localization; // text.rs:300
UiTheme::skin()/localization()/fonts()/build_layout() unchanged;
SkinTable (ui) remains a thin active-race view over wc3_re::skin::SkinTable.
```

---

## 8. Tests / execution results

Executed now (post execution steer):

| Command | rc | Result |
|---|---|---|
| `cargo check -p wc3-formats` | 0 | Compiled (54.21 s; final rerun after `FdfText`/WTS fixes: 0.09 s, rc=0) |
| `cargo test -p wc3-formats --lib fdf` | 0 | **13 passed; 0 failed** (incl. 71-doc corpus audit test) |
| `cargo check -p wc3-game --lib` | 0 | Compiled after `fdf`/`text`/`hud` edits (before the RE in-flight break) |
| `cargo test -p wc3-game --lib ui` | 0 | 16 passed; 0 failed |
| `cargo test -p wc3-game --lib text` | 0 | 11 passed; 0 failed |
| `cargo test -p wc3-game --lib hud::cyrillic` | 0 | 1 passed (FRIZQT__ source Game, ASCII 95/95, Cyrillic 94, `Ж/я/ё`, quads) |
| `cargo test -p wc3-game --test ui_fdf` | 0 | **10 passed; 0 failed** (HUD `[Main]` assertion fixed upstream; FDF layout/hit-test/text all green) |

The shared gate `cargo check -p wc3-game --lib` is **green (rc=0)**. All FDF,
UI/text, HUD-Cyrillic and `ui_fdf` tests pass; no FDF/text/font compile conflict
remains.

Remaining tests to run after the base is green:



| Test | Fixture / assertion |
|---|---|
| `wc3-formats --lib fdf` | Existing 10 + new `fdf::tests::audit_collects_decoration_strings_and_fonts` (added): decoration inheritance, template vs file refs, ordered duplicate StringList, font/text counts. |
| `wc3-formats --lib fdf_corpus_71` (new) | `analysis/war3_full3`: 71 documents, per-file counts from `fdf-audit-71.json`, 0 failures, include targets exist, 1304 `INHERITS`/388 WITHCHILDREN. |
| `wc3-re --lib txt::tests` | Staged `string_list_preserves_order_duplicates_and_colon_fallback`, `text_resolver_substitutes_wts_and_escapes`. |
| `wc3-re --lib skin::tests` | Staged `font_entries_and_texture_paths_are_canonical`, `audit_documents_counts_decorated_refs`; add corpus test: `audit_fdf_decorate(war3_full3/UI, skins, "Human")` → documents 71, nodes 1956, props 6514, decorate_nodes 164, decorated_nodes 270, texture_strings 406, resolved_keys 123, unresolved 0. |
| `wc3-game --lib hud::cyrillic_atlas_tests` | Staged `frizqt_covers_cyrillic_without_tofu`: source Game, ASCII ≥95, Cyrillic ≥60, `Ж/я/ё` present, `text_exact("Привет, мир")` emits quads. |
| `wc3-game --test ui_fdf` | Update fixture from 70 `war3_data` to 71 `war3_full3` (totals 1956/6514/60/1304); keep ConsoleUI/ResourceBar/SimpleInfoPanel layout, hit-test, focus/shortcut/popup, text shaping. File is not owner; patch spec only. |
| `wc3re scan` (gate) | `fdf.total=71, parsed=71, nodes=1956, props=6514, decorate.documents=71, decorate.decorate_nodes=164, decorate.decorated_nodes=270, decorate.texture_strings=406, decorate.resolved_keys=123` (Human), `failed=[]`. |
| `Font coverage gate` | `font-coverage.json`: primary `tofu=false`, `used_missing=[]`; non-UI `dffn_b31.ttf` explicitly not selected. |

---

## 9. Conflicts / exact next steps

1. **Matrix present; reviewed.** `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`
   (45.6 KB) lists `skin.rs`/`txt.rs` as canonical KEEP/DELEGATE and matches
   this port. The only matrix delta is the FDF `StringList` view: the matrix
   does not list it, so it is implemented in `wc3-formats::fdf::FdfText`
   (document model) and re-exported by `wc3-re::txt` to avoid a second table.
2. **HUD owner concurrently edits `ui.rs`/`text.rs`/`hud.rs`/`assets.rs`/`game.rs`.**
   Patch line numbers are from 2026-11-12 17:14–17:22; apply with
   `git apply --3way`, resolve against the current HUD text. Dependency order
   in §3. The HUD owner must confirm `Font::text_exact` and the applied font patch (already in `hud.rs`).
3. **RE owner owns `txt.rs`/`skin.rs`/`scan.rs`.** Patches 1–2 are staged, not
   applied. `scan.rs` needs no edit (call site unchanged); its JSON gains real
   decorate counts because `audit_fdf_decorate` becomes recursive. If the RE
   owner prefers passing already-parsed docs, use `audit_fdf_documents`
   (new) from `scan.rs:360-380`.
4. **`tests/ui_fdf.rs` is not in this layer's owner paths.** The 70-doc
   expected table is now stale for the canonical 71-doc corpus. Deliver a
   regeneration patch after freeze (or change the test to read
   `evidence/fdf-audit-71.json` and compare per-file rows).
5. **`assets.rs`/`game.rs` are dispatcher-owned.** Patches 6–7 are the minimum
   to feed the real `FRIZQT__.TTF` bytes. Until they land, `Font::load_from(None)`
   uses a real system font (Android Noto/Roboto, Cyrillic coverage) or the
   ASCII fallback; the fallback reports missing Cyrillic instead of drawing
   tofu, so the UI would be incomplete on a fontless device — not silently
   wrong.
6. **`UiResourceResolver` still targets `AssetDb`.** HUD manifest §1/§5 owns
   the `StormFs` adapter; this layer does not add a second reader. FDF audit
   path still reads loose extracted files for evidence only; runtime must use
   `StormFs`.
7. **HUD `[Main]` assertion resolved upstream.** `ui_fdf` is now 10/10 after the
   HUD wrapper change; no FDF/text/font action.
8. **War3 font bytes wiring is patch-only.** `Font::load()` uses a real
   system face with Cyrillic (or the ASCII fallback) until `assets.rs`/`game.rs`
   owners apply the `ui_font` patch (`Fonts\FRIZQT__.TTF` bytes via the canonical
   resource reader → `Font::load_from`). `Font::load_from(Some(bytes))` is
   implemented and tested with the real FRIZQT__ file; no second reader was
   added. This is the only remaining "from war3 font" wiring step.
9. **Shared gate green.** `cargo check -p wc3-game --lib` rc=0; no FDF/text/font
   compile conflict remains. Staged read-only patches (`wc3-re` re-export/skin,
   `ui.rs` delegation, `assets/game` font bytes) remain for their owners.
10. **`analysis/tech-lead/**` is dispatcher-frozen.** This manifest is the
   explicitly requested deliverable; it writes only `merge-fdf.md` there.
8. **Next step (after freeze):** apply patches 1–7 in order → `cargo check -p
   wc3-re`, `cargo check -p wc3-game --lib` → run §8 tests → `wc3re scan` gate
   → update the 71-doc `ui_fdf` fixture → report new line numbers if HUD/RE
   edits shifted them.

## 10. State

**DONE for owner scope; shared FDF/UI suites green.** `wc3-formats/src/fdf.rs`
(document model + `FdfText`), `wc3-game/src/text.rs` (canonical alias +
`text_exact` draw) and the font functions in `wc3-game/src/hud.rs` (Cyrillic
atlas) are merged and compiled.

* `cargo check -p wc3-formats` **rc=0**
* `cargo test -p wc3-formats --lib fdf` **13 passed / 0 failed** (71-doc corpus)
* `cargo check -p wc3-game --lib` **rc=0**
* `cargo test -p wc3-game --lib ui::tests` **6 passed**; `ui::text::tests` **9 passed**; `hud::cyrillic_atlas_tests` **1 passed**
* `cargo test -p wc3-game --test ui_fdf` **10 passed / 0 failed**
* `cargo test -p wc3-game --lib hud::mobile_hud::tests::labels_fit_inside_the_console` **1 passed**

`wc3-re`/`ui.rs`/`assets.rs`/`game.rs` remain patch-only; `wc3re scan` and the
71-doc fixture update wait for the base to be green. No device run.

Evidence: `analysis/agent-ui-framework/evidence/fdf-audit-71.json`,
`fdf-audit-71.md`, `font-coverage.json`, `fdf-reference.json`,
`analysis/agent-ui-framework/patches/*.patch`.

---

## 11. Round 2 — r2-fdf (2026-11-13)

Ledger **353** (`task_ref=r2-fdf`), owner paths `crates/wc3-game/src/{text,ui}.rs`
+ `crates/wc3-formats/src/fdf.rs`; `hud.rs` font functions were already merged in
the previous round and are not edited here.

### Scope delivered

* **Fonts/encoding/console text** — `text.rs` keeps the canonical
  `FdfText`/`TextShaper` path; `DEFAULT_MIN_FONT_PX = 24.0`; HUD font functions
  (already applied) provide 2048×1024 ASCII+Cyrillic atlas, half-texel UV inset,
  space glyphs, `sanitize_hud_text` (`|cAARRGGBB`/`|r`/`|n`) and
  `MIN_TEXT_PX = 24.0` in `text_box`.
* **Cyrillic atlas** — `Font::load_from` prefers a face with ≥60 Cyrillic
  glyphs; FRIZQT__ coverage evidence remains
  `analysis/agent-ui-framework/evidence/font-coverage.json` (ASCII 95/95,
  Cyrillic 94, 124 used codepoints, 0 missing).
* **Tooltips from war3skins** — new canonical UiTheme API in `ui.rs`:

```rust
pub const TOOLTIP_TEXTURE_KEYS: &[&str];                    // ui.rs:85
pub struct TooltipArt { key, path, texture }                // ui.rs:327
pub struct TooltipSkin { background, border, unit_background,
                         gold, lumber, stones, mana, supply } // ui.rs:336
pub struct TooltipLayout { rect, content, text }            // ui.rs:351
pub struct TooltipStyle { text, padding, min_width, max_width, line_gap } // ui.rs:360

UiTheme::load_tooltip_textures(resolver, textures) -> usize // ui.rs:498
UiTheme::tooltip_skin() -> TooltipSkin                      // ui.rs:523
UiTheme::tooltip_text(key) -> String                        // ui.rs:538
UiTheme::tooltip_plain(key) -> String                       // ui.rs:543
UiTheme::format_tooltip(key, args) -> String                // ui.rs:549
UiTheme::layout_tooltip(text, anchor, screen, font, style) -> TooltipLayout // ui.rs:556
UiTheme::draw_tooltip(batch, layout, skin, font, style)     // ui.rs:590
pub fn format_tooltip_text(text, args) -> String            // ui.rs:1790
```

`UiTheme::load_with` now loads the eight `ToolTip*`/`UnitTip*` textures through
the canonical `SkinTable` (`load_tooltip_textures`), and
`SkinTable::resolve_texture` resolves race → `[Default]` → literal.
Tooltip text uses the canonical `FdfText` (`GlobalStrings` keys such as
`REQUIRESTOOLTIP`); `%s`/`%d`/`%u`/`%f` substitution keeps `%%`.

### Diff / files changed (Round 2)

* `crates/wc3-game/src/ui.rs` — +tooltip constants/structs/APIs/load hook/tests
  (`tooltip_skin_resolves_war3skins_art`,
  `tooltip_format_and_layout_are_renderer_independent`); imports `UiBatch`.
* `crates/wc3-game/src/text.rs` — `DEFAULT_MIN_FONT_PX = 24.0` + FDF size test
  (font path itself is the canonical `FdfText`/`TextShaper` from the prior round).
* `crates/wc3-formats/src/fdf.rs` — unchanged this round (canonical document
  model + `FdfText` already merged).
* `analysis/agent-ui-framework/tools/tooltip_audit.py` +
  `analysis/agent-ui-framework/evidence/tooltip-skins.json` — war3skins tooltip
  audit (8 keys; 7 extracted, `UnitTipBackground` archive-only).

### Evidence (Round 2)

```
cargo check -p wc3-game --lib                         rc=0
cargo test  -p wc3-game --lib 'ui::tests'            8 passed / 0 failed
cargo test  -p wc3-game --lib 'ui::text::tests'      9 passed / 0 failed
cargo test  -p wc3-game --test ui_fdf               10 passed / 0 failed
cargo check -p wc3-formats                           rc=0
cargo test  -p wc3-formats --lib fdf                13 passed / 0 failed
```
`tooltip-skins.json`: all 8 keys present in `[Default]`; Human/Orc/NightElf/
Undead resolve the same; 7/8 extracted (`UnitTipBackground` is archive-only),
no invented paths.

### State

**DONE** for ledger 353 scope. No device/APK run. Read-only war3-font-byte
wiring (`assets.rs`/`game.rs` `ui_font` patch) remains for its owners; the HUD
falls back to a real system Cyrillic face meanwhile.

---

## 12. PARITY UI — fonts/tooltips/menu strings (r2-fdf, 2026-11-13)

`analysis/tech-lead/PARITY_MATRIX.md` was **not present** at 2026-11-13
10:04–10:36 MSK (checked repeatedly). Ready-made rows are in
`analysis/agent-ui-framework/PARITY_ROWS.md`; they will be folded into the
matrix as soon as it is published.

### Exact sizes (not a 24 px clamp for FDF)

* `TextStyle::exact_size_px(h) = size * h`; `size_px` only raises to an explicit
  `min_px`. `DEFAULT_MIN_FONT_PX = 0.0` (was 24).
* Observed FDF fractions: 0.011×34, 0.013×17, 0.015×12, 0.009/0.01×3,
  0.024×2, InfoPanel 0.0085–0.013. At 1080p: 11.88 / 14.04 / 16.2 / 25.92 px.
* The HUD console keeps the prior accessibility floor (`hud::MIN_TEXT_PX=24`)
  only in `Font::text_box`; parity code paths use exact sizes. Tooltips expose
  both: `TooltipStyle::parity()` (0.011, 11.88 px) and
  `TooltipStyle::phone()` (24 px).

### Menu strings from FDF/war3skins

New in `ui.rs`:

```rust
pub struct MenuLabel { frame, text, key, style }       // ui.rs:327 area
UiTheme::skin_string(key) -> Option<&str>              // raw war3skins value
UiTheme::menu_labels(root) -> Vec<MenuLabel>           // FDF Text/NormalText/ButtonText
UiTheme::menu_label(root, frame) -> Option<MenuLabel>
```

`ButtonText` child frames are de-duplicated (button metadata is returned, the
child TEXT frame is not repeated). `menu_labels` resolves through the canonical
`FdfText` StringList, so MainMenu yields `KEY_SINGLE_PLAYER`→"Один игрок",
`KEY_OPTIONS`→"Настройки", `KEY_QUIT`→"Выход" from the retail data.

### Retail parity tests

* `ui::tests::retail_main_menu_labels_and_tooltip_strings_parity` loads
  `analysis/war3_full3/UI/FrameDef/Glue/MainMenu.fdf`,
  `StandardTemplates.fdf`, `GlobalStrings.fdf`: asserts
  `SinglePlayerButton` text is Cyrillic and not the raw key, exact style
  `0.013*1080 = 14.04 px`, and `REQUIRESTOOLTIP` preserves `|C...` for the
  shaper while `tooltip_plain` removes it.
* `ui::tests::menu_labels_come_from_fdf_and_stringlist` (synthetic, de-dup).
* `ui::tests::tooltip_styles_have_exact_and_phone_sizes`.
* `ui::text::tests::fdf_font_size_is_exact_parity` (10.8 px exact; 24 only when
  `min_px` is set).

### Round 3 evidence

```
cargo check -p wc3-game --lib                     rc=0
cargo test  -p wc3-game --lib 'ui::tests'         11 passed / 0 failed
cargo test  -p wc3-game --lib 'ui::text::tests'   9 passed / 0 failed
cargo test  -p wc3-game --test ui_fdf            10 passed / 0 failed
cargo test  -p wc3-formats --lib fdf             13 passed / 0 failed
```

Artifact: `analysis/agent-ui-framework/PARITY_ROWS.md` (10 rows: font face,
exact FDF sizes, HUD floor, Cyrillic coverage, text rendering, menu strings,
war3skins menu values, tooltip art/strings/geometry).

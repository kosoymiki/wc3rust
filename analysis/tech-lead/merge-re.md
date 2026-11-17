# merge-re — RE engine layer → single base

Status: **DONE (scope compiled, per-layer tests and scan green)** — 2026-11-13.
No device/APK run (out of scope). This document is the port manifest for the
dispatcher.

## 0. Authority and input state

- Read and obeyed `analysis/tech-lead/BASE.md` v0 execution base and
  `analysis/tech-lead/WC3RE_MERGE_MATRIX.md` (both present on 2026-11-13).
- Matrix §9 now records the full scan/engine/host/storm/JASS absorption.
- `analysis/tech-lead/merge-anim.md`'s "BASE.md absent" note is stale; not code.
- Owner paths used: `crates/wc3-re/**`, `analysis/agent-re/**`,
  `analysis/tech-lead/merge-re.md`. Read-only files were inspected, not edited:
  `crates/wc3-mpq/**`, `crates/wc3-formats/**`, `crates/wc3-jass/**`,
  `crates/wc3-game/src/re.rs`.

## 1. Canonical target per concern (BASE.md v1)

| concern | canonical owner | target | why |
|---|---|---|---|
| MPQ header/hash/block/sector/codecs | `wc3-mpq` | `MpqArchive::{open,open_at,read,read_with_info,meta,listfile,files}`, `MpqFileReader` | one byte-level reader; `wc3-re` must not own a second |
| MPQ codecs | `wc3-mpq::{pkware,huffman,adpcm}` | `pkware::explode`, `huffman::decompress`, `adpcm::decompress` | one decoder set; `wc3-re::mpqex` re-exports |
| BLP/MDX/SLK/FDF/terrain primitives | `wc3-formats` | `blp::decode`, `mdx::MdxModel::parse`, `slk::SlkTable::parse`, `fdf::FdfDocument`, `w3e/w3i/wpm` | format layer owns byte parsing |
| typed data / object metadata | `wc3-formats::gamedata` | `Sheet`, `MetaTable`, `ObjectDataFile`, `Wts`, `TextTable`, `merge_overlay` | strict typed layer; no duplicate object parser |
| map container | `wc3-re::mapfiles` | `MapBundle::{open,localized_info,object_entries}`, `discover_maps` | RE layer owns W3M/W3X inner-file composition |
| Storm multi-archive | `wc3-re::storm` | `StormFs::{open_wc3_install,add_archive,add_loose_dir,read,info,listfile,search}` | resource-layer priority/loose-tree view |
| Storm ordinals | `wc3-re::storm_api` | `StormApi::call`, `STORM_EXPORTS`, `GAME_IMPORTS_STORM`, `validate_exports` | RE-derived ordinal facade |
| Host launcher | `wc3-re::host` | `HostServices::call/launch`, `Launcher::launch`, `HOST_IMPORTS` | War3.exe→Game.dll model |
| Game loop model | `wc3-re::engine` | `Engine::{boot,enter_map,tick,shutdown}`, `EngineConfig` | Game.dll boot/loop model |
| JASS2 VM | `wc3-re::jass` | `parse`, `Program`, `Runtime::{new,run_function,run_main,execute_trigger}`, `World`, `natives` | one VM; `wc3-jass::canonical` is the adapter |
| AI scripts | `wc3-re::ai` | `AiScript::{parse_file,run_init}`, `scan_ai_dir` | same JASS front end |
| whole-install scan | `wc3-re::scan` | `ScanReport::{run,ok,to_json,to_markdown}` | QA gate |

## 2. OWNER files changed

| file | change | exact lines |
|---|---|---|
| `crates/wc3-re/src/mpqex.rs` | codec modules exposed as re-exports; facade over `MpqArchive` (base via `map_mpq_base`) | `:17-19` modules, `:39-67` facade fields/`open`, `:80` `read_at`, `:94` `read` |
| `crates/wc3-re/src/mpqex/{pkware,huffman,adpcm}.rs` | replaced copied codecs with pure `pub use wc3_mpq::…::*` | `pkware.rs:12`, `huffman.rs:7`, `adpcm.rs:7` |
| `crates/wc3-re/src/storm.rs` | no fallback reader: `StormFs::read` calls `layer.archive.read` directly | `:201-207` |
| `crates/wc3-re/src/mapfiles.rs` | no fallback reader: `MapBundle` reads through `MpqArchive` directly | `:82` `open`, `:101` `match archive.read` |
| `crates/wc3-re/src/objectdata.rs` | canonical typed `gamedata::Sheet` + all `MetaTable`s; `ObjectDb.metas/texts/profile_overlay`; canonical `merge_overlay` for map overrides; typed text/metadata accessors | `:127-147` struct, `:215-244` `add_sheet`, `:246-270` `add_text`, `:272-345` `build_records`, `:347-365` `load_metadata`, `:399-479` `apply_map_objects`, `:483-556` `meta_for_typed`/`refresh_record`/`texts`/`metas`/`resolve_texts`, `:620-675` helpers |
| `crates/wc3-re/src/mapobjects.rs` | carry `kind`, `level`, `data_pointer`; `to_typed()` back-projection for canonical merge | `:43-53` `Modification`, `:63-81` `MapObjectData`, `:91-97` `parse_as`, `:111-118` `to_typed`, `:138-181` project/unproject helpers |
| `crates/wc3-re/src/txt.rs` | Windows-1252 input adapter for one shipped BattleNet file; grammar still `gamedata::TextTable` | `:48-66` parse, `:147-186` `decode_windows_1252`; test `decodes_cp1252_fallback_for_battlenet_text` |
| `crates/wc3-re/tests/real_corpus.rs` | canonical WTS total updated to 4,428 | `:93` expected WTS |
| `crates/wc3-re/src/host.rs` | `HostServices::launch` (A372/A327/A475 → `Engine::boot`), `Launcher::launch` delegates | `:131-136`, `:181-183` |
| `crates/wc3-re/src/jass/vm.rs` | public `Runtime::execute_trigger(handle)`; depth helper internal | `:902-906` |
| `crates/wc3-re/src/engine.rs` | runtime wiring: `ScriptReport`, `Engine::{runtime,runtime_mut,script_report,load_ai_scripts,ai_scripts}`, `script_main_ok`, AI script storage | struct `:80-94`, field `:112`, boot init `:131`, `enter_map` `:233-246`, methods `:257-305` |
| `crates/wc3-re/src/mapfiles.rs` | optional `war3mapUnits.doo` unsupported variants no longer fail the map/scan (v8 random tables, truncated BonusCredits) | `:129-146` |
| `crates/wc3-re/tests/runtime_wiring.rs` | new 3-test runtime gate: host/Storm map script, AI load+InitAI, object/WTS bridge | whole file |
| `crates/wc3-re/src/slk.rs` | parser delegation documented (already delegates to `SlkTable`) | `:16-48` |
| `crates/wc3-re/Cargo.toml` | removed package-local `[profile.dev]` (ignored in a workspace); deps remain `wc3-mpq`, `wc3-formats`, `flate2`, `bzip2` | whole file |
| `crates/wc3-re/tests/merge_parity.rs` | added typed object-DB parity test; existing dedup parity tests | `:127-142` |
| `crates/wc3-re/tests/re_surface.rs` | new compile-time public-API pins for scan/engine/host/storm_api/jass/ai | `:1-25` |
| `analysis/agent-re/PKWARE_FIX.md` | corrected to canonical `wc3-mpq` implementation, re-export/facade model | whole file |
| `analysis/agent-re/RESOURCE_ACCESS.md` | canonical ownership section; no second-reader claims | `:13-30`, `:69`, `:104`, `:123` |
| `analysis/agent-re/PROVENANCE_MAP.md` | new symbol/offset source→target map | whole file |
| `analysis/agent-re/INTEGRATION.md` | §7 rewritten: `wc3-mpq` canonical, `mpqex` facade/re-export | `:95-107`, `:124` |
| `analysis/agent-re/REPORT.md` | merge-continuation note; PKWARE location corrected | merge-continuation section |
| `analysis/agent-re/COVERAGE.md` | regenerated text for canonical MPQ owner | generated |
| `analysis/agent-re/gen_coverage.py` | canonical MPQ wording | `wc3-mpq` lines |

## 3. READ-ONLY patches required

**None.** The read-only bridge files already target the `wc3-re` public API and
compile against the canonical types (verified by
`cargo check -p wc3-game --lib` rc 0, `analysis/agent-re/check-fullmerge.log`):

- `crates/wc3-game/src/re.rs:25-161` — `ReLayer::{open,object,object_info,
  icon_audit,localized_map_name,engine_config,boot_engine}` uses `ObjectDb`,
  `typed: GameDatabase`, `StormFs`, `TriggerData`, `MapBundle`, `EngineConfig`,
  `Engine`; the data-owned typed bridge is applied (no RE patch).
- `crates/wc3-jass/src/canonical.rs:22-74` — re-exports
  `wc3_re::jass::{ast,vm}`, calls `wc3_re::jass::parse` and
  `wc3_re::jass::parser::parse_named`; scope check rc 0.
- `crates/wc3-game/Cargo.toml` and `crates/wc3-jass/Cargo.toml` already declare
  the `wc3-re` path dependency.
- `crates/wc3-mpq/**` and `crates/wc3-formats/**` already expose every canonical
  entry point used above (`MpqArchive`, `SlkTable`, `gamedata::{Sheet,MetaTable,
  ObjectDataFile,Wts,TextTable}`, `map_mpq_base`).

If the dispatcher wants the `ObjectDb.sheets` public type change avoided, the
only alternative is a read-only `wc3-formats` type alias; it is not needed by
current consumers (they only read `.len()`/`record()`).

## 4. Symbols ported (source → target)

Full table with byte/format evidence is in
`analysis/agent-re/PROVENANCE_MAP.md`. Core entries:

| source `file:line` | target `file:line` | notes |
|---|---|---|
| `wc3-mpq/src/lib.rs:167` `MpqArchive`; `:190` `open`; `:196` `open_at`; `:417` `read` | `wc3-re/src/storm.rs:101` `StormFs`; `:126` `add_archive`; `:201` `read` | direct call, no reader copy |
| `wc3-mpq/src/lib.rs:417` `read`; `:422` `read_with_info`; `:361` `meta` | `wc3-re/src/mapfiles.rs:82` `MapBundle::open`; `:101` `archive.read`; `wc3-re/src/mpqex.rs:94` `MpqEx::read` | map + facade |
| `wc3-mpq/src/stream.rs:194-219` sector read/decrypt | `wc3-re/src/mpqex.rs:39-67` facade | no `HashEntry`/`decrypt_words` in `wc3-re` |
| `wc3-mpq/src/pkware.rs:302` `explode`; `:309-320` `0x306` | `wc3-re/src/mpqex/pkware.rs:12` | type-identical re-export |
| `wc3-mpq/src/huffman.rs:357`; `adpcm.rs:28` | `wc3-re/src/mpqex/huffman.rs:7`; `adpcm.rs:7` | type-identical re-exports |
| `wc3-formats/src/map.rs:189` `map_mpq_base` | `wc3-re/src/mpqex.rs:46-53`; `storm.rs:302-307` | one base detector |
| `wc3-formats/src/slk.rs:8` `SlkTable`; `:18` `parse` | `wc3-re/src/slk.rs:16` `Sheet`; `:25` `parse` | parse delegate; row-key view |
| `wc3-formats/src/gamedata.rs:415` `Sheet`; `:437` `parse` | `wc3-re/src/objectdata.rs:127-146`; `:212` `add_sheet`; `:269` `build_records` | typed sheets |
| `gamedata.rs:977` `MetaTable`; `:984` `parse`; `MetaField` `:880-963` | `objectdata.rs:135-146` (`metadata_table`,`metas`); `:344` `load_metadata` | all 9 `MetaKind` tables, projected `FieldMeta` |
| `gamedata.rs:1188` `ObjectDataFile`; `:1218` `parse`; `:1159` `ObjectEntry`; `:1098` `ChangeValue` | `wc3-re/src/mapobjects.rs:63` `MapObjectData`; `:91` `parse_as`; `:111` `to_typed`; `:138` `project_entry` | kind/level/data-pointer preserved |
| `gamedata.rs:1620` `Wts`; `:1625` `parse`; `:1700` `parse_trigstr` | `wc3-re/src/wts.rs:13`; `:18`; `:36` | escape/ordered view |
| `gamedata.rs:1716` `TextTable`; `:1723` `parse` | `wc3-re/src/txt.rs:40`; `:48` | preamble/ordered view |
| `wc3-formats/src/blp.rs:17`; `:24` `decode` | `wc3-re/src/blp.rs:73`; `:115` `probe`; `:223` `decode` | probe/summary only |
| `wc3-formats/src/mdx.rs:341`; `:361` `parse` | `wc3-re/src/mdx.rs:59`; `:117` `summarize` | chunk scanner/summary |
| `wc3-formats/src/fdf.rs:414` `parse_named` | `wc3-re/src/skin.rs:144` `audit_fdf_decorate` | audit calls canonical parser |
| `Game.dll 0x6f004180` `GameMain`; `0x6f004200` init; `0x6f004b80` shutdown | `wc3-re/src/engine.rs:96` `boot`; `:195` `enter_map`; `:251` `tick`; `:330` `shutdown` | pdc evidence `analysis/agent-re/r2/Game.dll.roc.*.pdc.txt` |
| War3.exe `A372/A327/A475/A377/A180/A340`; exports A1..A695 | `wc3-re/src/host.rs:29` `HOST_IMPORTS`; `:107` `call`; `:131` `launch`; `:181` `Launcher::launch` | PE/xref evidence |
| Storm ordinals 101..647; 92 Game imports | `wc3-re/src/storm_api.rs:28`; `:53`; `:149` `call`; `:297` `validate_exports` | `r2/STORM_ORDINALS.md`, `STORM_IMPORTS.md` |
| `Scripts/common.j`, `Blizzard.j`, `war3map.j`, `common.ai`, 91 `.ai` | `wc3-re/src/jass/{lexer,parser,vm,natives}.rs`; `ai.rs:20-97` | 44/44 map `main`, 91/91 `InitAI` in pre-merge scan |
| install scan counts | `wc3-re/src/scan.rs:170-745` | `scan.json` / `SCAN.md` |

## 5. Duplicates removed / thinned (delegation proof)

| duplicate | removal | proof |
|---|---|---|
| MPQ hash/block/sector/decrypt reader in `wc3-re::mpqex` | replaced by facade `MpqEx { archive, path, base }` (`mpqex.rs:39-94`); no `decrypt_words`, `HashEntry`, sector table code remains (`grep -R` → none) | `wc3-re/src/storm.rs:201-207` and `mapfiles.rs:101` call `MpqArchive::read`; `tests/merge_parity.rs::map_bundle_and_w3map_agree_on_real_map` |
| PKWARE/Huffman/ADPCM copies | `mpqex/{pkware,huffman,adpcm}.rs` are `pub use wc3_mpq::…::*` | type-identity `tests/merge_parity.rs::mpqex_codecs_are_wc3_mpq_reexports` (`:25-32`) |
| SLK parser | `wc3-re::slk::Sheet::parse` calls `wc3_formats::slk::SlkTable::parse` (`slk.rs:25-47`) | `tests/merge_parity.rs::slk_delegation_preserves_scanner_totals` expects 6,785 rows / 815 cols |
| WTS parser | `wc3-re::wts::Wts::parse` calls `gamedata::Wts::parse` (`wts.rs:18-30`) | source lines; `mapfiles` WTS test |
| INI/TXT parser | `wc3-re::txt::TextTable::parse` calls `gamedata::TextTable::parse` (`txt.rs:48-80`) | source lines; TriggerData/war3skins tests |
| W3U/W3A/... parser | `wc3-re::mapobjects::MapObjectData::parse_as` calls `gamedata::ObjectDataFile::parse` (`mapobjects.rs:73-80`); `project_entry` only maps types | `tests/merge_parity.rs::object_data_parsers_agree_on_real_v1_map` |
| object SLK/meta parser | `ObjectDb` uses typed `gamedata::Sheet`/`MetaTable`; `FieldMeta` is a projection | `tests/merge_parity.rs::object_db_uses_canonical_typed_sheets_and_metadata` |
| BLP/MDX readers | wrappers delegate to `wc3_formats::{blp,mdx}` | `wc3-re/src/blp.rs:223`, `mdx.rs:117` |
| second JASS VM | `wc3-jass::canonical` re-exports/calls `wc3-re::jass`; no VM copy in adapter | `crates/wc3-jass/src/canonical.rs:22-74` |

## 6. Public API consumed by the base

Exact signatures:

```rust
// wc3-re::storm
impl StormFs {
    pub fn open_wc3_install(root: impl AsRef<Path>) -> Result<Self>;
    pub fn read(&self, path: &str) -> Result<Vec<u8>>;
    pub fn info(&self, path: &str) -> Option<FileInfo>;
    pub fn listfile(&self) -> Vec<String>;
    pub fn search(&self, prefix: &str, suffix: &str, limit: usize) -> Vec<String>;
}
// wc3-re::mpqex (compat)
impl MpqEx {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn read(&self, name: &str) -> Result<Vec<u8>>;
    pub fn read_at(path: impl AsRef<Path>, name: &str) -> Result<Vec<u8>>;
}
// wc3-re::engine
impl Engine {
    pub fn boot(config: EngineConfig) -> Result<Self>;
    pub fn enter_map(&mut self, map_path: &Path) -> Result<()>;
    pub fn tick(&mut self, dt: f64);
    pub fn shutdown(&mut self);
}
// wc3-re::host
impl HostServices {
    pub fn call(&mut self, ordinal: u32, args: &[i64]) -> i64;
    pub fn launch(&mut self, config: EngineConfig) -> Result<Engine>;
}
// wc3-re::storm_api
impl StormApi {
    pub fn call(&mut self, ordinal: u32, args: &[StormValue]) -> Result<StormValue>;
}
pub fn validate_exports(actual: &[u32]) -> (usize, Vec<u32>, Vec<u32>);
// wc3-re::jass
pub fn parse(source: &str) -> Result<Program, ParseError>;
impl Runtime {
    pub fn new(program: Program, seed: u64) -> Self;
    pub fn run_function(&mut self, name: &str) -> Result<Value>;
    pub fn run_main(&mut self) -> Result<Value>;
    pub fn execute_trigger(&mut self, handle: u32) -> Result<Value>;
}
// wc3-re::ai
pub fn scan_ai_dir(dir: &Path, seed: u64) -> Result<AiCensus>;
// wc3-re::scan
impl ScanReport {
    pub fn run(root: &Path, maps_root: &Path, seed: u64) -> Result<Self>;
    pub fn ok(&self) -> bool;
    pub fn to_json(&self) -> String;
    pub fn to_markdown(&self) -> String;
}
```

Game-side bridge: `wc3_game::re::ReLayer::{open, object, icon_audit,
localized_map_name, engine_config, boot_engine}` (`crates/wc3-game/src/re.rs:25-137`).
JASS-side bridge: `wc3_jass::canonical::{parse, parse_named, merge, run,
run_sources, run_program, RunReport}` (`crates/wc3-jass/src/canonical.rs:22-74`).

## 7. Per-layer verification (run 2026-11-13; green)

Commands and observed results:

1. `crates/wc3-re/tests/merge_parity.rs`
   - `mpqex_codecs_are_wc3_mpq_reexports`
   - `slk_delegation_preserves_scanner_totals` (fixture: all 33 `analysis/war3_full3/**/*.slk`; expected 6,785 rows / 815 columns)
   - `object_data_parsers_agree_on_real_v1_map` (fixture: `analysis/war3_map/Human04/war3map.w3u`)
   - `object_db_uses_canonical_typed_sheets_and_metadata` (fixture: `analysis/war3_full3/Units/UnitData.slk`, `UnitMetaData.slk`; `hfoo`, `uhpm`)
   - `map_bundle_and_w3map_agree_on_real_map` (fixture: `Maps/Campaign/Human04.w3m`)
2. `crates/wc3-re/tests/re_surface.rs`
   - `re_layer_public_signatures_are_stable` (compile-time `fn` pointers for scan/engine/host/storm_api/jass/ai).
3. Existing `crates/wc3-re/tests/real_corpus.rs` (all 3,935 BLP headers, 2,048 MDX, 33 SLK, 91 AI, 44 maps, 4,428 WTS strings) — passed.
4. Existing `crates/wc3-game/src/re.rs` tests: `opens_install_objects_and_triggers`, `resolves_map_name_through_wts`, `icon_audit_resolves_all_references`.
5. Existing `crates/wc3-jass/src/canonical.rs` tests: `canonical_engine_runs_the_shared_script`, `legacy_and_canonical_engines_agree_on_arithmetic`.
6. `wc3re scan analysis/war3_full3 --json ... --md ...` observed after merge:
   3,935 BLP / 2,048 MDX / 33 SLK / **67 TXT** / 71 FDF / 44 maps / 44 `main` /
   91 AI / **4,428 WTS** / 676 TriggerData / 1,182 icons / 0 map errors;
   `scan.json` `ok=true`, elapsed 183.06 s. (The canonical WTS parser yields
   4,428 vs the pre-merge local view 4,421; canonical is authoritative.)
7. Dispatcher gates (not run here): full `cargo test -p wc3-mpq -p wc3-formats
   -p wc3-re -p wc3-jass`, `cargo check -p wc3-game --lib`,
   `cargo metadata --no-deps`, device/APK. Per-layer scope commands run:
   `cargo check -p wc3-re -p wc3-jass` rc 0 and
   `cargo test -p wc3-re -p wc3-jass` rc 0.

Observed risk resolution: `ObjectDb` uses the strict `gamedata::Sheet` parser;
object count stayed 1,665 and `hfoo`/`uhpm` parity passed. No fallback parser
was added.

## 8. Conflicts / dependencies

- **Matrix:** `WC3RE_MERGE_MATRIX.md` is present in the current tree; BASE.md
  v1 plus that matrix were used. No matrix conflict remains.
- **`wc3-data`:** BASE.md lists it as a standalone workspace root, not a member
  of the root workspace. The root `Cargo.toml` currently includes `wc3-re` but
  not `wc3-data`; out of this owner’s write set. Dispatcher decision needed if
  `wc3-data` must join the graph.
- **Game bridge type change:** `ObjectDb.sheets` is now
  `BTreeMap<String, gamedata::Sheet>` instead of the resource-local `slk::Sheet`.
  Current consumers (`wc3-game/re.rs`, `scan`, `bin/wc3re`) only use `.len()` or
  `record()`; no read-only patch required. If another layer indexes
  `ObjectDb.sheets` as the old type, dispatcher should apply the type-alias
  patch in `analysis/tech-lead/patches/`.
- **`analysis/tech-lead/merge-anim.md`** says BASE.md is absent; stale text,
  not code.
- **Scope tests/build** were run: `cargo check -p wc3-re -p wc3-jass` rc 0;
  `cargo test -p wc3-re -p wc3-jass` rc 0; `wc3re scan` rc 0. Full workspace
  suite, `wc3-game` check, and device/APK remain dispatcher gates.

## 9. Exact next step

1. Dispatcher runs the full workspace gates: `cargo test -p wc3-mpq
   -p wc3-formats -p wc3-re -p wc3-jass`, `cargo check -p wc3-game --lib`,
   `cargo metadata --no-deps`.
2. If `wc3-game` fails on the typed `ObjectDb.sheets` change, apply the
   type/consumer patch noted in §8 (current consumers only use `.len()` and
   `record()`; no patch is currently required).
3. After the full suite, run device/APK acceptance outside this scope.
4. Keep `analysis/tech-lead/merge-re.md` as the RE row; update only if the
   dispatcher changes BASE v2 or the matrix.

## 10. State

**DONE** — scope compiled and per-layer tests/scan are green (2026-11-13):
`cargo check -p wc3-re -p wc3-jass` rc 0; `cargo test -p wc3-re -p wc3-jass`
rc 0 (44 lib + 5 merge_parity + 1 re_surface + 2 real_corpus + 3 runtime_wiring + 6 wc3-jass);
`wc3re scan` rc 0, `ok=true`, 211.24 s, 0 map errors, 67/67 TXT, 44/44 maps, 4,428 WTS. Runtime gate: map `main` + `InitCustomTriggers`; executed trigger count > 0; AI 91/91; object/WTS bridge green.
Evidence: `analysis/agent-re/{PROVENANCE_MAP,PKWARE_FIX,RESOURCE_ACCESS,COVERAGE,INTEGRATION,REPORT}.md`,
`analysis/agent-re/{scan.json,SCAN.md,check.log,test.log,check-data.log,test-data.log}`, and
`crates/wc3-re/tests/{merge_parity,re_surface,real_corpus}.rs`. No device/APK
run (not part of this scope).

## 10.1 Data-layer P1/P2 follow-up (2026-11-13)

Applied the exact `analysis/tech-lead/merge-data.md` §3 P1/P2 patches to the RE
owner paths, with two compile-required adaptations recorded here:

- P1.1/P1.2/P1.3/P1.4/P1.5/P1.7/P1.8 applied: `ObjectDb` now carries
  `metas: BTreeMap<MetaKind,MetaTable>`, `texts: Vec<TextTable>`,
  `profile_overlay: ProfileOverlay`, and all 9 metadata sheets are parsed by
  `MetaTable::parse`. `texts()`, `metas()`, `resolve_texts()`,
  `meta_for_typed()` and `refresh_record()` exist at
  `crates/wc3-re/src/objectdata.rs:483-556`; helpers at `:620-675`.
- P1.6 applied: `apply_map_objects` calls canonical
  `gamedata::merge_overlay` (`objectdata.rs:399-479`), so `uhpm` lands in
  `UnitBalance.HP` and `unam` in the text field. Adaptation 1: the patch's
  `base_kind = base.kind` mixed `gamedata::ObjectKind` and
  `objectdata::ObjectKind`; a `object_kind_from_typed` projection was added
  (`:664`). Adaptation 2: `ProfileOverlay` keys are lowercased, so the profile
  replacement removes the case-insensitive existing key before insert
  (`:462-474`) to keep `field("Name")` deterministic.
- P2 applied: `Modification.level/data_pointer` (`mapobjects.rs:43-53`),
  `MapObjectData.kind` + manual `Default` (`:63-81`), `parse_as` kind
  (`:91-97`), `to_typed()` (`:111-118`), and project/unproject helpers
  (`:138-181`).
- New test `objectdata::tests::applies_map_objects_through_canonical_merge`
  (`objectdata.rs:705-733`) uses a synthetic v1 `w3u` (`hfoo`, `uhpm=777`,
  `unam=TestName`) and asserts `UnitBalance.HP`/`Name`.

Verification after the patches:

```text
cargo check -p wc3-re -p wc3-jass --offline -> rc 0
cargo test  -p wc3-re -p wc3-jass --offline -> rc 0
  wc3-re lib 33 passed; merge_parity 5; re_surface 1; real_corpus 2
  wc3-jass 6 passed
```

Logs: `analysis/agent-re/check-data.log`, `analysis/agent-re/test-data.log`.
No device/APK run.

## 10.2 Full merge absorption (2026-11-13)

`scan`, `engine`, `host`, `storm`, `storm_api` and JASS are absorbed into the
single `wc3-re` API; `wc3-jass` exposes the canonical VM through
`wc3_jass::canonical`, and `wc3-game::re::ReLayer` consumes `StormFs`,
`ObjectDb`, `MapBundle`, `TriggerData`, `Engine` and the typed database.

| concern | single API entry point | caller | proof |
|---|---|---|---|
| scan | `wc3_re::scan::ScanReport::{run,ok,to_json,to_markdown}` | `wc3re scan`, `tests/re_surface.rs` | scan `ok=true`, 4,428 WTS, 0 map errors |
| engine | `wc3_re::engine::Engine::{boot,enter_map,tick,shutdown}` | `wc3-game::re::ReLayer::boot_engine` | `cargo check -p wc3-game --lib` rc 0 |
| host | `wc3_re::host::{HostServices,Launcher,HOST_IMPORTS}` | RE API / `wc3re` | `host` test; PE summary |
| storm | `wc3_re::storm::StormFs` | `ReLayer::open`, mapfiles, objectdata, icons, scan | no second archive reader |
| storm ordinals | `wc3_re::storm_api::StormApi`, `STORM_EXPORTS`, `GAME_IMPORTS_STORM` | `wc3re info`, RE tests | 370 exports / 92 imports |
| JASS | `wc3_re::jass::{parse,Program,Runtime}` | `wc3_jass::canonical`, `sim_bridge` | 44/44 map `main`, 91/91 AI |

Matrix §9 mirrors this table; no read-only patch is required for wc3-game
(`check-fullmerge.log` rc 0).

## 10.3 Round-2 game-runtime wiring (2026-11-13)

- `Engine::enter_map` remains the one map-script path: merges `common.j` +
  `Blizzard.j` + `war3map.j`, runs `main` (which calls `InitCustomTriggers` and
  `RunInitializationTriggers`).
- New `Engine::script_report()` exposes `main_ok`, merged function count,
  registered triggers/actions and executed triggers/actions. `Engine::{runtime,
  runtime_mut}` expose the canonical JASS runtime to the game/sim bridge.
- `Engine::load_ai_scripts(Scripts/)` parses all `.ai` through `wc3-re::ai`;
  `ScriptReport`/`runtime_wiring` prove `human.ai::InitAI` runs.
- `HostServices::launch` + `StormFs` are covered end-to-end: host ordinals
  372/327/475, loose-tree mount, `Units\UnitData.slk` read, map script boot.
- `MapBundle` treats `war3mapUnits.doo` unsupported variants (v8 random tables,
  truncated BonusCredits) as optional placement metadata, so map/scan remain
  0-error while `war3map.j`/W3E/W3I/WTS/object data still parse.
- Object bridge: `ObjectDb::{metas,texts,profile_overlay,apply_map_objects,
  resolve_texts}` and map overrides are exercised by
  `runtime_wiring::object_data_and_wts_overrides_reach_object_db`.
- Map placement: `Engine` now spawns `war3mapUnits.doo` units and
  `war3map.doo` doodads/destructables into the JASS `World`, de-duplicating
  against script-created type+position matches. `Engine::map_spawn()` returns
  `MapSpawnReport` (`map_units/map_doodads/spawned_*/matched_*`). Synthetic
  unit+doodad placement is verified by
  `engine::tests::spawns_map_units_and_doodads_into_jass_world`; Human04
  doodads are verified in `runtime_wiring::host_launch_mounts_storm_and_runs_map_script`.

Verification (round 3): `cargo check -p wc3-re -p wc3-jass` rc 0;
`cargo test -p wc3-re -p wc3-jass` rc 0 (**44 lib** + 5 merge_parity + 1
re_surface + 2 real_corpus + 3 runtime_wiring + 6 wc3-jass =
`analysis/agent-re/test-final.log`); `cargo check -p wc3-game --lib` rc 0
(`analysis/agent-re/check-game-r3.log`, input E0533 fixed by its owner);
`wc3re scan` rc 0 with `scripts.unimplemented={}`, `maps.errors=[]`, 44/44
`main` (`analysis/agent-re/scan-final.json`); `wc3re parity` rc 0 over 44 maps
(`qa-jass-map-runtime.json`). The verified `game.rs` patch
(`patch-game-parity46.diff`) compiles (rc 0) and passes
`cargo test -p wc3-game --lib map_units_spawn` when applied
(`analysis/agent-re/game-map-units-spawn.log`); the file was restored
byte-identically (sha256 checked) because `game.rs` stays HUD-owned.

### 10.4 Round-3 PARITY #4/#6/#7 delivery (2026-11-13)

- **`war3mapUnits.doo` (PARITY #4)**: `wc3_formats::map::parse_units` is aligned
  with HiveWE `src/base/units.ixx` — `random_type` payload (`0`=4 B, `1`=8 B,
  `2`=count*8 B, other=0 B), `item_table_pointer` and STR/AGI/INT only for
  `subversion >= 11`, group/roll/pitch/lights for `version >= 13`. Human04
  (v7/sub9, `random_type=0xFFFF_FFFF`) parses **90/90** placements and every map
  in the install now parses its unit table (6 723 placements across 44 maps;
  near-zero `war3mapUnits.doo` errors). `Engine::map_spawn()` returns
  `MapSpawnReport`; placements spawn into the JASS `World` with type+position
  de-duplication against `CreateAllUnits` (468 spawned, 6 255 matched).
- **All missing natives (PARITY #7)**: `wc3re scan` reports
  `scripts.unimplemented = {}` (was 92 names / 3 073 call sites). Gameplay
  natives mutate the deterministic `World`; UI/camera/audio/weather/campaign
  controls are explicit no-ops. `jass::natives::parity_tests` adds 9 per-native
  gates (`TriggerRegisterPlayerStateEvent`, `TriggerRegisterUnitInRange`,
  `QuestCreateItem`, `SetResourceAmount`/`GetResourceAmount`,
  `AttachSoundToUnit`, camera bounds, fog/weather no-ops,
  `SetUnitMoveSpeed`, `ForForce`+`GetEnumPlayer`).
- **Game frame loop (PARITY #6) — APPLIED**: HUD applied
  `analysis/tech-lead/patch-game-parity46.diff` on 2026-11-13 12:02 (post-apply
  SHA-256 `fad4edd2…`); RE re-verified on the applied file:
  `cargo check -p wc3-game --lib` rc 0, `map_units_spawn` 2 passed,
  full game lib 95 passed / 18 ignored / 0 failed. The diff
  adds `Game.engine`/`Game.bridge`, boots `Engine` from the `Assets` map
  provenance (`map=` debug line, install root from `mpq=` or map ancestors),
  re-enters the map in `reset_scenario` (skipping the fixed demo scenario), and
  swaps the canonical `Sim` into `JassBridge::step_engine` every tick (no
  clone/allocation per frame). Acceptance `map_units_spawn` passes with the
  patch: 90 placements, `main_ok`, executed triggers > 0, `world.game_time`
  advances in `Game::update`, sim units live.
- **Headless artifact (Phase 4)**: `wc3re parity` → `qa-jass-map-runtime.json`
  + `MAP_RUNTIME.md` (44/44 maps, 713 executed triggers, 1 313 713 VM steps)
  and the stdlib gate `analysis/agent-re/qa_map_runtime.py`
  (`qa.jass.map_runtime`), rc 0.

## 11. H2 persistence

- `capture_id`: **579** (`merge-re-port`); static cleanup: **584**; green run: **598**;
  round-2 parity **644/646/650**; round-3 parity **665** (+ final capture below)
- `experience_id`: **4300** (facade/re-export dedup + type-identity parity)
- `experience_id`: **4301** (typed `ObjectDb` Sheet/MetaTable projection)
- `experience_id`: **4302** (PARTIAL state + post-freeze test plan)
- `monitor`: ledger **313**, state **completed**, `verification.ok=true`,
  `terminal=true`, no warnings; `h2_ids_json` =
  `{"capture_id":579,"final_capture_id":584,"green_capture_id":598,"experiences":[4300,4301,4302]}`.

Round-3 (PARITY #4/#6/#7): `capture_id`: **675** (round-3 parity; earlier
**665**); `experience_id`: **4378** (sim-swap Engine integration); ledger **350**
state **completed**, `verification.ok=true`, no warnings; `h2_ids_json` includes
`scan-final.json`, `qa-jass-map-runtime.json`, `patch-game-parity46.diff`,
`test-final.log`, `game-map-units-spawn.log`, `PARITY46_FINAL.md`.

`H2_PERSISTED`. Scope verification is green and the ledger is closed
`completed`; the dispatcher still owns the full workspace/device gates.

### Data-bridge follow-up (2026-11-13)

- `capture_id`: **607**; monitor ledger **338**, state **completed**,
  `verification.ok=true`.
- Evidence: `analysis/agent-re/check-data.log`, `test-data.log`, and §10.1.

### Round 8 (PARITY #11/#14 runtime, 2026-11-13)

- Ability runtime: `Unit.abilities` is now `BTreeMap<String, AbilityState>`
  (level/permanent/cooldown); natives `UnitAdd/RemoveAbility`,
  `Get/Set/Inc/DecUnitAbilityLevel`, `UnitMakeAbilityPermanent`,
  `UnitResetCooldown`, `SelectHeroSkill`, `SetPlayerAbilityAvailable`,
  `AbilityId(2String)`, `GetSpellAbilityId/Unit`, `GetSpellTarget*`.
- Cast API: `Runtime::cast_ability` + `Engine::cast_ability` publish spell
  context and fire `EVENT_PLAYER_UNIT_SPELL_{CAST,EFFECT,FINISH}` (273/274/275)
  and unit events (290/291/292); `Engine::grant_ability/unit_abilities`.
- Item runtime: `Item` carries owner player, held_by unit, charges, level,
  visible/droppable/pawnable/invulnerable/powerup/sellable/drop flags, drop_id;
  6-slot inventory with capacity checks, slot-preserving removal,
  `UnitUseItem*`, pickup/drop/use events (48/49/50, 85/86/87) with
  `GetManipulatedItem`; shop stock (`Add/RemoveItem/UnitToStock`, all-stock
  defaults, item-type slots); item pools (`CreateItemPool`…`PlaceRandomItem`,
  `ChooseRandomItem[Ex]`); `Engine::use_item/unit_items`.
- Trigger fix: `TriggerRegisterPlayerUnitEvent` reads the unit-type filter from
  argument 3 and treats null as “no filter” (previously decoded the event id),
  so `TriggerRegisterAnyUnitEventBJ` registrations match fired events.
- Evidence: `cargo test -p wc3-re --lib` **61 passed** (14 new
  `ability_item_tests`), `cargo check -p wc3-re --all-targets` rc 0,
  `scan-r8.json` `unimplemented={}`/0 errors/44 main/791 natives,
  `wc3re parity` 44/44 + 713 triggers + 6 723 units, `qa.jass.map_runtime`
  rc 0. Report `analysis/agent-re/PARITY_R8.md`.
- Corpus note: `analysis/war3_full3/Scripts/common.j` is trimmed (no spell
  constants, no ability-level/shop natives); canonical event ids/APIs taken
  from `analysis/war3x_data/Scripts/common.j`. `scripts.unimplemented` is a
  map-call census, not full native-surface coverage.
- External red: `cargo check -p wc3-game --lib` rc 101 —
  `sim_bridge.rs` calls `Sim::spawn_def_exact`, absent in `wc3-sim` on the
  current tree (sim owner in flight; READ_ONLY for RE).
- `capture_id`: **756**; `experience_id`: **4422**; ledger **350**
  `completed`, `verification.ok=true`.

### Round 9 (P0 JASS parser gap close, 2026-11-13)

- Root cause in the **standalone** `wc3-jass` parser: (1) the `return` arm called
  `skip_newlines()` before checking for a newline, so a bare `return` consumed
  the next line (`endfunction`/`set`/`endif`) as its value — also the cause of
  six retail maps failing with `unknown statement endfunction`; (2) the `if`
  arm overwrote `else_branch` per `elseif`, losing the first-true branch.
- Fix: bare `return` terminates at `Newline`/`End` (`return nothing` accepted on
  the same line); `if` collects branches and rebuilds the else-chain from the
  last branch backwards, preserving the final `else`.
- Removed the unused `wc3-sim` dependency from `crates/wc3-jass/Cargo.toml`
  (only comments referenced it), decoupling the parser gate from gameplay-layer
  build state. `wc3-re::jass` was already correct; two parser tests now lock
  both parsers on the same edge cases.
- Evidence: `wc3-jass` lib **11 passed** (5 new), `triggers_gates` integration
  **PASS 45/45 maps + 3/3 bare return + 1/1 elseif**, `wc3-re` lib **63 passed**
  (2 new parser tests), release `wc3re scan` `ok=true`/0 errors/
  `unimplemented={}`/44 main/791 natives, canonical QA
  `GATE triggers.jass 45/45`, `GATE triggers.return 3/3`,
  `GATE triggers.elseif 1/1`, overall `PARITY PARTIAL gates=23 fail=0 gap=3`
  (`analysis/agent-qa/parity/report-r9.json`).
- Report `analysis/agent-re/PARITY_R9_PARSER.md`.
- `capture_id`: **762**; `experience_id`: **4428**; ledger **350**
  `completed`, `verification.ok=true`.

### Phase 0 (full WC3 port base, 2026-11-13)

- `wc3_re::manifest` (`manifest.rs:59/:252`): 18 archives, **7 451** named files,
  **543 237 719 B** payload, **0** read failures; 67 unnamed blocks reported per
  archive. Caller `wc3re manifest`; 4 tests. Artifacts
  `analysis/agent-re/phase0/{manifest.json,MANIFEST.md}`.
- `wc3_re::gameplay::GameplayConfig` (`gameplay.rs:68/:951`): 812 units, 812
  weapons, 763 abilities, 273 items, 79 MiscGame constants; weapon formula
  validation 716/717 avg, 570/570 DPS. Runtime caller
  `crates/wc3-game/src/game.rs:274` → `Sim::apply_gameplay`
  (`crates/wc3-sim/src/lib.rs:2898`); sim test `lib.rs:4523`. Artifacts
  `analysis/agent-re/phase0/{constants.json,CONSTANTS.md}`.
- `wc3_re::cutscenes` (`cutscenes.rs:103/:585`): 44 maps, 19 420 functions,
  4 560 triggers, 9 981 cinematic calls, 1 313 transmissions, 999 cameras,
  4 603 regions, 1 763 sounds, 4 428 WTS strings. Caller `wc3re cutscenes`;
  3 tests. Artifacts `analysis/agent-re/phase0/{cutscenes.json,CUTSCENES.md}`.
- Port matrix `analysis/agent-re/phase0/PORTING_MATRIX.md` (26 rows,
  ADOPT/ADAPT/REJECT, file:line targets); toolchain
  `analysis/agent-re/phase0/TOOLCHAIN.md`; overall report
  `analysis/agent-re/PHASE0.md`.
- Gates: `cargo test -p wc3-re --lib` **74 passed**; `cargo check -p wc3-re
  --all-targets` rc 0; `cargo check -p wc3-game --lib` rc 0; sim config test
  rc 0; manifest/gameplay/cutscenes CLI rc 0; StormLib probe rc 0; MDX→glTF
  Footman rc 0 (5 geosets/505 vertices/304 triangles).
- `capture_id`: **789**; `experience_id`: **4453**; ledger **350**
  `completed`, `verification.ok=true`.

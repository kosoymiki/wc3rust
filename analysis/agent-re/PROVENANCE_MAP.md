# RE/engine provenance map (BASE.md v1)

Generated 2026-11-12 for the merge. Every row is a byte/API source → target
mapping; line numbers are from the current tree (no tests were run while the
base was split). Canonical owners are those in `analysis/tech-lead/BASE.md`.

## 1. Container / codecs

| concern | canonical source | resource-layer target | delegation proof |
|---|---|---|---|
| MPQ header/hash/block tables, decryption | `crates/wc3-mpq/src/lib.rs:167` (`MpqArchive`), `:190` `open`, `:196` `open_at`, `:417` `read`, `:422` `read_with_info`, `:361` `meta` | `crates/wc3-re/src/storm.rs:101` (`StormFs`), `:126` `add_archive`, `:201` `read`; `crates/wc3-re/src/mapfiles.rs:67` `MapBundle`, `:82` `open`, `:101` `archive.read` | `MapBundle`/`StormFs` call `wc3_mpq::MpqArchive` directly; no hash/block/sector code remains under `crates/wc3-re/src` (`grep -R "decrypt_words\|struct HashEntry"` → none) |
| MPQ sector table + dword-tail decryption | `crates/wc3-mpq/src/stream.rs:21` (`MpqFileReader`), `:194-219` sector read/decrypt, `:210` `decompress_into` | `crates/wc3-re/src/mpqex.rs:39` (`MpqEx`), `:94` `read` | `MpqEx` is a facade over `MpqArchive`; it stores only `archive/path/base` (`mpqex.rs:39-67`) |
| MPQ codecs: PKWARE/Huffman/ADPCM | `crates/wc3-mpq/src/pkware.rs:302` `explode`, `:309-320` `0x306` acceptance; `huffman.rs:357`; `adpcm.rs:28` | `crates/wc3-re/src/mpqex/pkware.rs:12`, `huffman.rs:7`, `adpcm.rs:7` | pure `pub use wc3_mpq::…` re-exports; type-identity gate `tests/merge_parity.rs:28-32` (`fn` pointer assignments) |
| archive base detection (`HM3W`/MPQ) | `crates/wc3-formats/src/map.rs:189` `map_mpq_base` | `crates/wc3-re/src/mpqex.rs:46-53`; `storm.rs:302-307` | both call `wc3_formats::map::map_mpq_base`; test parity `tests/merge_parity.rs` (map bundle vs `W3Map`) |
| MPQ metadata projection | `wc3_mpq::FileMeta` (`lib.rs:88`) | `crates/wc3-re/src/storm.rs:61` `FileInfo`; `mpqex.rs:31` `FileMetaEx` | field-by-field projection only (`from_meta`, `meta`) |

## 2. Format primitives (BLP/MDX/SLK/FDF/terrain)

| concern | canonical source | resource-layer target | delegation proof |
|---|---|---|---|
| BLP1/BLP2 decode | `crates/wc3-formats/src/blp.rs:17` `BlpImage`, `:24` `decode` | `crates/wc3-re/src/blp.rs:73` `BlpProbe`, `:115` `probe`, `:223` `decode` | `blp::decode` calls `wc3_formats::blp::decode`; adds header probe/RGBA view only |
| MDX chunk/model | `crates/wc3-formats/src/mdx.rs:341` `MdxModel`, `:361` `parse` | `crates/wc3-re/src/mdx.rs:59` `MdxSummary`, `:117` `summarize` | `mdx::parse` calls `MdxModel::parse`; chunk scanner/summary only |
| SLK grid | `crates/wc3-formats/src/slk.rs:8` `SlkTable`, `:18` `parse` | `crates/wc3-re/src/slk.rs:16` `Sheet`, `:25` `parse` | `Sheet::parse` calls `SlkTable::parse`; adds row-key view; parity `tests/merge_parity.rs::slk_delegation_preserves_scanner_totals` (6785 rows/815 cols) |
| typed SLK / metadata / object data | `crates/wc3-formats/src/gamedata.rs:415` `Sheet`, `:437` `parse`; `:977` `MetaTable`, `:984` `parse`; `:1188` `ObjectDataFile`, `:1218` `parse`; `:1549` `merge_overlay` | `crates/wc3-re/src/objectdata.rs:127` `ObjectDb`, `:140` `load_dir`, `:171` `load_storm`, `:241` `build_records`, `:310` `load_metadata`; `mapobjects.rs:59` `MapObjectData`, `:73` `parse_as` | `ObjectDb.sheets` is `BTreeMap<String, gamedata::Sheet>`; `metadata_table` is `MetaTable`; `MapObjectData` is a projection of `ObjectDataFile` (no byte loop) |
| WTS grammar | `crates/wc3-formats/src/gamedata.rs:1620` `Wts`, `:1625` `parse`, `:1700` `parse_trigstr` | `crates/wc3-re/src/wts.rs:13` `Wts`, `:18` `parse`, `:36` `resolve` | `parse` calls `gamedata::Wts::parse`; adds escape decode + ordered map |
| INI/text tables | `crates/wc3-formats/src/gamedata.rs:1716` `TextTable`, `:1723` `parse` | `crates/wc3-re/src/txt.rs:40` `TextTable`, `:48` `parse`, `:147` `decode_windows_1252` | delegates parse; adds preamble/case-insensitive section, quoted list helpers, and a CP1252 input adapter for `iconindex_bel.txt` (0x96 at 2513) |
| FDF | `crates/wc3-formats/src/fdf.rs:406` `FdfDocument`, `:414` `parse` | `crates/wc3-re/src/skin.rs:39` `SkinTable`; `skin.rs:144` `audit_fdf_decorate` | `audit_fdf_decorate` calls `FdfDocument::parse_named`; no FDF reader in `wc3-re` |
| W3E/W3I/WPM | `crates/wc3-formats/src/w3e.rs:56` `Terrain`, `:69` `parse`; `w3i.rs:6` `MapInfo`, `:18` `parse`; `wpm.rs:6` `PathingMap`, `:14` `parse` | `crates/wc3-re/src/mapfiles.rs:116` `parse_inner`, `:165` `localized_info` | `MapBundle` embeds the canonical types; no re-parse |

## 3. Resource / engine API

| concern | implementation | public signature | evidence |
|---|---|---|---|
| Storm multi-archive | `crates/wc3-re/src/storm.rs:101` | `StormFs::open_wc3_install(root: impl AsRef<Path>) -> Result<Self>` (`:160`), `read(&self, &str) -> Result<Vec<u8>>` (`:201`), `info`, `listfile`, `search` | 3,935 BLP / 2,048 MDX / 44 maps via scan; no fallback reader |
| Storm ordinal facade | `crates/wc3-re/src/storm_api.rs:123` | `StormApi::call(&mut self, u32, &[StormValue]) -> Result<StormValue>` (`:149`), `validate_exports` (`:297`), 370 ordinals (`:28`), 92 Game imports (`:53`) | `tests/storm_api::export_table_matches_observed_pe`; `STORM_ORDINALS.md` |
| Host launcher | `crates/wc3-re/src/host.rs:77` | `HostServices::call(&mut self, u32, &[i64]) -> i64` (`:107`), `launch(EngineConfig) -> Result<Engine>` (`:131`), `HOST_IMPORTS` (`:29`), `Launcher::launch` (`:181`) | `GameMain` host calls A372/A327/A475; `r2/War3.exe.key-xrefs.txt` |
| Engine boot/tick | `crates/wc3-re/src/engine.rs:79` | `Engine::boot(EngineConfig) -> Result<Engine>` (`:96`), `enter_map(&Path)` (`:195`), `tick(f64)` (`:251`), `shutdown()` (`:330`) | `GameMain` 0x6f004180, init 0x6f004200, shutdown 0x6f004b80 pdc |
| JASS lexer/parser | `crates/wc3-re/src/jass/lexer.rs:52` `lex`; `parser.rs:23` `parse`, `:37` `parse_named`; `ast.rs` | `jass::parse(&str) -> Result<Program, ParseError>`, `jass::parse_named` | common.j/Blizzard.j/war3map.j parse; 44/44 map scripts |
| JASS VM | `crates/wc3-re/src/jass/vm.rs:451` `Runtime` | `Runtime::new(Program,u64) -> Self` (`:479`), `run_function(&mut self,&str) -> Result<Value>` (`:549`), `run_main` (`:554`), `execute_trigger(&mut self,u32) -> Result<Value>` (`:883`) | 44 `main` runs, 1,311,867 steps (pre-merge scan) |
| JASS natives | `crates/wc3-re/src/jass/natives.rs:45` `call_native` | internal dispatch; `Runtime::unimplemented` census is public field (`vm.rs:456`) | 791 native declarations; unimplemented census in `SCAN.md` |
| AI scripts | `crates/wc3-re/src/ai.rs:20` | `AiScript::parse_file(&Path)` (`:29`), `run_init(&Program,u64)` (`:50`), `scan_ai_dir(&Path,u64) -> Result<AiCensus>` (`:97`) | 91/91 parsed, 91 `InitAI` runs (pre-merge scan) |
| Whole-install scan | `crates/wc3-re/src/scan.rs:170` | `ScanReport::run(&Path,&Path,u64) -> Result<Self>` (`:190`), `ok` (`:562`), `to_json` (`:575`), `to_markdown` (`:745`) | `analysis/agent-re/scan.json` / `SCAN.md` |
| object DB | `crates/wc3-re/src/objectdata.rs:127` | `ObjectDb::load_dir/load_storm`, `record`, `field_by_id`, `apply_map_objects`, `icon_paths` | 1,665 objects; typed sheets/MetaTable |
| map container | `crates/wc3-re/src/mapfiles.rs:67` | `MapBundle::open`, `localized_info`, `object_entries`, `discover_maps` | 44/44 maps, 4,421 WTS strings |

## 4. Game / JASS adapters (READ-ONLY, verified)

| concern | file:line | status |
|---|---|---|
| Game bridge | `crates/wc3-game/src/re.rs:25` `ReLayer`; `:39` `open`; `:112` `object`; `:117` `icon_audit`; `:122` `localized_map_name`; `:128` `engine_config`; `:137` `boot_engine` | present; consumes `wc3_re` only |
| JASS canonical adapter | `crates/wc3-jass/src/canonical.rs:22` `parse`; `:27` `parse_named`; `:32` `merge`; `:42` `RunReport`; `:56` `run`; `:61` `run_sources`; `:74` `run_program` | present; re-exports `wc3_re::jass` types |
| Game dependency | `crates/wc3-game/Cargo.toml` `wc3-re = { path = "../wc3-re" }` | present |
| JASS dependency | `crates/wc3-jass/Cargo.toml` `wc3-re = { path = "../wc3-re" }` | present |

No READ-ONLY patch is required for this layer: `ReLayer` and
`wc3-jass::canonical` already target the public `wc3-re` API, and
`wc3-mpq`/`wc3-formats` already expose the canonical entry points used above.

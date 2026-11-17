# WC3RE_MERGE_MATRIX — file-to-file / item-to-item absorption

Date: 2026-11-12. Scope: `crates/wc3-re` (RE agent base) ↔ root crates (`wc3-mpq`, `wc3-formats`, `wc3-jass`, `wc3-data`, `wc3-assetdb`, `wc3-game/assets.rs`).

Operator order: **additive absorption, one base**. `wc3-re` stays the public RE/engine API; root crates keep the format primitives they own. `DROP` is not used: duplicates are removed only where the loser is retained behind a delegating facade and a parity test proves the behavior.

Legend: **KEEP** = canonical owner, no second copy; **MERGE** = facade over a stronger root implementation, unique parts kept; **DELEGATE** = parser moved to the canonical owner, this path is a thin wrapper. `cp` = root counterpart file:line. `call` = who consumes it.

## 1. Codec duplication decision (explicit)

`crates/wc3-re/src/mpqex/{pkware,huffman,adpcm}.rs` were copies of `wc3-mpq/src/{pkware,huffman,adpcm}.rs`. Evidence: `huffman.rs` and `adpcm.rs` were byte-identical (`diff -q` = identical); `pkware.rs` differed only by the end-of-input `0x306` acceptance and debug prints. Decision:

| codec | canonical | action | proof |
|---|---|---|---|
| PKWARE DCL | `wc3-mpq::pkware::explode` | 0x306 acceptance moved into canonical; `wc3-re` re-exports | Human04 `war3map.wts` decodes; `wc3-mpq/tests/human04_map.rs`; `mpqex::tests::reads_encrypted_pkware_wts_from_roc_map` |
| Huffman | `wc3-mpq::huffman::decompress` | `wc3-re` re-exports (copy deleted) | `diff -q` identical + `merge_parity::mpqex_codecs_are_wc3_mpq_reexports` (type identity) |
| ADPCM | `wc3-mpq::adpcm::decompress` | `wc3-re` re-exports (copy deleted) | same |

Additional canonicalization found during the merge:

- Per-sector tag is authoritative even when it disagrees with the file mask (Human04 wts: zlib mask 0x02, PKWARE sectors 0x08). `wc3-mpq::decompress_into` now trusts known tags; stored detection uses the declared sector length.
- MPQ encryption covers whole dwords only; a trailing 1–3 byte remainder is plaintext. Padding-and-decrypting it corrupted zlib adler32 tails (`War3.mpq (listfile)`). `wc3-mpq` now reads/decrypts exact sector bytes.
- `wc3-re::mpqex::MpqEx` is now a facade over `MpqArchive`; the second hash/block/sector parser is gone.

## 2. Root crate ownership (what stays where)

| crate/module | owns | consumed by |
|---|---|---|
| `wc3-mpq` | MPQ v0 container, crypt tables, sectors, codecs, listfile, index/stream | wc3-formats, wc3-re, wc3-assetdb, wc3-audio |
| `wc3-formats::blp` | BLP1/BLP2 decode, mip chain, JPEG/DXT/BGRA | wc3-re, wc3-assetdb, wc3-anim, apps |
| `wc3-formats::mdx` | MDX v800 parse, tracks/interpolation (incl. quaternion squad), model graph | wc3-re, wc3-anim, wc3-render, apps |
| `wc3-formats::slk` | lenient SLK grid (well-formedness + legacy view) | gamedata::Sheet, wc3-re::slk |
| `wc3-formats::gamedata` | strict SLK Sheet, MetaTable, ObjectDataFile, Wts, TextTable, overlay merge | wc3-data, wc3-re, wc3-formats::map/objects |
| `wc3-formats::{w3e,w3i,wpm,fdf}` | terrain/info/pathing/FDF primitives | wc3-re::mapfiles, wc3-game, FDF/HUD |
| `wc3-formats::map` | legacy `W3Map` compatibility view + `map_mpq_base`/`open_map_archive` | apps/headless, apps/bench, `wc3-game::assets` (patch pending) |
| `wc3-jass` | legacy sim-facing VM (`JassRuntime`) + `canonical` adapter to `wc3-re::jass` | apps/headless, wc3-sim users |
| `wc3-data` | typed game DB over gamedata | data layer/game |
| `wc3-assetdb` | cache/provider over MPQ/formats/audio | wc3-game::assets |
| `wc3-re` | Storm FS, ordinal/host/engine models, map bundle, object DB, icons/skin/terrain, TriggerData, JASS/AI/scan, mpqex facade | wc3-game, wc3-jass, wc3re CLI |
| `wc3-game` | presentation + `re::ReLayer` bridge | herofolends-android |

## 3. Per-file and per-public-item matrix

### `ai.rs` — AI script access and InitAI execution

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (no root counterpart)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/ai.rs`
- Call path: engine, scan, wc3re ai
- Evidence: scan 91/91 AI, ai::tests::parses_all_local_ai_scripts

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `AiScript` | 20 | KEEP | Public type/const of the AI script access and InitAI execution layer; retained. |
| `parse_file` | 29 | KEEP | Public API retained; see file-level decision. |
| `parse_source` | 34 | KEEP | Public API retained; see file-level decision. |
| `run_init` | 50 | KEEP | Public API retained; see file-level decision. |
| `AiRun` | 75 | KEEP | Public type/const of the AI script access and InitAI execution layer; retained. |
| `AiCensus` | 84 | KEEP | Public type/const of the AI script access and InitAI execution layer; retained. |
| `scan_ai_dir` | 97 | KEEP | Public API retained; see file-level decision. |

### `blp.rs` — BLP resource facade: probe/encoding/mips

- Canonical owner: **wc3-formats::blp owns decode; wc3-re owns probe**
- Root counterpart: wc3-formats/src/blp.rs:10 MipLevel, :17 BlpImage, :24 decode
- Decision: **MERGE (decode delegates; probe unique)**
- Target: `crates/wc3-re/src/blp.rs`
- Call path: icons, scan, HUD/re, assets patch
- Evidence: blp::tests::probes_and_decodes_command_button; scan 3,935/3,935 BLP

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `BLP1_HEADER_BYTES` | 46 | MERGE | Public type/const of the BLP resource facade layer; retained. |
| `BLP2_HEADER_BYTES` | 47 | MERGE | Public type/const of the BLP resource facade layer; retained. |
| `BlpEncoding` | 50 | MERGE | Public type/const of the BLP resource facade layer; retained. |
| `label` | 60 | MERGE | Public API retained; see file-level decision. |
| `BlpProbe` | 73 | MERGE | Public type/const of the BLP resource facade layer; retained. |
| `DecodedBlp` | 91 | MERGE | Public type/const of the BLP resource facade layer; retained. |
| `rgba` | 97 | MERGE | Public API retained; see file-level decision. |
| `mips` | 101 | MERGE | Public API retained; see file-level decision. |
| `probe` | 115 | MERGE | Header-only probe; 156-byte BLP1 / 148-byte BLP2, encoding + mip count. Unique to resource layer, used by scan to classify 3,935 textures without decode. |
| `decode` | 223 | MERGE | Delegates to `wc3_formats::blp::decode` (single decoder); wraps probe metadata + mip chain. |
| `decode_first_mip` | 233 | MERGE | RGBA8 first mip + probe; HUD command buttons 64×64. |

### `engine.rs` — Game.dll boot/tick/enter_map model

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (unique)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/engine.rs`
- Call path: wc3-game::re::ReLayer::boot_engine, wc3re scan
- Evidence: engine::tests::boots_headless_on_extracted_install

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `EngineState` | 39 | KEEP | Public type/const of the Game.dll boot/tick/enter_map model layer; retained. |
| `BootStep` | 50 | KEEP | Public type/const of the Game.dll boot/tick/enter_map model layer; retained. |
| `EngineConfig` | 57 | KEEP | Public type/const of the Game.dll boot/tick/enter_map model layer; retained. |
| `new` | 67 | KEEP | Public API retained; see file-level decision. |
| `Engine` | 79 | KEEP | Public type/const of the Game.dll boot/tick/enter_map model layer; retained. |
| `boot` | 96 | KEEP | Public API retained; see file-level decision. |
| `enter_map` | 195 | KEEP | Public API retained; see file-level decision. |
| `tick` | 251 | KEEP | Public API retained; see file-level decision. |
| `shutdown` | 330 | KEEP | Public API retained; see file-level decision. |

### `error.rs` — RE error taxonomy (Io/Mpq/Format/Parse/Unsupported/Vm)

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: wc3-mpq::MpqError, wc3-formats::FormatError (converted)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/error.rs`
- Call path: all wc3-re modules
- Evidence: From impls compile; used by every module

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Result` | 5 | KEEP | Public type/const of the RE error taxonomy (Io/Mpq/Format/Parse/Unsupported/Vm) layer; retained. |
| `ReError` | 8 | KEEP | Public type/const of the RE error taxonomy (Io/Mpq/Format/Parse/Unsupported/Vm) layer; retained. |

### `host.rs` — War3.exe host-service ordinal model

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (unique)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/host.rs`
- Call path: engine/host, wc3re info
- Evidence: host::tests::exports_and_imports_match_pe_evidence

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `HOST_IMPORTS` | 29 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `HostCall` | 67 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `HostServices` | 77 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `WindowState` | 86 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `call` | 107 | KEEP | Public API retained; see file-level decision. |
| `frame` | 125 | KEEP | Public API retained; see file-level decision. |
| `LaunchPlan` | 132 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `Launcher` | 141 | KEEP | Public type/const of the War3.exe host-service ordinal model layer; retained. |
| `new` | 148 | KEEP | Public API retained; see file-level decision. |
| `plan` | 158 | KEEP | Public API retained; see file-level decision. |
| `launch` | 172 | KEEP | Public API retained; see file-level decision. |
| `imported_host_ordinals` | 179 | KEEP | Public API retained; see file-level decision. |

### `icons.rs` — Command-button icon audit/resolve

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (HUD local path list is the duplicate, deleted by HUD layer)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/icons.rs`
- Call path: HUD, scan, wc3-game::re
- Evidence: scan 1,182/1,182 icons; re::tests::icon_audit_resolves_all_references

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `IconAudit` | 14 | KEEP | Public type/const of the Command-button icon audit/resolve layer; retained. |
| `complete` | 26 | KEEP | Public API retained; see file-level decision. |
| `audit` | 32 | KEEP | Public API retained; see file-level decision. |
| `resolve_icon` | 84 | KEEP | Public API retained; see file-level decision. |

### `jass/ast.rs` — JASS2 AST

- Canonical owner: **wc3-re::jass (canonical)**
- Root counterpart: wc3-jass/src/lib.rs:139 BinOp, :156 Expr, :171 Stmt (legacy)
- Decision: **KEEP (canonical); wc3-jass keeps legacy + canonical adapter**
- Target: `crates/wc3-re/src/jass/ast.rs`
- Call path: parser, vm, engine, ai, wc3-jass::canonical
- Evidence: parser tests + 44 maps + 91 AI

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Expr` | 4 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `UnOp` | 21 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `BinOp` | 28 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `LocalDecl` | 44 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `Stmt` | 53 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `TypeDecl` | 87 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `NativeDecl` | 94 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `GlobalDecl` | 102 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `FunctionDecl` | 112 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `Program` | 122 | KEEP | Public type/const of the JASS2 AST layer; retained. |
| `merge` | 132 | KEEP | Public API retained; see file-level decision. |
| `function` | 139 | KEEP | Public API retained; see file-level decision. |
| `native` | 145 | KEEP | Public API retained; see file-level decision. |
| `function_names` | 151 | KEEP | Public API retained; see file-level decision. |

### `jass/lexer.rs` — JASS2 lexer

- Canonical owner: **wc3-re::jass (canonical)**
- Root counterpart: wc3-jass/src/lib.rs:21 lex (legacy)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/jass/lexer.rs`
- Call path: parser, ai
- Evidence: lexer::tests::lexes_rawcodes_strings_and_numbers

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `TokenKind` | 6 | KEEP | Public type/const of the JASS2 lexer layer; retained. |
| `Token` | 32 | KEEP | Public type/const of the JASS2 lexer layer; retained. |
| `LexError` | 38 | KEEP | Public type/const of the JASS2 lexer layer; retained. |
| `lex` | 52 | KEEP | Public API retained; see file-level decision. |

### `jass/mod.rs` — JASS re-exports

- Canonical owner: **wc3-re::jass**
- Root counterpart: —
- Decision: **KEEP**
- Target: `crates/wc3-re/src/jass/mod.rs`
- Call path: engine, ai, wc3-jass::canonical
- Evidence: compiles; parse/Runtime re-exports

_No public items (binary/private module)._

### `jass/natives.rs` — Native registry: every common.j native, typed defaults + unimplemented census

- Canonical owner: **wc3-re::jass (canonical)**
- Root counterpart: wc3-jass/src/lib.rs:700-890 small sim-native table (legacy)
- Decision: **KEEP canonical; legacy retained for wc3-sim**
- Target: `crates/wc3-re/src/jass/natives.rs`
- Call path: Runtime::call_native, scan unimplemented census
- Evidence: scan map main 44/44, 1,311,867 steps; unimplemented census

_No public items (binary/private module)._

### `jass/parser.rs` — JASS2 parser

- Canonical owner: **wc3-re::jass (canonical)**
- Root counterpart: wc3-jass/src/lib.rs:196 parse (legacy)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/jass/parser.rs`
- Call path: parse/parse_named, engine, ai
- Evidence: parser::tests::parses_real_common_and_map_script

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `ParseError` | 9 | KEEP | Public type/const of the JASS2 parser layer; retained. |
| `parse` | 23 | KEEP | Public API retained; see file-level decision. |
| `parse_named` | 37 | KEEP | Public API retained; see file-level decision. |

### `jass/vm.rs` — Deterministic interpreter + world (players/units/items/triggers/timers)

- Canonical owner: **wc3-re::jass (canonical)**
- Root counterpart: wc3-jass/src/lib.rs:662 JassRuntime (legacy)
- Decision: **KEEP canonical; wc3-jass::canonical adapter proves parity**
- Target: `crates/wc3-re/src/jass/vm.rs`
- Call path: engine, ai, wc3-jass::canonical
- Evidence: vm tests + 44 map main + wc3-jass::canonical::tests

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Value` | 19 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `as_int` | 30 | KEEP | Public API retained; see file-level decision. |
| `as_real` | 41 | KEEP | Public API retained; see file-level decision. |
| `as_bool` | 58 | KEEP | Public API retained; see file-level decision. |
| `as_str` | 70 | KEEP | Public API retained; see file-level decision. |
| `handle` | 82 | KEEP | Public API retained; see file-level decision. |
| `is_null` | 90 | KEEP | Public API retained; see file-level decision. |
| `format_real` | 100 | KEEP | Public API retained; see file-level decision. |
| `Slot` | 109 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `scalar` | 115 | KEEP | Public API retained; see file-level decision. |
| `Wc3Rng` | 125 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `new` | 130 | KEEP | Public API retained; see file-level decision. |
| `next_u32` | 136 | KEEP | Public API retained; see file-level decision. |
| `int` | 147 | KEEP | Public API retained; see file-level decision. |
| `real` | 156 | KEEP | Public API retained; see file-level decision. |
| `Player` | 166 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Unit` | 183 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Destructable` | 212 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Item` | 222 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Group` | 231 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Location` | 236 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Rect` | 242 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Region` | 250 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Trigger` | 256 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `TriggerEvent` | 266 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Timer` | 283 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Sound` | 293 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Effect` | 303 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Quest` | 312 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Dialog` | 324 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Leaderboard` | 331 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Multiboard` | 338 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `BoolExpr` | 347 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `EventContext` | 355 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `World` | 370 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `Runtime` | 451 | KEEP | Public type/const of the Deterministic interpreter + world (players/units/items/triggers/timers) layer; retained. |
| `new` | 479 | KEEP | Public API retained; see file-level decision. |
| `from_sources` | 508 | KEEP | Public API retained; see file-level decision. |
| `function_names` | 540 | KEEP | Public API retained; see file-level decision. |
| `native_names` | 544 | KEEP | Public API retained; see file-level decision. |
| `run_function` | 549 | KEEP | Public API retained; see file-level decision. |
| `run_main` | 554 | KEEP | Public API retained; see file-level decision. |

### `lib.rs` — fourcc/rawcode helpers + module map

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: —
- Decision: **KEEP**
- Target: `crates/wc3-re/src/lib.rs`
- Call path: all modules
- Evidence: used by mapobjects/jass/image paths

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `fourcc` | 58 | KEEP | Public API retained; see file-level decision. |
| `fourcc_str` | 67 | KEEP | Public API retained; see file-level decision. |
| `rawcode_bytes` | 72 | KEEP | Public API retained; see file-level decision. |
| `rawcode_value` | 83 | KEEP | Public API retained; see file-level decision. |

### `mapfiles.rs` — W3M/W3X container + inner-file bundle + WTS localization

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: wc3-formats/src/map.rs:73 W3Map, :86 open (compat view); shared :183 map_mpq_base
- Decision: **KEEP canonical; W3Map compatibility pending assets patch**
- Target: `crates/wc3-re/src/mapfiles.rs`
- Call path: engine, scan, wc3-game::re, world
- Evidence: mapfiles::tests::opens_real_roc_campaign_map; scan 44/44

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `MAP_INNER_FILES` | 32 | KEEP | Public type/const of the W3M/W3X container + inner-file bundle + WTS localization layer; retained. |
| `InnerFile` | 61 | KEEP | Public type/const of the W3M/W3X container + inner-file bundle + WTS localization layer; retained. |
| `MapBundle` | 67 | KEEP | Public type/const of the W3M/W3X container + inner-file bundle + WTS localization layer; retained. |
| `open` | 82 | KEEP | Public API retained; see file-level decision. |
| `has` | 164 | KEEP | Public API retained; see file-level decision. |
| `entry` | 170 | KEEP | Public API retained; see file-level decision. |
| `localized_info` | 177 | KEEP | Public API retained; see file-level decision. |
| `object_entries` | 188 | KEEP | Public API retained; see file-level decision. |
| `MapAudit` | 195 | KEEP | Public type/const of the W3M/W3X container + inner-file bundle + WTS localization layer; retained. |
| `from_bundle` | 206 | KEEP | Public API retained; see file-level decision. |
| `discover_maps` | 224 | KEEP | Public API retained; see file-level decision. |

### `mapobjects.rs` — Map object-data projection

- Canonical owner: **wc3-formats::gamedata::ObjectDataFile (parser); wc3-re projection**
- Root counterpart: wc3-formats/src/gamedata.rs:1184 ObjectDataFile, :1212 parse; wc3-formats/src/objects.rs:28 ObjectData
- Decision: **DELEGATE (implemented)**
- Target: `crates/wc3-re/src/mapobjects.rs`
- Call path: objectdata, mapfiles
- Evidence: merge_parity::object_data_parsers_agree_on_real_v1_map; mapobjects tests

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `ModValue` | 34 | DELEGATE | Public type/const of the Map object-data projection layer; retained. |
| `Modification` | 43 | DELEGATE | Public type/const of the Map object-data projection layer; retained. |
| `ObjectEntry` | 52 | DELEGATE | Public type/const of the Map object-data projection layer; retained. |
| `MapObjectData` | 59 | DELEGATE | Public type/const of the Map object-data projection layer; retained. |
| `parse` | 67 | DELEGATE | Public API retained; see file-level decision. |
| `parse_as` | 73 | DELEGATE | Public API retained; see file-level decision. |
| `entries` | 82 | DELEGATE | Public API retained; see file-level decision. |
| `entry_count` | 86 | DELEGATE | Public API retained; see file-level decision. |
| `fields` | 92 | DELEGATE | Public API retained; see file-level decision. |

### `mdx.rs` — MDX chunk scanner + summary + texture/layer views

- Canonical owner: **wc3-formats::mdx owns parse; wc3-re owns scanner/summary**
- Root counterpart: wc3-formats/src/mdx.rs:341 MdxModel, :361 parse
- Decision: **MERGE (parse delegates; chunk scan unique)**
- Target: `crates/wc3-re/src/mdx.rs`
- Call path: anim, scan, render, wc3-game::re
- Evidence: mdx::tests::scans_real_footman_model; scan 2,048/2,048

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `MdxChunk` | 32 | MERGE | Public type/const of the MDX chunk scanner + summary + texture/layer views layer; retained. |
| `chunks` | 42 | MERGE | Chunk-walk only (tag/size/offset) with bounds checks; unique, feeds MdxSummary and unknown-tag preservation. |
| `MdxSummary` | 77 | MERGE | Public type/const of the MDX chunk scanner + summary + texture/layer views layer; retained. |
| `from_model` | 96 | MERGE | Public API retained; see file-level decision. |
| `parse` | 128 | MERGE | Delegates to `wc3_formats::mdx::MdxModel::parse`. |
| `summarize` | 132 | MERGE | chunks + parse + counts; scan uses it for 2,048 models. |
| `textures` | 139 | MERGE | Public API retained; see file-level decision. |
| `layers` | 145 | MERGE | Public API retained; see file-level decision. |
| `referenced_textures` | 157 | MERGE | Resolves material layers to TEXS paths + replaceable ids; renderer contract. |

### `mpqex.rs` — Correcting map/archive read facade

- Canonical owner: **wc3-mpq::MpqArchive (reader); wc3-re facade**
- Root counterpart: wc3-mpq/src/lib.rs:417 read, :450 stream, :499 decompress_into; codecs :pkware/:huffman/:adpcm
- Decision: **DELEGATE (implemented)**
- Target: `crates/wc3-re/src/mpqex.rs`
- Call path: storm, mapfiles, wc3re extract
- Evidence: mpqex::tests::reads_encrypted_pkware_wts_from_roc_map; wc3-mpq/tests/human04_map.rs

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `MPQ_MAGIC` | 23 | DELEGATE | Public type/const of the Correcting map/archive read facade layer; retained. |
| `FileMetaEx` | 27 | DELEGATE | Public type/const of the Correcting map/archive read facade layer; retained. |
| `MpqEx` | 36 | DELEGATE | Public type/const of the Correcting map/archive read facade layer; retained. |
| `open` | 43 | DELEGATE | Public API retained; see file-level decision. |
| `open_at` | 50 | DELEGATE | Public API retained; see file-level decision. |
| `path` | 60 | DELEGATE | Public API retained; see file-level decision. |
| `base` | 64 | DELEGATE | Public API retained; see file-level decision. |
| `listfile` | 68 | DELEGATE | Public API retained; see file-level decision. |
| `block_count` | 72 | DELEGATE | Public API retained; see file-level decision. |
| `read_at` | 77 | DELEGATE | Public API retained; see file-level decision. |
| `meta` | 81 | DELEGATE | Public API retained; see file-level decision. |
| `read` | 91 | DELEGATE | Public API retained; see file-level decision. |

### `mpqex/adpcm.rs` — ADPCM codec

- Canonical owner: **wc3-mpq::adpcm**
- Root counterpart: wc3-mpq/src/adpcm.rs:28 decompress
- Decision: **DELEGATE (re-export, byte-identical)**
- Target: `crates/wc3-re/src/mpqex/adpcm.rs`
- Call path: mpqex decompress_sector
- Evidence: merge_parity::mpqex_codecs_are_wc3_mpq_reexports

_No public items (binary/private module)._

### `mpqex/huffman.rs` — Huffman codec

- Canonical owner: **wc3-mpq::huffman**
- Root counterpart: wc3-mpq/src/huffman.rs:357 decompress
- Decision: **DELEGATE (re-export, byte-identical)**
- Target: `crates/wc3-re/src/mpqex/huffman.rs`
- Call path: mpqex decompress_sector
- Evidence: merge_parity::mpqex_codecs_are_wc3_mpq_reexports

_No public items (binary/private module)._

### `mpqex/pkware.rs` — PKWARE DCL explode

- Canonical owner: **wc3-mpq::pkware**
- Root counterpart: wc3-mpq/src/pkware.rs:302 explode (+0x306 fix ultra-ceded)
- Decision: **DELEGATE (re-export; fix moved to canonical)**
- Target: `crates/wc3-re/src/mpqex/pkware.rs`
- Call path: mpqex decompress_sector
- Evidence: wc3-mpq human04 test; mpqex wts test

_No public items (binary/private module)._

### `objectdata.rs` — Install object DB composition (SLK+Func+Strings+MetaData+map overlays)

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: wc3-formats/src/gamedata.rs:977 MetaTable, :415 Sheet; wc3-data/src/lib.rs typed DB
- Decision: **KEEP canonical; parsers delegate**
- Target: `crates/wc3-re/src/objectdata.rs`
- Call path: icons, scan, wc3-game::re, world
- Evidence: objectdata::tests::loads_real_object_database (1,665 objects)

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `ObjectKind` | 39 | KEEP | Public type/const of the Install object DB composition (SLK+Func+Strings+MetaData+map overlays) layer; retained. |
| `label` | 73 | KEEP | Public API retained; see file-level decision. |
| `FieldMeta` | 89 | KEEP | Public type/const of the Install object DB composition (SLK+Func+Strings+MetaData+map overlays) layer; retained. |
| `ObjectRecord` | 100 | KEEP | Public type/const of the Install object DB composition (SLK+Func+Strings+MetaData+map overlays) layer; retained. |
| `field` | 113 | KEEP | Public API retained; see file-level decision. |
| `slk_field` | 120 | KEEP | Public API retained; see file-level decision. |
| `ObjectDb` | 127 | KEEP | Public type/const of the Install object DB composition (SLK+Func+Strings+MetaData+map overlays) layer; retained. |
| `load_dir` | 136 | KEEP | Public API retained; see file-level decision. |
| `load_storm` | 167 | KEEP | Public API retained; see file-level decision. |
| `record` | 307 | KEEP | Public API retained; see file-level decision. |
| `field_by_id` | 313 | KEEP | Public API retained; see file-level decision. |
| `apply_map_objects` | 332 | KEEP | Public API retained; see file-level decision. |
| `counts` | 370 | KEEP | Public API retained; see file-level decision. |
| `sheet_counts` | 378 | KEEP | Public API retained; see file-level decision. |
| `icon_paths` | 386 | KEEP | Public API retained; see file-level decision. |
| `known_ids` | 417 | KEEP | Public API retained; see file-level decision. |
| `IconRef` | 423 | KEEP | Public type/const of the Install object DB composition (SLK+Func+Strings+MetaData+map overlays) layer; retained. |

### `scan.rs` — Whole-install verification scanner

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (unique)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/scan.rs`
- Call path: wc3re scan, QA gate
- Evidence: scan 0 errors; real_corpus tests

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `BlpScan` | 23 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `MdxScan` | 37 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `SlkScan` | 54 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `TxtScan` | 64 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `FdfScan` | 73 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `ScriptScan` | 83 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `MapScan` | 99 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `ObjectScan` | 112 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `TerrainScan` | 124 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `TriggerScan` | 134 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `SkinScan` | 147 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `ArchiveScan` | 155 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `ArchiveInfo` | 160 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `ScanReport` | 170 | KEEP | Public type/const of the Whole-install verification scanner layer; retained. |
| `run` | 190 | KEEP | Public API retained; see file-level decision. |
| `ok` | 562 | KEEP | Public API retained; see file-level decision. |
| `to_json` | 575 | KEEP | Public API retained; see file-level decision. |
| `to_markdown` | 745 | KEEP | Public API retained; see file-level decision. |

### `skin.rs` — war3skins + FDF DecorateFileNames resolution

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (HUD local SkinTable is duplicate, to delete)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/skin.rs`
- Call path: HUD/FDF, scan, re
- Evidence: skin::tests::resolves_race_skin_and_default

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `SkinSection` | 24 | KEEP | Public type/const of the war3skins + FDF DecorateFileNames resolution layer; retained. |
| `get` | 30 | KEEP | Public API retained; see file-level decision. |
| `SkinTable` | 39 | KEEP | Public type/const of the war3skins + FDF DecorateFileNames resolution layer; retained. |
| `load` | 45 | KEEP | Public API retained; see file-level decision. |
| `parse` | 50 | KEEP | Public API retained; see file-level decision. |
| `section` | 80 | KEEP | Public API retained; see file-level decision. |
| `resolve` | 88 | KEEP | Public API retained; see file-level decision. |
| `resolve_list` | 95 | KEEP | Public API retained; see file-level decision. |
| `decorate` | 109 | KEEP | Public API retained; see file-level decision. |
| `texture_entries` | 116 | KEEP | Public API retained; see file-level decision. |
| `DecorateAudit` | 136 | KEEP | Public type/const of the war3skins + FDF DecorateFileNames resolution layer; retained. |
| `audit_fdf_decorate` | 144 | KEEP | Public API retained; see file-level decision. |

### `slk.rs` — SLK resource view (row keys/index)

- Canonical owner: **wc3-formats::slk::SlkTable (parser); wc3-re view**
- Root counterpart: wc3-formats/src/slk.rs:8 SlkTable, :16 parse
- Decision: **DELEGATE (implemented)**
- Target: `crates/wc3-re/src/slk.rs`
- Call path: objectdata, terrain, scan
- Evidence: merge_parity::slk_delegation_preserves_scanner_totals (6,785 rows/815 cols)

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Sheet` | 16 | DELEGATE | Public type/const of the SLK resource view (row keys/index) layer; retained. |
| `parse` | 25 | DELEGATE | Public API retained; see file-level decision. |
| `len` | 50 | DELEGATE | Public API retained; see file-level decision. |
| `is_empty` | 54 | DELEGATE | Public API retained; see file-level decision. |
| `column_index` | 58 | DELEGATE | Public API retained; see file-level decision. |
| `row_index` | 65 | DELEGATE | Public API retained; see file-level decision. |
| `cell` | 69 | DELEGATE | Public API retained; see file-level decision. |
| `cell_at` | 74 | DELEGATE | Public API retained; see file-level decision. |
| `record` | 78 | DELEGATE | Public API retained; see file-level decision. |
| `column_names` | 91 | DELEGATE | Public API retained; see file-level decision. |
| `records` | 96 | DELEGATE | Public API retained; see file-level decision. |

### `storm.rs` — Multi-archive Storm FS + loose tree + priority

- Canonical owner: **wc3-re (canonical); wc3-mpq container**
- Root counterpart: wc3-mpq/src/lib.rs MpqArchive; wc3-formats/src/map.rs:183 map_mpq_base
- Decision: **KEEP canonical**
- Target: `crates/wc3-re/src/storm.rs`
- Call path: engine, objectdata, icons, scan, re
- Evidence: storm::tests::opens_real_war3_archive_when_present

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `MPQ_MAGIC` | 47 | KEEP | Public type/const of the Multi-archive Storm FS + loose tree + priority layer; retained. |
| `W3M_MAGIC` | 49 | KEEP | Public type/const of the Multi-archive Storm FS + loose tree + priority layer; retained. |
| `normalize_path` | 53 | KEEP | Public API retained; see file-level decision. |
| `FileInfo` | 61 | KEEP | Public type/const of the Multi-archive Storm FS + loose tree + priority layer; retained. |
| `StormLayer` | 93 | KEEP | Public type/const of the Multi-archive Storm FS + loose tree + priority layer; retained. |
| `StormFs` | 101 | KEEP | Public type/const of the Multi-archive Storm FS + loose tree + priority layer; retained. |
| `new` | 116 | KEEP | Public API retained; see file-level decision. |
| `add_archive` | 126 | KEEP | Public API retained; see file-level decision. |
| `add_loose_dir` | 141 | KEEP | Public API retained; see file-level decision. |
| `open_wc3_install` | 160 | KEEP | Public API retained; see file-level decision. |
| `layers` | 179 | KEEP | Public API retained; see file-level decision. |
| `exists` | 184 | KEEP | Public API retained; see file-level decision. |
| `info` | 189 | KEEP | Public API retained; see file-level decision. |
| `read` | 201 | KEEP | Public API retained; see file-level decision. |
| `read_with_codecs` | 229 | KEEP | Public API retained; see file-level decision. |
| `listfile` | 243 | KEEP | Public API retained; see file-level decision. |
| `search` | 261 | KEEP | Public API retained; see file-level decision. |
| `archive_file_counts` | 278 | KEEP | Public API retained; see file-level decision. |
| `loose_root_for` | 299 | KEEP | Public API retained; see file-level decision. |

### `storm_api.rs` — Storm ordinal facade

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (unique)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/storm_api.rs`
- Call path: engine/host
- Evidence: storm_api::tests::export_table_matches_observed_pe (370)

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `STORM_EXPORTS` | 28 | KEEP | Public type/const of the Storm ordinal facade layer; retained. |
| `GAME_IMPORTS_STORM` | 53 | KEEP | Public type/const of the Storm ordinal facade layer; retained. |
| `storm_name` | 63 | KEEP | Public API retained; see file-level decision. |
| `storm_module` | 89 | KEEP | Public API retained; see file-level decision. |
| `StormValue` | 104 | KEEP | Public type/const of the Storm ordinal facade layer; retained. |
| `StormFile` | 114 | KEEP | Public type/const of the Storm ordinal facade layer; retained. |
| `StormApi` | 123 | KEEP | Public type/const of the Storm ordinal facade layer; retained. |
| `new` | 135 | KEEP | Public API retained; see file-level decision. |
| `mount` | 143 | KEEP | Public API retained; see file-level decision. |
| `call` | 149 | KEEP | Public API retained; see file-level decision. |
| `unimplemented_game_imports` | 286 | KEEP | Public API retained; see file-level decision. |
| `validate_exports` | 297 | KEEP | Public API retained; see file-level decision. |

### `terrain.rs` — TerrainArt tiles/cliffs/water/weather

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (world local palette is duplicate, to delete)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/terrain.rs`
- Call path: world/terrain, scan, re
- Evidence: terrain::tests::resolves_real_terrain_tiles (98 textures)

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `TerrainTile` | 26 | KEEP | Public type/const of the TerrainArt tiles/cliffs/water/weather layer; retained. |
| `texture_path` | 43 | KEEP | Public API retained; see file-level decision. |
| `variant_uv` | 48 | KEEP | Public API retained; see file-level decision. |
| `TerrainArt` | 57 | KEEP | Public type/const of the TerrainArt tiles/cliffs/water/weather layer; retained. |
| `load_dir` | 65 | KEEP | Public API retained; see file-level decision. |
| `load_terrain` | 76 | KEEP | Public API retained; see file-level decision. |
| `tile` | 114 | KEEP | Public API retained; see file-level decision. |
| `texture_paths` | 118 | KEEP | Public API retained; see file-level decision. |

### `trigger.rs` — TriggerData/TriggerStrings registry

- Canonical owner: **wc3-re (canonical)**
- Root counterpart: — (unique)
- Decision: **KEEP**
- Target: `crates/wc3-re/src/trigger.rs`
- Call path: HUD/touch, scan, re
- Evidence: trigger::tests::parses_real_trigger_data (676)

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `GuiFunction` | 32 | KEEP | Public type/const of the TriggerData/TriggerStrings registry layer; retained. |
| `arg_count` | 44 | KEEP | Public API retained; see file-level decision. |
| `TriggerStrings` | 50 | KEEP | Public type/const of the TriggerData/TriggerStrings registry layer; retained. |
| `for_section` | 68 | KEEP | Public API retained; see file-level decision. |
| `TriggerData` | 79 | KEEP | Public type/const of the TriggerData/TriggerStrings registry layer; retained. |
| `load_dir` | 94 | KEEP | Public API retained; see file-level decision. |
| `parse` | 103 | KEEP | Public API retained; see file-level decision. |
| `all_functions` | 142 | KEEP | Public API retained; see file-level decision. |
| `string_coverage` | 152 | KEEP | Public API retained; see file-level decision. |
| `script_coverage` | 172 | KEEP | Public API retained; see file-level decision. |

### `txt.rs` — Ordered text tables with preamble/case-insensitive sections/quoted lists

- Canonical owner: **wc3-formats::gamedata::TextTable (parser); wc3-re view**
- Root counterpart: wc3-formats/src/gamedata.rs:1704 TextTable, :1710 parse
- Decision: **DELEGATE (implemented)**
- Target: `crates/wc3-re/src/txt.rs`
- Call path: objectdata, skin, trigger, scan
- Evidence: txt::tests (2), scan 67/67 TXT

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Section` | 16 | DELEGATE | Public type/const of the Ordered text tables with preamble/case-insensitive sections/quoted lists layer; retained. |
| `get` | 22 | DELEGATE | Public API retained; see file-level decision. |
| `get_all` | 30 | DELEGATE | Public API retained; see file-level decision. |
| `TextTable` | 40 | DELEGATE | Public type/const of the Ordered text tables with preamble/case-insensitive sections/quoted lists layer; retained. |
| `parse` | 48 | DELEGATE | Public API retained; see file-level decision. |
| `section` | 81 | DELEGATE | Public API retained; see file-level decision. |
| `section_names` | 87 | DELEGATE | Public API retained; see file-level decision. |
| `get` | 91 | DELEGATE | Public API retained; see file-level decision. |
| `entries` | 95 | DELEGATE | Public API retained; see file-level decision. |
| `split_list` | 104 | DELEGATE | Public API retained; see file-level decision. |
| `split_list_quoted` | 111 | DELEGATE | TriggerData display layouts; keeps comma separators inside quotes. |

### `wts.rs` — WTS map strings + escapes + multi-reference resolve

- Canonical owner: **wc3-formats::gamedata::Wts (parser); wc3-re view**
- Root counterpart: wc3-formats/src/gamedata.rs:1616 Wts, :1620 parse
- Decision: **DELEGATE (implemented)**
- Target: `crates/wc3-re/src/wts.rs`
- Call path: mapfiles, scan
- Evidence: wts::tests (2), Human04 Cyrillic string

| public item | line | decision | behavior / note |
|---|---:|---|---|
| `Wts` | 13 | DELEGATE | Public type/const of the WTS map strings + escapes + multi-reference resolve layer; retained. |
| `parse` | 18 | DELEGATE | Delegates to `wc3_formats::gamedata::Wts::parse`; applies escape decoding. |
| `get` | 31 | DELEGATE | Public API retained; see file-level decision. |
| `resolve` | 36 | DELEGATE | Replaces every TRIGSTR_n occurrence (including embedded), stronger than typed Wts::resolve. |

## 4. Duplicates removed / thinned (with proof)

| duplicate | removed from | canonical | proof |
|---|---|---|---|
| PKWARE/Huffman/ADPCM decoders | `wc3-re/src/mpqex/*.rs` | `wc3-mpq/src/*.rs` | byte-identical diff for huffman/adpcm; type-identity test; Human04 wts test |
| MPQ hash/block/decrypt/sector reader | `wc3-re/src/mpqex.rs` | `wc3-mpq::MpqArchive` | `MpqEx` now only opens/reads/meta; `mpqex` tests pass through MpqArchive |
| SLK parser | `wc3-re/src/slk.rs` | `wc3-formats::slk::SlkTable` | `merge_parity::slk_delegation_preserves_scanner_totals` (6,785 rows / 815 cols) |
| WTS parser | `wc3-re/src/wts.rs`, `wc3-formats/src/map.rs::parse_wts` | `wc3-formats::gamedata::Wts` | `Wts::parse` delegates; map resolves via typed Wts; Human04 Cyrillic test |
| Text table parser | `wc3-re/src/txt.rs` | `wc3-formats::gamedata::TextTable` | ordered view + preamble retained; scan TXT 67/67 |
| Object-data parser | `wc3-re/src/mapobjects.rs`, `wc3-formats/src/objects.rs` | `wc3-formats::gamedata::ObjectDataFile` | `merge_parity::object_data_parsers_agree_on_real_v1_map`; w3a extended test |
| Kind table bug (w3d/w3b/w3h) | `wc3-formats::gamedata::ObjectKind::extension` | HiveWE loader anchor | `object_kind_extensions_match_hivewe_loader` |
| HM3W/MPQ base scan | `wc3-re/src/storm.rs`, `wc3-re/src/mapfiles.rs`, `wc3-re/src/mpqex.rs` | `wc3-formats::map::map_mpq_base` | storm/mapfiles/mpqex call it; copy removed |
| Second JASS implementation | `wc3-jass` (kept for sim) | `wc3-re::jass` | `wc3-jass::canonical` adapter + parity test; legacy VM retained, not deleted |
| MDX chunk walker | `wc3-re::mdx::chunks` | `wc3-formats::mdx::MdxModel::chunks` | patch 0002 applied; `wc3-re::mdx::chunks` projects the list (offset-8, String tag) |
| Doodad placement parser | `wc3-game/src/doodads.rs::parse_doo` | `wc3-formats::map::parse_doodads` | new canonical parser + `MapBundle.doodads`; world deletes the private reader |

## 5. Public API delivered to the game/base

- `wc3_game::re::ReLayer::{open,object,object_info,typed,icon_audit,localized_map_name,engine_config,boot_engine}` (bridge; typed DB `object_info` included).
- `wc3_re::scan::{ScanReport,BlpScan,MdxScan,SlkScan,TxtScan,FdfScan,ScriptScan,MapScan,ObjectScan,TerrainScan,TriggerScan,SkinScan,ArchiveScan}` (`ScanReport::{run,ok,to_json,to_markdown}`).
- `wc3_re::engine::{Engine,EngineConfig,EngineState,BootStep}` (`Engine::{boot,enter_map,tick,shutdown}`).
- `wc3_re::host::{HOST_IMPORTS,HostServices,Launcher,LaunchPlan,WindowState}` (`HostServices::{call,frame,launch}`, `Launcher::launch`).
- `wc3_re::storm::{StormFs,StormLayer,FileInfo}` (`StormFs::{open_wc3_install,add_archive,add_loose_dir,read,info,listfile,search}`).
- `wc3_re::storm_api::{StormApi,StormValue,StormFile,STORM_EXPORTS,GAME_IMPORTS_STORM}` (`StormApi::call`, `validate_exports`).
- `wc3_re::jass::{parse,Program,Runtime,World,Value}` (`Runtime::{new,run_function,run_main,execute_trigger}`); `wc3_jass::canonical::{parse,parse_named,merge,run,run_sources,run_program,RunReport}`.
- `wc3_formats::map::map_mpq_base` (shared container base detection).
- `wc3_formats::gamedata::Wts::iter` (ordered view for resource layer).
- `wc3_formats::objects::ObjectData::parse_as(kind, bytes)` (kind-aware compatibility).
- `wc3_re::mapobjects::MapObjectData::parse_as(kind, bytes)` (kind-aware map overrides).
- `wc3_mpq::{pkware,huffman,adpcm}` public modules (canonical codecs).

## 6. Verification contract (filled after freeze)

| gate | command | expected | rc | evidence |
|---|---|---|---|---|
| workspace graph | `cargo metadata --no-deps` | one `wc3-re`, one of each crate | **0** | 28 packages, 0 duplicate names; `wc3-data` joined the root graph |
| MPQ | `cargo test -p wc3-mpq` | 10 tests (9 lib + Human04) | **0** | 9 lib + 1 Human04 pass |
| Formats | `cargo test -p wc3-formats` | all tests incl. objects/gamedata | **0** | 58 pass |
| RE | `cargo test -p wc3-re` | 44 + parity/runtime | **0** | 44 lib (incl. 9 `jass::natives::parity_tests`) + 5 merge_parity + 1 re_surface + 2 real_corpus + 3 runtime_wiring |
| Data | `cargo test -p wc3-data` | real_data suite | **0** | 34+2+8+20+1 pass |
| JASS | `cargo test -p wc3-jass` | 6 tests (4 legacy + 2 canonical) | **0** | 6 pass |
| Base suite | 8 core crates | all pass | **0** | **322 tests pass** (35 suites) |
| Render | `cargo test -p wc3-render --lib` | GPU scope tests | **0** | 25 pass |
| Game check | `cargo check -p wc3-game --lib` | 0 errors | **0** | finished, 6 warnings; `analysis/agent-re/check-game-r3.log` |
| Game tests | `cargo test -p wc3-game` | all pass | **0** | 63 lib (5 ignored) + integration suites; world doodads 2,509/2,509, seam 1.301; ui_fdf 10/10 |
| Scan | `wc3re scan` | 3935 BLP / 2048 MDX / 44 maps / 0 errors / 0 unimplemented natives | **0** | overall true; 67 TXT, 4,428 WTS, 0 map errors, `scripts.unimplemented={}`, 44/44 `main`; `analysis/agent-re/scan-final.json` |
| APK | `cargo build --release -p herofolends-android` | success | **0** | first build 4m16s, libmain.so 7.3 MB; final rebuild after last fixes |

## 7. Open conflicts / decisions

- `wc3-game/src/assets.rs` is frozen: the MapBundle/WTS patch is `analysis/tech-lead/patches/0001-assets-mapbundle-wts.patch`; until applied, `W3Map` remains a compatibility view.
- Anim layer proposes `wc3-anim` as the single animator; the game/render mesh types (`wc3-gpu` vs `wc3-game::assets::MeshData`) need one canonical owner (manifest merge-anim.md §4).
- HUD layer proposes deleting the local `SkinTable`, icon path list and generated chrome; those are presentation duplicates of `wc3-re::skin/icons` and real BLPs (manifest merge-hud.md).
- Render layer notes `MapBundle` does not expose `war3map.doo` bytes; world layer must either parse via `MapBundle` naming or add a typed accessor (manifest merge-render.md).

## 8. Post-matrix merges (2026-11-13 execution)

| merge | source | target | proof |
|---|---|---|---|
| MDX chunk walker + node-object count | `wc3-re::mdx`, anim audit CLI | `wc3-formats::mdx::{MdxModel::chunks,node_object_count,Chunk}` | patch 0002 applied; `chunk_walk_and_node_pivot_count_match_real_model`; 542-model sweep 528 PASS/14 WARN/0 FAIL |
| BLP1 all-mip chain | `wc3-formats::blp` | same | patch P1 applied; 28,748/28,748 mips decoded, `mips_incomplete_files=0` |
| Doodad placement parser | `wc3-game/doodads.rs::parse_doo` | `wc3-formats::map::parse_doodads` + `MapBundle.doodads` | doodads test 2,509/2,509 placed, 86 models, 0 failures |
| Typed object DB bridge | `wc3-data` | `wc3-game::re::ReLayer::{typed,object_info,typed_from_objects}` | `typed_object_info_resolves_footman` passes (name/icon/stats) |
| ObjectDb sheet-key normalization | `wc3-re::objectdata` | same | `load_storm` uses `\`-last-component stems; `slk_field` lowercases/strips path; archive `LSst.dir` resolves |
| JASS→sim orders | `wc3-re::jass` + `wc3-jass` | `ScriptOrder` + `sim_bridge` | patches applied; `wc3 check -p wc3-re -p wc3-jass -p wc3-game --lib` rc 0; sim 99 + bridge 4 tests |
| Workspace unification | root | `wc3-data` member; nested `[workspace]` removed | `cargo metadata` 28 packages, 0 duplicate names |
| One renderer | `wc3-gpu` | `wc3-render` | `wc3-gpu` is a 44-line re-export facade; render 25 tests |

## 9. FULL MERGE absorption (scan/engine/host/storm/JASS, 2026-11-13)

All five concerns are absorbed into `wc3-re` as the canonical resource/engine API;
no second reader/model/VM exists outside it. `wc3-jass` keeps the legacy sim VM
only and exposes the canonical engine through `wc3_jass::canonical`.

| concern | canonical entry point | game/base caller | duplicate check | evidence |
|---|---|---|---|---|
| install scan | `wc3_re::scan::ScanReport::{run,ok,to_json,to_markdown}` (`scan.rs:190/562/575/745`) | `wc3re scan`; `wc3-re/tests/re_surface.rs:18` type pin | no other `ScanReport` in `crates/`/`apps/` | `analysis/agent-re/scan.json` `ok=true`; 3,935 BLP / 2,048 MDX / 33 SLK / 67 TXT / 71 FDF / 44 maps / 91 AI / 4,428 WTS / 0 errors |
| Game.dll loop | `wc3_re::engine::Engine::{boot,enter_map,tick,shutdown}` (`engine.rs:96/195/251/330`) | `wc3_game::re::ReLayer::boot_engine` (`re.rs:161`) | no other `Engine` model outside `wc3-re` | `engine::tests::boots_headless_on_extracted_install`; GameMain pdc (`analysis/agent-re/r2/`) |
| host launcher | `wc3_re::host::{HostServices,Launcher,HOST_IMPORTS}` (`host.rs:29/77/107/131/181`) | RE API / `wc3re info`; `wc3-game` boots through `Engine` | no second host-ordinal table | `host::tests::exports_and_imports_match_pe_evidence`; `r2/SUMMARY.md` |
| Storm FS / ordinals | `wc3_re::storm::StormFs` (`storm.rs:101`), `wc3_re::storm_api::StormApi` (`storm_api.rs:123`) | `ReLayer::open` uses `StormFs`; `wc3re info` reads ordinal tables | `mpqex` is a facade; no hash/sector code in `wc3-re` | `storm` test, `storm_api::export_table_matches_observed_pe` (370 ordinals / 92 imports) |
| JASS2 VM | `wc3_re::jass::Runtime` (`jass/vm.rs:451`; methods `:479/549/554/883`) | `wc3_jass::canonical` (`canonical.rs:22-74`); `sim_bridge` consumes `ScriptOrder` | legacy `wc3-jass::JassRuntime` retained only for `wc3-sim`; canonical adapter parity test | 44/44 map `main`, 91/91 `InitAI`, `legacy_and_canonical_engines_agree_on_arithmetic` |

Scope compile gate after absorption: `cargo check -p wc3-re -p wc3-jass -p wc3-game --lib`
rc **0** (`analysis/agent-re/check-fullmerge.log`). Per-layer tests:
`cargo test -p wc3-re -p wc3-jass` rc **0** (33 lib + 5 parity + 1 surface +
2 corpus + 6 JASS). No device/APK run in this round.

## 10. Round-2 runtime wiring (2026-11-13)

- `wc3_re::engine::Engine::script_report()` exposes map `main`/`InitCustomTriggers`
  trigger and action execution counts; `runtime()`/`runtime_mut()` expose the
  canonical JASS world to the game bridge.
- `Engine::load_ai_scripts(Scripts/)` parses all `.ai` via `wc3-re::ai`;
  `runtime_wiring::ai_scripts_load_and_run_init` proves `InitAI` runs.
- `HostServices::launch` → `Engine::boot` → `StormFs` → map script is covered by
  `runtime_wiring::host_launch_mounts_storm_and_runs_map_script`.
- `MapBundle` keeps unsupported optional `war3mapUnits.doo` variants out of
  `errors` (v8 random tables / BonusCredits truncation), so scan remains
  `0` map errors; object/WTS overrides are covered by
  `runtime_wiring::object_data_and_wts_overrides_reach_object_db`.
- Map placement: `Engine` spawns `war3mapUnits.doo` units and `war3map.doo`
  doodads/destructables into the JASS `World` with type+position de-duplication;
  `Engine::map_spawn()` returns `MapSpawnReport`.
- Evidence: `analysis/agent-re/check-r2.log`, `test-r2.log`, `scan-r2.json`,
  `SCAN-r2.md` (scope rc 0; 35 lib + 5 + 1 + 2 + 3 + 6 tests; scan ok=true,
  211.24 s, 44/44 maps, 4,428 WTS, 0 errors).

## 10.1 Round-3 PARITY #4/#6/#7 (2026-11-13)

- **#4 map units**: `wc3_formats::map::parse_units` follows HiveWE
  `src/base/units.ixx` exactly — random payload by `random_type`
  (`0`=4 B, `1`=8 B, `2`=count*8 B, other=0 B), `subversion >= 11` item
  table/hero stats, `version >= 13` group/lights. Human04 90/90 placements;
  all 44 maps' unit tables parse (6,723 placements, 468 engine-spawned,
  6,255 matched to `CreateAllUnits`); `Engine::map_spawn`/`MapSpawnReport`;
  `Engine::map()` accessor. Proof: `runtime_wiring::host_launch_*` asserts every
  placement exists in the JASS world; `merge_parity` asserts Human04 == 90 and
  zero `MapBundle.errors`.
- **#7 natives**: `wc3re scan` `scripts.unimplemented = {}` (was 92 names /
  3,073 call sites); `jass::natives::parity_tests` (9 per-native gates) covers
  the top-weight names plus `ForForce`/`GetEnumPlayer` context.
- **#6 game loop**: `analysis/tech-lead/patch-game-parity46.diff` (verified
  rc 0; `cargo test -p wc3-game --lib map_units_spawn` passes when applied;
  `game.rs` restored byte-identically after verification).
- **Headless artifact**: `wc3re parity` → `analysis/agent-re/qa-jass-map-runtime.json`
  + `MAP_RUNTIME.md` (44/44 `main`, 713 executed triggers, 1,313,713 steps);
  stdlib gate `analysis/agent-re/qa_map_runtime.py` (`qa.jass.map_runtime`) rc 0.
- Evidence: `analysis/agent-re/test-final.log` (rc 0, 44 lib + 5 + 1 + 2 + 3 +
  6), `check-game-r3.log` (rc 0), `scan-final.json`, `game-map-units-spawn.log`.

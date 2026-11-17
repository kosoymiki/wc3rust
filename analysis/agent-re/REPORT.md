# REPORT — agent-re (full WC3 base RE / decompilation / Rust port)

## Goal

Deliver the complete local-base reverse-engineering and port layer for the
operator's own Warcraft III RoC data into `crates/wc3-re`: byte-level
provenance of every original binary/archive/script/table, r2 deep analysis,
a real JASS/trigger VM, object-data/ID mapping, Storm API facade, Game.dll
engine and War3.exe host models, a canonical resource-access API, and a
whole-install verification gate. No sample-only claims, no stubs, no refusal —
the operator owns this game and this is in-scope RE.

## Actions

- Created `crates/wc3-re` (26 modules, ~9.5k lines) and added it to the root
  workspace (`Cargo.toml` member; standalone `[workspace]` removed).
- Implemented the resource-access layer: `storm`, `mpqex` (correcting MPQ
  reader), `blp`, `mdx`, `slk`, `txt`, `wts`, `trigger`, `objectdata`,
  `mapobjects`, `mapfiles`, `skin`, `terrain`, `icons`, `jass/{lexer,parser,
  vm,natives}`, `ai`, `engine`, `host`, `storm_api`, `scan`.
- Added the whole-corpus scanner (`wc3re scan`) and integration tests.
- Root-caused and fixed the encrypted PKWARE sector failure with byte-level
  evidence and a StormLib C-donor ground truth (`PKWARE_FIX.md`).
- Built `analysis/agent-re/r2/`: PE headers/sections/exports/imports for 12
  binaries, full function lists, `GameMain`/init/shutdown/Storm/War3 entry
  pdc, all 370 Storm ordinals with RVAs, 92 Game→Storm imports, TFT delta.
- Wrote `COVERAGE.md` (generated from `scan.json` + `INVENTORY.tsv`),
  `RESOURCE_ACCESS.md`, `INTEGRATION.md`, `PKWARE_FIX.md`, this report.
- H2: capture 566, experiences 4281/4282/4283, monitor ledger 308 closed
  `completed` with verification `ok=true`.

## Evidence

### A. PKWARE / encrypted sector root cause (fix, not workaround)

- `Human04.w3m` wts block: offset `0x2e7b7`, csize `0x9a5` (2,469), size
  `0x195c` (6,492), flags `0x80030200` (exists|compressed|encrypted|FIX_KEY;
  file mask `0x02`).
- Sector table at `0x2e9b7` decrypts to `[12, 1485, 2469]`.
- Sector 0 stored length 1,473; StormLib reads the physical encrypted length
  rounded to a dword (1,476), decrypts as dwords, then truncates to 1,473.
- Decrypted sector tag is `0x08` (PKWARE) even though the file mask is zlib.
  Payload begins `00 06 a6 50 91 92 c4 ...`.
- StormLib `donors/StormLib/src/pklib/explode.c` compiled with the C harness
  (`analysis/agent-re/tmp/pk_test.c`) produced **4096 bytes** beginning
  `STRING 0\r\n{` and returned `CMP_ABORT` (4) because `Expand()` returns
  `0x306` at end-of-input after the full output. This is the documented
  StormLib behavior, not a corrupt stream.
- Fix in canonical `wc3-mpq`: `decompress_into` receives the declared sector
  length, trusts a known sector tag before the mask, and `pkware::explode`
  accepts `0x306` only when output is non-empty; the caller enforces
  `decoded.len() >= expected`. `wc3-re::mpqex` is now a facade/re-export only.
- Evidence: `mpqex::tests::reads_encrypted_pkware_wts_from_roc_map`,
  `mpqex::tests::reads_all_inner_files`,
  `mapfiles::tests::opens_real_roc_campaign_map` (WTS > 50 entries and
  Cyrillic content), `scan maps.errors=[]`, `wts_entries=4428`.

### B. W3U v1 format fix

- Human01 `war3map.w3u` (2,490 B): `version=1`, base table 11 entries, then
  new table 28 entries; total parsed to EOF.
- First bytes: `nban 00000000 01000000 uabi 03000000 00 00000000 ngno` —
  NUL-terminated string + u32 sanity, not length-prefixed. Evidence in
  `analysis/agent-re/tmp/h01.w3u`; donor:
  `War3Net .../SimpleObjectModification.cs:17-25` and
  `War3Net.Common/Extensions/BinaryReaderExtensions.cs:5` (`ReadChars`).
- Result: 44 maps, 1,009 object entries, 0 errors (was 38 failing maps).

### C. Final test run (root workspace, 2026-11-12)

Command:

```bash
cd /data/data/com.termux/files/home/src/herofolends-rs
ulimit -v 4000000
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p wc3-re --offline
```

Observed (`analysis/agent-re/cargo-test.log`):

```
unittests src/lib.rs:      31 passed; 0 failed; 0 ignored
unittests src/bin/wc3re:    0 passed; 0 failed
tests/real_corpus.rs:       2 passed; 0 failed; 0 ignored
```

The earlier 27/2 review state is stale: the two failures were fixed
(PKWARE sector-0 and WTS read) and the green suite is now 31 + 2 = 33 tests.

`cargo check -p wc3-re --offline` from the root: `0 errors, 27 warnings`
(warnings are dead-code/unreachable duplicate native arms, not failures).
`cargo metadata` lists `crates/wc3-re/Cargo.toml` as a workspace package.

### D. Whole-install scan (PASS)

`wc3re scan analysis/war3_full3 --json analysis/agent-re/scan.json --md
analysis/agent-re/SCAN.md` → `RC=0`, `ok=true`, 92.81 s:

| subsystem | result |
|---|---|
| BLP decode | 3,935/3,935 (98,722,207 px; 2,920 JPEG + 1,015 paletted) |
| MDX parse | 2,048/2,048 (20,502 chunks, 5,127 geosets, 4,209 materials) |
| SLK | 33/33 (6,785 rows, 815 columns) |
| TXT | 67/67 (2,532 sections, 19,678 entries) |
| FDF | 71/71 (1,956 nodes, 6,514 props) |
| JASS source | 3/3; 641 functions, 791 natives |
| Map scripts | 44/44 parsed; 44/44 `main` executed; 1,311,867 steps; 0 failures |
| AI | 91/91 parsed; 91 `InitAI` runs; 0 failures |
| Maps | 44/44 opened; 708 inner files; 0 errors; 4,428 WTS strings |
| Object DB | 11 sheets, 1,665 objects, 117 metadata fields, 1,182 icons |
| TriggerData | 676 functions; 25 events / 24 conditions / 326 actions / 301 calls; 676/676 display strings |
| TerrainArt | 98/98 texture paths resolve |
| Skins | 5 sections, 319 texture entries; 22 absent from this RoC base (reported) |

### E. r2 / PE

`analysis/agent-re/r2/SUMMARY.md` (SHA-256 from `INVENTORY.tsv`):

| binary | size | r2 funcs | exports | imports |
|---|---:|---:|---:|---:|
| War3.exe | 713,494 | 1,913 | 695 (A1..A695) | 487 |
| Game.dll RoC | 7,725,118 | 24,537 | 1 (GameMain 0x6f004180) | 931 |
| Storm.dll RoC | 311,360 | 730 | 370 (ordinals) | 226 |
| WorldEdit.exe | 3,470,102 | 10,924 | 0 | 625 |
| Game.dll TFT | 8,671,295 | 28,001 | 1 (GameMain 0x6f005e70) | 725 |
| Storm.dll TFT | 356,417 | 847 | 451 | 239 |

370 Storm ordinals (19 named from `StormLib/storm.dll/storm.def`), 92
Game→Storm imports, `GameMain`/init/shutdown pdc, TFT delta.

## Result

**DONE.** `crates/wc3-re` is a working, tested, documented canonical access
layer and RE port for the local base. All 44 map scripts execute; all textures
and models parse; object data/trigger/UI/terrain access is typed and covered by
tests. Remaining gaps are explicit and owned (see below), not silent.

### Subsystem classification

| subsystem | status |
|---|---|
| Storm/MPQ (incl. encrypted PKWARE) | PORTED + TESTED |
| BLP1/BLP2 decode | PORTED + TESTED (all 3,935) |
| MDX chunk/model | PORTED + TESTED (all 2,048) |
| SLK/TXT/FDF/WTS | PORTED + TESTED |
| TriggerData/TriggerStrings | PORTED + TESTED (676/676) |
| Object DB + W3U mapping | PORTED + TESTED |
| JASS lexer/parser/VM | PORTED + TESTED (44 mains) |
| AI scripts | PORTED (parse + `InitAI`) |
| Engine loop/state | PARTIAL (RE-derived boot/shutdown; frame order inferred) |
| Host launcher | PARTIAL (typed model from PE evidence) |
| Storm API facade | PARTIAL (file API + ordinal map; 19 names) |
| Audio/renderer | NOT-STARTED here (owned by `wc3-audio`/`wc3-render`) |

## Risks / Next

1. `War3.mpq` (438,529,036 B) and the ISOs are absent from the repo/bounded
   search; the extracted 10,523-file tree is the base. Copy the archive/ISO and
   rerun `wc3re scan` to re-derive archive-internal provenance.
2. TFT `War3x.mpq`/`War3xlocal.mpq` open (5,610/1,156 listfile entries) but are
   not yet ingested; diff TFT SLK/object/UI tables next.
3. 351 Storm ordinals remain unnamed (PE has ordinal-only exports); classify
   by source-file string xrefs rather than invent names.
4. 22 skin paths are absent from this RoC base (TFT/UI models), reported by
   scan. No silent drop.
5. JASS native census is reported (top: `triggerregisterplayerstateevent`,
   `unitsharevision`, `questcreateitem`); these are platform/render/audio
   surfaces for other owners. Map `main` still completes 44/44.
6. Engine per-frame phase order is inferred; boot/shutdown call sites are
   address-backed. Decompile the host frame callbacks for exact ordering.

## H2_PERSISTED

- `capture_id`: **566** (main build)
- `final_capture_id`: **567** (post-review strict sector decode)
- `monitor` final `h2_ids_json`: `{"capture_id":566,"final_capture_id":567,"experiences":[4281,4282,4283]}`
- `experience_id`: **4281** (PKWARE encrypted sector dword + 0x306)
- `experience_id`: **4282** (W3U v1 two-table/NUL string)
- `experience_id`: **4283** (measure local crates before porting formats)
- `monitor`: ledger **308**, state **completed**, verification `ok=true`.

## SESSION_CLOSED

Ledger 308 closed `completed`; `last_verification.ok=true`, `terminal=true`,
no warnings/missing checks. Artifacts: `crates/wc3-re`, `analysis/agent-re/`
(COVERAGE, RESOURCE_ACCESS, INTEGRATION, PKWARE_FIX, scan.json/SCAN.md,
INVENTORY.tsv, cargo-test.log, r2/).

## Merge-continuation verification (2026-11-13)

- `cargo check -p wc3-re -p wc3-jass` → rc **0**.
- `cargo test -p wc3-re -p wc3-jass` → rc **0**; 32 lib + 5 merge_parity +
  1 re_surface + 2 real_corpus + 6 wc3-jass tests.
- `wc3re scan analysis/war3_full3` → rc **0**, `ok=true`, 183.06 s; 3,935 BLP,
  2,048 MDX, 33 SLK, **67 TXT**, 71 FDF, 44/44 maps, 44/44 map `main`,
  91/91 AI, **4,428 WTS**, 676 TriggerData, 1,182 icons, 0 map errors.
- `capture_id`: **579** (merge port), **584** (static cleanup), **598** (green run).
- `experience_id`: **4300**, **4301**, **4302**.
- `monitor`: ledger **313**, state **completed**, verification `ok=true`.
- Manifest: `analysis/tech-lead/merge-re.md`; provenance:
  `analysis/agent-re/PROVENANCE_MAP.md`.

The earlier capture 566/567 and monitor 308 remain the pre-merge RE evidence;
they are not used to claim post-merge verification.

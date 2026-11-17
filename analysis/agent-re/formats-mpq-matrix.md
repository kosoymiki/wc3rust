# Formats / MPQ donor intake — byte matrix (ledger 381)

Group: `formats/MPQ`. Donors: `donors/StormLib` (234 C, MPQ reference),
`donors/CascLib` (CASC container), `donors/War3Net` (C# WC3 formats + 99 test
maps). Our side: `crates/wc3-mpq`, `crates/wc3-formats`.

Evidence tier: **T1** = read at `file:line` in this workspace. Every verdict
below is T1; no README claim is used. Access date for all paths: 2026-11-14.

## 1. MPQ container

| # | Donor implementation (T1) | Mechanism | Our counterpart | Verdict |
|---|---|---|---|---|
| M1 | `donors/StormLib/src/SCompression.cpp:907-927` `dcmp_table` | v0 codec tag is a **bit mask**; decoders run in table order (bzip2, pkware, zlib, huffman, adpcm-stereo, adpcm-mono, sparse) | `crates/wc3-mpq/src/lib.rs` `decode_v0_mask` | **PORT** — the old code mapped one tag to one codec and could not read 0x01 (huffman only) or any chain |
| M2 | `donors/StormLib/src/SCompression.cpp:1048-1143` `SCompDecompress2` | v1+ fixed switch: 0x02/0x08/0x10/0x12(LZMA)/0x20(sparse)/0x22(zlib→sparse)/0x30(bzip2→sparse)/0x41/0x81 | `decode_v2` | **PORT** |
| M3 | `donors/StormLib/src/StormLib.h:278-286` | LZMA = **0x12**, sparse = **0x20** ("this value is NOT a combination of flags") | `MPQ_COMPRESSION_*` constants | **FIX** — our reader had 0x20 as LZMA and never recognised 0x12 |
| M4 | `donors/StormLib/src/sparse/sparse.cpp:211-285` `DecompressSparse` | u32 BE output size; `0x8n` = n+1 literals; `0x0n` = n+3 zeros | new `crates/wc3-mpq/src/sparse.rs` + unit tests | **PORT** |
| M5 | `donors/StormLib/src/SFileReadFile.cpp:163-210` | "some sectors may not be compressed… determined only by comparing uncompressed and compressed size"; raw==0 ⇒ sector of zeros | `decompress_unit` stored rule + `stream.rs` zero-sector branch | **PORT** — old code used `declared_len == expected` and errored on empty sectors |
| M6 | `donors/StormLib/src/SFileReadFile.cpp:191-196` | `MPQ_FILE_IMPLODE` without `MPQ_FILE_COMPRESS` = tag-less PKWARE stream | `decompress_unit` implode branch; `MpqFileReader::new` includes IMPLODE in the compressed test | **PORT** |
| M7 | `donors/StormLib/src/SFileReadFile.cpp:130-140` + `SBaseCommon.cpp:627-680` | encrypted single-unit sectors keyed `key+i`; unnamed encrypted block key recovered by content (RIFF/PE/XML) | `stream.rs` `detect_key_by_content` + existing sector decryption | **PORT** (content detection new) |
| M8 | `donors/StormLib/src/SBaseCommon.cpp:992-1100` `LoadMpqTable` | tables may run past EOF: read as much as possible, zero-fill the rest, remember the cut | `open_at_indexed` `load_table` + `Header::truncated_tables` + `is_malformed()`/`table_counts()` | **PORT** — required to open Spazzler-protected maps |
| M9 | `donors/StormLib/src/SBaseFileTable.cpp:585-590` | block table size is cut to `(FileSize - BlockTablePos) / 16` | same rule in `load_table` sizing | **PORT** |
| M10 | `donors/StormLib/src/SBaseFileTable.cpp:474-481` + `SFileOpenArchive.cpp:311-360` | for WC3 maps Storm.dll **ignores the format version** (v1 is treated as v0); header found on 0x200 steps with `header_size >= 0x20` | `open_at_indexed` accepts 0/1, rejects ≥2 explicitly | **PORT (v0/v1 scope)** |
| M11 | `donors/StormLib/src/StormLib.h:252-259` `MPQ_FILE_VALID_FLAGS_W3X` + `SBaseFileTable.cpp:1056` | map block flags are masked; `SINGLE_UNIT`/`PATCH_FILE` are dropped | `MPQ_FILE_VALID_FLAGS_W3X` mask at block-table load (map detected via `.w3m/.w3x/.w3n` or `HM3W`) | **PORT** — fixes PKCompressed (bogus SINGLE_UNIT bit) |
| M12 | `donors/StormLib/src/SBaseCommon.cpp:1258-1268` | `MPQ_FILE_SECTOR_CRC` adds one dword to the sector offset table | `stream.rs` `crc_entry` | **PORT** |
| M13 | `donors/StormLib/src/SBaseCommon.cpp:1713-1740` `IsPseudoFileName` | unnamed blocks are addressable as `File########.ext` by block index | `MpqArchive::pseudo_name`/`parse_pseudo_name`/`read_block`/`existing_block_count`/`unnamed_block_count` | **PORT** |
| M14 | `donors/StormLib/src/SFileFindFile.cpp` (`szFileName` table) | listing requires names; without a listfile only pseudo-names exist | `probe_names` / `discover_known_names` (hash-probe the canonical WC3 name set) + `KNOWN_WC3_NAMES` | **PORT (adapted)** — hash probing recovers real names that StormLib cannot |
| M15 | `donors/StormLib/src/SBaseFileTable.cpp:600-700` (v2/v3/v4 headers, HET/BET, compressed tables) | 64-bit offsets, extended tables | — | **REJECT for now** — no WC3 corpus fixture; `UnsupportedVersion(v>=2)` is explicit instead of a misparse |
| M16 | `donors/CascLib/**` | CASC/TVFS/Overwatch containers (`CascDecrypt.cpp`, `overwatch/`), no MPQ header parser (only an HTTP-stream comment at `src/common/FileStream.cpp:1057`) | — | **REJECT** — different container family; nothing to port for MPQ or WC3 binary formats |

## 2. WC3 binary formats (War3Net is the serializer reference)

| # | Format | War3Net anchor (T1) | Our file | Verdict |
|---|---|---|---|---|
| F1 | `war3map.w3e` header | `Serialization/Binary/Environment/MapEnvironment.cs:12-45` (`Width = read - 1`, `Left/Bottom` f32) | `crates/wc3-formats/src/w3e.rs` | **ADAPT** — width is stored as `tiles per row` (raw value); `world_size()` already subtracts 1. Documented in the struct |
| F2 | `w3e` tile v11 | `Environment/TerrainTile.cs:23-32` (1 byte: 4-bit texture, 4-bit flags) | `w3e.rs` parse | **KEEP** — already matched |
| F3 | `w3e` tile v12+ | `Environment/TerrainTile.cs:15-22,40-44` + HiveWE `src/base/terrain.ixx:415-430` | `w3e.rs` `TILE_SIZE_V12=8`, u16 texture/flags, `texture_indices`, `trailing_bytes` | **PORT** — v12 maps failed with "unexpected terrain tile size" before |
| F4 | `w3e` variation bits | HiveWE `terrain.ixx:432-433` (`&0x1F`, `>>5`) | `TerrainTile::variation`/`cliff_variation` | **FIX** — was 4/4 bits, now 5/3 |
| F5 | `war3map.wpm` | `Environment/MapPathingMap.cs:10-30` | `crates/wc3-formats/src/wpm.rs` | **KEEP** + `trailing_bytes` for exact-consumption checks |
| F6 | `war3map.w3i` | `Info/MapInfo.cs:10-28` (v18 gate for map/editor version, v27 for game version, then 4 C strings) | `crates/wc3-formats/src/w3i.rs` | **FIX** — v8 beta files were read with 8 extra header bytes, truncating every name ("Gnoll Wood" → "od") |
| F7 | `war3mapUnits.doo` | `Widget/MapUnits.cs:10-35`, `Widget/UnitData.cs:10-90`, `MapWidgetsFormatVersion.cs` (v6..v13), `MapWidgetsSubVersion.cs` (7/9/10/11) | `crates/wc3-formats/src/map.rs` `parse_units_ex` | **FIX** — v6 rejected; group id gate v13→v12; item table/hero stats gates now `sub == 11` (and `sub == 10` for stats) per `UnitData.cs:34-54`; v12 trailing 12 bytes; v13 roll/pitch/lights |
| F8 | `war3map.doo` | `Widget/DoodadData.cs:10-62`, `Widget/MapDoodads.cs:20-43`, `Widget/SpecialDoodadData.cs:10-16` | `map.rs` `parse_doodads_ex` | **FIX** — v6 rejected; the special-doodad section (`i32 version, i32 count, 16-byte records`) was ignored |
| F9 | `war3map.w3u/w3t/w3b/w3d/w3a/w3h/w3q` | `Serialization/Binary/Object/ObjectData.cs:10-21`, `SimpleObjectModification.cs:10-29`, `SimpleObjectDataModification.cs:10-16`, `LevelObjectDataModification.cs:10-18`, `VariationObjectDataModification.cs:10-18` | `crates/wc3-formats/src/gamedata.rs` `ObjectDataFile::parse_ex` | **KEEP/VERIFY** — field order matches: `OldId, NewId, [v3 set count+ints], modCount, {Id, Type, [Level|Variation, Pointer], Value, Sanity}`. 36/36 corpus object files consume exactly |
| F10 | object value types | War3Net `ObjectDataModification.cs:5-18` (Bool/Char = 1 byte) vs Warsmash `War3ObjectDataChangeset.java:574-577` (`readInt()`) | `gamedata.rs` reads type 4/5 as i32 | **OPEN (documented)** — donors disagree; no corpus file uses 4/5 (verified by exact consumption), so the i32 reading is not falsified. Kept Warsmash semantics; tracked below |
| F11 | `war3map.shd` | `Environment/MapShadowMap.cs:10-18` (opaque byte blob) | — | **REJECT** — no structure to port (raw cells) |
| F12 | `war3map.w3r/w3c/w3s/mmp` | `Environment/MapRegions.cs`, `MapCameras.cs`, `Audio/MapSounds.cs`, `MapPreviewIcons.cs` | — | **OUT OF SCOPE** for this intake (not in the task's format list, no consumer yet) |

## 3. Bugs found and fixed (evidence)

1. **LZMA/sparse tag swap** (`lib.rs`): tag 0x20 was decoded as LZMA and 0x12
   was unsupported; StormLib defines the opposite (`StormLib.h:278-286`).
2. **Single-tag codec dispatch**: 0x01 (huffman only) and every multi-codec
   chain failed. Replaced with the mask chain / v2 switch.
3. **Zero-length and stored sectors**: `declared == expected` → now `>=`;
   raw==0 sectors emit zeros (`SFileReadFile.cpp:203-206`).
4. **`MPQ_FILE_IMPLODE` solo streams** were read as stored.
5. **Protected-map container**: block/hash tables past EOF made `open` fail
   ("failed to fill whole buffer"). Now cut like StormLib; Jurassic Park opens
   with 150/150 blocks (`table_counts` = 92673 declared / 150 real).
6. **WC3 map flag mask**: PKCompressed's bogus `SINGLE_UNIT` bit hid the sector
   offset table; masking with `MPQ_FILE_VALID_FLAGS_W3X` fixes all reads
   (verified against StormLib: `flags=0x80000200`, 10880-byte BLP1).
7. **w3e v12**: 8-byte tiles (was rejected).
8. **w3i v8**: 8 extra header bytes shifted the strings.
9. **units/doodads v6**: rejected although War3Net supports the RoC beta
   format.
10. **units field gates**: group id from v12, item table `sub==11`,
    hero stats `sub==11 || sub==10`, v12 trailing 12 bytes.
11. **doodads special table**: ignored → now parsed (`SpecialDoodad`).
12. **w3e variation bits**: 5/3 split (HiveWE).

## 4. Corpus result (gate)

Command: `cargo test -p wc3-mpq -p wc3-formats` → **rc=0** (86 tests).

`crates/wc3-formats/tests/map_corpus.rs` walks all 99 donor maps
(`donors/War3Net/tests/.../TestData/Maps`):

```
coverage: maps=99 opened=99 w3e=99 wpm=64 w3i=99 doo=99 units=98
objects={"w3a":5,"w3b":5,"w3d":7,"w3h":5,"w3q":3,"w3t":5,"w3u":6}
absent={"units":1,"wpm":35}
padded: [("Jurassic Park Survival EE v6.4.w3x","units",1),
         ("PKCompressed.w3x","w3d",4)]
```

- 99/99 containers open; 0 parse failures.
- Every parsed file is consumed **exactly** except two pinned upstream
  artifacts: 1 junk byte in the Spazzler-protected map's units file and 4 zero
  pad bytes in `PKCompressed.w3x`'s `w3d`. Both are also unconsumed by
  War3Net's readers (they do not check EOF).
- `wpm` is absent in 35 maps and `war3mapUnits.doo` in 1 (donor corpus
  composition, not a parser gap).

MPQ oracle checks (`crates/wc3-mpq/tests/listfile_less.rs`):

- `PKCompressed.w3x`: no listfile, 321 blocks / 316 existing / 303 unnamed;
  hash probing recovers 13 real names; `read_block(103)` equals the StormLib
  extraction byte-for-byte (SHA-256 pinned
  `4915edda…492f`, 10880 bytes).
- `Jurassic Park …w3x`: opens at 1536 with a cut block table (150 entries) and
  reads `war3map.wpm` (`MP3W`).

## 5. Known limitations / risks

- MPQ v2/v3/v4 headers (64-bit offsets, HET/BET, compressed tables) are
  rejected with `UnsupportedVersion`, not misparsed. No WC3 carrier uses them;
  all 99 corpus maps are v0 (one declares v1).
- `TerrainTile::texture_flags` keeps the legacy packed layout for source
  compatibility with `wc3-game`; v12 texture indices ≥16 live in
  `Terrain::texture_indices` / `texture_index(x, y)`.
- Object value types 4 (bool) / 5 (char) are read as i32 (Warsmash) while
  War3Net reads 1 byte. Unresolved donor conflict; not exercised by the corpus.
- Sector CRC checksums are skipped (offset table handled, Adler32 not
  verified); no corpus file sets `MPQ_FILE_SECTOR_CRC`.

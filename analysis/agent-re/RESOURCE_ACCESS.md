# RESOURCE ACCESS — Warcraft III base manual (byte-level)

This is the canonical access manual for the operator's local Warcraft III RoC
base (`analysis/war3_full3`, 10,523 extracted files) and the TFT archives
(`War3x.mpq`, `War3xlocal.mpq`). Every layout statement below is tied to a byte
offset, a flag, a donor `file:line`, or a runnable function/test in
`crates/wc3-re`. Counts come from `analysis/agent-re/scan.json`
(`wc3re scan`) unless a specific file is named.

Environment: Termux/Android aarch64, `r2 6.2.6`, `rabin2`, root `u:r:ksu:s0`
(`su -c id` => `uid=0`). Scan date: 2026-11-12.

## 0. Canonical ownership (BASE.md v1)

- `wc3-mpq` owns MPQ header/hash/block/sector/codec byte logic; `wc3-re::mpqex`
  is a facade and its `pkware/huffman/adpcm` modules are pure re-exports.
- `wc3-formats` owns format primitives (`blp`, `mdx`, `slk::SlkTable`, `fdf`,
  `w3e/w3i/wpm`, `gamedata::{Sheet,MetaTable,ObjectDataFile,Wts,TextTable}`).
- `wc3-re` owns the resource/engine layer and delegates every byte parser;
  `storm`, `mapfiles`, `objectdata`, `slk`, `txt`, `wts`, `mapobjects` are
  views/projections, not second readers.
- `wc3-jass::canonical` is the adapter to `wc3-re::jass`; `wc3-game::re::ReLayer`
  is the game-side bridge.

## 0.1 API map (what to call)

| need | API | file |
|---|---|---|
| open base + TFT archives, priority, loose fallback | `StormFs::open_wc3_install`, `add_archive`, `add_loose_dir` | `src/storm.rs` |
| read encrypted/PKWARE sectors correctly | `wc3_mpq::MpqArchive::read` (`mpqex::MpqEx` is a compatibility facade) | `crates/wc3-mpq/src/{stream,lib,pkware}.rs`, `src/mpqex.rs` |
| read a texture | `blp::probe`, `blp::decode` | `src/blp.rs` |
| scan/parse a model | `mdx::chunks`, `mdx::summarize`, `mdx::referenced_textures` | `src/mdx.rs` |
| spreadsheet row/column | `slk::Sheet::parse/cell/record/row_index` | `src/slk.rs` |
| `key=value` tables | `txt::TextTable::parse/section/get` | `src/txt.rs` |
| TriggerData/TriggerStrings | `trigger::TriggerData::load_dir` | `src/trigger.rs` |
| object DB / 4CC mapping | `objectdata::ObjectDb::load_dir/field_by_id` | `src/objectdata.rs` |
| map object data | `mapobjects::MapObjectData::parse/fields` | `src/mapobjects.rs` |
| map container / inner files | `mapfiles::MapBundle::open` | `src/mapfiles.rs` |
| WTS strings | `wts::Wts::parse/resolve` | `src/wts.rs` |
| UI skin / FDF decoration | `skin::SkinTable`, `skin::audit_fdf_decorate` | `src/skin.rs` |
| terrain tiles | `terrain::TerrainArt::load_dir` | `src/terrain.rs` |
| icon audit | `icons::audit` | `src/icons.rs` |
| JASS parse/execute | `jass::parse`, `jass::Runtime` | `src/jass/` |
| AI scripts | `ai::scan_ai_dir` | `src/ai.rs` |
| engine boot/tick | `engine::Engine::boot/tick` | `src/engine.rs` |
| host/Storm facade | `host::Launcher`, `storm_api::StormApi` | `src/host.rs`, `src/storm_api.rs` |
| whole-install verification | `scan::ScanReport::run` / `wc3re scan` | `src/scan.rs` |

## 1. Storm/MPQ

### 1.1 Header (v0, 32 bytes)

Verified on `War3.mpq`, `War3x.mpq`, `War3xlocal.mpq`, `setup.mpq` and all 44
maps:

```text
0x00 char[4] "MPQ\x1a"
0x04 u32     header_size (0x20)
0x08 u32     archive_size
0x0C u16     format_version (0 = classic)
0x0E u16     sector_size_shift (512 << value; WC3 = 3 => 4096)
0x10 u32     hash_table_offset
0x14 u32     block_table_offset
0x18 u32     hash_table_count (power of two)
0x1C u32     block_table_count
```

Offsets are relative to the archive base. A `.w3m` has a 512-byte `HM3W`
header before the MPQ data; `mpqex::MpqEx::open` and `StormFs::add_archive` use
`wc3_formats::map::map_mpq_base`, which scans the first 4 KiB for `MPQ\x1a`
and stores that base.

### 1.2 Hash/block tables

- Hash entry (16 B): `name1 u32`, `name2 u32`, `locale u32`, `block_index u32`.
- Block entry (16 B): `file_offset u32`, `compressed_size u32`, `size u32`,
  `flags u32`.
- Both tables are encrypted with the WC3 crypt table and the keys
  `hash("(hash table)",3)` / `hash("(block table)",3)`.
- Lookup: `start = hash(name,0) & (hash_count-1)`, linear probe; `fname1/2 =
  hash(name,1/2)`. Empty slot `block_index == 0xFFFFFFFF` terminates; deleted
  slot `0xFFFFFFFE` is skipped. `hash()` uppercases bytes and uses
  `seed1=0x7FED7FED`, `seed2=0xEEEEEEEE`; the 0x500-entry crypt table is
  generated from `seed=0x100001` with `seed = seed*125+3 mod 0x2AAAAB`.
- Evidence: `crates/wc3-mpq/src/lib.rs` (`hash_string`, `find_block`,
  `decrypt_words`); local archives and 44 maps open with 0 hash failures.

### 1.3 Flags and encryption

| flag | meaning |
|---:|---|
| `0x80000000` | file exists |
| `0x02000000` | delete marker |
| `0x01000000` | single unit (one compressed block, no sector table) |
| `0x00010000` | encrypted |
| `0x00020000` | FIX_KEY |
| `0x00000200` | compressed |
| `0x00000100` | imploded (legacy) |

File key = `hash(basename_uppercase,3)`; with FIX_KEY:
`key = (key + file_offset) ^ size`. Encrypted-compressed sector table uses
`key-1`; each sector uses `key+sector_index`. Encrypted-uncompressed files
encrypt each sector with `key+sector_index` (not the whole file). Evidence:
`crates/wc3-mpq/src/stream.rs` and `lib.rs::decompress_into`;
`wc3-re::mpqex` is a facade; Human04 WTS read.

### 1.4 Compression codecs

File mask = `(flags >> 8) & 0xFF`. Known bits:
`0x01` Huffman, `0x02` zlib, `0x08` PKWARE DCL, `0x10` bzip2, `0x12` LZMA
(composite tag), `0x40` ADPCM mono, `0x80` ADPCM stereo, `0x41`/`0x81`
Huffman+ADPCM.

Per-sector payload (compressed multi-sector files) starts with a tag byte:
`0x02` zlib, `0x08` PKWARE, `0x10` bzip2, `0x20` LZMA, `0x40`/`0x80` ADPCM,
`0x41`/`0x81` Huffman+ADPCM. A sector whose stored length equals the expected
uncompressed length is stored verbatim even if compressed is set.

**Sector table + encrypted tail (critical):** table has `sector_count+1` u32
entries. `sector_count = ceil(size / sector_size)`. The canonical reader reads
the exact declared sector length (`end-start`), decrypts whole dwords only and
leaves the trailing 1..3 bytes plaintext (`wc3-mpq/src/stream.rs:194-205`).
Do not pad-and-decrypt the tail. `decompress_into` receives the declared sector
length separately so a stored sector is detected by
`declared_len == expected`, not by the possibly-dword-padded buffer length.
Evidence and root cause: `analysis/agent-re/PKWARE_FIX.md`.

Local example (`Human04.w3m`): wts block offset 0x2e7b7, csize 0x9a5, size
0x195c, flags 0x80030200; table `[12, 1485, 2469]`; sector 0 tag `0x08`
(PKWARE) despite file mask `0x02` (zlib); StormLib donor explode yields 4096
bytes beginning `STRING 0\r\n{`.

### 1.5 Paths, listfile, priority, locale

- Paths are case-insensitive; `/` and `\` are interchangeable; leading
  separators are stripped (`normalize_path`).
- `(listfile)` is a normal archive file. Maps often have no listfile; the
  loader probes the fixed inner names in §9.
- `StormFs::open_wc3_install` adds layers in this priority order (first wins):
  `War3Patch.mpq`, `patch.mpq`, `War3xlocal.mpq`, `War3x.mpq`, `War3.mpq`,
  `setup.mpq`. Loose trees are consulted after all archives; first loose root
  wins.
- `locale` is parsed/stored (`StormFs`/`StormApi`); the hash table `locale`
  field is not used for filtering yet. This is an explicit gap.
- Evidence: `storm.rs`, `storm_api.rs`; archive scan:
  `War3x.mpq` 5,610 listfile / 5,634 blocks, `War3xlocal.mpq` 1,156 / 1,167.

## 2. BLP textures

### 2.1 BLP1 (all 3,935 local textures are BLP1)

```text
0x00 char[4] "BLP1"
0x04 u32 content      0 = JPEG, 1 = paletted
0x08 u32 alpha_bits   0/1/4/8
0x0C u32 width
0x10 u32 height
0x14 u32 picture_type
0x18 u32 picture_subtype
0x1C [u32;16] mip offsets (absolute file offsets)
0x5C [u32;16] mip sizes
0x9C palette[256 x BGRA]  OR  u32 jpeg_header_size + JPEG header
```

Paletted: one byte per pixel at mip 0; alpha plane follows the pixel plane,
packed at `alpha_bits` (1/4/8). JPEG: CMYK data where C/M/Y are inverted BGR and
K is inverted alpha; reconstruct by feeding the four JPEG components to RGBA as
described in `crates/wc3-formats/src/blp.rs::cmyk_pixel` (validated against
Village_Dirt/Village_Grass). Mips are decoded for every stored level;
`mip_count=0` indicates an exporter stub (none in the local corpus).

Local corpus (scan): 2,920 JPEG + 1,015 paletted; alpha_bits 0 = 2,356,
alpha_bits 8 = 1,579; 3,935/3,935 decode; 98,722,207 pixels; top dimensions
64×64 (1,843), 256×256 (594), 128×128 (549), 32×32 (229), 512×256 (133).

### 2.2 BLP2 (not present locally; supported by the decoder)

```text
0x00 char[4] "BLP2"
0x04 u32 version
0x08 u8 compression  1 = paletted, 2 = DXT, 3 = BGRA
0x09 u8 alpha_bits
0x0A u8 alpha_type
0x0B u8 has_mips
0x0C u32 width
0x10 u32 height
0x14 [u32;16] mip offsets
0x54 [u32;16] mip sizes
0x94 palette[256 x BGRA] + [256 x u8] alpha table (paletted)
```

DXT: `alpha_bits` 0/1 with alpha_type 0 => DXT1 opaque, alpha_bits 1 => DXT1
alpha, 3/4/5 => DXT5, 8 => DXT3. BGRA is stored B,G,R,A; decoder emits RGBA.
No BLP2 file exists in this base (observed); `wc3-re` still exposes the path
for TFT/Reforged data.

Color: WC3's GL1 pipeline samples these textures without a gamma conversion;
the port uploads RGBA8 as sRGB to match the original framebuffer (inferred,
documented in `blp.rs`). Wrap is caller-controlled; BLP has no wrap field.

## 3. MDX models

Chunk container:

```text
0x00 char[4] "MDLX"
0x04 chunk sequence: char[4] tag, u32 size, u8[size] payload
```

Known chunks and fixed payload facts (local 2,048 models, 20,502 chunks):

| tag | payload |
|---|---|
| `VERS` | u32 version (800 locally) |
| `MODL` | name[80], animation bounds u32×2, blend_time u32 |
| `SEQS` | 132 B records: name[80], start u32, end u32, move_speed f32, flags u32, rarity f32, sync u32, extent (radius + min/max vec3 = 28 B) |
| `GLBS` | u32 global sequence durations |
| `TEXS` | 268 B: `replaceable_id u32`, `path[260]`, `flags u32` |
| `MTLS` | material list: priority, flags, shader, LOD, name list, layers; layer holds blend mode, shading flags, `texture_id`, `texture_animation_id`, coord id, alpha, optional alpha/texture-id tracks |
| `GEOS` | geoset: vertex/index/primitive arrays, matrices, matrix groups, extents, material id |
| `GEOA` | geoset animations: alpha + colour tracks |
| `BONE` | node + geoset_id + geoset_animation_id |
| `HELP`/`ATCH`/`EVTS` | node records; attachments add path/attachment id |
| `PIVT` | vec3 pivots |
| `PREM`/`PRE2` | particle emitters |
| `CAMS` | camera definitions |
| `RIBB`, `TXAN` | preserved as unknown tags (2,048-model corpus; no parse failure) |

Sizes: 6,512 sequences, 5,127 geosets, 4,209 materials, 5,216 textures, 3,093
geoset animations, 21,426 bones, 3,024 attachments. `mdx.rs::referenced_textures`
resolves `MTLS` layer `texture_id` to `TEXS` path + replaceable id. Animation
sampling (tracks, geoset visibility/alpha) belongs to the anim owner; the access
layer supplies sequence ranges and tracks.

`TEXS.replaceable_id`: 0 = literal path, 1 = team colour, 2 = team glow; higher
ids refer to the engine replaceable-texture table (not fully enumerated in this
crate — explicit gap; the renderer owner must map them; paths are retained).

## 4. UI skins and FDF

`UI\war3skins.txt` is `[Section] key=value` (`txt::TextTable`). `[Main]`
`Skins=Orc,Human,NightElf,Undead`; each race section maps logical names to
archive paths:
`Console=UI\Console\Orc\OrcUi.mdl`, `ConsoleTexture01=UI\Console\Orc\OrcUITile01`,
`CommandBasicStruct=ReplaceableTextures\CommandButtons\BTNHumanBuild.blp`, music,
fonts. `[Default]` is the fallback for every missing key. Multi-value playlists
use `;`.

Resolution order (`skin::SkinTable::resolve`): requested race section → default
section → literal. `.mdl` names are logical; the archive stores `.mdx`, so the
scanner tries `path.mdx` too. 5 sections, 319 texture entries; 22 paths absent
from this RoC base are reported, not silently dropped.

FDF (`UI\**\*.fdf`, 71 files, 1,956 nodes / 6,514 props): `Frame "KIND" "NAME"
[INHERITS ...] { ... }`, `Texture`/`String`/`Layer`/`StringList`, `IncludeFile`,
`SetPoint` anchors, `DecorateFileNames,`. **`DecorateFileNames`** means a string
property is a skin key, not a literal path; use
`SkinTable::decorate(skin, string)` (race → Default → literal). Decorate
audit: 71 documents parsed; the property occurs in 37 files; resolved keys are
reported by `scan` (`fdf.decorate`).

## 5. Terrain

`TerrainArt\Terrain.slk` columns: `tileID, dir, file, comment, name, buildable,
footprints, walkable, flyable, blightPri, convertTo, InBeta`. Texture path is
`<dir>\<file>.blp`; each BLP is a 512×512 atlas with a 2×2 set of 256×256
variants. `w3e` variation nibble selects the quadrant: `uv = (0.0|0.5,
0.0|0.5)` (`terrain::TerrainTile::variant_uv`). 33 tile records produce 98
texture paths; all 98 resolve (scan).

`CliffTypes.slk`: `cliffID, cliffModelDir, rampModelDir, texDir`; `Water.slk`
and `Weather.slk` are parsed as row maps. `w3e` tile is 7 bytes:
`height u16`, `water u16` (bits 0–13 height, bit 14 edge), `texture_flags u8`
(low nibble texture index, high nibble flags: ramp/water/boundary), `variation u8`
(low nibble variation, high nibble cliff variation), `cliff u8` (low nibble
level, high nibble texture). `wc3-formats::w3e` is the reference implementation.

## 6. Command-button icons

Icons are not SLK columns in RoC; they are text-layer fields:
`Units\*UnitFunc.txt`, `*AbilityFunc.txt`, `*ItemFunc.txt`, `CampaignUnitFunc.txt`
etc. contain `Art=ReplaceableTextures\CommandButtons\BTN*.blp` (also
`Researchart`, `Unart`, `Targetart`). Upgrade/ability values can be a
comma-separated level list; iterate every element. Retail
`CampaignUnitFunc.txt:557` contains the typo `Art=Art=ReplaceableTextures...`;
strip repeated `Art=` prefixes. Command buttons are 64×64 BLP1.

Audit (scan): 1,182 references, 655 unique paths, 1,182 resolved, 0 missing, 0
wrong size, 0 probe failures. `icons::audit` returns the census; HUD/touch
should sample the first 64×64 BLP and scale to the device.

## 7. Scripts: JASS, triggers, AI

### 7.1 Files and init order

- `Scripts\common.j` — 791 native declarations + constants/types (110,398 B).
- `Scripts\Blizzard.j` — BJ wrappers/constants (289,7xx B).
- `Scripts\Cheats.j` — cheat commands.
- `Scripts\common.ai` — AI natives and 60+ functions, including `InitAI`.
- 91 `.ai` race/melee/campaign scripts (each defines `main` as an endless
  strategy loop with `Sleep`).
- 44 maps carry `war3map.j`; a map script is self-contained for user functions
  but **does not include Blizzard.j**, so the VM merges common.j + Blizzard.j +
  war3map.j before executing.

Human04 `main` order (observed): `SetCameraBounds`, `SetDayNightModels`,
`SetAmbientDaySound`, `SetAmbientNightSound`, `SetMapMusic`, `InitSounds`,
`CreateRegions`, `CreateCameras`, `InitUpgrades`, `InitTechTree`,
`CreateAllDestructables`, `CreateAllUnits`, `InitBlizzard`, `InitGlobals`,
`InitCustomTriggers`, `RunInitializationTriggers`. `RunInitializationTriggers`
calls `ConditionalTriggerExecute(gg_trg_Map_Initialization)`; the map-init
trigger has an action but no event, so `TriggerExecute` drives the actions.

### 7.2 TriggerData / TriggerStrings

Sections and formats (local `UI\TriggerData.txt`, 169,395 B):

| section | entry |
|---|---|
| `[TriggerCategories]` | `KEY=display,icon[,hideName]` |
| `[TriggerTypes]` / `[TriggerTypeDefaults]` / `[TriggerParams]` | type metadata |
| `[TriggerEvents]` | `FunctionName=argType,...` (implicit first `trigger` argument) |
| `[TriggerConditions]` | `FunctionName=argType,...` |
| `[TriggerActions]` | `FunctionName=argType,...` |
| `[TriggerCalls]` | native-call GUI signatures |

A companion `_<key>_Defaults`, `_<key>_Limits`, `_<key>_Category`,
`_<key>_Display`, `_<key>_UseWith` carries editor metadata. Values are
comma-separated with quoted raw text; `TextTable::split_list_quoted` preserves
quotes. Counts: 43 categories, 133 types, 25 events, 24 conditions, 326 actions,
301 calls = 676 functions; `TriggerStrings.txt` has a layout for all 676
(`strings_resolved=676`, `strings_missing=[]`). `~parameter` placeholders in the
display layout map to the argument list — the touch UI can render actions
without the World Editor.

### 7.3 JASS VM

Lexer: identifiers, decimal/hex (`0x`, `$`) ints, real literals, strings with
`\" \\ \n \r \t`, `'abcd'` rawcodes packed big-endian (`'hfoo' =
0x68666F6F`), `//` comments, newlines significant. Parser: `type … extends`,
`native … takes/returns`, `globals/endglobals` (`constant`, `array`),
`function/endfunction`, `local`, `set`, `call`, `if/elseif/else/endif`,
`loop/endloop`, `exitwhen`, `return`, `debug` prefix; expressions include
`and/or/not`, comparisons, arithmetic, string `+`, `function Foo` code values.
The VM supports scalars/arrays, deterministic step and loop limits, and a
world state (players/units/items/destructables/groups/locations/rects/regions/
triggers/timers/sounds/effects/quests/dialogs/leaderboards/multiboards/forces).
Handles start at 0x100; `Player(i)` is handle `i`. Trigger events are stored
(timer/player/unit/death/region/state/variable/dialog); `TriggerExecute`
evaluates conditions then runs action code. `TriggerEvaluate` is condition-only.
`ForGroup` sets `GetEnumUnit`; `Condition`/`Filter`/`And`/`Or`/`Not` build
boolexpr handles.

RNG: deterministic xorshift64*; `GetRandomInt(low,high)` inclusive, swaps bounds
when `low>high` (Warsmash
`donors/WarsmashModEngine/.../Jass2.java:9590-9612`); `GetRandomReal` uniform.
Exact retail sequence is not reproduced (explicit limitation), but every run is
reproducible.

Execution result (scan): 44/44 map scripts parsed, 44/44 `main` executed,
1,311,867 steps, 0 failures. 91/91 AI scripts parsed and 91 `InitAI` runs.
Unimplemented natives are counted, never silently swallowed
(`scripts.unimplemented`); top entries are platform/render/audio natives
(`triggerregisterplayerstateevent`, `unitsharevision`, `questcreateitem`, …).

AI: parse with the same front end; `common.ai` globals/natives are merged;
`InitAI` is executed, `main` is not (it is an endless `Sleep` loop by design).

## 8. SLK, TXT, object data

### 8.1 SLK

`ID;PWXL` spreadsheet. `C;` records set current X/Y and `K"value"` cells;
`F;` formulas are read as cells; `E` is an empty run. Row 1 is the column
header; first column of each row is the object id. Key columns observed:
`unitID` (UnitData), `unitBalanceID` (UnitBalance), `unitWeapID` (UnitWeapons),
`unitAbilID` (UnitAbilities), `unitUIID` (unitUI), `alias` (AbilityData),
`itemID`, `upgradeid`, `tileID`, `cliffID`. 33 tables / 6,785 rows / 815 columns
parse with 0 failures.

### 8.2 TXT and object database

`*Func.txt`/`*Strings.txt` are `[id] field=value` sections; the object DB merges
SLK rows + text fields. `Units\UnitMetaData.slk` columns:
`ID, field, slk, index, displayName, sort, type, stringExt, caseSens,
canBeEmpty, minVal, maxVal, useHero, useUnit, useBuilding, useItem,
useSpecific`. Example: `uhpm -> field HP, slk UnitBalance, index -1,type int`;
`unam` is a text field. After BASE v1, `ObjectDb` stores canonical
`wc3_formats::gamedata::Sheet` views and parses `UnitMetaData.slk` through
`gamedata::MetaTable::parse(MetaKind::Unit, bytes)`; `field_by_id` resolves map
modification 4CCs through that table. The legacy `FieldMeta` map is a projection
of `MetaField`, not a second parser. 11 sheets, 1,665 objects: 475 units, 572 abilities, 133
items, 128 destructables, 278 doodads, 58 upgrades, 21 unknown (records whose
source file is not a known object table).

### 8.3 Map object data (`war3map.w3u/.w3t/.w3a/.w3b/.w3d/.w3h/.w3q`)

RoC v1 is a **two-table** format (Human01: 11 base + 28 custom entries):

```text
u32 version (1)
u32 base_count
  base_count × entry
u32 new_count
  new_count × entry

entry:
  char[4] old_id
  char[4] new_id       (zero for a base modification)
  u32 modification_count
  modification:
    char[4] id
    u32 type (0 int, 1 real, 2 unreal, 3 string, 4 int64)
    value (string = NUL-terminated UTF-8, War3Net ReadChars)
    u32 sanity (0 in retail)
```

Evidence: `analysis/agent-re/tmp/h01.w3u` bytes 0x08–0x20:
`6e 62 61 6e` (`nban`), `00 00 00 00` (new id), `01 00 00 00` (one mod),
`75 61 62 69` (`uabi`), `03 00 00 00` (string), `00` (empty), `00 00 00 00`
(sanity), then `6e 67 6e 6f` (`ngno`) at 0x21. Version 3 adds an unknown array
after `new_id` (`War3Net SimpleObjectModification.cs:17-25`). 44 maps, 1,009
entries, 0 parse errors.

### 8.4 WTS

`STRING <n>` / `{` / value lines / `}`. Escapes `\n \r \t \" \\`; unresolved
`TRIGSTR_n` stays literal. 44 maps, 4,428 strings. `Wts::resolve` replaces
`TRIGSTR_n` in `w3i` name/author/description/recommended players.

## 9. Map container (W3M/W3X)

RoC `.w3m`: 512-byte `HM3W` header, then MPQ at the first `MPQ\x1a` (0x200
observed). TFT `.w3x` is plain MPQ. Inner files probed (17 names):
`war3map.j`, `war3map.w3e`, `war3map.w3i`, `war3map.wpm`, `war3map.w3u/.w3t/
.w3a/.w3d/.w3b/.w3h/.w3q`, `war3map.wts`, `war3map.wtg`, `war3map.wct`,
`war3map.w3r`, `war3map.w3c`, `war3map.w3s`, `war3map.doo`,
`war3mapUnits.doo`, `war3map.mmp`, `war3map.shd`, `war3map.imp`,
`war3mapMap.blp`, `war3mapPreview.tga`. Maps often have no listfile; reads use
hash probes. 44/44 maps open, 708 inner files, 44 scripts, 669,068 terrain
tiles, 1,009 object entries, 4,428 WTS strings, 0 errors.

## 10. Engine, host, Storm API

### 10.1 Game.dll (RoC) boot

`GameMain` at `0x6f004180` (121 bytes):
1. host `A372`, `A327` (host/game setup);
2. host `A475` with `ecx=0x67616d65` (`"emag"` = reversed `game`);
3. host `A377` formats a 260-byte path;
4. Storm `503` (with `"Logs"`), `585`;
5. `fcn.6f004200`; on failure host `A180`, `fcn.6f004b80`, host `A340`.

`fcn.6f004200` (432 bytes):
- mutex `"Warcraft III Game Application"` (single instance);
- Storm read `ui\startupstrings.txt` (`0x6f53ac1a`);
- callbacks `0x6f004d80`, `0x6f004f80`;
- init `0x6f004b20`, `0x6f001380`, `0x6f0018a0(50)`, `0x6f004b70`,
  `0x6f004c10` (map load gate), `0x6f004930`;
- on success: `war3mapMap.blp`, `war3mapMap.tga`, callbacks, `0x6f071270`,
  `0x6f006410`, `0x6f0048b0`, return 1; on failure `0x6f004b80`, return 0.

`fcn.6f004b80` shutdown: `0x6f001380`, `0x6f0013c0`, `0x6f0351b0`, Storm
`496`, `0x6f071280`, host `A179`, `A105`, `0x6f004d20`, Storm `403`, `470`.
Artifacts: `analysis/agent-re/r2/Game.dll.roc.*.pdc.txt`.

The Rust `Engine` mirrors boot steps and keeps a step log with the evidence
string; `tick` advances time/timers and fires zero-time timer events. The
per-frame input→sim→render→audio order is **inferred** (the host owns the
frame loop; Game.dll registers callbacks).

### 10.2 Host launcher

`War3.exe` exports 695 ordinal-only functions `A1..A695`; imports 487 entries.
Strings `Game.dll`, `Couldn't open Game.dll.`, `Game.dll is corrupt.`,
`Storm.dll`, `opengl`; imports `LoadLibraryA`, `GetProcAddress`, `FreeLibrary`.
`Game.dll` imports a 695-ordinal host subset (observed call sites A372/A327/
A475/A377/A180/A340) and 92 Storm ordinals. `host::Launcher` models
`LoadLibraryA("Game.dll") → GetProcAddress("GameMain") → GameMain(host table)`.

### 10.3 Storm ordinal facade

`Storm.dll` RoC exports 370 functions at ordinals 101…647; names are absent from
the PE. `donors/StormLib/storm.dll/storm.def` supplies 19:
252 CloseArchive, 253 CloseFile, 262 Destroy, 264 GetFileArchive, 265
GetFileSize, 266 OpenArchive, 267 OpenFile, 268 OpenFileEx, 269 ReadFile, 270
SetBasePath, 271 SetFilePointer, 272 SetLocale, 273 GetBasePath, 275
GetArchiveName, 276 GetFileName, 293 OpenFileArchive, 465 SetLastError, 551
Compress, 552 Decompress. `storm_api::StormApi` implements the file/archive
subset over `StormFs`; unimplemented ordinals are recorded, never faked.
`r2/STORM_ORDINALS.md` has all 370 with RVAs; `r2/STORM_IMPORTS.md` lists the 92
Game imports. Remaining names are unknown — explicit gap, no invented names.

TFT delta: Game.dll 28,001 funcs (RoC 24,537), GameMain `0x6f005e70`; Storm.dll
847 funcs / 451 exports (RoC 730/370); Frozen Throne.exe 106 funcs; TFT
`Game.pdb` path in `r2/Game.dll.tft.info.txt`.

## 11. Edge cases (all covered by tests or explicit reports)

1. **Encrypted sector dword tail + PKWARE `0x306`** — see
   `PKWARE_FIX.md`; WTS now reads.
2. **W3U v1 two tables and NUL strings** — `mapobjects.rs`; 38 maps previously
   failed under the length-prefixed assumption.
3. **`Art=Art=` retail typo** — `objectdata.rs` strips repeated prefixes.
4. **Upgrade art lists** — comma-separated level icons expanded.
5. **`.mdl` in war3skins vs `.mdx` in archives** — scanner tries both.
6. **Maps without listfile** — hash probing.
7. **BLP1 JPEG CMYK + inverted alpha** — decoder handles all 2,920 JPEG BLP1.
8. **MDX unknown chunks `RIBB`/`TXAN`** — preserved, not fatal.
9. **Skin paths absent from RoC base** — 22 reported, not dropped.
10. **Unimplemented natives** — census in `SCAN.md`; no silent stubs.
11. **`War3.mpq`/ISO absent** — external boundary recorded in `COVERAGE.md`.
12. **TFT archives** — open with listfile counts; full TFT ingest is a gap.
13. **Windows-1252 TXT** — `UI\Widgets\BattleNet\chaticons\iconindex_bel.txt`
    contains byte `0x96` at offset 2513 (en dash). `wc3-re::txt` normalizes only
    invalid-UTF-8 input through a CP1252 table, then calls the canonical
    `gamedata::TextTable` parser; scan is 67/67 TXT.

## 12. Provenance

- `analysis/agent-re/INVENTORY.tsv` — 10,555 files, size + SHA-256.
- `analysis/agent-re/scan.json` / `SCAN.md` — decode/parse/execute counts.
- `analysis/agent-re/r2/` — PE headers/sections/exports/imports, function
  lists, pdc decompilations, Storm ordinals, TFT delta inputs.
- `analysis/agent-re/PKWARE_FIX.md` — sector-0 proof.
- Donors cited inline: StormLib (pklib, storm.def), War3Net (object data,
  `ReadChars`), Warsmash (RNG bounds), wc3-formats/wc3-mpq (existing local
  crates). No donor code was executed untrusted; the C PKWARE harness was run
  only for ground-truth validation on the operator's own file.

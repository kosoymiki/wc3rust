# ENGINE_MAP — Storm/MPQ crypto & compression, map formats, JASS VM, trigger path

Layer: **r2 RE — archive formats + JASS/trigger scripts** (H2 ledger 407, task `wc3rust/re-formats`).
Repo: `/data/data/com.termux/files/home/src/herofolends-rs`.
Report only — no source edits. Format/data porting is ledger 394; sim script API is 393/403.

| item | value |
|---|---|
| binaries | `analysis/game_files/Files100_Storm.dll` (311 360 B, RoC), `analysis/tft_files_Files100_Storm.dll` (356 417 B, TFT), `analysis/tft_files_Files100_Game.dll` (8 341 504 B, TFT 1.27 build), `analysis/game_files/Files100_WorldEdit.exe` (3.3 MB, cross-check only) |
| tools | radare2 6.2.6 (`aa`, `af`, `pdf`, `/x`, `/r`, `pxw`), rabin2 (`-z`, `-S`, `-i`, `-E`); r2ghidra available but not needed for the results below |
| image base | Storm.dll `0x15000000`, Game.dll `0x6f000000` (both PEs map `vaddr = base + paddr`, verified with `rabin2 -S`) |
| labels | `observed` = command output in this document; `inferred` = reasoned from observed instructions but not executed/confirmed; `unknown` = not established |
| access date | 2026-11-15 |

Reproduce the extracted tables:

```
python3 analysis/agent-re/formats/extract_natives.py     # 2192 registrations -> jass_natives_engine.json
python3 analysis/agent-re/formats/make_jass_natives.py   # 1096 unique natives + our gap -> jass_natives.json
python3 analysis/agent-re/formats/extract_dispatch.py    # opcode map + node-type table -> jass_dispatch_tables.json
python3 analysis/agent-re/formats/verify_formats.py      # one-shot recount, prints the numbers quoted below
```

Raw disassembly dumps used as evidence are in the same directory
(`storm_SFileOpenArchive.pd.txt`, `storm_SFileOpenFileEx.pd.txt`, `storm_read_internal.pd.txt`,
`storm_readdata.pd.txt`, `storm_sector.pd.txt`, `game_jass_call.pd.txt`, `game_jass_exec.pd.txt`,
`game_vm_dispatch.pd.txt`, `game_w3e_parser.pd.txt`, `game_w3e_parse2.pd.txt`,
`game_w3e_header.pd.txt`, `storm_roc.strings.txt`, `storm_tft.strings.txt`, `game_tft.strings.txt`).

---

## 1. Storm / MPQ

### 1.1 Crypto primitives (`observed`)

Crypt table initialiser: `fcn.1501bea0` (RoC, 163 B) with an inlined duplicate at `0x1501b012`.
Exact arithmetic from `0x1501bee0..0x1501bf2a`:

```
seed = 0x00100001
for i in 0..0x100 step 4:          ; 4 outer iterations, each filling 4 interleaved rows
    for k in 0..5:                 ; 5 entries per row
        seed = (seed * 125 + 3) % 0x2AAAAB     ; imul eax,0x7d / add 3 / div 0x2aaaab
        hi   = (seed & 0xFFFF) << 16           ; and 0xffff / shl 0x10
        seed = (seed * 125 + 3) % 0x2AAAAB
        table[i + k*0x100] = hi | (seed & 0xFFFF)
```

→ 0x500 entries, the standard Blizzard table.  `observed`.

`hash_string`:

| call site | hash kind | evidence |
|---|---|---|
| `0x1501b313` (inline) | `0x000` (table offset) | `mov esi, dword [ecx + eax*4]` — no `<<8` |
| `fcn.1501b610` (99 B) | parameter: `0x100` / `0x200` | `shl ecx, 8` then `add ecx, eax` → index `(kind<<8)+ch` |
| `0x1501c110`, `0x1501c21b`, `0x1501cd52`, `0x150186c0` (inline) | `0x300` (file key) | crypt offset `+0xc00` = 0x300<<2 |

Body (identical in all copies): `seed1 = 0x7FED7FED`, `seed2 = 0xEEEEEEEE`;
per `toupper(ch)`: `seed1 = table[(kind<<8)+ch] ^ (seed1 + seed2)`;
`seed2 = (seed2*0x21 + ch) + seed1 + 3` (compiled as `imul edi,edi,0x21` … `lea edi,[edi+esi+3]`).

Block decrypt (`0x1501c170` for the hash table, `0x1501c280` for the block table, `0x150186c0`
for the sector-offset table, `0x15018932` for sector data):

```
seed = 0xEEEEEEEE
for each dword:
    seed += table[0x400 + (key & 0xFF)]
    data ^= seed + key
    key = ((~key << 0x15) + 0x11111111) | (key >> 0x0B)
```

`observed` (all four sites share the same instruction sequence).

### 1.2 Archive open (`observed`)

`SFileOpenArchive` = ordinal 266 @ `0x1501bb60`; header parse starts in the function around
`0x1501bf50`, magic compare at `0x1501bffc`:

```
0x1501bffc  cmp dword [eax], 0x1a51504d      ; "MPQ\x1a"
0x1501c004  cmp dword [eax+4], 0x20          ; header size >= 32
0x1501c008  jae 0x1501c060                   ; accept
0x1501c013  add eax, 0x200                   ; else retry at file offset 512 (HM3W maps)
```

Accepted header is used **as a v1 header only** — no version dispatch, no high-dword merge for
64-bit table offsets:

| field | offset | use |
|---|---|---|
| sector size shift | `+0x0E` (byte only: `mov cl, byte [ecx+0xe]`) | `sectorSize = 512 << shift` (`0x1501c080..0x1501c092`) |
| hash table offset | `+0x10` | `fcn.15018c70(file, base+offset, buf, count*16)` |
| block table offset | `+0x14` | same, `count*28` |
| hash table entries | `+0x18` | bytes = `count*16` (`shl eax,4`) |
| block table entries | `+0x1C` | bytes = `count*28` (`imul edx,edx,0x1c`) |

Table decryption keys are computed inline: `HashString("(hash table)", 0x300)` at `0x1501c0fa`
and `HashString("(block table)", 0x300)` at `0x1501c20a` — strings at `0x15042978` / `0x15042968`.

`(attributes)` is read **during archive open** (`observed`, `0x1501c442` pushes the string
`0x15042958`, calls `SFileOpenFileEx` ordinal 268, `SFileGetFileSize` 265, `SFileReadFile` 291):

```
flags = attr[4]; size must equal 8 + (flags&1 ? 4*n : 0) + (flags&2 ? 8*n : 0)   ; 0x1501c49a..0x1501c4bd
```

i.e. the engine loads CRC32 / FILETIME arrays for the block table at open time.  There is **no**
`(listfile)` handling anywhere: `grep -c '(listfile)'` over the string dumps of both Storm.dll
builds and Game.dll returns **0**.

### 1.3 Lookup semantics (`observed`)

`fcn.1501b2e0` (801 B, 259 insns) = name → hash entry:

```
h0 = HashString(name, 0x000); h1 = HashString(name, 0x100); h2 = HashString(name, 0x200)
index = h0 & (hashTableEntries - 1)          ; 0x1501b37b..0x1501b384  (count-1, not validated)
loop:
    if entry.blockIndex == 0xFFFFFFFF  -> miss            ; 0x1501b38d..0x1501b398
    if entry.blockIndex == 0xFFFFFFFE  -> skip            ; 0x1501b3af..0x1501b3b3 (deleted)
    if entry.name1 == h1 && entry.name2 == h2:
        if entry.wLocale  != 0 && entry.wLocale  != currentLocale   -> skip   ; 0x1501b3b5..0x1501b3d0
        if entry.platform != 0 && entry.platform != currentPlatform -> skip   ; 0x1501b3bf..0x1501b3dc
        hit
    index = (index + 1) & mask ; stop after wrapping to the start   ; 0x1501b3e4..0x1501b3f9
```

Locale/platform come from globals `0x15045784`/`0x15045786` (`SFileSetLocale` ordinal 272).
The block's flags are then inspected at `0x1501b425` (`mov ecx,[entry+0xc]; test ecx,ecx; js …`)
→ `0x1501b5e5` `test byte [ebp+8], 4` / `test ecx, 0x1ff00` → error `0x3ee` (1006).
`inferred`: this is the "file does not exist / not readable" gate; the exact condition depends on
the caller-supplied open mode and was not exercised.

### 1.4 File open — key derivation (`observed`)

`SFileOpenFileEx` = ordinal 268 @ `0x1501c9e0`. Key derivation at `0x1501cd26..0x1501cdad`:

```
p = name; p = strrchr(p, ':'); p = strrchr(p, '\\')      ; 0x1501cd26..0x1501cd44  (basename only)
key = HashString(p, 0x300)                               ; inline loop 0x1501cd52..0x1501cd85
if (block.flags & 0x20000) {                             ; MPQ_FILE_FIX_KEY
    key = (key + (block.offset - archive.baseOffset)) ^ block.size
}
```

`archive.baseOffset` is `[archive+0x12c]` (0 for a plain archive, 512 for a `.w3m/.w3x`).

### 1.5 Read path — sectors, encryption, stored units (`observed`)

`SFileReadFile` (ordinal 269) → ordinal 291 (`fcn.1501cfb0`) → cache read `fcn.150181f0` →
sector load `fcn.150185e0` (1539 B).

Sector count, computed at open (`0x1501cdb5`, stored at `[file+0x124]`):

```
sectorCount = ceil(block.size / sectorSize)      ; block.size = uncompressed size
if (block.flags & 0xFF00) == 0  -> no sector-offset table (uncompressed file)
else allocate (sectorCount+1)*4 bytes             ; 0x1501cdcd..0x1501cde8  (no SECTOR_CRC dword)
```

`fcn.150185e0`:

| step | address | behaviour |
|---|---|---|
| table read | `0x15018646` | `read(file, block.offset, buf, 4*sectorCount+4)` |
| table decrypt | `0x15018693..0x15018705` | only if `flags & 0x10000` (ENCRYPTED); key = **fileKey-1** (`dec eax` @ `0x150186ae`) |
| sector size | `0x150188be..0x1501890d` | `size = table[i+1] - table[i]`; last sector clamped by `block.size & (sectorSize-1)` (full sector when the size is an exact multiple) |
| sector decrypt | `0x15018910..0x15018973` | key = **fileKey + sectorIndex** (`add ecx, esi` @ `0x1501891c`) |
| stored check | `0x15018a55..0x15018a5b` | `if sectorSize <= rawSize` → copy raw, no decompression |
| IMPLODE | `0x15018a6f` | `(flags & 0xFF00) == 0x100` → PKWARE DCL expand with a `0x3134`-byte work buffer (`0x15018b13`) via `fcn.15037030` |
| COMPRESS | `0x15018a76` | `(flags & 0xFF00) == 0x200` → `fcn.1500f7c0` (mask decoder) |
| both bits | `0x15018a76` | `0x300` falls through to `0x15018b98` → **no decompression** |

### 1.6 Compression codecs (`observed`)

`SFileDecompress` ordinal 552 @ `0x1500f7a0` → `fcn.1500f7c0` (457 B) iterates a table of
`(handler, mask)` pairs from the end backwards, applying every handler whose mask bit is set
in the sector tag byte.

RoC Storm.dll (`analysis/game_files/Files100_Storm.dll`):

| mask | handler | codec | table slot |
|---:|---|---|---|
| `0x08` | `0x1500eb70` | PKWARE DCL implode (work buffer `0x3134` @ `0x1500eb85`) | `0x1503e34c/0x1503e350` |
| `0x02` | `0x1500ec50` | deflate/zlib (zlib 1.1.3 banner in `.data` `0x150439bc`) | `0x1503e344/0x1503e348` |
| `0x01` | `0x1500f340` | Huffman | `0x1503e33c/0x1503e340` |
| `0x80` | `0x1500f200` | ADPCM stereo (wave) | `0x1503e334/0x1503e338` |
| `0x40` | `0x1500ef70` | ADPCM mono (wave) | `0x1503e32c/0x1503e330` |

`loop bound: cmp edi, 0x1503e330 / jge` → **5 masks, 0x10 (bzip2) and 0x20 (sparse) absent**.

TFT Storm.dll uses the same table layout shifted by `+0x8000`: masks `0x40→0x1500f820`, `0x80→0x1500fab0`,
`0x01→0x1500fc00`, `0x02→0x1500f480`, `0x08→0x1500f290` (table `0x1504635c..0x1504637c`,
loop bound `0x15046360`). Also 5 codecs, **no bzip2**; `deflate 1.1.4` banner at `0x15046e40`.

The compressor table is separate (`0x1503e308..0x1503e328` RoC):
`0x80→0x1500ec70`, `0x01→0x1500f1a0`, `0x02→0x1500f220`, `0x08→0x1500ebf0`, `0x40→0x1500ea20`.

### 1.7 Divergences — our reader vs the engine

Legend: **M** = semantic mismatch that can change decoded bytes; **L** = leniency (we accept
more/less than the engine, no wrong bytes); **C** = cosmetic/metadata.

| # | item | engine (observed) | `crates/wc3-mpq` | class |
|---|---|---|---|---|
| 1 | sector-offset table size | `(n+1)*4`; `MPQ_FILE_SECTOR_CRC` adds no dword (`0x1501cdc3`, `0x15018654`) | reads `(n+1)*4` + 4 when SECTOR_CRC (`src/stream.rs:191-198`) | **C** — the extra dword is past the last used offset and decryption is a dword stream, so offsets/sector data are identical |
| 2 | `IMPLODE|COMPRESS` (0x300) | no decompression, raw copy (`0x15018a76`) | tag decoder runs (`src/lib.rs:897-905`) | **M** (pathological files only) |
| 3 | bzip2 `0x10` / sparse `0x20` | not present in either Storm build | `decode_v0_mask` supports both (`src/lib.rs:958-966`) | **L** (extra codecs; WC3 data never uses them) |
| 4 | mask application order | PKWARE, zlib, Huffman, ADPCM-stereo, ADPCM-mono | bzip2, PKWARE, zlib, Huffman, ADPCM-stereo, ADPCM-mono, sparse | equal on the shared subset — **no divergence** |
| 5 | `LZMA 0x12` | absent | rejected for v0, accepted for v1 (`src/lib.rs:988-1030`) | **L** (v1-only path unreachable for WC3 v0 archives) |
| 6 | hash table count | used as `count-1` mask, not validated (`0x1501b37b`) | requires power of two, `<= MAX_TABLE_ENTRIES` (`src/lib.rs:373-380`) | **L** (we reject malformed, engine would probe) |
| 7 | locale / platform in hash lookup | entries with non-zero `wLocale`/`wPlatform` only match the current locale (`0x1501b3b5..0x1501b3dc`) | `find_block` ignores entry bytes 8..11 (`src/lib.rs:560-586`) | **M** (localised archives resolve a different entry) |
| 8 | block flags | used raw | masked by `MPQ_FILE_VALID_FLAGS(_W3X)` (`src/lib.rs:424-437`) | **L** (we may ignore e.g. `0x0004_0000` patch flag) |
| 9 | header validation | `MPQ\x1a` + `headerSize >= 0x20`; retry only at `+0x200` (`0x1501bff6..0x1501c013`) | magic + `version <= 1` + power-of-two checks + arbitrary caller-supplied `base` (`src/lib.rs:344-390`) | **L** |
| 10 | sector size shift | `byte [header+0x0E]`, no range check (`0x1501c07b`) | `& 0xFF`, rejects `> 15` (`src/lib.rs:381-387`) | **L** (shift 16..31 would overflow in our reader, engine wraps via x86 `shl`) |
| 11 | `(attributes)` | parsed at open, 8 + 4n/8n payload (`0x1501c49a`) | not parsed | **C** (metadata only) |
| 12 | `(listfile)` | never read by Storm or Game.dll (`0` string hits); WorldEdit.exe does reference both `(listfile)` and `(attributes)` (editor archive browser) | primary name source; `probe_names` fallback | **C** (our discovery path is a superset) |
| 13 | table truncation | declared size allocated and read; short read tolerated (`0x1501c09d` result unused) | truncated tables zero-filled, `header.truncated_tables` flagged (`src/lib.rs:392-414`) | **C** (our behaviour is safer, no wrong bytes for valid archives) |
| 14 | FIX_KEY adjustment | `(key + (offset - archiveBase)) ^ block.size`, only when `0x20000` (`0x1501cd99..0x1501cdac`); block offsets are normalised by `[archive+0x12c]` | same arithmetic on the archive-relative `block.offset` (`src/lib.rs:548-557`) | match |
| 15 | sector table decryption key | `fileKey - 1` (`0x150186ae`) | `key.wrapping_sub(1)` (`src/stream.rs:203`) | match |
| 16 | sector decryption key | `fileKey + sectorIndex` (`0x1501891c`) | same pattern in `stream.rs` | match |
| 17 | stored-unit rule | `rawSize <= expected` → raw copy (`0x15018a5b`) | `declared_len >= expected` (`src/lib.rs:905`) | match |
| 18 | file-name key input | basename after the last `:` or `\` (`0x1501cd26`) | basename after the last `\` or `/` (`src/lib.rs:552`) | match for `war3map.*`; differs only for names containing `:` |

Only item **7** (hash-entry locale/platform filtering) and item **2** (the degenerate
`0x300` flag combination) can change decoded bytes; item 7 is the one worth fixing first.
Items **3, 5, 6, 8, 9, 10** are leniency differences that cannot corrupt a valid WC3 archive.

---

## 2. Map formats

### 2.1 Loader registry (`observed`)

`0x6f28da40` is the "load map object file" chain; each `call 0x6f28d980` builds the archive name
`war3map<ext>` (`"%s%s"` with `war3map` @ `0x6f752b78`), reads it, computes a rolling checksum
(`rol3` over dwords then bytes, `0x6f28d9e5..0x6f28da28`) and dispatches to a parser:

| extension | string | note |
|---|---|---|
| `.w3e` | `0x6f7565a4` | terrain |
| `.wpm` | `0x6f75659c` | pathing |
| `.doo` | `0x6f77ea1c` | doodads / units placement |
| `.w3u` | `0x6f77139c` | unit object data |
| `.w3b` | `0x6f7713bc` | destructable object data (only when a flag arg is non-zero) |
| `.w3d` | `0x6f7713c4` | doodad object data |
| `.w3a` | `0x6f7713ac` | ability object data |
| `.w3q` | `0x6f7713b4` | upgrade object data |

`.w3t` (items) is not in this chain even though the string exists (`0x6f7713a4`); the item table is
loaded through the game-data path instead (`inferred`).

### 2.2 W3E (`observed`)

`0x6f3eb180` → `0x6f3eb1a0` (parser) + `0x6f3eb2c0` (header) + `0x6f3eb540` (tiles).

```
0x6f3eb1c0..0x6f3eb1de   4-byte magic compared against the constant at 0x6f6b6f28 ("W3E!")
0x6f3eb1ed               version = read_u32()
0x6f3eb1f5               if version < 2 -> fail          (dead check, next line subsumes it)
0x6f3eb1fe               if version < 3 -> fail
0x6f3eb207               if version > 11 -> fail         <-- engine ceiling
0x6f3eb216               parse header(version)
0x6f3eb226               parse tiles(version)
0x6f3eb22f               version >= 7 -> extra init (0x6f3dc840)
0x6f3eb281               version <  9 -> legacy finish (0x6f3e6000)
```

Header (`0x6f3eb2c0`): `version >= 6` reads the custom-tileset byte at `[ctx+0x1d0]` and the ground
texture **count** at `[ctx+0x1cc]`; ground/cliff texture records are **336 bytes** each
(`imul eax, eax, 0x150` @ `0x6f3eb491`, `0x6f3eb4bb`, `0x6f3eb525`); unknown texture ids produce
`WESTRING_WARNING_INVALIDTEXTUREID` (`0x6f3eb373`). No explicit width/height upper bound found in
the header parser (dims are read and multiplied directly) — `observed` by absence of a comparison
against a dimension constant in `game_w3e_header.pd.txt`.

Engine-side limits our parser must honour:

* **version ≤ 11** — a v12 tile layout is *not* loadable by the TFT engine (parser rejects).
* tile record = 7 bytes / version ≤ 11; the engine's per-version branches are 7 and 9.
* texture arrays are 336-byte records; ground/cliff lists are separate.

### 2.3 W3I (`observed`)

`0x6f3be8f0` is a thin wrapper that calls `0x6f3c8400(ctx, ".w3i", parser, ctx)`. The parser body
was not decoded (see §5); our reader's v18/v27 field gating is from War3Net, not from this binary.

### 2.4 WPM (`observed`)

Loaded through the same registry (`.wpm` @ `0x6f28da54`); no format-specific constants inspected —
the parsing happens inside `0x6f5c83a4`'s target, which was not isolated (see §5).

### 2.5 DOO and object data (`observed` / `inferred`)

`.doo` is in the registry; `.w3u/.w3a/.w3q/.w3d/.w3b` are loaded only behind the "extended data"
flag at `0x6f28da76` (`test edi, edi; je`), i.e. the engine loads them in a second pass
(`inferred`: campaign/expansion object overrides). Object-data files are **not** version-checked in
the loader chain; the per-file `version` dword is read by the object parser.

### 2.6 WTS (`observed`)

`0x6f39dc04` appends `".wts"` to the map name after truncating at the last `.` or `\`
(`0x6f39dbe0..0x6f39dbf6`), and the literal `TRIGSTR_` exists at `0x6f797358`.
So the engine resolves `TRIGSTR_<n>` through `war3map.wts` at load time.

### 2.7 WCT / WTG (`observed`)

`grep -ic '\.wtg\|\.wct'`:
Game.dll → **0**, `Files100_WorldEdit.exe` → both present (`.wct` @ `0x6ce340`, `.wtg` @ `0x6ce348`).
`UI\TriggerData.txt` exists in **both** binaries (Game.dll `0x6f79a664`, WorldEdit `0x6c7a94`).
Conclusion: **the game never reads `war3map.wtg` or `war3map.wct`**; the GUI trigger tree is
editor-only and reaches the engine only as compiled `war3map.j`.

### 2.8 Divergences — our parsers vs the engine

| # | item | engine | `crates/wc3-formats` | class |
|---|---|---|---|---|
| 1 | W3E versions | accepts 3..11, rejects ≥12 (`0x6f3eb207`) | parses v11 (7-byte tile) **and** v12 (8-byte tile) (`src/w3e.rs:1-11`) | **L** (extra format the engine cannot load) |
| 2 | W3E tile layout | version branch at 7 and 9 only | single v11/v12 layout | to verify against the 7/9 branches (§5) |
| 3 | W3E texture record | 336-byte records, invalid id → warning, load continues (`0x6f3eb373`) | textures stored as `[u8;4]` code only (`src/w3e.rs:55-58`) | **C** (code preserved; no warning text) |
| 4 | W3E dims | no upper bound observed | `width*height` iteration bounded by input length | **C** |
| 5 | WCT | not read by the engine | not implemented | none (editor-only) |
| 6 | WTG | not read by the engine | not implemented | none (editor-only) |
| 7 | WTS | `war3map.wts`, `TRIGSTR_` | `gamedata::Wts::parse` + `TRIGSTR_` resolution (`src/fdf.rs:846-901`) | match |
| 8 | object data | loaded through the map registry with `.w3u/.w3a/.w3q/.w3d/.w3b` | `gamedata::ObjectDataFile` with `w3u/w3a/w3t/w3q/w3d/w3b/w3h` (`src/gamedata.rs:1057-1067`) | superset (we also read `.w3t`) |
| 9 | DOO | `.doo` in registry (doodads+units) | `map.rs::parse_units` / `parse_doodads` | match (both sections) |

---

## 3. JASS VM

### 3.1 Modules (`observed`)

Game.dll carries the Jass2 source paths as assert strings:

```
..\Game/JassWar3.cpp            0x6f786528
scripts\common.j                0x6f786560      common.j        0x6f77c088
blizzard.j                      0x6f77f61c
engine\source\Jass2\Jass.cpp          0x6f7abd74
engine\source\Jass2\Instance.cpp/.h   0x6f7ac352 / 0x6f7abef0
engine\source\Jass2\Compile.cpp/.h    0x6f7ac218 / 0x6f7ac258
engine\source\Jass2\Nodes.h           0x6f7ac1dc
engine\source\Jass2\parser.cpp        0x6f7acfcc
engine\source\Jass2\scanner.cpp       0x6f7aed48
```

plus RTTI names `.?AVJassScript@@` (`0x6f7abd48`), `.?AVCJassFunc@@` (`0x6f7865a0`),
`.?AUJassHandle@@` (`0x6f7abea4`), `.?AUJassString@@` (`0x6f7ac090`), `.?AUFrame@@` (`0x6f7abedc`).

Parser + AST (`Nodes.h`) + compiler (`Compile.cpp`) + instance (`Instance.cpp`) all present →
`inferred`: the engine compiles `.j` text to an internal instruction stream and executes it; it is
not a pure text/AST walker.

### 3.2 Native registry (`observed`)

Registration call site pattern (found 2192 times, `extract_natives.py`):

```
68 <sig ptr>   push signature string        e.g. "(Hunit;)R"
ba <name ptr>  mov  edx, name string        e.g. "GetUnitX"
b9 <func ptr>  mov  ecx, implementation
e8 <rel32>     call 0x6f62f140              ; registrar
```

Three contiguous runs:

| run | sites | entries | impls |
|---|---:|---:|---|
| A | `0x6f25fee0..0x6f26087c` | 124 | AI/editor natives (`Captain*`, `SetCampaignAI`, `DebugS`, `DisplayText*`) — unique impls |
| B | `0x6f2998f5..0x6f29e4d1` | 972 | core natives (`Deg2Rad` … `Preloader`) — unique impls |
| C | `0x6f3cc46d..0x6f3d19f9` | 1096 | **stub table**: every entry points at `0x6f3d1a20` = `xor eax,eax; ret` |

Counts (`observed`, command output): 2192 registrations → **1096 unique native names**
(A ∪ B), 1096 stub registrations (C, disabled script host).

Registrar `fcn.6f62f140` (271 B) parses the signature:

* walks the parameter list, skipping `H<name>;` handle type names, and **counts parameters**
  (skips `V`), storing the count at `native+0x20`;
* maps the return-type character through an index table at `0x6f62f270` (chars `B..V`) and a
  jump table at `0x6f62f250`, storing a type code at `native+0x38`.

Type codes (`observed`, both tables read with `px`):

| signature char | `V` | `C` | `I` | `R` | `S` | `H…;` | `B` |
|---|---:|---:|---:|---:|---:|---:|---:|
| code | 0 | 3 | 4 | 5 | 6 | 7 | 8 |

Parameter-count histogram over the 1096 natives (command output in `jass_natives.json`):

| params | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| natives | 199 | 371 | 257 | 140 | 70 | 32 | 11 | 10 | 4 | 2 |

Return-type histogram (top): `V` 522, `B` 161, `I` 91, `R` 78, `S` 16, `Hunit;` 50,
`Hevent;` 21, `Hplayer;` 11.

Calling convention (`observed`):
* natives receive one **value array** (`ebp+8` = first slot in every implementation, e.g.
  `0x6f2b0160` `mov ecx, [ebp+8]`), return the value in `eax` (float in `eax` for `R`,
  handle/index in `eax` for `H/I/B`, string handle for `S`);
* the engine does **not** enforce per-parameter types at call time — only the count and the
  return type live in the native record — so a faithful interpreter needs a tagged value type plus
  the return-type coercion (int/real/bool/string/handle) applied at the boundary.

### 3.3 Call path (`observed`)

```
0x6f62fc60(script, name, a, b, c)      ; JassScript::CallByName
   -> fcn.6f62ebd0(script, name)        ; find function by name
   -> fcn.6f643c20(script, func, ...)   ; push a Frame (".?AUFrame@@") and execute
0x6f62fff0                              ; get current JassScript (thread-local context)
0x6f292f90                              ; find function object (ExecuteFunc path)
0x6f2cca60                              ; ExecuteFunc native: new thread, then run target
```

`ExecuteFunc` (`0x6f2cca60`) resolves the string (`0x6f5c83f8`), finds the function
(`0x6f292f90`), builds a thread (`0x6f3252d0`, `0x6f086fc0`, `0x6f5fc940`, `0x6f2953c0`) and
starts it (`0x6f325730`) — i.e. the target runs **in a new thread**, matching JASS semantics.

### 3.4 Dispatch tables (`observed`)

| table | function | entries | data |
|---|---|---:|---|
| opcode dispatch | `0x6f2774d0` (7707 B, 1963 insns, 260 basic blocks) | 133 opcode bytes (1..133) → 133-byte map `0x6f279424` → **68 handlers** (`0x6f279314`, all distinct, `0x6f2775ee..0x6f2792a6`) | `jass_dispatch_tables.json` |
| node-type dispatch | `0x6f20f510` | **68 node types** (0..0x43) → `0x6f20fa80` (66 unique targets) | `jass_dispatch_tables.json` |

`inferred`: the 68-node table is the AST/expression evaluator (`cmp ecx, 0x43` then
`jmp [ecx*4+0x6f20fa80]` on a node tag at `[node+8]`); the 133-entry map is the instruction
dispatcher of the compiled-script executor. Per-opcode semantics were **not** decoded (see §5) —
the handler addresses are exported so a follow-up pass can label them one by one.

### 3.5 Execution budget (`observed`)

Both script entry points pass `0x493e0` = **300 000** as the last argument
(`0x6f2ccf0e` for `config`, `0x6f2ccf30` for `main`). `inferred`: the per-thread instruction budget
before the thread is killed (the classic "op limit").

### 3.6 What a faithful interpreter needs

1. **Value model**: `V/C/I/R/S/H/B` with the engine's codes (0/3/4/5/6/7/8); `H…` handles are
   opaque indices; `C` (code) is a function reference.
2. **Native surface**: 1096 natives, each with `(name, param_count, return_type, impl)`;
   no per-parameter type checks at call time, but the return value must be coerced to the declared
   type (the engine returns through `eax` and the VM tags it).
3. **Function call semantics**: named lookup (`0x6f62ebd0`), a `Frame` stack, `ExecuteFunc` /
   `TriggerExecute` spawn a **new thread**, `TriggerSleepAction`/`PolledWait` suspend it.
4. **Execution budget**: 300 000 ops per thread.
5. **Compile step**: text `.j` → instruction stream (Jass2 `Compile.cpp`); our runtime parses text
   directly, which is acceptable as long as observable semantics match (call-by-value, integer
   32-bit wrap, real = 32-bit float in the engine — `unknown` whether the VM keeps f64 internally).
6. **Globals/arrays**: `udg_*` globals + `bj_*` from `blizzard.j` must exist for real maps;
   our runtime currently loads neither `common.j` nor `blizzard.j`.

### 3.7 Gap list — our sim vs the engine

Command-produced (`make_jass_natives.py`):

| metric | value |
|---|---:|
| engine unique natives | **1096** |
| registered in `crates/wc3-jass/src/lib.rs` (`register_natives`) | 35 |
| of those, actual engine natives | **27** |
| of those, script-library functions, not natives | **8** (`I2B`, `B2I`, `StringLength`, `ModuloInteger`, `ModuloReal`, `BJDebugMsg`, `PolledWait`, `GetGameTime`) |
| engine natives missing in our runtime | **1069** (97.5 %) |

Implemented engine natives (27): `Atan2, Cos, CreateTrigger, CreateUnit, DisplayTextToPlayer,
GetOwningPlayer, GetRandomInt, GetUnitState, GetUnitTypeId, GetUnitX, GetUnitY, I2R, I2S, KillUnit,
Pow, R2I, R2S, RemoveUnit, S2I, SetUnitPosition, SetUnitState, Sin, SquareRoot, SubString,
TriggerAddAction, TriggerExecute, TriggerRegisterTimerEvent`.

The 8 non-natives are `common.j`/`blizzard.j` script functions in the real engine — they must be
provided by shipping the script library, not by native dispatch (otherwise map scripts that
redefine or alias them diverge).

Full per-name table (`name`, `signature`, `engine_address`, `registration_site`, `implemented`) is in
**`analysis/agent-re/formats/jass_natives.json`**.

---

## 4. Triggers and map start

### 4.1 `war3map.j` execution (`observed`)

Two tiny script entry points:

```
0x6f2ccf00  RunConfig:  script = current(); CallByName(script, "config", 0, 1, 1, 0x493e0)
0x6f2ccf30  RunMain:                              CallByName(script, "main",   0, 0, 0, 0x493e0)
```

Callers: `0x6f28dc58` (config) and `0x6f28ec7d` (main) inside the map-load path.

Context around the calls:

* before `config` (`0x6f28dc2b..0x6f28dc4c`): game-option flags are OR-ed into the game record
  (`[eax+0x38] |= flags` from options `0x20`/`0x40`) and the slot value is stored at `[eax+0x34]`;
* between `config` and `main`: `0x6f2f88a0` / `0x6f2f87f0` (start-up sequence),
  `[esi+0x390]` set/cleared (a "running" flag), `word [esi+0x28]` copied to `[esi+0x2a]`
  (current player slot bookkeeping);
* after `main`: `0x6f1d62b0(1)`, `0x6f314aa0(1)` / `0x6f314a30(1)` (camera/fog),
  `0x6f233590(0)` (game start).

`inferred` sequence at map start: compile `common.j` + `blizzard.j` + `war3map.j` →
run **`config`** (map/player configuration, `main` not yet run) → start-up/first-frame setup →
run **`main`** (which calls `InitBlizzard`, `InitGlobals`, `InitCustomTriggers`, `RunInitializationTriggers`
in stock maps) → enable gameplay.

No engine-side trigger-tree loading: `war3map.wtg` is absent from Game.dll (§2.7).  The `Trigger*`
natives (`TriggerRegisterTimerEvent`, `TriggerRegisterPlayerUnitEvent`, … 35 in total,
`Trigger` prefix) are the only route from script to the event system.

### 4.2 Event/trigger natives in the engine (`observed`)

35 natives have the `Trigger` prefix, including all 20 `TriggerRegister*` variants
(`TriggerRegisterTimerEvent`, `…TimerExpireEvent`, `…EnterRegion`, `…LeaveRegion`, `…UnitEvent`,
`…UnitStateEvent`, `…UnitInRange`, `…PlayerUnitEvent`, `…PlayerEvent`, `…PlayerStateEvent`,
`…PlayerChatEvent`, `…PlayerAllianceChange`, `…GameEvent`, `…GameStateEvent`, `…DialogEvent`,
`…DialogButtonEvent`, `…DeathEvent`, `…TrackableHitEvent`, `…TrackableTrackEvent`,
`…VariableEvent`, `…FilterUnitEvent`) plus `TriggerAddAction`, `TriggerAddCondition`,
`TriggerRemoveAction`, `TriggerRemoveCondition`, `TriggerClearActions`, `TriggerClearConditions`,
`TriggerExecute`, `TriggerExecuteWait`, `TriggerEvaluate`, `TriggerSleepAction`,
`TriggerWaitOnSleeps`, `TriggerWaitForSound`, `TriggerSyncStart`, `TriggerSyncReady`,
`TriggerReset`-family (`ResetTrigger`), `DestroyTrigger`, `EnableTrigger`, `DisableTrigger`,
`IsTriggerEnabled`, `GetTriggeringTrigger`, `GetTriggerEventId`, `GetTriggerEvalCount`,
`GetTriggerExecCount`.

Our runtime has `CreateTrigger`, `TriggerAddAction`, `TriggerRegisterTimerEvent`, `TriggerExecute`
— i.e. periodic timers only, no conditions, no events, no `TriggerSleepAction` suspension.

---

## 5. Unknowns and next steps

| item | status | next step |
|---|---|---|
| per-opcode semantics of the 133-entry dispatcher | `unknown` — handler addresses exported in `jass_dispatch_tables.json` | decode each of the 68 handlers with `pdf` and label (literal push, binary op, call, jump, return) |
| node-type identity for the 68-entry table | `unknown` | same approach on `0x6f20fa80`; correlate with `Nodes.h` string xrefs |
| W3I parser body | `unknown` — thin wrapper `0x6f3be8f0` only | follow `0x6f3c8400` into the registered parser |
| WPM / DOO bodies | `unknown` | same registry hook |
| engine W3E v7 vs v9 field differences | partially `observed` (two branches) | decode `0x6f3eb540` (tiles) per version |
| `MPQ_FILE_SECTOR_CRC` in the engine | `observed` absent in the read path | confirm with a crafted archive (needs engine execution — out of scope here) |
| locale/platform effect on real WC3 archives | `inferred` from the probe code | enumerate an archive's hash entries with non-zero locale to measure impact |
| whether the VM keeps `real` as f32 or f64 | `unknown` | inspect the arithmetic handlers (`0x6f20f510` cases) for `fld/fstp dword` vs `qword` |

---

## Appendix A — evidence command log

```
su -c 'id'                                  -> uid=0(root) context u:r:ksu:s0
r2 -v                                       -> radare2 6.2.6 +0 abi:174
rabin2 -S analysis/game_files/Files100_Storm.dll      -> 5 sections, .text 0x1000..0x3d000
rabin2 -S analysis/tft_files_Files100_Game.dll        -> ImageBase 0x6f000000, .text 0x691000 B
rabin2 -z ... > analysis/agent-re/formats/{storm_roc,storm_tft,game_tft}.strings.txt
r2 -q -e bin.cache=true -i <script> <dll>   -> all *.pd.txt dumps listed above
python3 analysis/agent-re/formats/extract_natives.py    -> natives=2192 unique=1096
python3 analysis/agent-re/formats/make_jass_natives.py  -> implemented 27 / missing 1069
python3 analysis/agent-re/formats/extract_dispatch.py   -> handlers 68 (68 unique), nodes 68 (66 unique)
```

## Appendix B — key address index

| address | what |
|---|---|
| `0x1501bea0` | crypt-table init (RoC) |
| `0x1501b610` | `hash_string(name, kind)` |
| `0x1501b2e0` | hash-table lookup / probe |
| `0x1501bb60` | `SFileOpenArchive` |
| `0x1501c060..0x1501c2c5` | header parse, table read + decrypt, `(attributes)` |
| `0x1501c9e0` | `SFileOpenFileEx` (FIX_KEY derivation at `0x1501cd99`) |
| `0x150185e0` | sector load (table, keys, stored rule) |
| `0x1500f7c0` | mask decompressor (5 codecs) |
| `0x1503e330..0x1503e350` | RoC decompress table |
| `0x1504635c..0x1504637c` | TFT decompress table |
| `0x6f62f140` | JASS native registrar |
| `0x6f62fc60` / `0x6f643c20` | call-by-name / frame push |
| `0x6f2ccf00` / `0x6f2ccf30` | `config` / `main` script entry points |
| `0x6f2774d0` | opcode dispatcher (133 → 68) |
| `0x6f20f510` | node-type dispatcher (68) |
| `0x6f28da40` | map-file registry chain |
| `0x6f3eb1a0` / `0x6f3eb2c0` | W3E parser / header |
| `0x6f3be8f0` | W3I loader wrapper |
| `0x6f39dc04` | `.wts` name construction |

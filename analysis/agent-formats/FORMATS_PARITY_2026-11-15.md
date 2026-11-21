# ARCHIVE/FORMAT PARITY — MPQ locale filter, five-codec whitelist, W3E version bounds

Ledger 411 · task `wc3rust/formats-parity` · date 2026-11-15 · operator session
Evidence base: `analysis/agent-re/formats/ENGINE_MAP.md` + `VERIFY.txt` (r2 lane 407),
re-checked in this session with `r2`/`rabin2` where a claim needed a byte-level anchor.

## Scope correction (observed)

The dispatch located the MPQ reader at `crates/wc3-formats/src/lib.rs:560-586`. The reader is
`crates/wc3-mpq/src/lib.rs`; the cited line range matches `find_block` there (the hash-entry
probe). All MPQ changes are therefore in `crates/wc3-mpq/**`, W3E changes in
`crates/wc3-formats/**`. Nothing outside those two crates plus `analysis/agent-formats/**` was
written.

## Changes

### 1. Locale / platform hash-entry filtering (`mpq.locale-platform-filter`)

Engine: `0x1501b3b5..0x1501b3dc` skips a hash entry whose `wLocale` / `wPlatform` is non-zero and
not equal to the current value; the globals live at `0x15045784` / `0x15045786` and are
**zero-initialised** (`px` of the shipped RoC Storm.dll: 8 zero bytes at `0x15045780`). No
Game.dll build calls `SFileSetLocale` (`FF 15 ..` against the ordinal-272 IAT slot is absent in
both RoC `0x6f5ffd3c` and TFT `0x6f69295c`), so the engine's effective filter is
"neutral entries only".

`crates/wc3-mpq/src/lib.rs`
- `HashEntry` now carries `locale`/`platform` (low/high u16 of hash-entry word 2), parsed in
  `open_at_indexed`.
- `MpqArchive` carries `locale`/`platform` (default 0), with `locale()`, `platform()`,
  `set_locale()`, `set_platform()` and a `(listfile)` cache rebuild so `by_name` cannot go stale
  after a switch.
- `find_block` applies the engine predicate and keeps probing when an entry is filtered out.
- `contains()` now probes the hash table as well as the listfile cache (it previously reported
  `false` for every name in an archive without `(listfile)`, i.e. every shipped map).

### 2. Five-codec whitelist (`mpq.unknown-codec-rejected`)

Engine: `SFileDecompress` (`0x1500f7c0`) walks five `(mask, handler)` slots and stops at the
table bound (`cmp edi,0x1503e330` RoC / `0x15046360` TFT); masks are `0x08` PKWARE, `0x02` zlib,
`0x01` Huffman, `0x80` ADPCM-stereo, `0x40` ADPCM-mono. bzip2 `0x10`, sparse `0x20` and LZMA
`0x12` have **no** handler.

`crates/wc3-mpq/src/lib.rs`
- `MPQ_CODEC_MASK_WC3` (0xCB) + a five-row decode table; any other tag bit returns
  `MpqError::UnsupportedCompression(tag)` before a decoder runs.
- `MPQ_FILE_IMPLODE | MPQ_FILE_COMPRESS` (`0x300`): the engine's two equality tests
  (`(flags & 0xFF00) == 0x100` @ `0x15018a6f`, `== 0x200` @ `0x15018a76`) both fail and the unit is
  copied verbatim (`0x15018b98`); the reader now does the same instead of running a tag decoder
  (ENGINE_MAP §1.7 item 2, the second of the two byte-changing divergences).
- The unreachable v1 decode path (`decode_v2`, LZMA helper, bzip2/sparse branches) was removed:
  `open_at_indexed` forces the v0 layout the engine uses, so that code could never run, while its
  presence contradicted the measured five-codec table. `crates/wc3-mpq/src/sparse.rs` is retired
  to `analysis/agent-formats/retired/sparse.rs`; `bzip2` and `lzma-rust2` dependencies dropped
  from `crates/wc3-mpq/Cargo.toml`. `CodecUsage` bits for bzip2/lzma/sparse stay as catalogue
  bits (a *declared* foreign mask can still be reported).
- Sector offset table: read exactly `(sectorCount + 1) * 4` bytes, with no
  `MPQ_FILE_SECTOR_CRC` dword — the engine allocates and reads exactly that
  (`0x1501cdcd..0x1501cde8`, `0x15018646`); StormLib's extra CRC entry is a later addition.
  (ENGINE_MAP §1.7 item 1; no decoded byte changes either way.)

### 3. W3E version bounds (`w3e.version-bounds`)

Engine: `if (version < 3) fail` (`0x6f3eb1fe`), `if (version > 11) fail` (`0x6f3eb207`,
`cmp eax, 0xb`), branches at 7 (`0x6f3eb42b`) and 9 (`0x6f3eb281`).

`crates/wc3-formats/src/w3e.rs`
- `VERSION_MIN = 3`, `VERSION_MAX_ENGINE = 11`, `VERSION_MAX = 12`.
- `Terrain::parse_classic_engine` — engine profile, 3..=11, rejects v12 and above with an explicit
  `FormatError::Unsupported` naming the ceiling.
- `Terrain::parse` — bounded multi-era entry point, 3..=12 (v12 = Reforged 8-byte tiles,
  War3Net `MapEnvironmentFormatVersion.v11|v12`). Versions below 3 or above 12 are rejected; the
  previous parser accepted **any** version and silently decoded v13+ with the v12 layout.
- Kept multi-era on purpose: the 99-map corpus contains two 3.0.0 maps with W3E v12, and the
  dispatch forbids corpus regressions; the classic engine profile is what the runtime uses.

### 4. Documentation of items 5 and 6

- `.wtg`/`.wct` are **editor-side only**: Game.dll has 0 string references to either (VERIFY.txt,
  WorldEdit.exe has 1 each). Comment added at the `probe_names` list in
  `crates/wc3-mpq/src/lib.rs` where both names appear; they stay listed because map archives ship
  them and tooling indexes them, never because a runtime opens them.
- Sector-table semantics (`(n+1)*4`, `n = ceil(size/sectorSize)`, key `fileKey-1`,
  `fileKey+index`, stored iff `raw >= expected`, `sectorSize = 512 << byte[+0x0E]`) were already
  matched; the one cosmetic difference (SECTOR_CRC dword) is now aligned as well.
- The 336-byte texture records at `0x6f3eb491/0x6f3eb4bb/0x6f3eb525` are the engine's **in-memory**
  texture structs (`imul eax,eax,0x150`) — the file stores 4-byte texture ids, which the parser
  already reads; nothing to change in the file layout.

## Tests

Canonical id → test function (mechanic-id policy: id in the doc comment, snake_case name):

| canonical id | test | file |
|---|---|---|
| `mpq.locale-platform-filter` | `mpq_locale_platform_filter` | `crates/wc3-mpq/tests/archive_parity.rs` |
| `mpq.unknown-codec-rejected` | `mpq_unknown_codec_rejected` | `crates/wc3-mpq/tests/archive_parity.rs` |
| `mpq.300-raw-copy` | `mpq_implode_compress_copies_raw` | `crates/wc3-mpq/tests/archive_parity.rs` (in-crate unit twin: `implode_compress_300_copies_raw_without_a_tag`) |
| `w3e.version-bounds` | `w3e_version_bounds` | `crates/wc3-formats/src/w3e.rs` |

The MPQ fixtures are built byte by byte with an independent implementation of the engine crypto
(crypt table, hash, block cipher) so the test does not reuse the code under test. The locale
fixture stores one name three times — locale `0x409` first in probe order, then platform 1, then
neutral — so a reader that ignores the fields returns the locale payload and fails.

Commands (both required by the dispatch):

```
CARGO_BUILD_JOBS=2 cargo test -p wc3-formats --offline   # rc=0: 72 + 1 (99-map corpus) + 4 tests
CARGO_BUILD_JOBS=2 cargo test -p wc3-mpq --offline       # rc=0: 13 lib + 3 + 1 + 2 integration
```

Logs: `test_formats_after.txt`, `test_mpq_after.txt`.

Falsification: with the locale/platform predicate forced to `true` the
`mpq_locale_platform_filter` test fails on the payload assertion (wrong entry returned); the
predicate was restored and the suite re-run green.

## Corpus run (99 maps) — before vs after

Tool: `cargo run -p wc3-formats --example map_corpus_audit` (the example behind
`crates/wc3-formats/tests/map_corpus.rs`).

| run | files | maps | opened | failures |
|---|---|---|---|---|
| before (`corpus_before.txt`) | 150 lines | 99 | 99 | 0 |
| after (`corpus_after.txt`) | 150 lines | 99 | 99 | 0 |

After stripping the newly added `engine=…` field, the two outputs are **byte-identical**
(`identical: True`, 150/150 lines): every other field (MPQ base, listfile/block counts, W3E
version+dims+tiles, WPM, W3I, DOO/units counts, W3U/W3A/W3T/W3Q/W3H/W3B/W3D base+custom counts) is
unchanged. Engine-profile split of the after run:

```
97  w3e=v11 engine=ok
 2  w3e=v12 engine=reject(unsupported format variant: w3e version above the classic engine
            ceiling (11))
```

The corpus test `every_donor_map_parses_through_the_formats_crate` now also asserts that split per
map, so a future regression in either direction fails the gate.

## Real-archive audits (regression proof beyond the corpus)

`cargo run -p wc3-mpq --example locale_audit -- War3.mpq War3x.mpq War3xlocal.mpq`:

```
War3.mpq:      hash=32768 blocks=10560 listfile=10523
  hash entries: locale!=0 0, platform!=0 0, both 0
  listfile names: resolution differs 0, resolves only via a tagged entry 0
War3x.mpq:     hash=32768 blocks=5634 listfile=5610   -> 0 / 0 / 0, differs 0
War3xlocal.mpq:hash=4096  blocks=1167 listfile=1156   -> 0 / 0 / 0, differs 0
```

Zero tagged entries in every shipped archive, so the new predicate cannot change a single asset
lookup; the divergence is confined to archives that actually carry localized duplicates (as the
engine intends).

`cargo run -p wc3-mpq --example codec_census -- War3.mpq War3x.mpq War3xlocal.mpq`:

```
War3.mpq:       files=10523 failures=0 decoded_bytes=710907954
War3x.mpq:      files=5610  failures=0 decoded_bytes=370102046
War3xlocal.mpq: files=1156  failures=0 decoded_bytes=128053163
observed codecs: stored, zlib, huffman, adpcm-mono, adpcm-stereo (and mixed per-sector tags)
```

17,289 files, 0 failures, and no file needs a codec outside the five — the whitelist and the
removal of bzip2/LZMA/sparse are safe for every shipped byte.

## Evidence index

| claim | evidence |
|---|---|
| locale/platform skip | ENGINE_MAP §1.3 (`0x1501b3b5..0x1501b3dc`); `px` `0x15045784` = 0; no `FF 15` to ordinal-272 IAT in either Game.dll |
| five codecs | ENGINE_MAP §1.6 / VERIFY.txt (`mask=[1,2,8,64,128]`, loop bounds) |
| `0x300` raw copy | ENGINE_MAP §1.5 (`0x15018a6f`, `0x15018a76` → `0x15018b98`) |
| W3E ceiling | ENGINE_MAP §2.2 (`0x6f3eb1fe`, `0x6f3eb207`, branches 7/9) |
| `.wtg`/`.wct` editor-only | VERIFY.txt (`Game.dll: .wtg=0 .wct=0`) |
| sector table | ENGINE_MAP §1.5 (`0x1501cdb5..0x1501cde8`, `0x15018646`) |
| corpus unchanged | `corpus_before.txt` vs `corpus_after.txt` (normalized identical) |

## Follow-ups outside this owner path (read-only for this lane)

1. `docs/ENGINE.md:13` and `docs/ENGINE.md:42`, `docs/PLAN.md:61` still describe the reader as
   "zlib/bzip2/stored". Correct text: five codecs `0x01/0x02/0x08/0x40/0x80`, bzip2/LZMA/sparse
   rejected with `UnsupportedCompression`. Owner: docs lane; the claim is now disproved by
   `codec_census` and by the engine table.
2. `crates/wc3-re/src/storm.rs:30` documents "8..16: 0x01 huffman, 0x02 zlib, 0x08 PKWARE, 0x10
   bzip2, 0x12 ..." as the sector-tag space; the 0x10/0x12 part is not decodable here. Owner: RE
   lane (read-only for this task).
3. `crates/wc3-formats/src/map.rs:15-17` has three pre-existing unused imports (warnings only,
   untouched by this task) — cosmetic cleanup for the next owner of that file.

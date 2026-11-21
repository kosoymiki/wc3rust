# PKWARE sector-0 root cause and fix (byte-level)

Evidence object: `analysis/war3_full3/Maps/Campaign/Human04.w3m`
(sha256 `4db18ce970979de27d3029f5c3c59b5a656f3e77c1cd924cabbf29cde21ee6e5`).

Canonical implementation after BASE v1: `wc3-mpq` owns the byte logic
(`crates/wc3-mpq/src/{stream.rs,lib.rs,pkware.rs}`); `wc3-re::mpqex` is a facade
and re-exports the codecs (see below).

## MPQ geometry

- `HM3W` header at 0x0000, MPQ signature at **0x0200** (`MpqArchive::open_at(base=512)`).
- `war3map.wts` block table entry: file offset **0x2e7b7** (190,375), compressed size **0x9a5** (2,469), uncompressed size **0x195c** (6,492), flags **0x80030200**:
  - `0x80000000` exists, `0x00000200` compressed, `0x00010000` encrypted, `0x00020000` FIX_KEY;
  - codec mask `(flags >> 8) & 0xFF = 0x02` (zlib at file level).
- Physical block start = 0x0200 + 0x2e7b7 = **0x2e9b7**.
- Sector size = `512 << header.block_size(3)` = **4096**; sector count = ceil(6492/4096) = 2.
- Sector table (12 bytes) at 0x2e9b7, decrypted with `hash("war3map.wts",3)+offset ^ size - 1` = `[12, 1485, 2469]`.
- Sector 0: declared length 1,473 (`1485-12`). `wc3-mpq` reads exactly 1,473
  bytes, decrypts whole dwords (1,472 bytes) and leaves the trailing 1 byte
  plaintext (`stream.rs:194-205`); the PKWARE tag byte is included in the
  sector, so the payload is 1,472 bytes.
- Decrypted sector bytes begin:
  `08 00 06 a6 50 91 92 c4 c9 11 10 30 ...`
  - `0x08` = **PKWARE DCL** sector tag (not present in the file-level zlib mask `0x02`);
  - `00 06` = PKWARE mode 0 (binary) + dictionary bits 6;
  - remaining bytes = implode stream.

## Ground truth via StormLib donor

`donors/StormLib/src/pklib/explode.c` compiled with clang (harness in
`analysis/agent-re/tmp/pk_test.c`) on the 1,472-byte payload returns output
length **4096** and starts with:

```
53 54 52 49 4e 47 20 30 0d 0a 7b ...   "STRING 0\r\n{..."
```

StormLib's `explode()` returns `CMP_ABORT` (4) because `Expand()` returns
`0x306` when the bit reader hits end-of-input immediately after the final
symbol, even though the full 4096 bytes were emitted. StormLib callers use the
output length and ignore that status (verified by the C output above).

## What it was NOT

- **Not the decryption key:** the decrypted sector table is exactly
  `[12, 1485, 2469]`; the decrypted sector starts with tag `0x08` and PKWARE
  header `00 06` (binary mode, dictionary bits 6). The StormLib C donor decoded
  the identical decrypted bytes to 4096 bytes.
- **Not the PKWARE literal/length/distance tables:** `donors/StormLib/src/pklib/
  explode.c` produced `STRING 0\r\n{...` from the same payload; only the Rust
  port's end-of-input status handling differed.
- **Not a corrupt/absent stream:** the decompressed size is exactly the sector's
  declared expected size (4096 for sector 0 / 2396 for sector 1).

## Two Rust-port defects (now fixed in the canonical crate)

1. **Codec selection:** `wc3-mpq` decided "stored" from
   `data.len() == expected` and only allowed a sector tag when it matched the
   file-level mask. Human04 declares zlib (`0x02`) but stores PKWARE (`0x08`),
   so the sector was sent to zlib. Fix: `decompress_into` receives the declared
   sector length (`declared_len`), treats the per-sector tag as authoritative
   (`lib.rs:527-540`), and detects stored only with
   `declared_len == expected` (`lib.rs:531`).
2. **PKWARE end-of-input:** the Rust port rejected `0x306` unconditionally.
   `wc3-mpq::pkware::explode` now accepts `0x306` when the output buffer is
   non-empty (`pkware.rs:309-320`); the caller validates the decoded length
   against the block table.

## Fix location / delegation

- Canonical: `crates/wc3-mpq/src/stream.rs:194-219` (exact sector read, dword
  decrypt, `declared_len`), `crates/wc3-mpq/src/lib.rs:499-620`
  (`decompress_into`), `crates/wc3-mpq/src/pkware.rs:309-320`.
- `crates/wc3-re/src/mpqex.rs`: `MpqEx` is a facade over `MpqArchive`; its
  `pkware`/`huffman`/`adpcm` submodules are type-identical re-exports of
  `wc3_mpq::{pkware,huffman,adpcm}` (`mpqex/{pkware,huffman,adpcm}.rs`).
- `crates/wc3-re/src/mapfiles.rs:100-113` reads through `MpqArchive` directly;
  no fallback reader remains. `StormFs::read` (`storm.rs:197-207`) does the same.
- Result (scan before the merge freeze): `maps.errors = []`,
  `wts_entries = 4428`, `ok=true` (`analysis/agent-re/scan.json`).

Tests (run only after BASE freeze): `mpqex::tests::reads_encrypted_pkware_wts_from_roc_map`,
`mpqex::tests::reads_all_inner_files`, `mapfiles::tests::opens_real_roc_campaign_map`,
`tests/merge_parity.rs::mpqex_codecs_are_wc3_mpq_reexports`.

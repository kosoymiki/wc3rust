# MPQ codec layer — audit status (LEDGER 486)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`, crate `crates/wc3-mpq`.

| ID | Priority | Status |
|----|----------|--------|
| AUD-003 | P1 | FIXED |
| AUD-008 | P2 | FIXED |

One-line registry rows (for the merged registry):

```
AUD-003 — FIXED — decode output is bounded by the declared unit size (take(expected+1) for zlib, explode_limited/decompress_limited for PKWARE/Huffman), symmetric > expected rejection, MAX_FILE_SIZE=512 MiB / MAX_SECTOR_SIZE=16 MiB / MAX_SECTORS caps checked before allocation, checked sector-table arithmetic, new MpqError::Oversize + MpqError::SizeLimitExceeded (and the typed error survives the Read boundary) — tests: aud003_zlib_output_exactly_expected_is_accepted, aud003_zlib_output_one_byte_over_declared_is_rejected, aud003_zlib_output_far_over_declared_is_rejected, aud003_multi_codec_chain_decodes_each_stage_within_budget, aud003_malformed_mask_is_rejected_before_decoding, aud003_stored_sector_fallback_is_preserved, aud003_zero_sector_is_sparse_without_allocation_above_the_declaration, aud003_zero_length_sector_streams_zeros, aud003_declared_size_above_the_cap_is_rejected, aud003_oversize_sector_in_a_real_archive_is_a_typed_error, aud003_block_entry_above_the_file_size_cap_is_rejected_before_allocating, aud003_stored_sectors_in_a_compressed_file_read_verbatim, aud003_large_but_legitimate_archive_decodes_cover_to_cover, largest_shipped_entry_is_below_the_cap — commit: b176901
AUD-008 — FIXED — ADPCM header validated before use (channels 1|2, header_len = 2+2*channels, shift <= MAX_SHIFT=15), decoder returns Result<Vec<u8>, MpqError>, no silent empty output; byte-identical to the pre-fix decoder on 288 legal cases — tests: adpcm::tests::{every_legal_shift_decodes, out_of_range_shift_is_rejected, empty_and_short_headers_are_rejected, illegal_channel_counts_are_rejected, mono_and_stereo_shapes_differ, output_is_bounded_by_the_input} — commit: b176901
```

---

## AUD-003 — MPQ decompression did not bound the actual output

**Status: FIXED**

### Reproduction (before the fix)

```
$ scripts/h2-cargo test -p wc3-mpq --lib zz_repro -- --nocapture
REPRO_AUD003 result_is_ok=true out_len=4096 expected=1024 (payload 26 B)
thread 'tests::zz_repro_oversize_zlib_is_accepted' panicked at crates/wc3-mpq/src/lib.rs:1464:
REPRO_AUD003 oversize output accepted: 4096 bytes for expected=1024
```

A 26-byte zlib payload produced 4096 bytes with `expected = 1024` and
`decompress_unit` returned `Ok`: `decode_one` used `expected` only as
`Vec::with_capacity` and `read_to_end` grew without limit, and the post-check
was one-sided (`out.len() < expected`, never `>`). `read_raw`/`read` then
collected the whole stream into a `Vec`.

### Mechanism of the fix (`src/lib.rs`, `src/stream.rs`, `src/pkware.rs`, `src/huffman.rs`)

- **Bounded decode.** `decode_one` gives zlib a
  `ZlibDecoder::new(payload).take(expected + 1)` reader, so `read_to_end`
  cannot hand back more than one byte past the declared size; PKWARE and
  Huffman gained `explode_limited` / `decompress_limited`, which stop with
  `SizeLimit` at the budget instead of growing the heap (`explode` /
  `decompress` keep their signature and use `MAX_FILE_SIZE` as the standalone
  budget). ADPCM output is derived from the input length (one `i16` per payload
  byte) and is checked like every other stage.
- **Symmetric oversize check.** `decompress_unit` now rejects
  `out.len() > expected` with `MpqError::Oversize { expected }`; the short-output
  rule is unchanged (stored fallback `out == payload` still accepted).
- **Hard limits.** `MAX_FILE_SIZE` = 512 MiB (block `size`/`compressed_size`),
  `MAX_SECTOR_SIZE` = 16 MiB (the largest sector a valid `block_size` shift of
  15 can declare), `MAX_SECTORS` = `MAX_FILE_SIZE / 512`. `check_block_sizes`
  runs in `check_readable` and at the top of `MpqFileReader::new`, i.e. before
  `Vec::with_capacity(block.size)`, `vec![0; compressed_size]` or the sector
  table allocation, and reports `MpqError::SizeLimitExceeded { requested, limit }`.
- **Checked arithmetic.** `sector_count + 1` and `(sector_count + 1) * 4` use
  `checked_add`/`checked_mul` (`Corrupt("sector offset table size overflow")`);
  the old `> 1_000_000` magic number is replaced by the derived `MAX_SECTORS`.
- **Typed error across `Read`.** `MpqFileReader::read` wraps the `MpqError`
  inside the `io::Error`; `restore_sector_error` downcasts it back, so
  `archive.read()` reports `Oversize`/`SizeLimitExceeded` instead of
  `Corrupt("corrupt sector payload")`.
- **Compatibility.** Every stored-sector rule is preserved: `!COMPRESS` copy,
  solo `IMPLODE`, `0x300` raw copy, `declared_len >= expected` stored sector,
  zero-length sector ⇒ `expected` zeros, and the zlib "does not compress" payload
  fallback.

New error variants (with `Display`):

```rust
MpqError::SizeLimitExceeded { requested: u64, limit: u64 },
MpqError::Oversize { expected: u64 },
```

### Tests

Unit tests in `crates/wc3-mpq/src/lib.rs` (`aud003_*`) and
`crates/wc3-mpq/src/adpcm.rs`; the AUD-003 set builds real archives with a
test-only v0 writer (header + encrypted hash/block tables + sector tables):

| Requirement | Test |
|---|---|
| zlib output exactly `expected` | `aud003_zlib_output_exactly_expected_is_accepted` |
| zlib output `expected + 1` | `aud003_zlib_output_one_byte_over_declared_is_rejected` |
| zlib output far larger (32 MiB from a ~300 KiB stream) | `aud003_zlib_output_far_over_declared_is_rejected` |
| multi-codec chain (zlib → ADPCM-mono) | `aud003_multi_codec_chain_decodes_each_stage_within_budget` |
| malformed mask (zlib + bzip2) | `aud003_malformed_mask_is_rejected_before_decoding`, `codec_tags_match_stormlib_dispatch` |
| zero sector / sparse sector | `aud003_zero_sector_is_sparse_without_allocation_above_the_declaration`, `aud003_zero_length_sector_streams_zeros` |
| stored-sector fallback | `aud003_stored_sector_fallback_is_preserved`, `aud003_stored_sectors_in_a_compressed_file_read_verbatim` |
| large but legitimate archive (8 MiB, 2048 sectors, read + stream) | `aud003_large_but_legitimate_archive_decodes_cover_to_cover` |
| oversize sector in a real archive (typed error) | `aud003_oversize_sector_in_a_real_archive_is_a_typed_error` |
| 4 GiB block entry rejected before allocating | `aud003_block_entry_above_the_file_size_cap_is_rejected_before_allocating` |
| declared size above the cap | `aud003_declared_size_above_the_cap_is_rejected` |
| cap above every shipped entry | `largest_shipped_entry_is_below_the_cap` |

Measured block-size maxima (printed by `largest_shipped_entry_is_below_the_cap`):

```
MPQ_MAX_BLOCK War3.mpq:      6591551 bytes  (6.3 MiB)
MPQ_MAX_BLOCK War3x.mpq:    10416294 bytes  (9.9 MiB)
MPQ_MAX_BLOCK War3xlocal.mpq: 1236952 bytes (1.2 MiB)
```

### Compatibility evidence (real archives)

`codec_census` decodes every listfile entry of `War3.mpq` and tallies the
codecs; the run before and after the fix is byte-identical:

```
War3.mpq: files=10523 failures=0 decoded_bytes=710907954
  stored: 4122
  stored+adpcm-mono+huffman: 25
  stored+zlib: 61
  stored+zlib+adpcm-mono+huffman: 93
  zlib: 3655
  zlib+adpcm-mono+huffman: 2532
  zlib+adpcm-stereo+huffman: 35
```

(`diff census-before.txt census-after.txt` → identical, rc=0 for both runs.)

---

## AUD-008 — ADPCM `shift` taken from the untrusted header without validation

**Status: FIXED**

### Reproduction (before the fix)

```
$ scripts/h2-cargo test -p wc3-mpq --test zz_repro_tmp -- --nocapture
test zz_repro_adpcm_shift_from_untrusted_header ...
thread 'zz_repro_adpcm_shift_from_untrusted_header' panicked at crates/wc3-mpq/src/adpcm.rs:83:29:
attempt to shift right with overflow
test zz_repro_adpcm_short_header_is_silent ... 0 bytes (silently empty)
```

`shift = data[1]` fed `step >> shift` directly; `shift >= 32` is a shift-overflow
panic (debug) or a masked shift (release) and a truncated header produced an
empty `Vec` that looked like a valid zero-length decode.

### Mechanism of the fix (`src/adpcm.rs`, `src/lib.rs`)

- `decompress(data, channels) -> Result<Vec<u8>, MpqError>` (was `-> Vec<u8>`).
- `channels` must be 1 or 2 (`MAX_CHANNELS`), the header must be at least
  `header_len(channels) = 2 + 2 * channels` bytes, and `shift` must be
  `<= MAX_SHIFT = 15` (`STEP_TABLE` peaks at 32767 = 15 bits; the encoder never
  emits more, and larger values can only produce zero deltas). Violations return
  `MpqError::Corrupt("ADPCM channel count must be 1 or 2" / "ADPCM header
  truncated" / "ADPCM shift out of range")` — no default and no silent empty
  output.
- Output is bounded by the input (`2 * len + header_len(channels)`), which the
  MPQ stage still checks against `expected`.
- `decode_one` propagates the `Result`; the `wc3-re::mpqex` re-export pin in
  `crates/wc3-re/tests/merge_parity.rs` was updated to the new signature (the
  re-export itself is unchanged).

### Tests

`crates/wc3-mpq/src/adpcm.rs` `mod tests`:

| Requirement | Test |
|---|---|
| every legal shift 0..=15, both channel counts | `every_legal_shift_decodes` |
| maximum legal shift | `every_legal_shift_decodes` (`MAX_SHIFT`) |
| 16/31/32/255 rejected | `out_of_range_shift_is_rejected` |
| empty / short header (mono and stereo) | `empty_and_short_headers_are_rejected` |
| channel counts 0/3/8 rejected | `illegal_channel_counts_are_rejected` |
| mono/stereo output shape | `mono_and_stereo_shapes_differ` |
| output bounded by input | `output_is_bounded_by_the_input` |

Byte-identity check against the pre-fix decoder (`git show HEAD:…/adpcm.rs`,
288 legal cases: 2 channel counts × 16 shifts × 9 payload lengths, xorshift
payloads; temporary example, removed after the run):

```
ADPCM_EQUIVALENCE ok cases=288
```

---

## Commands and exit codes

```
scripts/h2-cargo test -p wc3-mpq
  running 34 tests  -> test result: ok. 34 passed; 0 failed
  running 3 tests   -> archive_parity:            ok. 3 passed
  running 1 test    -> human04_map:               ok. 1 passed
  running 2 tests   -> listfile_less:             ok. 2 passed
  doc-tests: 0
scripts/h2-cargo check -p wc3-mpq --all-targets   -> Finished (rc=0)
scripts/h2-cargo check -p wc3-re  --all-targets   -> Finished (rc=0)
scripts/h2-cargo test  -p wc3-re  --test merge_parity -> ok. 5 passed (rc=0)
scripts/h2-cargo run -p wc3-mpq --example codec_census -- War3.mpq  -> rc=0
```

## Limitations

- The zlib hard cap is `expected + 1` bytes of *decoded* output; the decoder
  still allocates the compressed input (`block.compressed_size`, capped at
  512 MiB) exactly as StormLib's `SFileReadFile` does.
- HET/BET (v2+) archives remain unsupported (`UnsupportedVersion`), unchanged.
- `MAX_FILE_SIZE` is a policy value: a legitimate entry above 512 MiB would now
  be rejected. No shipped Warcraft III archive comes close (largest: 9.9 MiB).

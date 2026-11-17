# Media parser audit — AUD-009 / AUD-010 / AUD-011

Scope: untrusted-media parsers for the WC3 cinematics and textures.
Harness ledger: 488. Fix commit: `2a7a392ab17ec820f86215a835735a209b33040b`.
Base revision: `319588cc5c5087aa31102471075a19ac3acca518` (2026-11-17).
Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`,
host `aarch64-linux-android` (Termux). Every cargo invocation went through
`scripts/h2-cargo` (single-slot wrapper: one cargo at a time on the device).

| ID | Status | Where | Regression tests |
|----|--------|-------|------------------|
| AUD-010 | **FIXED** | `crates/wc3-bink/src/avi.rs` (`parse_strl`) | `rejects_every_short_strh_length`, `rejects_short_audio_strh`, `accepts_minimum_and_padded_strh`, `rejects_short_strf` |
| AUD-011 | **FIXED** | `crates/wc3-bink/src/avi.rs` (`parse_movi`) | `enforces_the_rec_depth_limit`, `deep_nesting_does_not_exhaust_the_stack`, `nested_rec_lists_keep_file_order`, `empty_nested_lists_are_skipped`, `rejects_malformed_declared_sizes`, `keeps_many_sibling_chunks_in_order`, `full_avi_with_nested_rec_lists`, `movi_fuzz_never_panics` |
| AUD-009 | **FIXED** | `crates/wc3-render/src/astc.rs` (`parse`, `to_image`, `payload_len`) | `layout_matches_the_block_grid`, `rejects_dimensions_that_overflow_the_u32_payload`, `rejects_u24_max_dimensions`, `rejects_zero_and_non_2d_sides`, `payload_length_is_exact`, `enforces_the_image_cap` |

## Reproduction before the fix

The three defects were reproduced first, in temporary `tests/repro_aud_pre.rs`
harnesses that used only the pre-fix public API. All three failed on the
unpatched tree; the same files passed after the patch and were then removed
(their cases live on as the permanent unit tests above).

| Defect | Command | Observed (pre-fix) | rc |
|--------|---------|--------------------|----|
| AUD-010 | `scripts/h2-cargo test -p wc3-bink --test repro_aud_pre -- --exact aud010_short_strh_must_error_not_panic` | `panicked at crates/wc3-bink/src/avi.rs:364:47: index out of bounds: the len is 7 but the index is 7` | 101 |
| AUD-011 | `scripts/h2-cargo test -p wc3-bink --test repro_aud_pre -- --exact aud011_deep_rec_must_error_not_recurse` | `process didn't exit successfully ... (signal: 11, SIGSEGV: invalid memory reference)` on a 2 400 236-byte file with 200 000 nested `LIST rec ` | 101 |
| AUD-009 | `scripts/h2-cargo test -p wc3-render --test repro_aud_pre` | `panicked at crates/wc3-render/src/astc.rs:73:24: attempt to multiply with overflow` (both `AstcFile::parse` and `decode`) | 101 |

Post-fix, the same three harness files: `2 passed` / `2 passed`, rc 0.

## AUD-010 — short `strh` panicked the AVI parser

**Mechanism.** `parse_strl` kept the `strh` body as a slice and, whenever a
`strf` was present, indexed `strh[0..7]` directly; `dwScale`/`dwRate`/`dwLength`
at offsets 20/24/32 were read through `rd_u32`, which silently returned 0 for a
short body. A chunk with a declared size of 4..35 bytes therefore panicked
(`avi.rs:364`), and a 0..3-byte chunk panicked one line earlier.

**Fix.** `STRH_MIN_LEN = 36` — the exact layout the parser reads
(`fccType`+`fccHandler` 0..8, `dwScale`/`dwRate` 20..28, `dwLength` 32..36) — is
enforced before any field access, and the `strf` minimums (20 for `vids`
through `biCompression`, 8 for `auds` through `nSamplesPerSec`) are enforced the
same way. Both return the new typed `AviError::InvalidHeader { what, len, need }`
instead of a panic or a zero-filled stream. `rd_u16`/`rd_u32` were removed: every
remaining field read is preceded by an explicit length check. Stream types other
than `vids`/`auds` are still ignored without a length requirement.

**Required test matrix.**

| Case | Test | Asserts |
|------|------|---------|
| `strh` length 0..7 | `rejects_every_short_strh_length` (loop `0..36`) | `InvalidHeader`, `len` echoed, 0..3 report `strh (fccType)` |
| `strh` length 8..35 | same loop | `InvalidHeader { what: "strh", len, need: 36 }` |
| exact minimum | `accepts_minimum_and_padded_strh` (`36`) | parses; `scale=41554`, `rate=1_000_000`, `length=7` |
| valid video + audio header | `accepts_minimum_and_padded_strh`, `parses_streams_and_payloads` | `vids`/`BLZ0`/800x368, `auds` 0x0055 / 2ch / 44.1 kHz |
| chunk padding | `accepts_minimum_and_padded_strh` (`37`, `56`) | odd 37-byte body: the pad byte is skipped and `strf` is still found |
| short `strf` | `rejects_short_strf` | `InvalidHeader { what: "strf", len: 19, need: 20 }`, audio `{ len: 7, need: 8 }` |
| fuzz | `fuzz_never_panics` | 64 mutated files + 64 random prefixes, no panic |

## AUD-011 — unbounded `LIST rec ` recursion

**Mechanism.** `parse_movi` called itself for every nested `LIST rec `, so the
depth was bounded only by the file. 200 000 levels (2.4 MB) killed the process
with SIGSEGV; with `panic = "abort"` in the release profile that is a process
crash on a hostile file.

**Fix.** The walk now uses an explicit range stack (`Frame { pos, end, depth }`)
instead of recursion. The enclosing frame is advanced past a nested list
*before* the child frame is pushed, which keeps the cursor monotonic, emits the
payload chunks in file order (including across nesting boundaries) and skips
each chunk's RIFF word padding exactly once.

Two structural limits are enforced as the new `AviError::Limit { what, limit }`:

* `MAX_MOVI_DEPTH = 32` nested `LIST rec ` containers — retail cinematics use
  one; the 33rd container is rejected.
* `MAX_MOVI_CHUNKS = 1_048_576` payload chunks collected from one `movi` list.

Both are also the memory bound: the stack is capped by the depth limit and `out`
by the chunk limit, so a crafted file cannot drive allocation from its own size
fields.

**Required test matrix.**

| Case | Test | Asserts |
|------|------|---------|
| depth 0 | `enforces_the_rec_depth_limit` | chunk directly in `movi` |
| depth 1 | `nested_rec_lists_keep_file_order`, `full_avi_with_nested_rec_lists` | one container, order preserved; full-file path |
| depth max | `enforces_the_rec_depth_limit` | 32 nested containers parse |
| depth max + 1 | `enforces_the_rec_depth_limit` | `Limit { what: "movi rec nesting depth", limit: 32 }` |
| stack exhaustion | `deep_nesting_does_not_exhaust_the_stack` | 200 000 levels return `Limit`, no SIGSEGV |
| empty nested LISTs | `empty_nested_lists_are_skipped` | `LIST rec ` with a 4-byte body yields zero chunks |
| malformed declared sizes | `rejects_malformed_declared_sizes` | size past the frame end → `Truncated { what: "movi chunk" }`; `u32::MAX` size too |
| many siblings | `keeps_many_sibling_chunks_in_order` | 10 000 chunks, order and offsets exact, offsets monotonic; odd-size padding |
| fuzz | `movi_fuzz_never_panics` | 256 mutated bodies + 96 random bodies, no panic |

## AUD-009 — ASTC payload length overflow

**Mechanism.** `parse` computed `blocks_x * blocks_y * 16` in `u32` and cast the
result to `usize`. With 6x6 blocks at 98304x98304 the product is exactly 2^32,
which debug builds panic on (`astc.rs:73`) and release builds wrap to 0 — a
16-byte header then validated as a 9.6-gigapixel image with an empty payload.
`CompressedImage::validate` re-derived the length with the same unguarded
arithmetic, so the non-debug path accepted the wrapped value.

**Fix.** A single `payload_len(block, size) -> Result<usize, String>` now owns
the layout:

* all products use `u64` with `checked_mul` and `usize::try_from`;
* hard caps `MAX_TEXTURE_SIDE = 16_384` (the `GL_MAX_TEXTURE_SIZE` floor of the
  supported Vulkan/GLES 3 adapters), `MAX_TEXELS = MAX_TEXTURE_SIDE²` and
  `MAX_BLOCKS = MAX_TEXELS` (ASTC's smallest block is 1x1) are applied before
  any layout arithmetic, so u24 header dimensions can never reach the multiply;
* `parse` validates the payload against it, `to_image` re-checks the in-memory
  `AstcFile` with it (`AstcFile` fields are public, so a hand-built value must
  not reach `validate()`), and the test fixture builds from it — the three can
  no longer disagree about a length.

**Required test matrix.**

| Case | Test | Asserts |
|------|------|---------|
| dimensions near u24 max | `rejects_u24_max_dimensions` | 0xFFFFFF and 0xFFFFFE → `exceeds the 16384-texel side limit` |
| overflow boundary | `rejects_dimensions_that_overflow_the_u32_payload` | 98304² @6x6 (2^32 bytes) and 32768² @1x1 (2^34) rejected; `decode` returns `Err` instead of panicking |
| valid blocks | `parses_a_6x6_file`, `round_trips_through_write`, `layout_matches_the_block_grid` | 6x6 and 8x8 grids, partial edge blocks rounded up |
| zero side | `rejects_zero_and_non_2d_sides` | `size` 0 / block 0 / z != 1 → `unsupported image size` / `unsupported block` |
| payload ± 1 byte | `payload_length_is_exact` | `payload is 31 bytes` for a 32-byte image, `payload is 33 bytes` for +1 |
| image cap | `enforces_the_image_cap` | 16384 passes the cap and fails on the payload check; 16385 → side limit, on either axis; also via `payload_len` with 1x1 blocks |

## Verification

```
scripts/h2-cargo test -p wc3-bink --lib                 # 62 passed, rc 0
scripts/h2-cargo test -p wc3-render --lib astc::tests   # 10 passed, rc 0
scripts/h2-cargo check -p wc3-render --lib              # rc 0
scripts/h2-cargo check -p wc3-bink --all-targets        # rc 0
```

`wc3-bink` unit tests: 62/62 (15 in `avi::tests`). `wc3-render` `astc::tests`:
10/10.

Workspace-wide runs of `wc3-render --lib`/`--test astc` were not obtainable at
commit time because `crates/wc3-sim` and `crates/wc3-mpq` were being edited by
other ledger agents: `error[E0004]`/`error[E0061]` in `wc3-sim` (a `todo!()` in
the JASS event registration match) and `error[E0308]` in `wc3-mpq`
(`MPQ_COMPRESSION_ADPCM_MONO`). Those crates are outside this packet's paths and
were left to their owners; `wc3-render` dev-dependencies pull them in, which is
why the regression gate for ASTC is the `--lib astc::tests` run above plus the
`check --lib` build.

## New findings raised during this pass

* **`CompressedLevel::expected_bytes` still multiplies in `u32`** —
  `crates/wc3-render/src/texture.rs:460`
  (`natural_pitch(width) * block_rows(height)`), with `natural_pitch`
  (`:444`) and `block_rows` (`:412`) both `u32`. For a legal block size this
  needs a side above 2^28 to overflow, and the ASTC path is now capped before
  it, so no in-tree untrusted path reaches it today; `is_consistent` is public
  and could be called with arbitrary dimensions by future code. Not fixed here
  because `texture.rs` is outside this packet's owner paths — needs its own
  register entry (suggested `AUD-009-N1`, P3) and the same `payload_len`-style
  checked arithmetic.

# Lane G — formats & containers (wc3-formats, wc3-mpq, wc3-bink)

**Ledger:** 527 · **Red-team partner:** lane H (ledger 528) · **Date:** 2026-11-18 (MSK)
**Audited HEAD:** `d7af459daf5c507d46c8a63741385e5d1091af26` (`d7af459`, 2026-11-18T05:57+03:00)
**Scope:** `crates/wc3-formats/**`, `crates/wc3-mpq/**`, `crates/wc3-bink/**` (TZ §5.3, §5.4, §5.5, FMT-NEW-001…008)
**Corpus pack:** `ev-b31da0d9c5a670b1` (`harness2_corpus_pack`, 2026-11-18T13:51+03:00)

The three crates were clean in `git status` at audit start (only an unrelated untracked
`crates/wc3-mpq/tests/real_archives_509.rs` from another lane). Source `file:line` below is
the worktree at `d7af459`. All probes are one-shot binaries compiled from sources stored in
`analysis/full-audit-2026-11-18/probes/G-TOOLS/`; no test file was added or edited
(operator test freeze).

---

## 0. Executive result

**No P0 and no P1 in lane G.** 15 confirmed findings: **11× P2, 2× P3, 2× P4**; one candidate
**disproved** (`FMT-NEW-008`). The P2 set is dominated by one pattern: *a small untrusted
header field drives an allocation or an index that is only bounded in one dimension*
(MDX `UVAS`, SLK grid, AVI dimensions), plus three BLP decode-contract bugs and one
valid-MPEG-4 crash on non-macroblock-aligned height.

The format boundary is **memory-safe** in Rust terms: every observed failure is a panic/abort
or a wrong result, never an out-of-bounds read in the C sense. The old excluded backlog
(AUD-001…AUD-032) is not duplicated; no finding below aliases an old item.

| Severity | Count | IDs |
|---|---|---|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 11 | WC3-FMT-001/002/003/005/007, WC3-MDX-001, WC3-FDF-001, WC3-SLK-001, WC3-BINK-001/002/003 |
| P3 | 2 | WC3-FMT-004, WC3-FMT-006 |
| P4 | 2 | WC3-MPQ-001, WC3-MPQ-002 |
| False positive | 1 | FMT-NEW-008 (WC3-FMT-008) |

Candidate map: FMT-NEW-001→WC3-FMT-001 CONFIRMED; 002→WC3-FMT-002 CONFIRMED; 003→WC3-FMT-003
CONFIRMED; 004→WC3-FMT-004 CONFIRMED (narrowed); 005→WC3-FMT-005 CONFIRMED; 006 lane J
(not audited here); 007→WC3-FMT-006 CONFIRMED (trigger overlaps 003); 008→**FALSE_POSITIVE**.
FMT-NEW-007's independent premise ("a decoder can return a short buffer with matching
dimensions") was **not** reproduced; the observed pad is driven by the dimension mismatch.

---

## 1. Method and evidence

- Read every production source file in the three crates (37 files, 21 091 lines) plus the 41
  examples, 10 tests and 3 manifests (static sweep counts in `FILE_COVERAGE.csv`).
- Every candidate was traced from raw bytes to the failure, then reproduced with a one-shot
  probe. Probe sources: `probes/G-TOOLS/audit_g_probe_formats.rs`,
  `audit_g_bink_probe.rs`, `audit_g_mpq_probe.rs`; per-finding commands and exact outputs
  are stored next to the generated inputs under `probes/G-<ID>/`.
- Builds: `scripts/h2-cargo build --example … -p wc3-formats|wc3-bink|wc3-mpq` (FIFO slot,
  no bypass). Release probe built with `--release` for the two panic findings because the
  workspace sets `panic = "abort"` (Cargo.toml:50-54).
- Primary source used once, for BLP2 layout: `https://wowdev.wiki/BLP`, accessed
  **2026-11-18**, method = stealth-browser MCP (batch spawn→navigate→get_page_content→close).
  Quoted anchor: *“The BLP2 header is 148 bytes, followed by a 1024-byte extended data region
  (palette or JPEG header)”* and *“If there is an alpha channel, then transparency data
  follows the image data in each mipmap level”*. Completeness: full page text retrieved
  (127 008 chars in the batch result); the relevant BLP2/paletted/alpha sections were read.
- No crash of another lane's build; probes ran alone, with `ulimit -v` caps where the point
  is an allocation (never to fake a failure — the same binary runs the control case without
  the cap).

**Limitations (observed, not hidden):** the local corpus has **no BLP2 file** (7 922 BLP files
scanned: 5 847 BLP1-JPEG, 2 075 BLP1-paletted, 0 BLP2) and no movie file in the extracted
archives (`movies` example returned `movies=0` on War3.mpq/War3x.mpq/War3xlocal.mpq), so the
BLP2 probes build spec-layout files and the MPEG-4 probes use ffmpeg/libxvid encodings.
Those are stated per finding.

---

## 2. Findings

### WC3-FMT-001 — BLP1 paletted mips ignore `sizes[level]`; missing alpha becomes opaque

ID: WC3-FMT-001
Title: BLP1 paletted mips ignore `sizes[level]`; missing alpha becomes opaque
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 121-179 (paletted branch); `count` read 131-135; alpha fallback 136-140; per-pixel alpha fallback 144-163
Affected targets/features: BLP1 `content_type == 1` textures (2 075 files in the local corpus)
Preconditions: a BLP1 paletted file whose `sizes[level]` does not cover `count` index bytes plus the alpha plane, or whose mip offset points at any other region of the file.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe blp1-overlap probes/G-FMT-001/overlap.blp` and `… blp1-alpha-missing probes/G-FMT-001/alpha-missing.blp` (sources in `probes/G-TOOLS/audit_g_probe_formats.rs`).
Observed behavior:
```
file=…/G-FMT-001/overlap.blp len=1180 offset=156 sizes[0]=1 count=16
decode=OK dims=4x4 rgba=64 px0=[128, 64, 0, 4] px4=[128, 64, 1, 5] alpha_all_255=false
file=…/G-FMT-001/alpha-missing.blp len=1196 data_at=1180 alpha_at=1196
decode=OK rgba=64 alpha_values=[255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]
```
Expected behavior: a mip whose declared `sizes[level]` cannot hold `count + ceil(count*alpha_bits/8)` is corrupt (`FormatError`); a missing alpha plane is not silently opaque; mip ranges may not overlap the header/palette/other mips.
Root cause: the loop checks only `offsets[level] != 0 && sizes[level] != 0` (128-129), then reads `bytes[start..start+count]` (133-135) and derives `alpha_start = start + count` (136) without consulting `sizes[level]`. `alpha.get(...).unwrap_or(&[])` (138-140) plus `unwrap_or(255)` (145) converts a short alpha plane into fully opaque pixels.
Impact: a malformed or overlapping BLP1 returns a visually wrong texture (palette bytes reinterpreted as indices, another mip's bytes as alpha, transparent art forced opaque) instead of `FormatError`. No OOB read: the slices stay inside `bytes`, as the candidate correctly scoped it.
Why not covered by excluded backlog: the excluded BLP items cover decompression limits and general parsing; this is the paletted branch's missing per-mip size contract with a live reproducer.
Fix specification: checked `count*alpha_bits` arithmetic; require `start + count + alpha_len <= start + sizes[level]` and the whole range inside the file; reject overlap of any two mip ranges and of `[0, 156+1024)`; treat missing alpha bytes as corruption, not as opaque; validate `alpha_bits ∈ {0,1,4,8}`.
Regression test(s): BLOCKED: operator test freeze. Intended: table-driven malformed BLP1 paletted fixtures (`size=count-1`, missing alpha, offset into palette, overlapping mips) asserting `Err`, plus a valid 1/4/8-bit alpha fixture asserting exact pixels.
Verification commands and exact result: above (two probes, rc=0 both; log files `probes/G-FMT-001/overlap.log`, `alpha-missing.log`).
Related findings: WC3-FMT-004 (systemic validator), WC3-FMT-007 (BLP2 alpha layout).

### WC3-FMT-002 — BLP2 paletted short mip is zero-filled instead of rejected

ID: WC3-FMT-002
Title: BLP2 paletted short mip is zero-filled instead of rejected
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 294-338; `take(count)` 318; zero-fill 328-330
Affected targets/features: BLP2 `compression == 1` textures (no local fixture; spec-layout probe)
Preconditions: a BLP2 paletted mip whose declared `sizes[i]` is smaller than `width*height`.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe blp2-short probes/G-FMT-002/short-spec.blp` (64×64, spec layout: palette at 148, mip at 1172, alpha plane after 4 095 index bytes).
Observed behavior:
```
file=…/short-spec.blp len=9363 width=64 height=64 sizes[0]=4095 count=4096 (spec layout mip@1172, alpha plane after indices)
decode=OK rgba=16384 px0=[10, 20, 30, 0] px15=[10, 20, 30, 0] px_last=[0, 0, 0, 0] alpha_eq_128=false zero_pixels=4096
```
Expected behavior: `sizes[0] < count` is a corrupt mip and must return `FormatError`; `sizes[0] > count` is at most a documented-padding case.
Root cause: after `for &index in data.iter().take(count)` (318-327) the code pads `rgba.resize(count*4, 0)` (328-330) whenever fewer than `count` indices were present, producing black/transparent pixels.
Impact: truncated/corrupt BLP2 mips render as partially black textures; the failure is indistinguishable from real content. Every pixel's alpha is also wrong because of WC3-FMT-007 (the `alpha_eq_128=false` line is that second defect, not this one).
Why not covered by excluded backlog: new BLP2 branch behaviour; the old BLP items did not cover paletted mip payload completeness.
Fix specification: require `sizes[i] >= count` for the index plane, take exactly `count` bytes, reject short mips; keep the alpha plane layout per spec (see WC3-FMT-007) and validate its length against `alpha_bits`.
Regression test(s): BLOCKED: operator test freeze. Intended: `size = count-1` → `Err`, `size = count` → exact pixels, `size = count + alpha_len` → exact pixels, valid legacy fixture if one exists.
Verification commands and exact result: above; log `probes/G-FMT-002/short-spec.log`.
Related findings: WC3-FMT-007 (alpha/size layout), WC3-FMT-004.

### WC3-FMT-003 — BLP JPEG decoder does not compare JPEG dimensions with the BLP mip size

ID: WC3-FMT-003
Title: BLP JPEG decoder does not compare JPEG dimensions with the BLP mip size
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 368-428; `decode()` 374-377; info checks 378-383; pad/truncate 412-417
Affected targets/features: BLP1 `content_type == 0` (JPEG) textures; local fixtures `analysis/Village_Dirt.blp`, `analysis/Village_GrassShort.blp` (both 512×256 / 256×256, `alpha_bits=8`)
Preconditions: an embedded JPEG whose frame size differs from the BLP header mip size on one or both axes.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe jpeg-dims analysis/Village_GrassShort.blp 512 512` and `… 128 128` (patches only the BLP header width/height; the JPEG stream is untouched).
Observed behavior:
```
header_dims=256x256 patched_to=512x512
decode=OK dims=512x512 rgba=1048576 expected=1048576 opaque_px=36572 alpha_nonzero_px=43694 px_after_orig=[0, 0, 0, 0]
header_dims=256x256 patched_to=128x128
decode=OK dims=128x128 rgba=65536 expected=65536 opaque_px=7666 alpha_nonzero_px=9319 px_after_orig=[60, 142, 52, 255]
```
Expected behavior: JPEG frame dimensions must equal the mip dimensions declared by the BLP; a mismatch is `FormatError` (or a documented retail exception, tested).
Root cause: `info.width/height` are only range-checked (381-383) and never compared with the `width`/`height` parameters; the output is then unconditionally resized/truncated to `width*height*4` (412-417). A 1×1 JPEG in a 512×512 mip would therefore yield one real pixel and 262 143 zero pixels.
Impact: internally contradictory BLP files decode "successfully" with a cropped or mostly-black texture; the corruption is hidden from the loader.
Why not covered by excluded backlog: the old list had no BLP JPEG dimension contract; this is the direct candidate with a live reproducer on a shipped-texture fixture.
Fix specification: require `info.width == width && info.height == height` before building the RGBA buffer; also require the decoded buffer length to match exactly (WC3-FMT-006); decode only after dimension validation (WC3-FMT-005).
Regression test(s): BLOCKED: operator test freeze. Intended: 1×1 JPEG in 512×512 BLP, one-axis mismatch, larger-than-expected JPEG, and two valid BLP1 JPEG fixtures (the local Village files) asserting exact dimensions.
Verification commands and exact result: above; logs `probes/G-FMT-003/jpeg-512.log`, `jpeg-128.log`.
Related findings: WC3-FMT-005, WC3-FMT-006.

### WC3-FMT-004 — no single strict offset/size validator across BLP branches

ID: WC3-FMT-004
Title: no single strict offset/size validator across BLP branches
Priority: **P3**
Status: **CONFIRMED**
Confidence: HIGH (source + overlap probe)
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 81-100 (BLP1 JPEG), 131-140 (BLP1 paletted), 221-227 (BLP2 palette/alpha), 236-240 (BLP2 DXT), 268-285 (BLP2 BGRA), 311-315 (BLP2 paletted)
Affected targets/features: all BLP1/BLP2 compression branches
Preconditions: any mip offset/size pair that points into the header/palette, overlaps another mip, or (BGRA) is not the exact single-mip payload.
Trigger / minimal reproduction: `blp1-overlap` (WC3-FMT-001 log) is the live overlap case: `offset=156` (inside the palette) with `sizes[0]=1` decodes without error. BGRA branch: `blp2_bgra` in `blp.rs:267-285` reads `width*height*4` from `offsets[0]` and never consults `sizes[0]` or `mips`.
Observed behavior: overlapping/header-region ranges are accepted by the paletted branch (probe above); the BGRA branch silently reads across `sizes[0]` and ignores every mip after 0.
Expected behavior: one validator used by every branch: `start >= header_end(branch)`, `start + size <= file_len` with checked arithmetic, no two mip ranges overlap, `sizes[i]` consistent with the branch's exact payload size (or documented padding), mip count honored.
Root cause: each branch implements its own partial check (`bytes.get(start..start+size)` for JPEG/DXT/paletted BLP2; `bytes.get(start..start+count)` for BLP1 paletted; no `sizes` for BGRA). There is no shared range type.
Impact: malformed archives are accepted and produce wrong textures (P3: no memory-safety impact in Rust; the loader cannot tell corruption from content).
Why not covered by excluded backlog: excluded BLP items are about decompression caps; this is branch consistency and range ownership.
Fix specification: introduce `MipRange::validate(bytes, offsets, sizes, header_len, required_payload)` and route every compression branch through it; forbid overlaps and header-region pointers; make BGRA enforce `sizes[0] == width*height*4` and mip-count 1.
Regression test(s): BLOCKED: operator test freeze. Intended: property test over (offset,size) pairs around the header/palette/EOF boundaries for each branch.
Verification commands and exact result: WC3-FMT-001 `overlap.log`; source trace above.
Related findings: WC3-FMT-001/002/007.

### WC3-FMT-005 — embedded JPEG is fully decoded before the BLP-level allocation limit applies

ID: WC3-FMT-005
Title: embedded JPEG is fully decoded before the BLP-level allocation limit applies
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`; external `jpeg-decoder 0.3.2` (`~/.cargo/registry/src/*/jpeg-decoder-0.3.2/src/decoder.rs`)
Exact line range(s): `blp.rs:374-383`; crate `decoder.rs:129` (`decoding_buffer_size_limit` field), `:152` (default `usize::MAX`), `:163-165` (`set_max_decoding_buffer_size`), `:293-300` (`decode`), `:400-410` (coefficient allocation)
Affected targets/features: BLP1 JPEG (`content_type == 0`) and any BLP with an embedded JPEG
Preconditions: a BLP whose header passes `check_pixels` (≤16 M px) but whose embedded JPEG announces up to 8192×8192 (67 M px).
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe jpeg-big probes/G-FMT-005/wrapped.blp probes/G-FMT-005/big5000.jpg` (147 238-byte 5000×5000 JPEG inside a BLP declaring 16×16).
Observed behavior:
```
file=…/wrapped.blp blob=147399 declared_blp=16x16 jpeg_blob=147238 elapsed_ms=7062 VmHWM_before_kb=4596 after_kb=210196 delta_kb=205600
decode=OK dims=16x16 rgba=1024 (BLP 16x16 expected 1024)
```
Expected behavior: the decoder must not allocate more than the BLP contract allows (16 M px = 64 MiB RGBA) and must reject an embedded JPEG whose dimensions exceed the declared mip, before decoding.
Root cause: `Decoder::decode()` (374-377) runs before `decoder.info()` (378-380); jpeg-decoder's default `decoding_buffer_size_limit` is `usize::MAX` and BLP never calls `set_max_decoding_buffer_size`. The 13-bit JPEG dimensions (≤8191) are checked only after the allocation.
Impact: a 147 KB file causes a 205 MB peak and ~7 s of CPU on the device (measured), i.e. a memory/CPU spike and a visible freeze from a single hostile or corrupt texture; on a memory-constrained device this can OOM.
Why not covered by excluded backlog: old item is MPQ decompression limits; this is the JPEG-in-BLP allocation order, with a measured peak.
Fix specification: call `decoder.read_info()` first, compare dimensions with the mip and with `MAX_PIXELS`, then `set_max_decoding_buffer_size(width*height*4)` before `decode()`; additionally cap `info.width*info.height` at `MAX_PIXELS`.
Regression test(s): BLOCKED: operator test freeze. Intended: a 16×16 BLP with a 5000×5000 JPEG must return `Err` with peak RSS below ~64 MiB.
Verification commands and exact result: above; log `probes/G-FMT-005/jpeg-big.log`.
Related findings: WC3-FMT-003, WC3-FMT-006.

### WC3-FMT-006 — decoded JPEG shorter than the mip is silently zero-padded

ID: WC3-FMT-006
Title: decoded JPEG shorter than the mip is silently zero-padded
Priority: **P3**
Status: **CONFIRMED** (code path observed; independent trigger not found — see below)
Confidence: MEDIUM
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 384-417; `chunks_exact` 387, 405; `resize(expected, 0)` 412-414; `truncate` 415-417
Affected targets/features: JPEG pixel formats RGB24 / L8 / CMYK32
Preconditions: decoded `rgba.len() < width*height*4` with matching declared dimensions (the observed trigger is the dimension mismatch of WC3-FMT-003; an independent decoder-short-output path was not reproduced).
Trigger / minimal reproduction: `jpeg-dims analysis/Village_GrassShort.blp 512 512` (WC3-FMT-003 log) exercises the same `resize(expected, 0)`.
Observed behavior: the output buffer is the 512×512 size the header asks for; the real JPEG supplies 256×256 and the remaining **196 608 pixels (75 %) are zero-filled** (the `px_after_orig` sample is `[0, 0, 0, 0]`).
Expected behavior: if the specification does not define padding, any length mismatch is `FormatError`; if a retail exception exists it is documented and validated.
Root cause: the length is normalized with `resize` instead of asserted.
Impact: silent zero pixels can mask a truncated decode (P3 because the demonstrated trigger is already caught by the dimension check once WC3-FMT-003 is fixed).
Why not covered by excluded backlog: candidate FMT-NEW-007; not in the old list.
Fix specification: `if rgba.len() != expected { return Err(FormatError::Corrupt("JPEG pixel buffer length")); }` and drop the resize/truncate.
Regression test(s): BLOCKED: operator test freeze. Intended: force a short decoder buffer (mock/truncated progressive scan) and require `Err`; assert `RGB24/L8/CMYK32` all exact.
Verification commands and exact result: WC3-FMT-003 logs; source trace above.
Related findings: WC3-FMT-003 (shared fix site).

### WC3-FMT-007 — BLP2 paletted alpha is read from a non-existent 1024-byte table; per-mip alpha is ignored

ID: WC3-FMT-007
Title: BLP2 paletted alpha is read from a non-existent 1024-byte table; per-mip alpha is ignored
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH (primary source + two probes)
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 219-227 (palette at 148..1172, “alpha” at 1172..2196), 294-327 (`alpha.get(index*4)`)
Affected targets/features: BLP2 `compression == 1` (palettized) textures — Reforged-era assets; zero fixtures in the local corpus
Preconditions: any valid BLP2 paletted texture with an alpha channel.
Trigger / minimal reproduction:
- `blp2-valid probes/G-FMT-002/valid64.blp 64 64` — valid 64×64, palette at 148, indices at 1172, alpha plane (all 128) after the indices.
- `blp2-small probes/G-FMT-002/small-valid.blp` — valid 16×16 (256 indices + 256 alpha bytes).
Observed behavior:
```
file=…/valid64.blp len=9364 width=64 height=64 valid spec layout
decode=OK rgba=16384 px0=[10, 20, 30, 0] all_alpha_128=false alpha_values=[0, 0, 0, 0]
file=…/small-valid.blp len=1684 width=16 height=16 valid spec layout (256 idx + 256 alpha at 1172)
decode=ERR TooShort
```
Expected behavior: alpha for pixel i comes from the per-mip alpha plane that follows the index plane; a valid 16×16 file decodes; a valid 64×64 file yields alpha 128.
Root cause: the decoder requires a 1024-byte “alpha table” at offset 1172 (`bytes.get(alpha_start..alpha_start+1024)`, 222-225) and samples it every 4 bytes (`alpha.get(index*4)`, 320). Per `wowdev.wiki/BLP` (accessed 2026-11-18), the BLP2 header is 148 B + 1024 B palette (ends at 1172) and *“transparency data follows the image data in each mipmap level”*; there is no global alpha table.
Impact: (a) every valid BLP2 paletted texture smaller than 2196 bytes is rejected with `TooShort` (e.g. 16×16, 8×8, 64×8); (b) every larger one gets alpha values taken from the first 1 KB of index data (observed 0 instead of 128) — wrong transparency across the texture. This is a format-contract bug, not an edge case.
Why not covered by excluded backlog: not in the old 32; FMT-NEW-002 only covered short-payload padding.
Fix specification: remove the 1172..2196 alpha-table read; for each mip read `count` indices then `ceil(count*alpha_bits/8)` alpha bytes from `offsets[i]`, with the range validated by WC3-FMT-004's validator; keep `alpha_bits` handling (1/4/8) identical to the BLP1 branch.
Regression test(s): BLOCKED: operator test freeze. Intended: valid spec-layout BLP2 paletted fixtures (16×16, 64×64, 1/4/8-bit alpha) asserting exact alpha, plus the short-mip case from WC3-FMT-002.
Verification commands and exact result: above; logs `probes/G-FMT-002/valid64.log`, `small-valid.log`; source anchor quoted in §1.
Related findings: WC3-FMT-002, WC3-FMT-004.

### WC3-MDX-001 — MDX geoset `UVAS` count allocates up to 103 GB before any read

ID: WC3-MDX-001
Title: MDX geoset `UVAS` count allocates up to 103 GB before any read
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/mdx.rs`
Exact line range(s): 1508-1509 (`let uv_set_count = r.u32()? as usize; let mut uv_sets = Vec::with_capacity(uv_set_count);`)
Affected targets/features: any `.mdx` model (map-imported models included)
Preconditions: a GEOS geoset that reaches `UVAS` with a forged count.
Trigger / minimal reproduction: `( ulimit -v 6000000; target/debug/examples/audit_g_probe mdx-uvas probes/G-MDX-UVAS/uvas.mdx )` — 144-byte MDX.
Observed behavior:
```
file=…/uvas.mdx len=144 geoset=120 uvas_count=4294967280
memory allocation of 103079214720 bytes failed
… Aborted
rc=134
```
Expected behavior: `uv_set_count` is validated against the remaining chunk bytes before allocation, as every other count in this parser is (`check_count`).
Root cause: every geoset count uses `r.check_count(count, stride)` except `uv_set_count`; `Vec::with_capacity(4 294 967 280)` of `Vec<Vec2>` requests 103 079 214 720 bytes immediately.
Impact: a tiny malicious/invalid model aborts the process (Rust alloc error → `abort`, `panic = "abort"`), i.e. the game dies when loading the model rather than reporting a corrupt asset.
Why not covered by excluded backlog: MDX parsing was not in the old 32.
Fix specification: `r.check_count(uv_set_count, 8)` (each UVB needs at least tag+count) before `with_capacity`, and cap the set count at a plausible ceiling.
Regression test(s): BLOCKED: operator test freeze. Intended: the 144-byte probe must return `Err(Corrupt)` with no allocation.
Verification commands and exact result: above; log `probes/G-MDX-UVAS/uvas.log`.
Related findings: WC3-SLK-001 (same class).

### WC3-FDF-001 — FDF block nesting has no depth limit: SIGSEGV at depth 5000

ID: WC3-FDF-001
Title: FDF block nesting has no depth limit: SIGSEGV at depth 5000
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/fdf.rs`
Exact line range(s): 1376-1431 (`parse_body` recurses per `{`); 416-423 (`max_depth` also recursive); 1182-1231 (`parse_document`)
Affected targets/features: FDF UI files loaded from archives/maps (`UI\FrameDef\…`)
Preconditions: a text FDF with deeply nested `Frame … {` blocks.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe fdf-depth probes/G-FDF-DEPTH/nested5000.fdf 5000` (128 890-byte file).
Observed behavior:
```
file=…/nested5000.fdf bytes=128890 depth=5000
Segmentation fault
rc=139
```
Control: depths 500, 1 000, 2 000, 3 000 parse (`parse=Ok(1) max_depth=Some(n)`); 5 000 crashes. Deterministic across three runs (5 000 / 10 000 / 20 000).
Expected behavior: a depth ceiling (e.g. 64; retail FDF is < 30) and a typed `Corrupt` error on overflow.
Root cause: mutual recursion of `parse_body` per nested block, no depth parameter; `FdfNode`'s recursive `max_depth`/`walk` have no guard either.
Impact: a crafted UI file kills the process with SIGSEGV (stack exhaustion) during map/UI load; no recoverable error.
Why not covered by excluded backlog: FDF parser state was a §5.3 audit area but not in the old 32.
Fix specification: add `depth: usize` to `parse_body`, reject `depth > MAX_FDF_DEPTH` (64) with `FormatError::Corrupt`; apply the same ceiling in `max_depth`/`walk` or make them iterative.
Regression test(s): BLOCKED: operator test freeze. Intended: depth 65 → `Err`, depth 32 parses; plus the 5 000 probe must not crash.
Verification commands and exact result: above; log `probes/G-FDF-DEPTH/fdf5000.log`.
Related findings: lane H's WC3-JASS-005 (same class, different parser).

### WC3-SLK-001 — SLK dense grid is bounded per axis only: 31 bytes → up to ~1.9 GB

ID: WC3-SLK-001
Title: SLK dense grid is bounded per axis only: 31 bytes → up to ~1.9 GB
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/slk.rs`; same pattern in `crates/wc3-formats/src/gamedata.rs`
Exact line range(s): `slk.rs:55-66` (per-axis check `x > MAX_COLUMNS || y > MAX_ROWS`), `74-96` (width×height grid materialised); `gamedata.rs:544-561` (same per-axis check), `594-626` (Values grid)
Affected targets/features: `*.slk` tables (game data and map/MPQ overrides)
Preconditions: one cell record with `X=1024;Y=65536` (both pass the `>` checks) in a ~31-byte file.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe slk-grid probes/G-SLK-GRID/scaled2.slk 1024 8192` (scaled), and the same binary at `1024 65536` (worst case; arithmetic below).
Observed behavior:
```
file=…/scaled.slk bytes=31 x=1024 y=4096
parse=OK rows=4095 columns=1024 cells_first_row=1024 elapsed_ms=688 VmHWM_before_kb=3852 after_kb=122532 delta_kb=118680
file=…/scaled2.slk bytes=31 x=1024 y=8192
parse=OK rows=8191 columns=1024 cells_first_row=1024 elapsed_ms=1643 VmHWM_before_kb=3732 after_kb=239736 delta_kb=236004
```
Worst case: `rows = 65 535`, `columns = 1 024`, each cell an empty `String` (24 B on the 64-bit target) → `65 535 × 1 024 × 24 ≈ 1.61 GB`, plus 65 535 row `Vec` headers; extrapolated from the measured 236 MB at 8 192 rows (8×). On a 32-bit build the allocation fails/aborts.
Expected behavior: a total-cell budget (`MAX_ROWS * MAX_COLUMNS` or an absolute cell cap ~4 M) is enforced; a violated budget is `FormatError`/diag skip, never a dense allocation.
Root cause: `MAX_ROWS`/`MAX_COLUMNS` are independent ceilings; the product `(max_y) * (max_x)` is never bounded, and the grid pads every row to the widest column with `String::new()`.
Impact: a 31-byte SLK (a map can override game tables) causes a multi-second freeze and ~0.1–1.9 GB of allocations; on 32-bit or a loaded device this is an OOM/abort during data load.
Why not covered by excluded backlog: SLK table parsing was in §5.3 but not the old 32.
Fix specification: enforce `width.checked_mul(height) <= MAX_CELLS` before building the grid (and in `Sheet::read`), or keep the sparse map and materialise lazily; report via `diag::note("slk_grid_too_large")`.
Regression test(s): BLOCKED: operator test freeze. Intended: `X=1024;Y=65536` → `Err`/skip with total cells ≤ cap; valid retail `UnitData.slk` unchanged.
Verification commands and exact result: above; logs `probes/G-SLK-GRID/scaled.log`, `scaled2.log`.
Related findings: WC3-MDX-001 (count-before-allocation class); `gamedata::Sheet` shares the root cause.

### WC3-BINK-001 — AVI `strf` dimensions size three frame buffers before any validation (up to 51.5 GB)

ID: WC3-BINK-001
Title: AVI `strf` dimensions size three frame buffers before any validation (up to 51.5 GB)
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-bink/src/lib.rs`, `crates/wc3-bink/src/avi.rs`
Exact line range(s): `avi.rs:420-421` (u16 width/height from `strf`), `lib.rs:117-131` (`rgba_len = w*h*4`; three `vec![0u8; rgba_len]`)
Affected targets/features: any AVI/`.mpq`/`.blz` movie opened through `BinkFile`
Preconditions: a minimal AVI whose `vids` `strf.biWidth/biHeight` is large (u16 max 65 535).
Trigger / minimal reproduction: `( ulimit -v 6000000; target/debug/examples/audit_g_bink_probe avi-dims probes/G-BINK-DIMS/huge.avi 65535 65535 )` — 338-byte AVI; control `tiny.avi 16 16`.
Observed behavior:
```
file=…/huge.avi bytes=338 declared=65535x65535 per_frame=17179344900 three_slots=51538034700
memory allocation of 17179344900 bytes failed
… Aborted
rc=134
```
Control:
```
file=…/tiny.avi bytes=338 declared=16x16 per_frame=1024 three_slots=3072
from_bytes=Ok((1, (16, 16))) … delta_kb=152
```
Expected behavior: dimensions are validated before allocation (e.g. `w*h <= MAX_MOVIE_PIXELS`, and 16-alignment/padding rules for MPEG-4), or the frame ring is allocated lazily from the decoded VOL size.
Root cause: only `w == 0 || h == 0` is checked (118-120); `rgba_len` and `FRAME_RING` (3) multiply the declared dimensions without a cap. `w*h*4` is computed in `usize`; at 65 535² it is 17.2 GB per frame.
Impact: a 338-byte file aborts the process (or reserves ~51 GB virtual) when the movie is opened; the game cannot recover.
Why not covered by excluded backlog: AVI demuxing was in §5.5, not the old 32.
Fix specification: cap movie pixels (e.g. 4096×4096 or the largest shipped cinematic 800×368 with margin), reject implausible dimensions with `BinkError::Corrupt`; allocate the ring from the first decoded VOL and verify it matches the AVI header (see WC3-BINK-002).
Regression test(s): BLOCKED: operator test freeze. Intended: `65535×65535` → `Err` with no allocation; 800×368 control opens and decodes.
Verification commands and exact result: above; logs `probes/G-BINK-DIMS/tiny.log`, `huge.log`.
Related findings: WC3-BINK-002.

### WC3-BINK-002 — MPEG-4 VOL dimensions larger than the AVI header panic in `to_rgba`

ID: WC3-BINK-002
Title: MPEG-4 VOL dimensions larger than the AVI header panic in `to_rgba`
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-bink/src/lib.rs`, `crates/wc3-bink/src/mpeg4/mod.rs`, `crates/wc3-bink/src/mpeg4/frame.rs`
Exact line range(s): `lib.rs:117-131` (ring sized from AVI), `mpeg4/mod.rs:184-187`/`254-261` (`ensure_size` from VOL), `frame.rs:319-337` (`to_rgba`; `debug_assert_eq!` 320; writes 331-334)
Affected targets/features: movies whose container header and MPEG-4 VOL disagree
Preconditions: AVI `strf` dimensions smaller than the VOL in the first video chunk.
Trigger / minimal reproduction: `target/release/examples/audit_g_bink_probe wrap-m4v probes/G-BINK-VOL/clip32.m4v probes/G-BINK-VOL/avi16-mismatch.avi 16 16` (valid 32×32 MPEG-4 stream in a 16×16 AVI); control `… 32 32`.
Observed behavior (release, `panic=abort`):
```
open=OK size=(16, 16) frames=1

thread 'main' panicked at crates/wc3-bink/src/mpeg4/frame.rs:331:17:
index out of bounds: the len is 1024 but the index is 1024
… Aborted
rc=134
```
Debug build hits the `debug_assert_eq!(out.len(), width*height*4)` first (`frame.rs:320`, left 1024 / right 4096). Control 32×32 decodes (`frame0=OK 32x32 rgba=4096`).
Expected behavior: `to_rgba` rejects a buffer shorter than the picture (typed error) or the ring is resized when VOL dims change; either way no panic.
Root cause: `to_rgba` assumes the caller's buffer matches the picture and only asserts in debug; the ring is allocated from the untrusted AVI header while the picture is allocated from the untrusted VOL, and nothing reconciles them.
Impact: a crafted/truncated movie aborts the process at the first decoded frame; with `panic=abort` this is a hard kill, no catch.
Why not covered by excluded backlog: not in the old 32.
Fix specification: make `to_rgba` return `Result<(), BinkError>` and check `out.len() >= width*height*4`; in `BinkFile::decode_frame`, compare VOL dims with `self.size()` and either resize the ring once or return `Corrupt("AVI/VOL dimension mismatch")`.
Regression test(s): BLOCKED: operator test freeze. Intended: 16×16 AVI + 32×32 VOL → `Err`, 32×32 control → ok.
Verification commands and exact result: above; logs `probes/G-BINK-VOL/mismatch16-release.log`, `control32.log`.
Related findings: WC3-BINK-001, WC3-BINK-003.

### WC3-BINK-003 — valid MPEG-4 height not a multiple of 16 panics in `idct_put` (planes are not macroblock-padded)

ID: WC3-BINK-003
Title: valid MPEG-4 height not a multiple of 16 panics in `idct_put` (planes are not macroblock-padded)
Priority: **P2**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-bink/src/mpeg4/frame.rs`, `crates/wc3-bink/src/mpeg4/mod.rs`, `crates/wc3-bink/src/idct.rs`
Exact line range(s): `frame.rs:71-102` (`y: vec![0; stride_y * (height + 1)]`, chroma `((height+1)/2 + 1)`), `mod.rs:554-570` (`store_intra_mb` writes full 8×8 blocks at `mb_y*16`), `idct.rs:173` (write)
Affected targets/features: MPEG-4 I/P frames whose VOL height is not a multiple of 16 (valid, e.g. 32×18)
Preconditions: a decodable MPEG-4 stream (libxvid-encoded, no resync markers) with height 18; AVI header matches 32×18.
Trigger / minimal reproduction: `target/release/examples/audit_g_bink_probe wrap-m4v probes/G-BINK-VOL/xvid32x18.m4v probes/G-BINK-VOL/avi-xvid32x18.avi 32 18` (368-byte stream; ffmpeg `-c:v libxvid`).
Observed behavior:
```
open=OK size=(32, 18) frames=1

thread 'main' panicked at crates/wc3-bink/src/idct.rs:173:13:
index out of bounds: the len is 96 but the index is 96
… Aborted
rc=134
```
Derivation of `len 96`: luma plane is `32 × (18+1) = 608` bytes; the last macroblock row starts at `dy = 16`, leaving `608 − 16×32 = 96` bytes, while `idct_put` writes 8 rows of 32. Control: libxvid 64×48 decodes fully (`concealed=0 last_err=None`).
Expected behavior: plane storage is padded to `ceil(height/16)*16` rows (plus edge row), or block writes are clipped to the picture; a valid non-aligned frame decodes or returns `Unsupported`.
Root cause: the picture allocation uses `height + 1` rows while the decoder iterates `mb_h = ceil(height/16)` macroblock rows and writes full 16×16/8×8 blocks.
Impact: a valid MPEG-4 movie with a non-macroblock-aligned height aborts the process at frame 0 (parse of valid input, not a malformed-file nicety); shipped cinematics are 800×368 (aligned) so retail is unaffected, custom/map movies are not.
Why not covered by excluded backlog: §5.5 asked for pathological dimensions; this is a concrete valid-dimension crash.
Fix specification: allocate `stride_y * (mb_h*16 + 1)` and `stride_c * (mb_h*8 + 1)` (or clip `idct_put`/`store_*` to `width/height`); add an explicit `Unsupported` for heights the writer cannot represent if clipping is not wanted.
Regression test(s): BLOCKED: operator test freeze. Intended: 32×18 and 64×48 libxvid streams; both decode or return `Err`, never panic.
Verification commands and exact result: above; logs `probes/G-BINK-VOL/odd-dims-xvid-release.log`, `odd-dims-xvid.log`.
Related findings: WC3-BINK-002.

### WC3-MPQ-001 — `MpqIndex::with_extension` slices a `String` at a non-char boundary (panic)

ID: WC3-MPQ-001
Title: `MpqIndex::with_extension` slices a `String` at a non-char boundary (panic)
Priority: **P4**
Status: **CONFIRMED**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-mpq/src/index.rs`
Exact line range(s): 304-311 (slice at 308)
Affected targets/features: public API; **no production caller today** (`rg with_extension` finds only this file and a test)
Preconditions: an index entry name whose byte length minus the suffix length falls inside a multi-byte UTF-8 character, and a caller invoking `with_extension`.
Trigger / minimal reproduction: `target/debug/examples/audit_g_mpq_probe index-ext 'ab€'`.
Observed behavior:
```
name="ab€" bytes=5 suffix=.x bytes=2

thread 'main' panicked at crates/wc3-mpq/src/index.rs:308:30:
start byte index 3 is not a char boundary; it is inside '€' (bytes 2..5 of string)
rc=101
```
Control `'abcé'` returns `[]` (boundary happens to be valid).
Expected behavior: `with_extension` compares the file extension case-insensitively without panicking on any valid UTF-8 name.
Root cause: `entry.name[entry.name.len() - suffix.len()..]` slices by bytes; `len() >= suffix.len()` is necessary but not sufficient for a char boundary.
Impact: latent public-API panic on non-ASCII asset names (MPQ listfiles are `from_utf8_lossy`, so non-ASCII is possible). No current product path, hence P4.
Why not covered by excluded backlog: not in the old 32.
Fix specification: use `entry.name.len().checked_sub(suffix.len())` then `get(..)`/`is_char_boundary`, or `entry.name.rsplit('.').next()` with `eq_ignore_ascii_case`; return no match on a non-boundary.
Regression test(s): BLOCKED: operator test freeze. Intended: `ab€`/`abcé`/ASCII names → no panic and correct matches.
Verification commands and exact result: above; logs `probes/G-MPQ-INDEX/index-ext.log`, `index-ext-panic.log`.
Related findings: none.

### WC3-MPQ-002 — movie discovery decodes every unnamed block of 512 KiB…256 MiB in full

ID: WC3-MPQ-002
Title: movie discovery decodes every unnamed block of 512 KiB…256 MiB in full
Priority: **P4**
Status: **CONFIRMED** (source path; no crafted-archive probe)
Confidence: MEDIUM
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/media.rs`
Exact line range(s): 33-39 (`SCAN_MIN_BYTES`/`SCAN_MAX_BYTES`), 352-388 (per-block `archive.read_block(block)`)
Affected targets/features: `discover_movies` / `extract_all` on archives without a listfile
Preconditions: an archive with many large unnamed blocks whose 4-byte prefix is not a movie magic.
Trigger / minimal reproduction: no probe; static path: `read_block` fully decodes the block (up to 512 MiB) only to inspect `is_movie_container(&bytes)` (373).
Observed behavior (source): N unnamed blocks cost N full decodes; a 100-block archive with 256 MiB declared blocks decodes 25 GiB sequentially, one 256 MiB buffer at a time.
Expected behavior: read a 4-byte prefix via a streaming reader (`archive.stream`/block reader), then decide.
Root cause: the scan uses the whole-file API for a 4-byte decision.
Impact: slow boot/scan on hostile or unusual archives (time, not memory growth); P4 because it is an efficiency/robustness gap, not a correctness defect, and no product path is proven to scan untrusted archives at boot.
Why not covered by excluded backlog: not in the old 32.
Fix specification: add `MpqArchive::read_block_prefix(block, n)` (or use `MpqFileReader` on the block) and test the magic on the prefix; keep `SCAN_MAX_BYTES` as the upper bound for the prefix read.
Regression test(s): BLOCKED: operator test freeze. Intended: N unnamed 1 MiB blocks → N × 4-byte reads.
Verification commands and exact result: `target/debug/examples/movies /data/…/War3.mpq` → `movies=0` (no movie assets in the extracted archives; scan completed).
Related findings: none.

### WC3-FMT-008 — FMT-NEW-008 (DXT block arithmetic) is a **FALSE_POSITIVE**

ID: WC3-FMT-008
Title: FMT-NEW-008 (DXT block arithmetic) is a **FALSE_POSITIVE**
Priority: **—**
Status: **FALSE_POSITIVE**
Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-formats/src/blp.rs`
Exact line range(s): 449-460 (`bw`, `bh`, `needed`, `out`)
Preconditions: none reachable.
Trigger / minimal reproduction: `target/debug/examples/audit_g_probe dxt-bounds`.
Observed behavior:
```
reachable_max_pixels=16777216 (MAX_PIXELS) -> 4096x4096: bw=1024 bh=1024 needed_dxt5=16777216 out_rgba=67108864 u32max=4294967295 usize_bits=64
4096x4096 short DXT: decode=ERR Corrupt("DXT payload shorter than blocks")
8192x8192 short DXT: decode=ERR Corrupt("BLP dimensions out of range")
20000x8192 DXT: decode=ERR Corrupt("invalid BLP2 dimensions")
```
Expected behavior: n/a.
Root cause / why it is not a defect: `decode_blp2` rejects `width/height > 8192` (209-211) and `check_pixels` rejects `width*height > 16 777 216` (51-57) before `decode_dxt`; the reachable maxima are `bw*bh*16 = width*height <= 16 777 216` and `width*height*4 = 67 108 864`, both far below `u32::MAX` and `isize::MAX` on 32-bit. No unchecked arithmetic can overflow on any supported target.
Impact: none.
Why not covered by excluded backlog: disproof of the candidate; no fix needed.
Fix specification: none (optional P4 style: `checked_mul` for API robustness).
Regression test(s): the arithmetic above is the evidence; no test needed.
Verification commands and exact result: above; log `probes/G-FMT-008/dxt-bounds.log`.
Related findings: none.

---

## 3. File coverage

All 91 in-scope files have a status and an observation; rows appended to
`analysis/full-audit-2026-11-18/FILE_COVERAGE.csv` (reviewer `lane-G`).

| Status | Files |
|---|---|
| AUDITED | 91 |
| NOT_APPLICABLE | 0 |
| BLOCKED | 0 |

**Production sources (37):**

| File | Observation |
|---|---|
| `wc3-formats/src/lib.rs` | `detect`/`parse_*_header` use `first_chunk`; magic/size checks exact; no trust issue. |
| `wc3-formats/src/blp.rs` | WC3-FMT-001/002/003/004/005/006/007; DXT bound disproved (FMT-008). |
| `wc3-formats/src/diag.rs` | Bounded channel: 8 lines/reason, counters in `Mutex` with poison recovery; no parsing. |
| `wc3-formats/src/fdf.rs` | Grammar/lexer `get`-safe; WC3-FDF-001 (recursion depth). |
| `wc3-formats/src/gamedata.rs` | Object-data `ParseLimits` (1e6 entries/changes, 1 MiB string) and count caps; `Sheet::read` shares WC3-SLK-001's dense-grid amplification (544-561, 594-626). |
| `wc3-formats/src/ini.rs` | Section/key parsing; `trim_matches('"')` strips repeated quotes (cosmetic only), no allocation from untrusted counts. |
| `wc3-formats/src/map.rs` | `parse_units_ex`/`parse_doodads_ex` read-bounded; special-doodad count capped at 1e6; `peek()>=0x20` skin discriminator documented. No finding. |
| `wc3-formats/src/mdx.rs` | All geoset/track counts use `check_count`; WC3-MDX-001 is the single unchecked `UVAS` count. |
| `wc3-formats/src/media.rs` | Atomic extract (temp+rename); WC3-MPQ-002 whole-block scan. |
| `wc3-formats/src/objects.rs` | Thin projection over `gamedata::ObjectDataFile`; errors mapped, no second parser (good). |
| `wc3-formats/src/slk.rs` | WC3-SLK-001 (per-axis-only grid bound). |
| `wc3-formats/src/w3e.rs` | Magic/version 3..=12, dims ≤1024, `count*tile_size ≤ remaining`; `center` f32 accepted without finite check (observation only). |
| `wc3-formats/src/w3i.rs` | Version-gated 18/27 offsets, NUL-terminated strings, lossy UTF-8; no issue. |
| `wc3-formats/src/wpm.rs` | Dims ≤4096, exact `width*height` cell read, disable-bit flags (0x02 unwalkable) correct. |
| `wc3-mpq/src/lib.rs` | `MAX_FILE_SIZE` 512 MiB, table caps 8 Mi, sector caps, 5-codec WC3 mask, locale/platform hash filtering; stored/decompress budget checks; no finding. |
| `wc3-mpq/src/stream.rs` | Sector table size checked; `start ≤ end ≤ csize`; sparse/encrypted sectors bounded; `seek` clamps to len. |
| `wc3-mpq/src/pkware.rs` | 0x2400 window, back-reference ≥ start check, 4 KiB flush budget; no finding. |
| `wc3-mpq/src/huffman.rs` | 8 MiB/declared budget; static tree; `Bits::read(16)` would underflow `bit_count` if called, but only 1/8-bit reads exist (observation). |
| `wc3-mpq/src/adpcm.rs` | Channels 1..2, shift ≤15, output ≤ `2*len+header`. |
| `wc3-mpq/src/index.rs` | Count cap 4 Mi, names ≤4096 B; WC3-MPQ-001 (`with_extension` boundary panic); `with_capacity(count)` can reserve ~160 MB before EOF (same class as MDX, index files are app-owned cache). |
| `wc3-bink/src/lib.rs` | WC3-BINK-001/002; audio PCM bounded by concatenated chunk bytes. |
| `wc3-bink/src/avi.rs` | RIFF bounds, `MAX_MOVI_DEPTH` 32, `MAX_MOVI_CHUNKS` 1 Mi; u16 dims unvalidated (feeds BINK-001). |
| `wc3-bink/src/bitreader.rs` | All accessors bounds-checked; `peek` zero-pads; start-code scans use `get`/length guards. |
| `wc3-bink/src/vlc.rs` | LUT index `peek(max_bits) < 1<<max_bits`, code lengths ≤13, Kraft-tested tables. |
| `wc3-bink/src/idct.rs` | 8×8 writes assume 8 rows are present; WC3-BINK-003 site (`:173`). |
| `wc3-bink/src/tables.rs` | Static constants only. |
| `wc3-bink/src/error.rs` | Plain error enum, no logic. |
| `wc3-bink/src/mp3/mod.rs` | Header/bitrate/rate tables; side-info reservoir check (279-281); frame sync requires a consistent following header; output bounded. |
| `wc3-bink/src/mp3/granule.rs` | Band loops stop on the 0 sentinel; `ist_pos` ≤38; reorder sums to 576. |
| `wc3-bink/src/mp3/huffman.rs` | Scalefactor partitions ≤36 entries + 3 sentinels; count1 guards `di+4` and bit limit. |
| `wc3-bink/src/mp3/synth.rs` | `lins` 2112 and PCM indices verified (max 1151 of 1152); no OOB. |
| `wc3-bink/src/mp3/tables.rs` | Generated normative tables (provenance header). |
| `wc3-bink/src/mpeg4/mod.rs` | MB loops within `mb_w/mb_h`; write path WC3-BINK-002/003. |
| `wc3-bink/src/mpeg4/vol.rs` | 13-bit dims, unsupported features rejected by name; dims not capped beyond 8191 → ~170 MB `Picture` (observation under BINK-001). |
| `wc3-bink/src/mpeg4/frame.rs` | Plane sizing `height+1` is WC3-BINK-003 root; `to_rgba` assumption is WC3-BINK-002 root. |
| `wc3-bink/src/mpeg4/coeff.rs` | Escape paths clamp `i` to `0..64` before `block[scan[i]]`; static run/level tables. |
| `wc3-bink/src/mpeg4/motion.rs` | MV grid offsets bounded by MB grid; edge blocks clamp to picture in copy helpers. |

**Examples (41), tests (10), manifests (3):** each row is a static sweep
(`lines`, `unsafe`, `unwrap`, `expect`, `panic!` counts) with the note that examples are
analysis CLIs (not on the runtime path) and tests are covered by the operator freeze
(no edits). Examples worth naming: `wc3-formats/examples/blp_info.rs` (4 `unwrap`, CLI),
`wc3-mpq/examples/read_named.rs` (5 `expect`, CLI), `wc3-bink/examples/probe.rs`
(10 `expect`, analysis). No production path enters through an example.

---

## 4. Blocked / not covered

- **No file is BLOCKED.** Every production source and every example/test/manifest has a row.
- **Regression tests:** BLOCKED by the operator test freeze; each finding carries the intended
  test and a one-shot reproducer instead.
- **No BLP2 or movie fixture existed locally**; BLP2 probes are spec-layout synthetic files and
  movie probes are ffmpeg/libxvid encodings. A retail-BLP2 differential remains open for the
  tech-lead's corpus lane (not a blocker for the findings, which are contract-level).
- **FMT-NEW-006 (FogMask)** is lane J by assignment, not audited here.

---

## 5. Red Team — lane H (ledger 528)

Attempted falsification of H's three P1s against `d7af459` + the current worktree:

- **WC3-JASS-001 (`config` never executed):** mechanism holds — `crates/wc3-re/src/engine.rs:305`
  calls only `run_main()`, and `rg 'run_function\("config"\)'` over `wc3-re`/`wc3-game` returns
  nothing. **Challenge (scope, not mechanism):** the impact sentence “the documented seat
  fallback … is unreachable” is **overstated** — `crates/wc3-game/src/campaign.rs:3329-3345`
  implements an independent fallback chain (`w3i` seat → `MAP_CONTROL_USER` scan → `sloc`/camera),
  so single-player seat selection still works; what is actually wrong is script-visible
  controller/name/team/start-location state. Severity P1 is defensible under “major behaviour
  incorrect across common maps”, but the “unreachable fallback” clause should be removed before
  merge. Not disproved as a defect.
- **WC3-JASS-002 (events registered, never dispatched):** holds. `fire_unit_event` has only
  spell/item call sites (`vm.rs:1811,1931,1975-1976`); no region/death producer exists in
  `wc3-re` or `wc3-game`. No counter-evidence found.
- **WC3-JASS-003 (`TriggerSleepAction` does not suspend the caller):** holds. `pending_sleep`
  is a single `Runtime` field (`vm.rs:843,1437,1578`), consumed by the next thread. No
  per-thread propagation exists. No counter-evidence found.

**What lane G disproved in its own scope:** FMT-NEW-008 (DXT arithmetic overflow) is a false
positive; the DXT path is bounded by `check_pixels` before the multiplication. FMT-NEW-007's
“independent of dimension mismatch” premise could not be reproduced (the observed zero-pad is
the dimension-mismatch path). FMT-NEW-001's “arbitrary OOB read” framing is correctly scoped
by the candidate — confirmed as a logical-boundary bug only, not an unsafe read.

---

## 6. Evidence index

| Path | Content |
|---|---|
| `probes/G-TOOLS/audit_g_probe_formats.rs` | BLP1/BLP2/JPEG/MDX/FDF/SLK probe source (moved out of the crate after use) |
| `probes/G-TOOLS/audit_g_bink_probe.rs` | AVI/VOL/MPEG-4 probe source |
| `probes/G-TOOLS/audit_g_mpq_probe.rs` | MPQ index probe source |
| `probes/G-FMT-001/{overlap,alpha-missing}.{blp,log}` | FMT-001 reproducer + output |
| `probes/G-FMT-002/{short-spec,small-valid,valid64}.{blp,log}` | FMT-002 + FMT-007 reproducers + output |
| `probes/G-FMT-003/jpeg-{512,128}.log` | FMT-003 output (fixture patched in memory; original untouched) |
| `probes/G-FMT-005/{big5000.jpg,wrapped.blp,jpeg-big.log}` | FMT-005 147 KB → 205 MB reproducer |
| `probes/G-FMT-008/dxt-bounds.log` | FMT-008 disproof |
| `probes/G-MDX-UVAS/uvas.{mdx,log}` | 144-byte MDX → 103 GB allocation abort |
| `probes/G-FDF-DEPTH/nested5000.fdf`, `fdf5000.log` | depth-5000 SIGSEGV |
| `probes/G-SLK-GRID/scaled{,2}.slk`, `*.log` | 31-byte SLK → 236 MB at 8 192 rows |
| `probes/G-BINK-DIMS/{tiny,huge}.avi`, `*.log` | 338-byte AVI → 17 GB alloc abort |
| `probes/G-BINK-VOL/{clip32.m4v,avi32.avi,avi16-mismatch.avi,*-mismatch*.avi}` + logs | VOL/AVI mismatch panic; libxvid 32×18 panic |
| `probes/G-MPQ-INDEX/index-ext*.log` | `with_extension` char-boundary panic |

All probes are reproducible with the exact commands in each finding; the binary must be rebuilt
by temporarily copying the probe source into `crates/<crate>/examples/` (the source is kept in
`G-TOOLS/` so the audited tree stays clean).

---

## 7. Fix wave — ledger 538 (2026-11-18)

Every finding in §2 was fixed in place, together with the interim-TZ items assigned to 538
(`analysis/tech-lead/INTERIM-TZ-ASSIGNMENT.md`: WC3-I-001, I-002, I-003, I-009, I-010, I-021).
Scope was the owner paths `crates/wc3-formats/**`, `crates/wc3-mpq/**`, `crates/wc3-bink/**`;
`crates/wc3-re/**` (lead 537) was **not** edited. Operator constraints held: no new `#[test]`
or `tests/**` files (`Regression test(s): BLOCKED: operator test freeze` + a one-shot probe),
no device commands, `scripts/h2-cargo` only, provenance instead of licence bookkeeping.

Probe sources for this wave: `probes/I-TOOLS/audit_i_blp_probe.rs`,
`audit_i_mdx_probe.rs`, `audit_i_mpq_probe.rs`; fixtures land as raw bytes under
`probes/I-001/`, `probes/I-003/`, `probes/I-009/`, `probes/I-010/`. Before/after logs:
`after-probes/logs/*` and `after-probes/i-logs/*` in the run directory named in §7.6.

`Regression test(s): BLOCKED: operator test freeze` applies to **every** finding in §2 and to
every item in §7.3; the intended test is recorded in the finding, and the executed check is the
one-shot probe (`#[test]`/`tests/**` were neither added nor edited by this lane). The lane-G
fixtures under `probes/G-*` were regenerated byte-identically by the after-run.

### 7.1 BLP2 field tuples — verified against the spec before the decoder changed

The interim TZ requires the allowed `compression` / `alpha_depth` / `preferred_format`
(`alphaEncoding`) tuples to be established from the formal spec and reference material
*before* touching the decoder. Sources read on **2026-11-18**:

| # | Source | Access | What it fixes in the code |
|---|---|---|---|
| S1 | `wowdev.wiki/BLP`, "Header" + "Struct definition" (page served through the Wayback snapshot `web.archive.org/web/20240121052633id_/https://wowdev.wiki/BLP`; the live host returns a Cloudflare interstitial, which was not bypassed) | 2026-11-18, stealth-browser MCP, full page text | `compression`: 1 = paletted, 2 = DXT, 3 = A8R8G8B8; byte 0x09 = alpha bit depth **0, 1 or 8** (with "bit depth = 4 … found in `Interface\Glues\Credits\*.blp`"); byte 0x0A = `BLPPixelFormat { PIXEL_DXT1 = 0, PIXEL_DXT3 = 1, PIXEL_ARGB8888 = 2, PIXEL_ARGB1555 = 3, PIXEL_ARGB4444 = 4, PIXEL_RGB565 = 5, PIXEL_A8 = 6, PIXEL_DXT5 = 7, PIXEL_UNSPECIFIED = 8, PIXEL_ARGB2565 = 9, PIXEL_BC5 = 11 }`; palette entry is `{ b, g, r, pad }`; "If there is an alpha channel, then transparency data follows the image data **in each mipmap level**"; "Some lower mipmap levels in compressed textures have incorrect size … create a separate buffer of validSize length … the rest bytes … can be filled with zeros" |
| S2 | War3Net `War3Net.Drawing.Blp/BlpFile.cs` (reference decoder for the same container), raw.githubusercontent.com | 2026-11-18, stealth-browser MCP | the decoder selection expression: `flag = alphaDepth > 1 ? (alphaEncoding == 7 ? DXT5 : DXT3) : DXT1` |

Consequences written into `wc3-formats/src/blp.rs`:

* `dxt_kind(alpha_bits, preferred_format)` — depth 0 → DXT1 opaque, depth 1 → DXT1
  punch-through, depth 4/8 → DXT5 **iff** `preferred_format == 7` else DXT3. The old code keyed
  only on `alpha_bits` (`8 → DXT3`, `3|4|5 → DXT5`), i.e. it decoded every 8-bit-alpha DXT5
  texture as DXT3 (chunky 4-bit alpha) and a 4-bit credits texture as DXT5.
* A `preferred_format` outside `{0, 1, 7, 8}` on a DXT payload (`PIXEL_BC5`, ARGB1555, …) and an
  `alpha_depth` outside `{0, 1, 4, 8}` are **typed `FormatError::Unsupported`**, never a guess.
  ARGB8888 payloads (compression 3) accept only `{2, 4, 8}`.
* Paletted alpha planes are per mip (`checked_alpha_bits` ∈ {0, 1, 4, 8}, `alpha_plane_len`
  = `ceil(count·bits/8)`), with the palette converted **BGRA → RGBA** on both BLP versions.
* Lower compressed mips may under-declare `sizes[i]`: mip 0 must be complete, lower mips are
  zero-filled to the block count (S1's documented retail exception). Everything else is strict.

Reference assets: **none locally** — the corpus has 7 922 BLP files and **zero** BLP2
(`analysis/war3_full`: 3 901 files, all BLP1: 2 886 JPEG, 1 015 paletted; alpha bits observed
only 0 and 8). The BLP2 fixtures are therefore spec-layout synthetic files under `probes/I-001/`
labelled as such, exactly like the lane-G BLP2 probes.

### 7.2 Lane-G findings — fix and after-evidence

| ID | Change (`file:line` at the fix) | After-evidence (probe → line) |
|---|---|---|
| WC3-MDX-001 | `mdx.rs:1536-1543` `check_count(uv_set_count, 8)` + `MAX_UV_SETS = 8` before `Vec::with_capacity` | 144-byte `uvas.mdx`: `parse=Err(Corrupt("count exceeds remaining chunk data")) VmHWM_before_kb=4544 after_kb=4940` (was `memory allocation of 103079214720 bytes failed`, rc=134) |
| WC3-SLK-001 | `slk.rs:24-29,80-98` `MAX_CELLS = 1_048_576` on `width*height` + advisory-`B` note; `gamedata.rs:46-50,562-576` per-sheet slot budget | 31-byte `scaled2.slk` (1024×8192): `parse=ERR Corrupt("SLK grid exceeds the cell budget") elapsed_ms=0 VmHWM_delta_kb=648` (was `parse=OK … elapsed_ms=4217 delta_kb=235848`) |
| WC3-FMT-001 | `blp.rs:139-190` `validate_mips` (header-end, declared, EOF, read-extent, overlap) + `decode_paletted` alpha plane inside `sizes[i]` | `blp1-overlap` → `decode=ERR Corrupt("BLP mip points into the header")` (was `OK … px0=[128, 64, 0, 4]`); `blp1-alpha-missing` → `ERR Corrupt("BLP mip payload shorter than its pixels")` (was `OK alpha_values=[255; 16]`) |
| WC3-FMT-002 | same validator, `count` index plane required, no `resize` padding | `blp2-short` → `ERR Corrupt("BLP mip payload shorter than its pixels")` (was `OK … zero_pixels=4096`) |
| WC3-FMT-003 | `blp.rs:527-545` `read_info()` then `info.width/height == mip` | `jpeg-dims 512×512` and `128×128` → `ERR Corrupt("JPEG dimensions do not match the BLP mip")` (was `OK … px_after_orig=[0,0,0,0]` / cropped) |
| WC3-FMT-004 | one `MipRange`/`validate_mips` used by every branch (see FMT-001) | covered by the FMT-001/002/007 lines; `blp1-mip-overlap` → `ERR Corrupt("BLP mip payloads overlap")` |
| WC3-FMT-005 | `blp.rs:520-546` header-first, `set_max_decoding_buffer_size(expected)` | `jpeg-big` (147 KB BLP over a 5000×5000 JPEG): `elapsed_ms=0 … delta_kb=880` (was `elapsed_ms=24393 … delta_kb=205476`) |
| WC3-FMT-006 | `blp.rs:568-573` exact `rgba.len() == expected`, no resize/truncate | same path as FMT-003/005: a short or mismatched decode is `Corrupt`, never zero-padded |
| WC3-FMT-007 | `blp.rs:210-270` per-mip alpha plane, no 1172..2196 "alpha table", BGRA→RGBA, depths {0,1,4,8} | `blp2-valid 64×64` → `decode=OK … all_alpha_128=true alpha_values=[128, 128, 128, 128]` (was `alpha_values=[0,0,0,0]`); `blp2-small 16×16` → `decode=OK … all_alpha_128=true` (was `ERR TooShort`) |
| WC3-BINK-001 | `lib.rs:71-79,126-136` `MAX_MOVIE_PIXELS = 2048×2048` on `strf`, `mpeg4/mod.rs:254-269` on VOL | 338-byte `huge.avi` (65535×65535): `from_bytes=Err(Corrupt("AVI video dimensions out of range")) delta_kb=64` (was `memory allocation of 17179344900 bytes failed`, rc=134); `tiny.avi 16×16` still `Ok((1, (16, 16)))` |
| WC3-BINK-002 | `frame.rs:339-352` `to_rgba -> Result` size check; `lib.rs:239-256,279-295` AVI/VOL contract | 16×16 AVI + 32×32 VOL: `frame0=ERR corrupt stream: AVI/VOL dimension mismatch` (was `index out of bounds: the len is 1024 but the index is 1024`, rc=134); 32×32 control `frame0=OK` |
| WC3-BINK-003 | `frame.rs:71-76` planes padded to `mb_h*16+1` / `mb_h*8+1` rows | libxvid 32×18 (valid, non-aligned): `frame0=OK 32x18 rgba=2304 concealed=0` (was `index out of bounds: the len is 96 but the index is 96` at `idct.rs:173`, rc=134); 64×48 control `decoded`, `concealed=0` |
| WC3-MPQ-001 | `index.rs:304-315` byte-slice compare, no `str` slicing | `index-ext 'ab€'` → `with_extension=OK []` (was `panicked … not a char boundary`, rc=101); ASCII control `["abc.x"]` unchanged |
| WC3-MPQ-002 | `lib.rs:922-998` `block_reader` + `read_block_prefix`; `media.rs:41-55,378-400` prefix scan + `SCAN_DECODE_BUDGET` | `block-prefix War3x.mpq`: `checked=75 prefix_mismatches=0`; largest block 10 416 294 B: `prefix_bytes=8 prefix_us=909 … whole_us=40936 prefix_matches=true` (45× less work for the same decision) |
| WC3-FMT-008 | none (false positive) | control `dxt-bounds`: oversize DXT still `ERR` |

### 7.3 Interim-TZ items — fix and after-evidence

| ID | Change | After-evidence (probe → line) |
|---|---|---|
| WC3-I-001 | `blp.rs:487-520` `dxt_kind(alpha_bits, preferred_format)` from S1/S2; unknown tuple → `Unsupported`; lower compressed mips zero-padded per S1 | before (`HEAD`, same probe): `dxt5-enc7` → `mip_alphas=[0, 0, 255, 255]` (DXT5 payload decoded as DXT3), `dxt3` → `mip_alphas=[240, 240, 240, 240]` (DXT3 payload decoded as DXT5), `bc5-enc11`/`depth3-enc7`/`argb8888-enc7` → `OK` (garbage accepted), `dxt-mip1-short` → `ERR` (spec-documented padding rejected). after: `dxt5-enc7` → `mip_alphas=[0, 255, 0, 0]`, `dxt3` → `[0, 255, 0, 0]`, identical payload labelled `enc1` → `[0, 0, 255, 255]` (the field selects the decoder), `bc5-enc11` → `ERR Unsupported("BLP2 DXT pixel format")`, `depth3-enc7` → `ERR Unsupported("BLP2 DXT alpha depth")`, `argb8888-enc7` → `ERR Unsupported("BLP2 ARGB8888 pixel format")`, `dxt-mip0-short` → `ERR Corrupt("BLP mip payload shorter than its pixels")`, `dxt-mip1-short` → `OK mips=2` (zero-filled lower mip), `argb8888` → `px0=[3, 2, 1, 4]` |
| WC3-I-002 | `decode_paletted` (BLP2 `alpha_in_size = false`): indices at `offsets[i]`, alpha plane at `+count`, palette BGRA→RGBA, depths {0,1,4,8} | before: `blp2-valid 64×64` → `all_alpha_128=false alpha_values=[0,0,0,0]`, `blp2-small 16×16` → `ERR TooShort`. after: `all_alpha_128=true alpha_values=[128,128,128,128]`, 16×16 → `OK` (FMT-002/007 logs) |
| WC3-I-003 | `MipRange` + declared/read extents; BLP1 alpha inside `sizes[i]` | before: `blp1-alpha-not-declared` → `OK` (alpha read past `sizes[0]`), `blp1-mip-overlap` → `OK mip_alphas=[…, 4, 5, 6, 7]` (mip 1's alpha taken from mip 0's index bytes). after: both `ERR Corrupt("BLP mip payload shorter than its pixels")` / `ERR Corrupt("BLP mip payloads overlap")`; `blp1-valid-2mip` unchanged → `mip_alphas=[0,17,34,51, 255,255,255,255]` with `0xFF` sentinel bytes in mip 1 (no bleed); `blp1-alpha-truncated` → `ERR` both sides |
| WC3-I-010 | `decode_jpeg_stream` compares `info.width/height` with the mip before decoding (see FMT-003) | before: `jpeg-257x256`, `jpeg-256x257`, `jpeg-192x192` all `OK` (resized/cropped/zero-padded, `dims=257x256` …). after: those three → `ERR Corrupt("JPEG dimensions do not match the BLP mip")`; `jpeg-256x256` → `OK dims=256x256 mips=9` (unchanged); fixtures in `probes/I-010/` |
| WC3-I-009 | `mdx.rs:1274-1302` reversed-pair check → **stable sort** by time + `diag::note("mdx_track_unsorted")`; `Track::sample` doc pins duplicate semantics | before: `descending` → `parse=OK times=[200, 100] sample(150)=Some(0.0)` (silently the last key's value; the linear answer is 0.5). after: `parse=OK times=[100, 200] sample(150)=Some(0.5)` + `GAME_DATA_SKIP reason=mdx_track_unsorted keys=2 inversion=200>100`; `ascending`/`duplicate`/`single`/`three-mixed` unchanged (`sample(100)=Some(0.0)` for the duplicate = earlier key) |
| WC3-I-021 | `lib.rs:1338-1357` `dedup_names` (case-insensitive, first spelling) at both listfile ingestion points; **`wc3-re/src/mapfiles.rs:98-103` is not in this lane's owner paths** — handoff to 537 below | before: the probe does not compile against `HEAD` (`read_block_prefix` is this lane's addition); the shipped archives carry `duplicates=0`, so dedup changes no shipped behaviour. after: the exact `mapfiles` loop on N real names — `n=1000 naive_us=29971 set_us=4194`, `n=8000 naive_us=785586 set_us=40696` (19× on `War3x.mpq`); prefix scan `prefix_mismatches=0`, `prefix_us=909` vs `whole_us=40936` (10.4 MB block) |

**Ownership handoff (I-021, O(N²) in `wc3-re`).** `MapBundle::open` still runs
`for name in &bundle.listfile { if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) … }`
(`crates/wc3-re/src/mapfiles.rs:98-103`). `crates/wc3-re/**` belongs to lead 537, so this lane did
not edit it. Ready-to-apply patch, same semantics (first spelling kept, MPQ case-insensitive
comparison), for 537:

```rust
let mut names: Vec<String> = MAP_INNER_FILES.iter().map(|s| s.to_string()).collect();
let mut seen: std::collections::HashSet<String> =
    names.iter().map(|n| n.replace('/', "\\").to_ascii_uppercase()).collect();
for name in &bundle.listfile {
    if seen.insert(name.replace('/', "\\").to_ascii_uppercase()) {
        names.push(name.clone());
    }
}
```

### 7.4 Regression evidence (retail corpus)

| Check | Result |
|---|---|
| `audit_i_blp_probe blp-corpus analysis/war3_full` | `files=3901 ok=3901 err=0` — every retail BLP1 (2 886 JPEG + 1 015 paletted) still decodes with exact `width*height*4` output |
| `audit_i_mdx_probe corpus analysis/war3_full` | `files=2048 ok=2048 err=0` — the monotonicity normalization accepts every shipped model (the 3 inverted files parse and are reported through `mdx_track_unsorted`) |
| `movies` example on `War3x.mpq` + `War3xlocal.mpq` | `movies=0`, rc=0 — discovery still completes after the prefix-scan rewrite |
| `read_block_prefix` vs `read_block` | `prefix_mismatches=0` over 75 blocks (War3x) and 291 blocks earlier (War3xlocal) |
| `scripts/h2-cargo test -p wc3-formats -p wc3-mpq -p wc3-bink` | `TEST_RC=0` — 16 test binaries, **216 passed / 0 failed** (no test file was added or edited) |

### 7.5 Build gate

`scripts/h2-cargo check -p wc3-formats -p wc3-mpq -p wc3-bink` → **rc=0** on the final tree
(`CHECK_RC=0` in `/data/data/com.termux/files/usr/tmp/opencode/final538c.log`, 2026-11-18T17:31;
the earlier run `final538.log` records the same rc for the same crates). No new warnings from the
changed files: the `wc3-formats/src/map.rs` unused-import warnings and the `wc3-bink`
`pending_dc` warning are pre-existing in `d7af459`.

### 7.6 Reproduce

```
scripts/h2-cargo check -p wc3-formats -p wc3-mpq -p wc3-bink
# lane G: copy probes/G-TOOLS/*.rs into crates/<crate>/examples/, then
/data/data/com.termux/files/usr/tmp/opencode/run-probes538.sh <bin-dir> <log-dir>
# interim TZ: copy probes/I-TOOLS/*.rs into crates/<crate>/examples/, then
/data/data/com.termux/files/usr/tmp/opencode/run-i538.sh <bin-dir> <log-dir>
```

Before logs: `/data/data/com.termux/files/usr/tmp/opencode/before-probes/logs` (lane G, built from
`git archive d7af459` in `.../wc3rust-head-538`) and `.../before-probes-i/logs` (interim TZ, the
same HEAD tree; the MPQ probe does not compile there because `read_block_prefix` is this lane's
addition). After logs: `.../after-probes/logs` and `.../after-probes/i-logs`.

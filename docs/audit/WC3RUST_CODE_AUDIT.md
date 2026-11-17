# WC3Rust — audit registry (single document)

Source specification: `WC3RUST_CODE_AUDIT_TZ_P0-P4.md` (operator, 2026-11-17).
Audit revision: `b64b9b6` (import); fixes landed on `main` in the commits listed below.
Rule (§0.1): every item carries `FIXED`, `REPRODUCED_NOT_A_BUG` or `BLOCKED`,
with a regression test and the commit that closed it. Details per item are in
the four area sections appended below, verbatim from the lanes that did the work.

## Status table

| ID | P | Status | Commit | Regression test(s) |
| --- | --- | --- | --- | --- |
| AUD-001 | P1 | FIXED | 66a36a3 | hud_layout 16/16 (compact drawer closed, refresh/resize/open/close) |
| AUD-002 | P1 | FIXED | 6a7468b | id_counters_above_the_id_space_are_rejected_before_allocating, every_entity_counter_is_bounded_by_the_id_space, unit_id_at_or_above_next_id_is_rejected, duplicate_unit_ids_are_rejected |
| AUD-003 | P1 | FIXED | b176901 | aud003_* (exactly/one/far larger, multi-codec, malformed mask, stored fallback, caps) |
| AUD-004 | P2 | FIXED | 66a36a3 | hud_layout_points_pixels_round_trip + device matrix |
| AUD-005 | P2 | FIXED | 6a7468b | jass_natives_spell_rawcode_covers_every_variant, ..._spell_cast_event_context_carries_ability_id |
| AUD-006 | P2 | FIXED (unsupported ids = explicit Partial) | 6a7468b | jass_natives_game_state_trigger_fires_across_a_threshold, ..._game_event_registration_fires_with_the_engine_event, ..._without_an_engine_emitter_is_registered_and_partial |
| AUD-007 | P2 | FIXED (button clicks have no headless input source — documented) | 6a7468b | jass_natives_dialog_state_round_trip, ..._timer_dialog_state_round_trip, ..._wrong_handle_is_a_diagnostic |
| AUD-008 | P2 | FIXED | b176901 | adpcm::tests::* (0..=15, 16/31/32/255 rejected, short header, mono/stereo) |
| AUD-009 | P2 | FIXED | 2a7a392 | astc::tests 10/10 (u24 dims, overflow boundary, zero side, payload ±1, cap) |
| AUD-010 | P2 | FIXED | 2a7a392 | avi::tests (strh 0..35, exact min 36, padded, malformed) |
| AUD-011 | P2 | FIXED | 2a7a392 | avi::tests (depth 0/1/32/33/200k, empty nested, bad sizes, 10k siblings) |
| AUD-014 | P2 | FIXED | 3966c3f | APK dumps: release `0661a7a9…` has no `debuggable`; debug `cd86414b…` has `debuggable=true`; `aapt2 --debug-mode` build type + link gate |
| AUD-015 | P2 | FIXED | 3966c3f | `aud015_*` 7 tests + on-device permission map `r-x` → `r-x` (pre-fix `r-x` → `rwx`); `linker_symbols_resolve` passes |
| AUD-026 | P3 | FIXED (SDL 3.5.0 contract proven; main-thread guard added) | 3966c3f | SDL source table in `status-android.md`; `SDL_IsMainThread` + `PENDING_CHECKPOINT`; `LIFECYCLE_LOG_CHECK_SELFTEST PASS`; device stress scenario |
| AUD-029 | P4 | FIXED | 3966c3f | `dist/security-summary-{release,debug}.json` gate PASS; the same gate exits 1 on the pre-fix debuggable APK |
| AUD-009-N1 | P3 | OPEN — next pass | — | `CompressedLevel::expected_bytes` still multiplies in u32 (`crates/wc3-render/src/texture.rs:460,444,412`); latent, `is_consistent` is public |

## Gates run for this registry

```
cargo check -p herofolends-android                    rc=0
cargo test  -p wc3-game --test hud_layout             16 passed / 0 failed
cargo test  -p wc3-sim                                390 passed / 0 failed
cargo test  -p wc3-mpq                                34 lib + 6 integration, rc=0
cargo test  -p wc3-bink --lib                         62 passed; --tests rc=0
cargo test  -p wc3-render --lib astc::tests           10 passed
```
The workspace-wide gate (§1.9: `test-all.sh` must not skip members) and the
P3/P4 items are the next pass; this document does not claim the repository is
defect-free (§0.3).

## Android release + Adreno tooling (LEDGER 492)

AUD-014 (build-type debuggable, proven from the APKs), AUD-015 (no RWX, bounded
linker scan, fail-closed target validation), AUD-026 (SDL 3.5.0 watch contract +
main-thread guard + lifecycle stress scenario) and AUD-029 (release security
summary/gate) are **FIXED** in commit `3966c3f`. Full reproduction, commands,
APK sha256s, permission-map before/after and limitations:
`docs/audit/status-android.md`.

---

# MPQ (crate wc3-mpq)

<!-- source: docs/audit/status-mpq.md -->

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


---

# Simulation & JASS (crate wc3-sim)

<!-- source: docs/audit/status-sim.md -->

# Simulation layer (`crates/wc3-sim`) — audit status (LEDGER 487)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`, base revision `319588cc` (2026-11-17).
Every cargo invocation went through `scripts/h2-cargo` (single-slot wrapper:
one cargo at a time, memory gate, 2 test threads) because other audit lanes were
building on the same device.

| ID | Priority | Status | Where | Regression tests |
|----|----------|--------|-------|------------------|
| AUD-002 | P1 | **FIXED** | `crates/wc3-sim/src/save.rs` (`MAX_ID_SPACE`, `check_id_counter`, `check_id_space`, `decode_payload`, `rebuild_derived`, `read_script_section`) | `save::tests::id_counters_above_the_id_space_are_rejected_before_allocating`, `save::tests::every_entity_counter_is_bounded_by_the_id_space`, `save::tests::unit_id_at_or_above_next_id_is_rejected`, `save::tests::duplicate_unit_ids_are_rejected`, `save::tests::id_space_counter_bound_holds_over_sampled_values` (all in `src/save.rs`), `save_load_minimal_sim_round_trip_keeps_counters_and_state` (`tests/save_load.rs`) |
| AUD-005 | P2 | **FIXED** | `crates/wc3-sim/src/jass/natives.rs` (`spell_rawcode`), `crates/wc3-sim/src/jass/scheduler.rs` (spell event context, `EventIds`) | `jass_natives_spell_rawcode_covers_every_variant`, `jass_natives_spell_cast_event_context_carries_ability_id`, `jass_natives_event_id_defaults_match_the_shipped_header` (`tests/jass_natives.rs`) |
| AUD-006 | P2 | **FIXED** | `crates/wc3-sim/src/jass/host.rs` (`RegEvent::GameStateEvent`/`GameEvent`, `Host::raise_game_event`, `game_event_support`), `jass/natives/world.rs`, `jass/natives/core.rs` (`game_state_value`, `SetIntegerGameState`), `jass/natives/trigger.rs` (`GetEventGameState`), `jass/vm.rs` (`EventContext.game_state`), `jass/scheduler.rs` (`run_game_events`) | `jass_natives_game_state_trigger_fires_across_a_threshold`, `jass_natives_game_event_registration_fires_with_the_engine_event`, `jass_natives_game_event_without_an_engine_emitter_is_registered_and_partial` (`tests/jass_natives.rs`), `save_load_game_state_and_game_event_registrations_survive` (`tests/save_load.rs`) |
| AUD-007 | P2 | **FIXED** | `crates/wc3-sim/src/jass/host.rs` (`Object::Dialog`/`Button`/`TimerDialog`), `jass/natives/world.rs` (dialog/timer-dialog natives), `save.rs` (format v6) | `jass_natives_dialog_state_round_trip`, `jass_natives_timer_dialog_state_round_trip`, `jass_natives_dialog_wrong_handle_is_a_diagnostic_not_a_silent_success` (`tests/jass_natives.rs`), `save::tests::version_5_file_loads_with_neutral_ui_state`, `save_load_dialog_state_survives_the_round_trip` |

Two defects met in the same code path were fixed with the same evidence standard
and are reported here rather than left as tails:

| Found while fixing | Status | What was wrong | Test |
|--------------------|--------|----------------|------|
| `EventIds::default()` | **FIXED** | Headerless defaults for `EVENT_UNIT_SPELL_EFFECT` (77), `EVENT_PLAYER_UNIT_SPELL_EFFECT` (44), `EVENT_UNIT_CHANGE_OWNER` (80), `EVENT_PLAYER_UNIT_CHANGE_OWNER` (43) did not match the shipped `common.j` (291 / 274 / 287 / 270). Live behaviour was saved by `EventIds::from_vm` overriding from the loaded header; a map without a header registered the wrong constant. | `jass_natives_event_id_defaults_match_the_shipped_header` parses the header and pins all 22 defaults |
| `save_load_every_entity_class_roundtrip` fixture | **FIXED** | The test pushed items/camps/destructables/projectiles with explicit ids but left the counters at 1, i.e. it saved a world whose next spawn would collide. The new counter validation rejected it, which is the correct answer; the fixture now advances the counters. | same test |

Determinism: `Sim::state_hash` is unchanged by this work (no new hashed field),
and `save.load` round trips still assert hash equality every 60 ticks for 600
ticks. New state that had to survive a save (dialog/button/timer-dialog, the two
trigger registrations) is in the versioned save, not in derived memory.

---

## Reproduction before the fix

Each defect was reproduced on the unpatched tree first, with a temporary probe
test that used only the pre-fix public API (`crates/wc3-sim/tests/zz_probe_*.rs`,
removed after the patch; copies kept outside the repo). The probe reproduced the
defect by *observing* it, so a green probe run meant "bug present".

### AUD-002 — external `next_id` sizes a dense allocation

```text
$ git stash push -- crates/wc3-sim/src/save.rs crates/wc3-sim/src/jass/{host,scheduler,natives}.rs ...
$ scripts/h2-cargo test -p wc3-sim --offline --test zz_probe_repro -- --test-threads=1 --nocapture
running 2 tests
test probe_pre_fix_next_id_is_unbounded ... PREFI X next_id=2_000_000 LOADED ok, next_id=2000000
thread 'probe_pre_fix_next_id_is_unbounded' panicked at crates/wc3-sim/tests/zz_probe_repro.rs:54:5:
AUD-002 reproduced: a crafted next_id above any documented ID-space bound loads
test probe_pre_fix_zero_next_id_is_accepted ... next_id=0 rejected: OutOfRange { section: "units.id", index: 1, limit: 0 }
test result: FAILED. 1 passed; 1 failed
rc=101
```

The crafted file was a real save with four payload bytes rewritten and the FNV-1a
checksum recomputed (the checksum is a corruption check, not authentication, and
the format documents it), so the parser accepted it as valid and
`rebuild_derived()` then ran `vec![u32::MAX; self.next_id.max(1) as usize]`
(`save.rs:4964`): the external scalar chose the allocation size. `next_id =
u32::MAX` asks for 16 GiB and aborts the process on this 11 GB device; that case
was **not** executed against the unpatched code on purpose (it is the OOM the
item describes), and it is covered post-fix by a test that returns in
milliseconds.

### AUD-005 — `spell_rawcode` returns 0 for every variant

```text
test probe_pre_fix_spell_rawcode_is_zero ... PREFI X spell=HolyLight rawcode=0x00000000
PREFI X spell=DivineShield rawcode=0x00000000
... (all 11 Spell::ALL variants) ...
test result: ok. 1 passed
```

### AUD-006 — game-state/game-event registrations are fake

```text
test probe_pre_fix_game_state_trigger_never_fires ... PREFI X game-state trigger fires=0, errors=[]
test probe_pre_fix_game_event_handle_is_synthetic ... PREFI X game event handle id=0 kind=GameEvent object_is_none=true returned_trigger_registrations=0
test result: ok. 2 passed
```

The state trigger never fires and reports no error (silent), and
`TriggerRegisterGameEvent` returns a handle that no host object backs and that is
stored on no trigger.

### AUD-007 — dialog/timer-dialog state is dropped

```text
test probe_pre_fix_dialog_state_is_dropped ... PREFI X host objects after dialog setup:
  {..., 1073741831: Dialog { buttons: [1073741832] }, 1073741832: Button { text: "Archmage", hotkey: 0 }}
test result: ok. 1 passed
```

`DialogSetMessage(d, "Choose your hero")` and `DialogDisplay(Player(0), d, true)`
leave no trace: the object holds only the button list, and `TimerDialogDisplay`
returns `Null` without storing anything.

---

## AUD-002 — save load lets an external counter size a dense allocation

**Status: FIXED**

### Why

`decode_payload()` read five ID counters as plain `u32`, checked only
`unit.id < next_id`, discarded the computed `max_id` (`let _ = max_id;`) and then
`rebuild_derived()` allocated `vec![u32::MAX; self.next_id.max(1) as usize]`. The
item/product/destructable/camp/projectile counters had no check at all, and
duplicate ids were accepted.

### Fix

* One ID space, justified by the format and the entity counts the simulation
  supports:

  ```rust
  // decode_payload rejects any grid larger than 1024x1024 = 2^20 cells, and every
  // saved entity stands on at least one cell, so no legitimate save can carry a
  // counter above 2^20. The only save-derived dense structure is the unit id index
  // (4 bytes per slot) => worst case 4 MiB, two orders of magnitude above the
  // entity counts the sim runs (thousands of units, tens of thousands of trees).
  pub const MAX_ID_SPACE: u32 = 1 << 20;
  ```

* `check_id_counter(field, value)` — `1..=MAX_ID_SPACE`, else
  `SaveError::InvalidIdSpace { field, value, detail }`.
* `check_id_space(field, section, next, ids)` — counter range **and** every id
  below the counter **and** no duplicate, returning the maximum real id.
* All five counters are validated before `Sim` reconstruction and before any
  allocation. The script handle counter is bounded in its own space
  (`OBJECT_HANDLE_BASE..=OBJECT_HANDLE_BASE + MAX_ID_SPACE`), because script
  handles start at `0x4000_0000`.
* The dense unit index is sized from the real ids
  (`max_unit_id + 1`), never from the counter; `Sim::add_unit` still grows it on
  demand, so a save with three units and a counter at the cap costs three slots.
* Old files keep loading: the format is version-gated, `SAVE_FORMAT_VERSION` is
  now 6 with a `decode_v6` arm, and v1–v5 still decode through their own arms
  (`save_load_version_5_file_still_loads_with_neutral_ui_state` writes a real v5
  payload).

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --lib save::
running 11 tests
test save::tests::id_counters_above_the_id_space_are_rejected_before_allocating ... ok
test save::tests::every_entity_counter_is_bounded_by_the_id_space ... ok
test save::tests::unit_id_at_or_above_next_id_is_rejected ... ok
test save::tests::duplicate_unit_ids_are_rejected ... ok
test save::tests::id_space_counter_bound_holds_over_sampled_values ... ok
test save::tests::version_5_file_loads_with_neutral_ui_state ... ok
...
test result: ok. 11 passed; 0 failed

$ scripts/h2-cargo test -p wc3-sim --offline --test save_load
test save_load_minimal_sim_round_trip_keeps_counters_and_state ... ok
test save_load_dialog_state_survives_the_round_trip ... ok
test save_load_full_state_roundtrip_600_ticks ... ok
test save_load_every_entity_class_roundtrip ... ok
test save_versioning_unknown_version_rejected ... ok
test result: ok. 13 passed; 0 failed
```

`next_id = 0`, `next_id = 2_000_000`, `next_id = u32::MAX` and
`MAX_ID_SPACE + 1` all return `SaveError::InvalidIdSpace` and the `u32::MAX` case
returns immediately (no 16 GiB allocation); the crafted files carry a recomputed
checksum, so the checksum never masks the defect.

---

## AUD-005 — `GetSpellAbilityId` lost the ability id

**Status: FIXED**

### Why

`spell_rawcode()` formatted `Debug` for the variant, discarded it and returned
`0`. Every `Event::SpellCast` therefore reached the script with
`EventContext.spell_id == 0`.

### Fix

```rust
pub fn spell_rawcode(spell: crate::Spell) -> i32 {
    unit_rawcode(spell.ability_id())
}
```

`Spell::ability_id()` is the existing explicit table: the typed spells' `SpellDef`
row id from the ability data, or the data-backed variant's own `id`. It is the
same value `spell_for_ability` resolves, so the event id and the cast path cannot
disagree. Rewriting the `Debug` name would have been the wrong source.

While wiring the test, `EventIds::default()` was found to disagree with the
shipped `common.j` for the two spell events and the two change-owner events
(77/44/80/43 vs 291/274/287/270). `EventIds::from_vm` masked it whenever a header
was loaded; the defaults are now the header values and a test parses
`common.j` to pin all 22.

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_spell_rawcode_covers_every_variant ... ok
test jass_natives_spell_cast_event_context_carries_ability_id ... ok
test jass_natives_event_id_defaults_match_the_shipped_header ... ok
test result: ok. 14 passed; 0 failed
```

The scheduler test registers a unit spell event on the header's
`EVENT_UNIT_SPELL_EFFECT`, pushes `Event::SpellCast { spell: HolyLight }` into the
simulation and asserts the script read `0x41486862` (`'AHhb'`) from
`GetSpellAbilityId()`.

---

## AUD-006 — game-state / game-event registrations were fake

**Status: FIXED** (engine-raised state limits; host-raised game events, explicitly
`Partial` for the ids the engine has no subsystem for)

### Why

`TriggerRegisterGameStateEvent` destructured its arguments into `_`, stored
nothing and returned `Handle::new(GameEvent, 0)` — a handle no object backed.
`TriggerRegisterGameEvent` ignored the trigger, stored nothing and returned
`Handle::new(GameEvent, event_id)`. Both looked successful to the script.

### Fix

* `RegEvent::GameStateEvent { state, op, value }` and
  `RegEvent::GameEvent { event }`, saved in the trigger state (tags 13/14 in the
  script section, version 6).
* `Host::game_state_watch` records the last comparison result per
  `(trigger, state, op, value)`; the trigger fires on the false → true crossing —
  the rule `TriggerRegisterPlayerStateEvent` already uses.
* `game_state_value(host, state)` is the single definition shared by
  `GetFloatGameState`, `GetIntegerGameState` **and** the event sampler, so a
  script cannot read one number and have a trigger fire on another. It reads the
  script's own state: `SetFloatGameState` (`fgamestate`, e.g.
  `GAME_STATE_TIME_OF_DAY = ConvertFGameState(2)`) and the now-implemented
  `SetIntegerGameState` (`igamestate`); an unset state reads 0, exactly like the
  getters did before.
* `Host::raise_game_event(id)` +
  `JassVm::run_game_events()` deliver `EVENT_GAME_*` events. The engine itself
  raises `EVENT_GAME_STATE_LIMIT` (`ConvertGameEvent(3)` in the shipped header)
  on every game-state crossing, which also fires
  `TriggerRegisterGameEvent(t, EVENT_GAME_STATE_LIMIT)` watches.
* `game_event_support(id)` states honestly which ids the engine raises and which
  are raise-only; a raise-only registration is recorded in
  `Host::raise_only_game_events` so tests/tools see the `Partial` verdict.
* `EventContext.game_state` carries the `gamestate` that fired, and the
  previously unimplemented `GetEventGameState` (the header documents it directly
  under `EVENT_GAME_STATE_LIMIT`) now answers with it, so a condition/action can
  tell which state crossed the limit.
* Both natives validate their handle, push a diagnostic to `vm.errors` and return
  `null` for a non-trigger or an unknown `limitop` instead of a fake success.
  Registrations return the engine's per-id `Object::GameEvent` handle (one object
  per `gameevent` id, like the engine's global events).

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_game_state_trigger_fires_across_a_threshold ... ok
test jass_natives_game_event_registration_fires_with_the_engine_event ... ok
test jass_natives_game_event_without_an_engine_emitter_is_registered_and_partial ... ok
test result: ok. 14 passed; 0 failed
```

The threshold test covers: no fire below the limit, one fire on the crossing, no
re-fire while held, no fire on the downward crossing, silence while disabled,
fire after enable, silence after destroy, `GetTriggerEventId() == 3` and
`GetEventGameState() == 2` (the state that fired). A separate round-trip test
(`save_load_game_state_and_game_event_registrations_survive`) proves the restored
world keeps both registrations, one event object per `gameevent` id, and fires
again on later crossings and host-raised events.

---

## AUD-007 — dialog / timer-dialog natives were no-ops

**Status: FIXED** (headless UI state; the renderer/HUD reads it from the host)

### Why

`Dialog` held only a button list, `dialog_set_message` dropped the text,
`DialogDisplay` did nothing and `TimerDialogDisplay` returned `Null`. A map that
showed a dialog produced no state a renderer or a test could read.

### Fix

* `Object::Dialog { message, shown_to, asynchronous, buttons }`,
  `Object::Button { text, hotkey, quit }`,
  `Object::TimerDialog { timer, title, shown, speed, remaining_override,
  time_color, title_color }` — the headless UI model, readable through the public
  `Host::object(handle)`.
* Natives: `DialogCreate`, `DialogAddButton`, `DialogAddQuitButton`,
  `DialogClear`, `DialogSetMessage`, `DialogDisplay` (records the player, `false`
  hides), `DialogSetAsync`, `DialogDestroy` (frees the dialog and the buttons it
  owns), `CreateTimerDialog`, `TimerDialogSetTitle`, `TimerDialogSetSpeed`,
  `TimerDialogSetRealTimeRemaining`, `TimerDialogSetTimeColor`,
  `TimerDialogSetTitleColor`, `TimerDialogDisplay`, `DestroyTimerDialog`.
* A call against a wrong handle is a diagnostic in `vm.errors` plus `null` —
  never a silent success.
* Save format v6 stores the new fields; a v5 file restores the neutral values
  (empty message, hidden, no async flag, default colours) and is proven with a
  genuinely v5 payload.

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_dialog_state_round_trip ... ok
test jass_natives_timer_dialog_state_round_trip ... ok
test jass_natives_dialog_wrong_handle_is_a_diagnostic_not_a_silent_success ... ok

$ scripts/h2-cargo test -p wc3-sim --offline --lib save::
test save::tests::version_5_file_loads_with_neutral_ui_state ... ok

$ scripts/h2-cargo test -p wc3-sim --offline --test save_load
test save_load_dialog_state_survives_the_round_trip ... ok
```

**Not implemented, stated instead of faked:** a dialog button *click* still has no
input source in the headless run — `TriggerRegisterDialogEvent` /
`TriggerRegisterDialogButtonEvent` keep their registration (`RegEvent::DialogEvent`)
and the engine has no code path that raises a click, exactly as before this item.
The native is not touched by AUD-007 and its registration is not a fake: it is a
real record that a driver can raise once an input source exists. Genuinely
engine-unsupported game events are reported through
`Host::raise_only_game_events` (AUD-006) rather than silently succeeding.

---

## Format change (declared)

`SAVE_FORMAT_VERSION` 5 → 6, append-only per the module's forward-compatibility
rule:

* `Object::Dialog` / `Object::Button` / `Object::TimerDialog` append their headless
  UI fields (gated on `version >= 6`, so v5 objects decode to neutral values).
* `RegEvent` tags 13 (`GameStateEvent`) and 14 (`GameEvent`).
* `EventContext.game_state` appended to the saved thread/event context (gated on
  `version >= 6`), so a thread parked on `TriggerSleepAction` keeps the state that
  fired its trigger; a v5 context restores `None`.

No gameplay field, network protocol, ABI or render pipeline changed. Determinism
is preserved: no new `state_hash` input, and `save.load` still produces an
identical hash and identical following ticks.

## Full-suite evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline
60 test targets, 390 tests:  390 passed; 0 failed; 0 ignored        (SIM_EXIT=0)
```

```text
$ scripts/h2-cargo test -p wc3-game --offline --test menu_save_load
test game_menu_save_load_roundtrip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored                     (GAME_EXIT=0)
```

The `wc3-game` run is the cross-crate check that the v6 save still restores a
live battle through the game layer's own save/load path (`Game::save_slot_path`,
`Sim::from_save_path`, digest equality).

Targeted loops while iterating (all green, `scripts/h2-cargo`):

```text
--lib                    61 passed
--test save_load         14 passed
--test jass_natives      14 passed
```

Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`, host
`aarch64-linux-android` (Termux), device RMX3709. Every run used
`scripts/h2-cargo` (flock + memory gate); `CARGO_INCREMENTAL=0`.


---

# Media parsers (wc3-bink, wc3-render)

<!-- source: docs/audit/status-media.md -->

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

The same window later closed `wc3-render --lib` itself:
`error[E0004]: non-exhaustive patterns: 'LightModel::Legacy' not covered` at
`crates/wc3-render/src/mesh.rs:1440` (another owner's in-flight lighting
refactor, retried three times over four minutes). That blocked
`crates/wc3-render/tests/astc.rs` and `tests/cache.rs`, which decode real
`astcenc` fixtures through `astc::decode`; `src/astc.rs` is byte-identical to the
tree that produced the 10/10 run (`git diff HEAD -- crates/wc3-render/src/astc.rs`
empty), so the committed module is the one that passed. The real fixtures are
66x66 @6x6 and 64x64 @8x8 — orders of magnitude below the new caps.

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


---

# HUD & Android (wc3-game, apps/android-native)

<!-- source: docs/audit/status-hud.md -->

# HUD / Android layer — audit status (LEDGER 489)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`.

| ID | Priority | Status |
|----|----------|--------|
| AUD-001 | P1 | FIXED |
| AUD-004 | P2 | FIXED |

---

## AUD-001 — Panic when refreshing the compact mobile HUD with the commands drawer closed

**Status: FIXED**

### Why

`apply_mobile_button_rects()` indexed `mobile.card_slots[slot]` for every card
action. In compact landscape the card is a drawer: `compose_landscape()` only
fills `card_slots` inside `if self.state.commands_open`
(`crates/wc3-game/src/mobile_hud.rs:1927-1932`), so the *valid* state
`LandscapeCompact + commands_open == false` has an empty `card_slots`. Every
`refresh_buttons()` / `resize()` / `set_safe_insets()` in that state reached the
index expression and panicked; the Android release profile is `panic = "abort"`,
so this is a process crash, not a recoverable error.

Confirmed against the import revision `b64b9b6`:

```text
b64b9b6 crates/wc3-game/src/game.rs:2217
    if let Some(button) = self.buttons.get_mut(*index) {
        button.rect = mobile.card_slots[slot];     // panic when card_slots is empty
    }
```

### Fix

`crates/wc3-game/src/game.rs:2221` (`apply_mobile_button_rects`):

- every card button starts at `Rect::new(0.0, 0.0, 0.0, 0.0)`;
- an empty `card_slots` returns immediately — the 12 actions keep their
  indices/actions but have no screen rect;
- each slot is read with `card_slots.get(slot)`; a drawer shorter than the
  logical card leaves the remaining actions list-only;
- no arbitrary geometry is invented: a zero-size rect is not a hit target
  (`wc3-input/src/lib.rs:43-48` `contains` is half-open on both axes, and
  `hit_button` skips `w <= 0 || h <= 0` in the slop pass,
  `wc3-input/src/lib.rs:654-676`).

Invariant now asserted by tests: `build_buttons()` always authors exactly 12
card slots (`hud.rs:2493-2511`, missing slots become `Button::empty()`),
`card_button_indices()` is `0..12`, and when `card_slots` is non-empty the
12 render rects and the touch snapshot are the same rectangles.

### Regression test

`game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive`
(`crates/wc3-game/src/game.rs`):

- 1280x720 px @ 2.0 px/dp = 640x360 dp (compact), drawer closed:
  `card_slots` empty, all 12 buttons zero-rect, no card touch target;
- `refresh_buttons()` with the empty list (the AUD-001 crash input);
- drawer open: all 12 actions map 1:1 to the rendered slots, every
  `touch_buttons` rect equals the rendered `Button::rect`, and
  `pointer_down(slot centre)` presses that exact index;
- resize with the drawer open (1600x900), close, portrait rotation (720x1280,
  card inline), back to compact, then the 320x240 minimum window.

Existing coverage kept green:
`game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target`
and the pure-layout matrix `crates/wc3-game/tests/hud_layout.rs`
(`hud_layout_no_ambiguous_overlap` asserts a closed compact drawer has no cells).

---

## AUD-004 — Android never passes the real density

**Status: FIXED**

### Why

`Game` starts with `density_scale: 2.0` (`game.rs:894-897`) and exposes
`set_density()` / `set_window_metrics()`, but the Android host called only
`game.resize()` and `set_safe_insets()`. Pre-fix evidence:

```text
$ grep -rn "set_window_metrics\|set_density" --include=*.rs apps crates | grep -v "fn set_"
crates/wc3-game/src/game.rs:894: // Replaced by `set_density` from the platform ...
```

No caller existed. The HUD was therefore always solved at a hardcoded 2.0
px/dp regardless of the device (a 3.0 density phone got a HUD 1.5x too small in
physical size), and the layout mode could be wrong (wide vs compact).

### Which SDL3 value is the density (observed in the vendored SDL 3.5.0)

Source: `donors/SDL` @ `4ca1b1c` (same headers/libs shipped in
`third_party/sdl3*`).

- `src/video/android/SDL_androidwindow.c:59-60` sets
  `window->w = Android_SurfaceWidth` — Android window coordinates are the
  surface **pixels** (`SDL_GetWindowSizeInPixels` returns the same size).
- `android-project/app/src/main/java/org/libsdl/app/SDLSurface.java:174-175`
  feeds `density = realMetrics.densityDpi / 160.0f` through
  `nativeSetScreenResolution(...)`; `src/video/android/SDL_androidvideo.c:224`
  stores it in `Android_ScreenDensity` and `:198` publishes it as
  `display->content_scale`.
- `src/video/SDL_video.c:1900-1911`: `SDL_GetWindowPixelDensity` is
  `pixel_w / window_w` → **1.0 on Android** (window coords == pixels).
- `src/video/SDL_video.c:1914-1931`: `SDL_GetWindowDisplayScale` is
  `pixel_density * content_scale` → the Android `densityDpi / 160` value, i.e.
  the scale UI content must be drawn at.

Correct source: `SDL_GetWindowDisplayScale`.
`SDL_GetWindowPixelDensity` is only a last-resort fallback (it is 1.0 here and
would reproduce the old bug). The symbols are exported by the shipped Android
library:

```text
$ readelf -Ws third_party/sdl3-android-build/libSDL3.so | \
    grep -E "SDL_GetWindowDisplayScale|SDL_GetPrimaryDisplay|SDL_GetDisplayContentScale"
SDL_GetPrimaryDisplay@@SDL3_0.0.0
SDL_GetWindowDisplayScale@@SDL3_0.0.0
SDL_GetDisplayContentScale@@SDL3_0.0.0
```

### Fix

`apps/android-native/src/lib.rs`:

- `window_density()` (`:392`) reads `SDL_GetWindowDisplayScale`, falls back to
  `SDL_GetDisplayContentScale(SDL_GetPrimaryDisplay())`, then 1.0; the value is
  clamped to `[0.5, 8.0]` by the game layer.
- `WindowMetrics { framebuffer, density, insets, insets_ok }` + `window_metrics()`
  (`:409`) snapshot `SDL_GetWindowSizeInPixels` + `SDL_GetWindowSafeArea`
  (window coords converted to pixels with the window/pixel ratio).
- `apply_window_metrics()` (`:432`) is the single platform entry:
  `Game::set_window_metrics(framebuffer_px, density, SafeInsets px)` and a
  `GAME_WINDOW_METRICS px=… dp=… density=… insets=…` log line whenever the
  snapshot changes.
- Called at window creation (`:1392`), on every surface resize/recovery
  (`:1680`), on `SDL_EVENT_WINDOW_SAFE_AREA_CHANGED` and
  `SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED` (`:1693`), on the starved-queue
  watchdog (`:1745`) and on `Lost` (`:1781`), plus a 30-frame poll that catches
  density changes the event filter does not surface.
- Never scaled twice: insets enter in pixels, `Game::rebuild_layout` divides
  them by density exactly once (`game.rs:1941-1947`), and `HudLayout::to_px`
  multiplies dp → px exactly once (`mobile_hud.rs:2271-2277`). Touch/finger
  coordinates are already framebuffer pixels (`finger_pos` uses the surface
  configuration).

### Regression tests

- `crates/wc3-game/tests/hud_layout.rs::hud_layout_points_pixels_round_trip`:
  1440x3120 px @ 2.75 → dp viewport; pixel insets 82.5/66 px → 30/24 dp → back
  to the same pixels; a 48 dp target is 132 px (not 363 px); the rendered
  pixel rect is the hit target (`HudHit::Card`) and the dp centre maps onto the
  same pixel centre.
- `game::tests::window_metrics_push_framebuffer_density_and_insets_once`:
  density 1.0 → wide, 2.0 → compact 640x360 dp; a 40 px inset at 2.0 is 20 dp
  and 40 px on screen (not 80 px); rotation to portrait solves an inline card;
  `set_density` rejects a no-op.

The device-side line to check in `logcat`:

```text
GAME_WINDOW_METRICS px=2414x1080 dp=805x360 density=3.000 insets=0,0,0,0
```

---

## Side repairs required to run the HUD test suite

1. **`wc3-game` unit-test target did not compile** after the relayout commit
   `c372783`: `MenuSkin` fields became `Option<MenuPart>` but two mock tests
   still built them from `Option<usize>` (17 errors in
   `cargo test -p wc3-game --lib`). Repaired in `mobile_hud.rs` tests with
   `menu_part_from` / `load_menu_pack_part` / `load_menu_ui_part`; production
   code untouched.

2. **`crate::hud::HudLayout.top_bar`** is documented as “the whole console, used
   as the top-level HUD blocker” (`hud.rs:2302-2319`) but was populated with
   `mobile.top_meta` (the top strip). Restored to `mobile.console`
   (`game.rs:1983`); `blocked_rects` already adds `mobile.top_meta` separately,
   so touch routing is unchanged and the legacy consumers (edge pan, hitbox
   debug, tests) get the documented rectangle.

3. `game::tests::safe_insets_keep_the_hud_inside_the_system_bars` asserted the
   pre-relayout contract (console inset horizontally). The relayout made the
   console background edge-to-edge and moved the insets onto the content
   (`hud_layout_console_edge_to_edge` is the authoritative test). The stale
   assertions were updated to the current contract: console edge-to-edge,
   content/resources/minimap inside the insets, vertical insets intentionally
   ignored by the console per the operator directive.

4. `ui_menu_restyled_headless_mock_writes_png` still asserted the old 44-56 px
   glue-button strip. The relayout clamps every stack entry to
   `[MENU_MIN_TARGET_PX (90), 150]` px (`mobile_hud.rs:4280`, the operator's
   90 px finger floor), so the measured 116.6 px is the intended target; the
   assertion now checks that clamp instead of the removed desktop height.

5. `game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target`
   assumed a 1280x720 *pixel* viewport is the wide inline card. With the 2.0
   phone density fallback it is a 640x360 dp compact drawer with no card on
   screen. The test now pins `set_density(1.0)`, asserts the tile meets
   `MIN_BUTTON_TARGET`, and probes the router's real pad
   (`max((min_target - h) / 2, slop)`, 4 px for a 48 dp tile) plus a point
   beyond it; the old 8 px probe was larger than the pad and could not pass on
   the 48 dp tile the relayout introduced.

---

## Evidence

Toolchain: `rustc` release pinned by the workspace, `scripts/h2-cargo`
(single cargo slot; the shared workspace is edited concurrently by other audit
lanes).

```text
$ scripts/h2-cargo check -p wc3-game --offline
rc=0
$ scripts/h2-cargo check -p herofolends-android --target aarch64-linux-android --offline
rc=0
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive
test game::tests::compact_landscape_closed_drawer_keeps_card_actions_non_interactive ... ok
test result: ok. 1 passed; 0 failed; 448 filtered out
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::window_metrics_push_framebuffer_density_and_insets_once
test game::tests::window_metrics_push_framebuffer_density_and_insets_once ... ok
test result: ok. 1 passed; 0 failed; 448 filtered out
$ scripts/h2-cargo test -p wc3-game --lib --offline -- mobile_hud::tests::ui_menu
test hud::mobile_hud::tests::ui_menu_restyled_headless_mock_writes_png ... ok
test hud::mobile_hud::tests::ui_menus_shipped_art_mock_pngs ... ok
test result: ok. 2 passed; 0 failed; 447 filtered out
$ scripts/h2-cargo test -p wc3-game --test hud_layout --offline
test result: ok. 16 passed; 0 failed; 0 ignored
$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::safe_insets_keep_the_hud_inside_the_system_bars
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::console_hud_rects_do_not_swallow_world_taps
<PENDING>

$ scripts/h2-cargo test -p wc3-game --lib --offline -- \
    game::tests::console_layout_blocks_world_clicks_and_placement
<PENDING>
```

Device-facing log lines added:

```text
GAME_WINDOW_METRICS px=2414x1080 dp=805x360 density=3.000 insets=0,0,0,0
GAME_SAFE_AREA unavailable (SDL_GetWindowSafeArea failed)   (unchanged, one-shot)
```

## Commit

`<filled after commit>`


---


# Infrastructure and quality-gate lane — status (LEDGER 493)

Registry: `docs/audit/WC3RUST_CODE_AUDIT.md` (single registry; this is its
infrastructure section).
Spec: `WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2 + AUD-020, AUD-025, AUD-030,
AUD-031, AUD-032, AUD-009-N1.
Repo: `herofolends-rs`, branch `main`. Cargo runs only through
`scripts/h2-cargo` (single device-wide slot).

| ID | P | Status | Regression test(s) |
| --- | --- | --- | --- |
| AUD-020 | P3 | FIXED | `sdl3::tests` 5/5 (zero, stock block, c_int boundary, overflow, split) |
| AUD-009-N1 | P3 | FIXED | `texture::tests` 16/16 (overflow boundary, legitimate large level, zero side, impossible level via `is_consistent`/`validate`) |
| AUD-025 | P3 | FIXED | `wc3-game --test input_limits --features movie_decoder` 7/7 (MPQ/AVI/ASTC/save/net + retail) |
| AUD-030 | P3 | FIXED | `scripts/test-all.sh` (metadata-derived members, host + target phases, JSONL summary) + `--self-test` PASS |
| AUD-030-N1 | P3 | FIXED | `wc3-replay` `command_encoding_roundtrip` (tags 6-13) + `unknown_ability_id_is_a_typed_error` |
| AUD-030-N2 | P3 | FIXED | `cargo check -p herofolends-bench` rc=0 |
| AUD-031 | P4 | FIXED (documented + diagnostic surfaced) | `wc3-re --lib unknown_native`; corpus typed errors; `docs/audit/ERROR_MODEL.md` |
| AUD-032 | P4 | FIXED (inventory + seat-default session leak) | `wc3-re --lib local_player`; `docs/audit/GLOBALS.md` |

New findings this lane: **AUD-030-N1**, **AUD-030-N2** (both pre-existing
workspace compile breaks that the new gate found on its first run; both fixed
here because a gate cannot be green otherwise).

---

## AUD-020 — SDL audio FFI byte count

**Reproduced (before).** `crates/wc3-audio/src/engine/sdl3.rs` at HEAD:

```rust
let bytes = samples.len() * std::mem::size_of::<f32>();   // unchecked
...
(self.api.put_data)(self.stream, samples.as_ptr() as *const c_void, bytes as c_int)
```

The multiplication is caller-controlled and the `as c_int` truncates whenever
the byte count exceeds `c_int::MAX` (536 870 911 f32 samples = 2 GiB of bytes),
turning the length negative. Arithmetic reproduction of the shipped expression
(`rustc -C debug-assertions=on`, helper file `/data/data/com.termux/files/usr/tmp/aud493_repro.rs`):

```
AUD-020 old c_int cast(600_000_000 samples) => -1894967296
AUD-020 old c_int cast(536_870_912 samples) => -2147483648
```

A negative byte count is exactly the "FFI argument with an unverified ABI width"
that §6.3.5 forbids; the old code could only fail inside SDL.

**Fix.** `crates/wc3-audio/src/engine/sdl3.rs`

- `MAX_PUT_BYTES = 1 MiB` (documented: 128x the stock 8 KiB mixer block).
- `put_bytes(samples) -> Result<c_int, AudioError>`: `checked_mul(4)` +
  `c_int::try_from`; both failure modes return `AudioError::Backend` with the
  count in the message, **before** the unsafe call.
- `chunk_lengths(samples)`: splits a larger block into ≤1 MiB transfers instead
  of truncating; `write` returns `Ok(0)` for an empty block and reports the
  written prefix if a later chunk fails.
- Tests: `zero_samples_need_zero_bytes`,
  `a_stock_mixer_block_matches_the_sdl_byte_count`,
  `the_c_int_boundary_is_accepted_and_the_next_sample_is_rejected`
  (536 870 911 samples = 2 147 483 644 B accepted; +1 sample rejected),
  `a_sample_count_that_overflows_the_byte_multiplication_is_an_error`
  (`usize::MAX`), `blocks_above_the_documented_maximum_are_split_not_truncated`.

Evidence:

```
$ scripts/h2-cargo test -p wc3-audio --lib sdl3
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 49 filtered out
```

## AUD-009-N1 — `CompressedLevel::expected_bytes` u32 arithmetic

**Reproduced (before).** HEAD expression
(`crates/wc3-render/src/texture.rs:460`, with `natural_pitch` at `:444` and
`block_rows` at `:412`):

```rust
fn expected_bytes(format, width, height) -> usize {
    (format.natural_pitch(width) * format.block_rows(height)) as usize
}
```

`natural_pitch` multiplies in `u32` and `expected_bytes` multiplies the two
`u32`s again. Reproduction of the same expression
(`/data/data/com.termux/files/usr/tmp/aud493_repro.rs`, ASTC 6x6,
1 000 002 x 1 000 002):

```
thread 'main' panicked at aud493_repro.rs:5:5: attempt to multiply with overflow
AUD-009-N1 old expected_bytes => Err("panic: multiply overflow")
AUD-009-N1 release-wrap value => 2064590736
```

True length is 444 446 222 224 B; the release build wraps it to 2 064 590 736 B,
so a 2 GB payload could satisfy `is_consistent` for a 444 GB level. That is the
latent validator bypass the registry recorded.

**Fix.** `crates/wc3-render/src/texture.rs`

- `pub const MAX_TEXTURE_SIDE: u32 = 16_384` with a compile-time
  `assert!(MAX_TEXTURE_SIDE == crate::astc::MAX_TEXTURE_SIDE)` so `.astc`
  parsing and in-memory validation cannot drift.
- `CompressedFormat::block_grid(width, height) -> Result<(u64, u64), String>`:
  zero/oversized sides rejected, division in `u64` (same policy as
  `astc::payload_len`).
- `expected_bytes` → `Result<usize, String>` using `block_grid` +
  `checked_mul` + `usize::try_from`; `is_consistent` is `is_ok_and(...)`, so an
  impossible level is `false`, never a panic; `CompressedImage::validate`
  reports the cap reason instead of indexing an impossible grid.
- `block_rows` / `row_pitch` / `natural_pitch` clamp their input side to the cap
  (pitch helpers feed the wgpu upload of already-validated levels).
- Tests: `expected_bytes_accepts_a_legitimate_large_level` (16384² ASTC 6x6 =
  2731² · 16, ETC2 4x4 = 4096² · 16),
  `expected_bytes_rejects_dimensions_that_would_overflow_u32` (`u32::MAX`
  combos, `MAX_TEXTURE_SIDE + 1`, 65 536²),
  `expected_bytes_rejects_zero_dimensions`,
  `is_consistent_is_false_for_impossible_levels_instead_of_panicking`
  (32 766² passes the block-alignment check and is still rejected by the cap),
  `a_level_one_byte_short_or_long_is_inconsistent`.

Evidence:

```
$ scripts/h2-cargo test -p wc3-render --lib texture::tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
```

## AUD-025 — unified input-limit policy

Deliverables:

- `docs/audit/INPUT_LIMITS.md` — one table for every external input path:
  file size, decoded bytes, dimensions, element counts, nesting depth, string /
  block size, total allocation and CPU-work bound, with the constant, the
  enforcing line and the typed error for each row (MPQ, AVI, ASTC, compressed
  mip levels, saves, network/replay, SDL audio FFI).
- `crates/wc3-game/tests/input_limits.rs` — the shared negative corpus. One
  hostile input per parser through its production entry point: MPQ oversized
  table count, AVI 33-level `LIST rec ` nesting (32 still walks), ASTC u24
  dimensions (16384² stays legal), save `declared = u32::MAX`, oversized save
  file (in-memory + sparse file), replay `declared = u32::MAX`, and a retail
  pass that opens all three shipped archives, asserts every block is below the
  cap and decodes `(listfile)`.
- One real gap closed while building the table: `Sim::from_save_path` read the
  whole file with `std::fs::read` before any length check.

  ```
  $ git show HEAD:crates/wc3-sim/src/save.rs | grep -n "fn from_save_path" -A 4
  5448:    pub fn from_save_path(path: impl AsRef<Path>) -> SaveResult<Sim> {
  5449-        let bytes = std::fs::read(path.as_ref())
  ```

  Fixed with `MAX_SAVE_BYTES = 64 MiB` (`save.rs:165`), a new
  `SaveError::Oversize { len, limit }` variant, a metadata check and a bounded
  `take(MAX_SAVE_BYTES + 1)` read (`save.rs:5487`), plus the same guard in
  `open_envelope` for callers that already hold the bytes, and a symmetric
  `save_to_path` refusal so the writer cannot produce a file the loader rejects.

Evidence:

```
$ scripts/h2-cargo test -p wc3-game --test input_limits --features movie_decoder
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Retail non-regression (caps far above shipped content, decode unchanged):

```
$ scripts/h2-cargo test -p wc3-mpq --lib largest_shipped_entry -- --nocapture
MPQ_MAX_BLOCK …/War3.mpq: 6591551 bytes (File00003957.bin)
MPQ_MAX_BLOCK …/War3x.mpq: 10416294 bytes (File00002561.bin)
MPQ_MAX_BLOCK …/War3xlocal.mpq: 1236952 bytes (File00000567.bin)
test result: ok. 1 passed
```

Limitation: the audit also asks for fuzzing with an allocation/time budget. The
corpus is deterministic and the caps are enforced before allocation; a fuzz
harness (`cargo-fuzz`/AFL on this device) was not run in this lane and is not
claimed. The parser crates keep their own cap tests as a second layer
(`aud003_*`, `avi::tests`, `astc::tests`, save `hostile` fixtures).

## AUD-030 — `test-all.sh` as a workspace gate

**Before.** The old script listed 14 crates by hand and stated it does not run
`cargo test --workspace`; `wc3-bink`, `wc3-data`, `wc3-re`, `wc3-platform`,
`wc3-gpu`, `wc3-vulkan-loader`, `wc3-turnip-vendor`, the hook crates and every
app were outside the gate.

**After.** `scripts/test-all.sh`:

1. Member list from `cargo metadata --no-deps` each run (30 members today);
   `--list` prints members + exclusions as JSON.
2. `cargo check --workspace` (host) — host is `aarch64-linux-android` here, so
   this is also the shipping-ABI check.
3. Target phase: every installed `*-linux-android` rustlib triple that has a
   linker gets `cargo check -p herofolends-android --target <triple>`; the other
   three triples are recorded as `SKIP` with the missing tool (no NDK linker on
   this host) — never as PASS.
4. Per-member `cargo test -p <member>` through `scripts/h2-cargo` (flock, memory
   floor, bounded jobs), serial, each under `timeout` (default 1800 s,
   `--timeout`), address-space capped at 4.5 GB.
5. One log per job; on failure the tail is printed and the log path kept.
6. Machine-readable `analysis/test-all.jsonl` (one object per job + summary).
7. `EXCLUDE` is the only skip mechanism and every entry names the reason
   (three `cdylib` Android hook shims with no test targets; their compile
   coverage is phase 1).

Self-test (`--self-test`), which the audit explicitly requires:

```
$ scripts/test-all.sh --self-test
SELF_TEST_PASS: failing test in wc3-bink made the gate exit 1
  self-test gate log: analysis/test-all/self-test.log
$ tail analysis/test-all/self-test.log
  | test aud030_self_test_must_fail ... FAILED
  | thread 'aud030_self_test_must_fail' panicked at crates/wc3-bink/tests/zz_aud030_self_test.rs:5:5
  | AUD-030 self-test: the gate must report this failure
TEST_ALL_FAIL failed=1 passed=0 skipped=0 (see analysis/test-all/ and analysis/test-all.jsonl)
```

`wc3-bink` is deliberately the probe crate: it is exactly one the old subset
never ran. The injected file is removed by a trap and is not part of the tree.

**Findings from the first real run (both fixed here):**

- **AUD-030-N1** — `crates/wc3-replay/src/lib.rs:166` did not compile:
  `Command::HarvestLumber/LearnAbility/ReviveHero/Repair` were missing from
  `encode_command` (E0004). Fixed with tags 10-13, a length-prefixed ability id
  for `LearnAbility` (decoded through the new
  `wc3_sim::save::intern_static_name`, so the `&'static str` identity is
  preserved), and round-trip tests for tags 6-13 plus a typed error for an
  unknown ability id.
- **AUD-030-N2** — `apps/bench/src/gpu_bench.rs:1197` constructed
  `wc3_render::Scene` without the new `eye` field (E0063). Fixed by forwarding
  `scene.eye`.

**Disk failure mode (found by the gate itself, fixed in the gate).** Gate run 4
lost the shared filesystem mid-phase: `wc3-mpq` failed after 158 s, every later
member failed in 0-1 s with an empty log, and the raw log showed
`scripts/test-all.sh: line 148: … No space left on device`. That is not a code
failure and reporting it as twenty member failures is dishonest. The gate now:
checks free space against `H2_TEST_ALL_MIN_FREE_KB` (default 1.5 GB) before
every job, and converts a failed job whose log contains
`No space left on device` into `TEST_ALL_ABORT reason=disk` with exit code 3
(distinct from 1 = tests failed, 2 = bad usage) and the log tail. Evidence of
the old behaviour is `analysis/aud030-full-gate4.log` (local, gitignored).

**Environment incident (recorded, not hidden).** The first full run hit
`No space left on device` (mold: "Disk full?") at 99 MB free; that was caused by
the 21 GB shared `target/` plus stale artifacts, not by the gate. Reclaimed:
`target/debug/incremental` (1.9 GB; `CARGO_INCREMENTAL=0` makes it dead weight)
and `target/debug/examples` (2.9 GB; rebuilt on demand) → 4.6 GB free. Cargo's
fingerprints for packages whose `.rmeta` were truncated by the full disk were
cleared with `cargo clean -p` for `wgpu-types`, `hashbrown`, `bitflags`,
`bytemuck`, `foldhash`, `allocator-api2`, `equivalent`, `naga-types`, after
which `cargo check --workspace` was rc=0 again.

## AUD-031 — error-model boundaries

Documented in `docs/audit/ERROR_MODEL.md`: the three boundaries (external
parsers → typed recoverable errors; JASS → diagnostics + `NativeStatus`; UI
headless → recorded/unsupported status), the exact error type per subsystem,
and the criterion "no external input path depends on a panic or an
indistinguishable default".

Cheap enforcement added:

- `Engine::unimplemented_natives()` (`crates/wc3-re/src/engine.rs:143`) and the
  boot log line `GAME_JASS_UNIMPLEMENTED natives=N calls=M top=…`
  (`crates/wc3-game/src/game.rs:1663`). Before this, `Runtime::unimplemented`
  was collected and **never read outside the VM**: an unknown native returned a
  typed default and left no product-visible trace.
- Test `crates/wc3-re/src/jass/vm.rs::tests::an_unknown_native_is_recorded_in_the_diagnostics`.
- The shared corpus (AUD-025) already asserts typed errors for the five parser
  entry points, which is the external-input half of the criterion.

```
$ scripts/h2-cargo test -p wc3-re --lib unknown_native
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 83 filtered out
```

Residual (documented, not hidden): ASTC/texture validation uses `String`
errors (structured, never matched by callers); `default_return` is deliberate
script compatibility; headless UI input stays unsupported per AUD-007.

## AUD-032 — global/session state

`docs/audit/GLOBALS.md` is the full inventory (scan command included) with four
classes: immutable process-wide data, process-wide caches, session state,
test-only. Only one entry was genuinely session-dependent and wrong:

**JASS seat default leaked between sessions.**
`crates/wc3-re/src/jass/vm.rs` held `DEFAULT_LOCAL_PLAYER` and
`DEFAULT_LOCAL_PLAYER_ANY` as process-wide atomics. A campaign mission sets the
human seat (Human01 is seat 2, not 0) before boot because `main` runs during
boot; nothing reset them. A melee map booted afterwards inherited seat 2, so
`GetLocalPlayer() == Player(0)` was false and every unguarded script block was
silently dropped.

- Fix: `reset_local_player_default()` (`vm.rs:540`) restores seat 0 / no
  wildcard.
- Call site: `attach_map_runtime` (`crates/wc3-game/src/game.rs:1610-1628`)
  resets before booting any map that is not under `Maps\Campaign`
  (`map_file_is_campaign`, now the path form of the existing
  `map_path_is_campaign`). Campaign maps keep the seat their layer installed.
- Test: `vm.rs::tests::local_player_default_is_session_state_and_resets`.

Documented limitation (not "fixed" by guessing): the campaign session globals
(`REGISTRY`, `ACTIVE`) have `clear_active_campaign()` but no production caller
in this tree; the campaign module is exercised by its own tests only. The
inventory records the required teardown for when the menu wires the campaign.

```
$ scripts/h2-cargo test -p wc3-re --lib local_player
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 83 filtered out
```

---

## Gates for this lane

<!-- gate-results -->

## Residual risks / next

- Repo-wide `cargo fmt --check` is not clean (pre-existing: `game.rs` alone has
  ~100 diff hunks, `save.rs`/`vm.rs`/`bench` have unrelated hunks). Files owned
  by this lane are rustfmt-clean (`texture.rs`, `sdl3.rs`, `wc3-replay`,
  `input_limits.rs`); a repo-wide format commit belongs to a dedicated pass, not
  to this lane, because it would conflict with the concurrent lanes.
- Cross-target Android checks are `SKIP` for armv7/i686/x86_64 on this host (no
  NDK linker); the shipping `aarch64-linux-android` ABI is checked as the host
  target. Installing an NDK would turn those rows into real checks.
- Fuzzing with per-input time/allocation budgets (AUD-025) is not run; the
  deterministic negative corpus plus crate-local cap tests are the evidence
  this lane provides.
- The `target/` directory is shared by all lanes; the disk incident above shows
  it needs periodic pruning (incremental + examples are safe to drop).

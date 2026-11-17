# Error-model boundaries (AUD-031)

Registry: `docs/audit/WC3RUST_CODE_AUDIT.md` (AUD-031, P4).
Audit revision: `b64b9b6`; boundaries documented and enforced by the AUD-031 lane
(LEDGER 493).

The criterion this document answers:

> No external input path depends on a panic or on a default value that the
> caller cannot distinguish from a legitimate result.

The repository has three kinds of boundary, and each gets one error model. A
caller is always able to answer "was this input rejected, ignored, unsupported
or accepted?"

## 1. External parsers — typed recoverable errors

Untrusted bytes never reach a `panic!`, `unwrap()` on input, or a silent zero.
Each parser owns an error enum; the enum variant names the reason, and the
message names the limit or field.

| Subsystem | Entry point | Error type | Notable variants | Consumer behaviour |
| --- | --- | --- | --- | --- |
| MPQ archive / map | `MpqArchive::open`, `read`, `stream` (`crates/wc3-mpq/src/lib.rs:423,968,1003`) | `MpqError` (`lib.rs:186`) | `BadMagic`, `UnsupportedVersion`, `Corrupt(&'static str)`, `SizeLimitExceeded { requested, limit }`, `StaleIndex` | A bad archive is reported as a missing asset with the error text; a map that fails to open does not abort the boot |
| AVI (cinematics) | `Avi::from_bytes` (`crates/wc3-bink/src/avi.rs:165`) | `AviError` (`avi.rs:22`) | `NotAvi`, `Truncated { what }`, `InvalidHeader { what, len, need }`, `Limit { what, limit }`, `Missing(&'static str)`, `Io` | Movie playback is skipped, the playing field keeps running |
| ASTC container | `astc::decode`, `AstcFile::parse` (`crates/wc3-render/src/astc.rs:105,210`) | `Result<_, String>` | message names the cap (`side limit`, `texel count`, `payload is N bytes, expected M`) | Texture is not uploaded; the caller keeps the CPU `TextureImage`/fallback path |
| Compressed mip levels | `CompressedImage::validate` (`crates/wc3-render/src/texture.rs:545`) | `Result<(), String>` | level index + dimensions + expected/actual bytes | Same as ASTC: image rejected before upload |
| Save file | `Sim::from_save`, `from_save_path` (`crates/wc3-sim/src/save.rs:5460,5487`) | `SaveError` (`save.rs:207`) | `BadMagic`, `UnknownVersion`, `LengthMismatch`, `Checksum`, `Oversize { len, limit }`, `InvalidIdSpace { field, value, detail }`, `TrailingBytes`, `BadTag` | Resume reports `GAME_RESUME_FAILED err=<variant>` and starts a fresh match; state is not partially mutated |
| Replay / network | `Replay::decode`, `NetSession` reader (`crates/wc3-sim/src/net.rs:1429,1974`) | `NetError` (`net.rs:60`) | `BadMagic`, `UnknownVersion`, `LengthMismatch`, `Checksum`, `InvalidValue { field, detail }`, `Protocol`, `Truncated` | Desync/abort is distinguishable from a normal disconnect |
| Audio FFI | `Sdl3Backend::write` (`crates/wc3-audio/src/engine/sdl3.rs:267`) | `AudioError` (`crates/wc3-audio/src/lib.rs:86`) | `Backend(String)` carrying the SDL error; `Unsupported`, `Corrupt` for decoders | `AudioEngine` counts `backend_errors` and keeps the render loop alive |

Rules that hold across this layer:

- Lengths/counts are checked **before** allocation, and the error names the cap
  (see `docs/audit/INPUT_LIMITS.md` for the table).
- A parse either returns the full value or an error; there is no "best effort"
  struct with zero-filled fields.
- The only `String`-typed errors are the two texture validators, whose messages
  are structured (level, dimensions, expected/actual). They are reported, not
  matched on; a caller that needs to branch on a cause uses the ASTC entry
  point, which fails first.

## 2. JASS — diagnostics, never an indistinguishable default

Two facts force a diagnostic model instead of `Result`:

1. Retail map scripts are not written by us: returning `Null`/throwing on an
   unknown native would break scripts that run today.
2. `main` runs during boot, before any UI exists to show an error.

So the VM records the *fact* and keeps executing:

| Signal | Where | Meaning |
| --- | --- | --- |
| `Runtime::unimplemented: BTreeMap<String, u64>` | `crates/wc3-re/src/jass/vm.rs:797`; incremented at the call site (`vm.rs:945`) | Native name → call count. The VM still returns the declared default (`Int(0)`, `Bool(false)`, …), which is script-compatible but **not** evidence that the call did anything |
| `Runtime::errors: Vec<String>` | `vm.rs:801` | Script-level diagnostics (invalid location, wrong handle type, …) raised by handlers |
| `Engine::script_errors` | `crates/wc3-re/src/engine.rs:125` | Boot-time errors, including unmatched script entities |
| `NativeStatus` | `crates/wc3-sim/src/jass/native_status.rs:37` | `Implemented`, `Partial`, `RecordedOnly`, `NoOp`, `Unsupported` — the audited classification behind `AUD-016` |

Boundary enforcement added by this lane: `Engine::unimplemented_natives()`
(`crates/wc3-re/src/engine.rs:143`) and the boot log line
`GAME_JASS_UNIMPLEMENTED natives=N calls=M top=name:count,…`
(`crates/wc3-game/src/game.rs:1663`). Before this, the map was collected and
never read, so an unimplemented native was only discoverable by reading the VM
struct; now every map boot states how many natives it could not execute.

Regression tests:

- `crates/wc3-re/src/jass/vm.rs::tests::an_unknown_native_is_recorded_in_the_diagnostics`
  — a script calling an unknown native leaves `unimplemented[name] == 1`.
- `crates/wc3-sim/tests/aud016_native_status.rs` — the registry classifies every
  native; `Unsupported > 0` is asserted rather than hidden.
- `crates/wc3-sim/tests/audit_sim2.rs:129-166` — invalid handles/locations are
  diagnostics in `vm.errors`, not silent coordinate `(0,0)`.

The sim-side VM (`wc3-sim/src/jass`) follows the same rule: `JassVm::errors`
carries the diagnostic and the handler returns the documented default only for
*valid* calls.

## 3. UI / headless — recorded or unsupported status

The HUD is the one layer with legitimate "nothing happened" answers, so it
reports which one:

| Situation | Status | Evidence |
| --- | --- | --- |
| Click on a card slot that the compact layout did not create | not interactive (zero/disabled rect, no hit target) | `AUD-001` fix + `crates/wc3-game/tests/hud_layout.rs` |
| Dialog/menu button in a headless run | no input source by design — documented unsupported, not a silent success | `AUD-007` registry row ("button clicks have no headless input source — documented") |
| JASS dialog state driven without an engine emitter | registered as `Partial`, visible in `NativeStatus` | `AUD-006` row + `jass_natives_*_without_an_engine_emitter_is_registered_and_partial` |
| Checkpoint skipped because the match ended / no map | explicit log with reason (`GAME_CHECKPOINT_SKIPPED reason=finished\|no_map`) | `apps/android-native/src/lib.rs:868-893` |

## Enforcement summary (cheap checks that keep the boundary honest)

```
# 1. Every external parser rejects malformed input with a typed error.
scripts/h2-cargo test -p wc3-game --test input_limits --features movie_decoder   # 7/7

# 2. Unknown JASS natives are diagnosable, not silently executed.
scripts/h2-cargo test -p wc3-re --lib unknown_native                            # 1/1

# 3. Audit registry classifications stay complete.
scripts/h2-cargo test -p wc3-sim --test aud016_native_status
```

## Residual weaknesses (known, not hidden)

- ASTC/texture validation returns `String`, not an enum. The message is
  structured and is never matched by callers, but a programmatic consumer
  cannot branch on the cause without parsing text. A typed
  `TextureError { OversizeSide, Overflow, PayloadMismatch { .. } }` is the
  natural upgrade if the asset pipeline ever needs it.
- `Runtime::default_return` deliberately returns a typed default for an
  unimplemented native. The diagnostic map is the contract; a script that reads
  the value cannot tell. This is the price of running unmodified retail
  scripts, and it is now logged at boot.
- Headless UI has no synthetic input layer (AUD-007). Adding one is a feature,
  not an error-model fix; until then the limitation is recorded in the
  registry instead of being papered over with silent defaults.

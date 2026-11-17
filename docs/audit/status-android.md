# Android release + Adreno tooling audit — AUD-014 / AUD-015 / AUD-026 / AUD-029

Harness² ledger: **492**. Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md`
§0/0.1/0.2. Repo: `herofolends-rs`, branch `main`, base revision
`a78d4d80a12e5b2dbc8b3cb805a5edbb47fb7204` (2026-11-17). Fix commit:
`3966c3fa95b8ade2c3c8977fd20a7daebeab7456` (code + scripts); this document is a
follow-up commit.

Environment (observed): RMX3709, Android 16 / API 36, aarch64 (Termux),
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1 (5f94df478 2026-08-27)`,
`aapt2 2.20-android-16.0.0_r4`, `apksigner 0.9`. Every cargo invocation went
through `scripts/h2-cargo` (single-slot wrapper); the APKs were built by
`scripts/build-apk.sh`.

| ID | Status | Where | Regression tests / evidence |
| --- | --- | --- | --- |
| AUD-014 | **FIXED** | `apps/android/app/src/main/AndroidManifest.xml`, `scripts/build-apk.sh` | Release APK dump has no `debuggable` (badging + xmltree, sha `0661a7a9…`); debug APK has `debuggable=true` (sha `cd86414b…`); link-time gate refuses a release manifest with the attribute |
| AUD-015 | **FIXED** | `crates/wc3-adrenotools/src/linker.rs`, `proc_maps.rs`, `aarch64.rs`, `bcn.rs` | `aud015_*` 7 tests + on-device permission map before/after (`r-x` → `r-x`, pre-fix `r-x` → `rwx`); `linker_symbols_resolve` passes |
| AUD-026 | **FIXED** (contract proven on the pinned SDL source; guard added) | `apps/android-native/src/lib.rs` | SDL 3.5.0 source table below; `SDL_IsMainThread` guard + deferred `PENDING_CHECKPOINT`; stress log checker self-test PASS; device scenario added |
| AUD-029 | **FIXED** | `scripts/release_security_summary.py`, `scripts/build-apk.sh`, `dist/` | Release gate PASS with debuggable/signing/cleartext/exported/permissions/ABI/libs/symbols; the same gate returns rc=1 on the pre-fix APK |

## AUD-014 — `android:debuggable="true"` was hardcoded in the main manifest

### Reproduction (pre-fix artifact, not source)

APK built at 13:29 before the change (`build/android-p0/herofolends-p0.apk`,
sha256 `d9a5810940cbdf8465f2a6828b92633d05481adcdb3a21fd3e1b58ff4b3c7eba`):

```text
$ aapt dump badging build/android-p0/herofolends-p0.apk | grep -i debuggable
application-debuggable
$ aapt2 dump xmltree --file AndroidManifest.xml build/android-p0/herofolends-p0.apk | grep debuggable
        A: http://schemas.android.com/apk/res/android:debuggable(0x0101000f)=true
```

### Fix

- `android:debuggable` removed from `apps/android/app/src/main/AndroidManifest.xml`.
- This project is packaged by `aapt2`, not Gradle. The build type therefore
  selects aapt2's `--debug-mode` — the packaging equivalent of the Gradle debug
  build type — in `scripts/build-apk.sh [--release|--debug]` (default release).
  Release links without the attribute (absent = false); debug links with it.
- `build-apk.sh` refuses to start if the main manifest contains
  `android:debuggable`, then re-reads the **linked** `base.apk` with
  `aapt2 dump xmltree` and aborts if the flag disagrees with the build type.
- Artifacts are separated: `build/android-p0/release/` and
  `build/android-p0/debug/`.

### Verification (from the signed APKs)

| Artifact | sha256 | `aapt dump badging` | `aapt2 dump xmltree` | Summary gate |
| --- | --- | --- | --- | --- |
| release | `0661a7a9018812cf62cacc91af3fcca6c8ca5a632e47a11d429407b654faa988` | no `application-debuggable` | 0 lines matching `:debuggable(` | PASS (`debuggable=false`) |
| debug | `cd86414b415fcb12c6ca18e56a5d367b3879796411c9803a054a86969368d107` | `application-debuggable` | `:debuggable(0x0101000f)=true` | PASS (`debuggable=true`) |

Both APKs package the same native library (`libmain.so` sha256
`4a6f9fe703c8eaac39bb293b640f211f2f5783ddaf731d7cf9709cba8d29bcf9`, verified
with `unzip -p … | sha256sum`), so the only difference is the build type.

## AUD-029 — automated release security summary

`scripts/release_security_summary.py` reads only the signed APK (badging,
compiled manifest, zip directory, signing block). `scripts/build-apk.sh` runs it
after signing, prints the report to stdout (the CI entry point; this repo has no
GitHub workflow file) and writes `dist/security-summary-<type>.json` + `.txt`.

Release summary (observed, `dist/security-summary-release.txt`):

```text
debuggable     : manifest=False badging=False
signing        : verified=True schemes={'v1': False, 'v2': True, 'v3': True, ...}
                 dn=CN=Android Debug, O=Android, C=US debug_key=True
cleartext      : allowed=False (manifest)
permissions    : none requested
abi filters    : ['arm64-v8a']
exported       : activity:org.libsdl.app.SDLActivity (launcher)
native libs    : 8 arm64-v8a libraries (libmain, libSDL3, libiconv, libc++_shared,
                 libvulkan, libdrm, libzstd, libz)
gate           : PASS
  warning: release APK is signed with the Android debug key
  warning: native libraries with a full symbol table: lib/arm64-v8a/libmain.so, lib/arm64-v8a/libSDL3.so
```

Hard gates (exit 1): release debuggable, release cleartext, unexpected ABI,
`apksigner` verification failure, debug APK *not* debuggable. Warnings (exit 0,
recorded in JSON): debug signing key, exported non-launcher components,
requested permissions, unstripped libraries, targetSdk drift.

Pre-fix proof that the gate catches AUD-014: running the summary on the old
APK printed `SECURITY GATE FAILED: release APK is debuggable` and exited 1
(`dist` output redirected to a temporary directory for that run). Post-fix both
build types exit 0.

`android:usesCleartextTraffic="false"` is now explicit in the manifest (it was
already the targetSdk=35 default); the summary reports the value and its
source, so a future edit that flips it fails the release gate.

## AUD-015 — Adreno linker made a whole mapping RWX and trusted the first `BL`

### Reproduction (pre-fix, on-device test)

A temporary harness called the pre-fix `find_loader_dlopen(dlopen_addr)` and
compared `/proc/self/maps` before/after:

```text
AUD015_PROTECTIONS before r=true w=false x=true after r=true w=true x=true found=Some(0x7726647298)
thread 'linker::tests::…' panicked: scanning must not change the dlopen mapping protections
left: (true, false, true)   right: (true, true, true)
rc=101
```

The whole 4 KiB `r-xp` page of
`/apex/com.android.runtime/lib64/bionic/libdl.so` became `rwxp` and stayed that
way. r2 on the same image (radare2, device file):

```text
18  0x0000101c 0x0000101c WEAK FUNC 36 dlopen
2   0x00001298 0x00001298 WEAK FUNC 16 imp.__loader_dlopen
0x00001030  bl sym.imp.__loader_dlopen
```

So the first `BL` on Android 16 targets a PLT stub (`+0x1298`) in libdl, not
the linker itself; the old code accepted it only because *any* mapped target was
accepted.

### Fix

- `make_rwx` is deleted. Reading uses `ReadableWindow`: the normal `r-x` mapping
  gets **no** `mprotect`; an execute-only (`--x`) mapping gets `READ` on the
  minimal page window containing the scanned bytes, and `Drop` restores the
  exact original `r/w/x` flags on every exit path (including errors). The
  pre/post test now reports `r-x` → `r-x`.
- Scan bounded to `DLOPEN_SCAN_BYTES = 256` from the `dlopen` entry, stops at
  `RET` (`aarch64::is_ret`), and skips BL targets that are unmapped or not
  executable. BL decoding is the architectural `decode_bl_target` (opcode mask +
  sign-extended imm26).
- Target validation (`validate_loader_target`): a candidate must be executable
  and either
  1. inside a linker mapping (`ld-android.so`, `linker64`, `linker`,
     `libdl_android.so`), or
  2. a PLT stub in the caller's own library with the exact four-instruction
     `adrp x16 / ldr x17,[x16,#off] / add x16 / br x17` shape, whose GOT slot is
     readable and either unresolved (zero) or points into a linker mapping.
- `dlsym(libdl, "__loader_dlopen")` is preferred when the platform exports it
  (Android 16 resolves it into `linker64`); it goes through the same mapping
  validation. On older linkers that hide the symbol, the bounded scan/PLT path
  is used.
- Fail closed: any rejected layout traces
  `linker layout rejected (api=…, supported 28..=36): …` and returns `None`;
  `driver.rs` already maps that to `DriverError::LinkerUnavailable`. No fallback
  call and no widened mapping.
- Supported versions are code constants: `MIN_SUPPORTED_API = 28` (hidden
  `__loader_*` entry points) and `MAX_VERIFIED_API = 36` (verified on this
  Android 16 device). A newer layout is accepted only if the same runtime
  fingerprint checks pass; it is not version-gated.
- Adjacent W^X hardening in `bcn.rs` (same audit scope): the patch page is now
  mapped `RW`, written, then sealed `RX` before any branch targets it; the
  patched `clear_result` page is made `RWX` only for the single word write and
  restored to `RX` immediately.

### Verification

- `scripts/h2-cargo test -p wc3-adrenotools` → **35 passed / 0 failed**, rc=0.
  New tests: `aud015_scan_accepts_a_linker_branch`,
  `aud015_scan_skips_foreign_and_non_executable_targets`,
  `aud015_scan_stops_at_ret_and_at_the_window_bound`,
  `aud015_same_mapping_target_is_returned_for_plt_validation`,
  `aud015_decodes_the_android16_plt_stub` (GOT slot 0x2140 from the real
  Android 16 words), `aud015_linker_mapping_fingerprint`,
  `aud015_scanning_leaves_mapping_protections_unchanged` (on-device),
  `aarch64::tests::recognizes_ret_variants_only`.
- On-device permission evidence:
  `AUD015_PERMISSIONS before r=true w=false x=true after r=true w=false x=true
  path=Some("/apex/com.android.runtime/lib64/bionic/libdl.so")
  target=Ok(0x79e024ae14)` — the target is validated into `linker64`.
- Turnip path unchanged: `linker_symbols_resolve` (asserts `load_status()` and
  `real_dlopen_ext().is_some()`) passes; `driver.rs`, `hooks.rs`, `kgsl.rs`
  were not modified.

### Limitation

No Adreno BCn device patch was executed by this lane (device installation is
the operator's). The BCn change is a page-permission change around the existing
verified patch bytes; its unit tests (`generated_patch_is_byte_identical_to_donor_blob`,
`patch_is_smaller_than_a_page`) pass, but the on-device Turnip/BCn smoke remains
an operator-run check.

## AUD-026 — `AtomicPtr<Game>` in the SDL event watch: thread/lifetime contract

Pinned source: SDL **3.5.0**, `donors/SDL`, commit `4ca1b1c`. The contract was
verified line by line before keeping any pointer access:

| Question | Source evidence | Result |
| --- | --- | --- |
| Which thread delivers app lifecycle events? | Java callbacks only enqueue (`SDL_android.c:1559-1565` `nativePause`, `2738-2794` `Android_SendLifecycleEvent`); `SDL_PumpEventsInternal` asserts `SDL_IsMainThread()` (`SDL_events.c:1549-1552`) and calls `Android_PumpEvents(0)` (`1570`); `Android_PumpEvents` handles the queue (`SDL_androidevents.c:214-236`) → `Android_HandleLifecycleEvent` (`179-200`) → `SDL_OnApplicationWillEnterBackground` (`SDL_video.c:6266-6281`) → `SDL_PushEvent` → `SDL_CallEventWatchers` (`SDL_events.c:1849-1870`) | Watcher runs synchronously on the thread that calls `SDL_PollEvent` (the game loop thread) |
| Can `SDL_RemoveEventWatch` overlap an in-flight callback? | `SDL_DispatchEventWatchList` holds `SDL_event_lock` across every callback (`SDL_eventwatch.c:42-80`); `SDL_RemoveEventWatchList` takes the same lock (`112-134`) | No: removal blocks until dispatch finishes, and after it returns the watcher is out of the list |
| Can the callback overlap game mutation or drop? | Callback is invoked inside `SDL_PollEvent` on the game-loop thread; removal happens before the pointer is cleared, which happens before `Game` drops | No for the save path. Not every `SDL_PushEvent` is on the main thread (`nativeFocusChanged` runs on the Java UI thread), so the callback must not assume it |

Code changes in `apps/android-native/src/lib.rs`:

- The save branch checks `SDL_IsMainThread()` (public SDL3 API). On the main
  thread it keeps the synchronous save (required before `BLOCK_ON_PAUSE` blocks
  the pump); off the main thread it sets `PENDING_CHECKPOINT` instead of
  dereferencing the pointer, and the loop writes the checkpoint after the pump.
- The exact contract (version + file:line) is documented at the statics and at
  the install/teardown sites. `GAME_FOR_CHECKPOINT` is cleared after
  `SDL_RemoveEventWatch` and before `Game` drops; the startup log now records
  `main_thread=<bool>` next to `watch=1`.

### Device-cycle stress scenario (scripted log check)

- `scripts/device-lifecycle-stress.sh`: N cycles of launch → HOME (pause) →
  resume, with `am force-stop` + relaunch every fifth cycle; collects
  `logcat -s Herofolends:I` plus the crash buffer. It never installs or replaces
  the APK (the operator owns installs).
- `scripts/check-lifecycle-stress-log.sh`: requires the watcher on the main
  thread, ≥ N background/foreground transitions, ≥ ⌈N/2⌉ checkpoint writes
  (coalesced pauses are allowed), ≥ one resume per force-stop, and zero
  `GAME_FATAL`/`FATAL EXCEPTION`/`signal 11|6`/`Abort message`/
  `GAME_CHECKPOINT_FAILED`/`GAME_RESUME_FAILED`.
- `scripts/test-lifecycle-log-check.sh` proves the checker offline: a healthy
  3-cycle log passes; the same log with a crash or with checkpoints removed
  fails. Observed: `LIFECYCLE_LOG_CHECK_SELFTEST PASS (ok=pass crash=fail
  missing-checkpoint=fail)`, rc=0.
- The device run itself is operator-executed (this lane does not touch the
  phone); the scenario and its acceptance check are in the tree and ready.

## Files changed

```
apps/android/app/src/main/AndroidManifest.xml     debuggable removed, cleartext explicit
apps/android-native/src/lib.rs                    SDL_IsMainThread guard, PENDING_CHECKPOINT, contract docs
crates/wc3-adrenotools/src/linker.rs              no RWX, bounded scan, target validation, fail closed
crates/wc3-adrenotools/src/proc_maps.rs           mapping path fingerprint (Region.path)
crates/wc3-adrenotools/src/aarch64.rs             is_ret
crates/wc3-adrenotools/src/bcn.rs                 W^X patch page, RWX restore after the single write
scripts/build-apk.sh                              build types, manifest gate, summary emission
scripts/release_security_summary.py               AUD-029 summary + hard gates
scripts/device-lifecycle-stress.sh                AUD-026 device scenario
scripts/check-lifecycle-stress-log.sh             AUD-026 log acceptance
scripts/test-lifecycle-log-check.sh               offline checker self-test
.gitignore                                        /dist/, __pycache__/
docs/audit/status-android.md                      this document
```

## Commands and results

| Command | rc | Result |
| --- | --- | --- |
| `scripts/h2-cargo test -p wc3-adrenotools -- --nocapture` | 0 | 35 passed, 0 failed |
| `scripts/h2-cargo build -p herofolends-android --release` | 0 | `Finished release` in 4m 35s; libmain sha `4a6f9fe7…` |
| `bash scripts/build-apk.sh --release` | 0 | APK sha `0661a7a9…`, summary gate PASS |
| `bash scripts/build-apk.sh --debug` | 0 | APK sha `cd86414b…`, summary gate PASS, `debuggable=true` |
| `python3 scripts/release_security_summary.py --apk <pre-fix APK> --build-type release` | 1 | `SECURITY GATE FAILED: release APK is debuggable` |
| `scripts/test-lifecycle-log-check.sh` | 0 | self-test PASS |
| `git diff --check` | 0 | no whitespace errors |

## Limitations / operator follow-ups

- The device cycle stress and the Adreno BCn smoke were not executed by this
  lane (no device installs). Both are scripted/ready; the owner runs them.
- Release and debug APKs use `apps/android/debug.keystore` (the existing build
  setup). The summary flags `debug_key=True`; a distribution build must use a
  production keystore, which is an owner key decision.
- `libmain.so` and `libSDL3.so` ship with a full symbol table; the summary
  reports it. Stripping is not part of AUD-014/AUD-029 and was not done.

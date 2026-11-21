# Lane K — platform, build, tooling, release gate (ledger 531)

**Audit:** WC3RUST_FULL_AUDIT_TZ_P0-P4, lane K (TZ §3.2, §5.1, §5.2, §5.13, §5.15, §5.16).
**Repository:** `~/src/wc3rust`
**HEAD audited (start = end):** `d7af459daf5c507d46c8a63741385e5d1091af26` (2026-11-18 05:57:18 +0300)
**Worktree at audit time:** dirty (183 paths, product lanes); findings that depend on working-tree state say so.
**Date:** 2026-11-18.
**No new tests were written.** Evidence = `file:line` + a one-shot probe under `analysis/full-audit-2026-11-18/probes/` + the exact output.

## Verdict

| question | answer |
|---|---|
| (a) can the pipeline install an APK that is not byte-identical to the audited source SHA? | **Yes.** Nothing in `scripts/**` reads `git rev-parse` or hashes the installed `base.apk`; the acceptance gate `apk.build.current_tree` passes on file existence (`K-01`). |
| (b) two builds from the same tree → same `.so`/APK hash? | **No for the APK**: `cp` + `zip` stamp the wall clock into the ZIP (proven, two hashes 2 s apart). **No for the `.so` across checkout paths**: absolute build paths are embedded. zipalign/apksigner *are* deterministic; d8 class order is *not* a source (disproved). |
| (c) `third_party/wgpu-hal` patch | **Minimal and sound** (9 lines, one file; 85/86 files byte-identical to crates.io 30.0.1). Timeout maps to `SurfaceError::Timeout` → `CurrentSurfaceTexture::Timeout`, which the app handles. It is a **no-op on Android < 11**, undocumented in the README it points at, and untested (`K-14`). |
| (d) lifecycle paths with a dead surface | Normal `Timeout/Occluded/Lost/Outdated` are covered; **any failure of `recover_surface` aborts the game** (`?` → `GAME_FATAL`), including the 1 s `ANativeWindow` publication window on resume (`K-06`). Campaign matches are never checkpointed (`K-04`). |
| (e) scripts that swallow a failure / accept a stale artifact | **Yes, several**: `final_report.py` static `rc=0` command list (`K-07`), `perf-gpu.sh` build pipeline masked by `tail` (`K-08`), `device_evidence.sh` newest-any-app screenshot (`K-12`), `game-selftest.sh` screenshot `cp` ignored, `build-apk.sh` mtime guard (`K-02`), `h2-cargo` queue (K-10). |

**Release recommendation: NO-GO for any QA/acceptance claim made through the current scripts.** The source-level fixes in lanes G–J can be correct while a stale APK is still installed and reported PASS. Install identity must be enforced before the next device verdict (§3.2).

## What ran, what did not

| check | state |
|---|---|
| `scripts/h2-cargo metadata --no-deps --offline` | **rc=0**, 31 workspace packages (`probes/K-RELEASE-GATE` context) |
| `scripts/h2-cargo clippy … -- -D warnings` | **rc=101** (`probes/K-BUILD-GATES`) |
| `rustfmt --check` (in-scope Rust, and the HEAD blob) | **rc=1** (`probes/K-QA-TRUTH` §7) |
| APK packaging/signing determinism | measured (`probes/K-REPRO-APK`) |
| stale-guard predicate | measured, live repo + fixture (`probes/K-STALE-GUARD`) |
| QA identity chain | measured by code predicate against the real APK (`probes/K-RELEASE-GATE`) |
| device run (install → first frame → menu → in-game → lifecycle stress) | **not run**: device runs are the tech-lead's (assignment rule 6). Reproducers provided. |
| `cargo test --workspace` / `test-all.sh` | **not run**: 30–70 min on the shared slot and the product lanes hold the critical-path APK; `test-all.sh` itself is audit-clean (see coverage). |
| `cargo clippy --workspace --all-targets --all-features` | not completed: the in-scope subset already fails; a workspace run would only enumerate more instances of `K-09`. |
| wgpu-hal unchanged files | checksum-level audit (`cmp` vs upstream), not line-by-line re-review — the risk surface is the patch. |

## Findings

Severity per TZ §6. `Status` per assignment rule 4. Evidence paths are under `analysis/full-audit-2026-11-18/probes/`.

**Non-duplication vs AUD-001…AUD-032.** No old ID is reused or re-worded. The closest neighbours are: `K-06` — a new error-propagation defect in `recover_surface` (the old lifecycle item was about density/config handling); `K-09` — a measured red `fmt`/`clippy` gate (the old test-coverage item was about missing tests); `K-11` — a path-traversal defect in an analysis extraction tool (the old MPQ items were parser bounds); `K-13` — Turnip *install-script* provenance (the old linker item was the RWX review). Each has an independent root cause and a probe in this lane.

### K-01 — P1 — release/QA gate cannot bind the APK to the audited source SHA — CONFIRMED

**Impact chain.** A stale or hand-picked APK can be installed and a device cycle can report PASS with that APK's hash, while the audited HEAD contains different code. This is exactly the 2026-11-17 failure (`analysis/device-qa-20261117/REPORT.md:19-26` — APK built 11:18, installed 11:20, HEAD `b64b9b6` at 11:25, 176-byte camera fix in the tree but not in the APK; §7 of that report calls the build stale).

**Evidence.**
- `scripts/qa/final_report.py:229-241` — the only "built from current tree" gate:
  ```python
  apk_sha = sha256(APK)
  gates.append(gate("apk.build.current_tree",
      "PASS" if apk_sha else "FAIL",
      f"APK built from current tree and installed: {str(apk_sha)[:12]}", ...))
  ```
  No `git`/tree/SHA input. `grep -rn "rev-parse\|git status" scripts --include='*.sh' --include='*.py'` → `NO_MATCH` (`K-RELEASE-GATE/observed.txt` §1).
- `scripts/qa/suite.py:72-79` — `--no-build`: `"ok": DEFAULT_APK.exists() or installed`; `:85-90` — `--no-install`: install phase `ok=True`. `Device.launch(install=False)` never hashes the installed package (`scripts/qa/device.py:384-418`; only `pm path` presence at `:54,63`).
- `scripts/qa/device_cycle.py:117` records `sha256(args.apk)`; `:124` launches with `install=False`. Same at `device_p0_cycle.py:175,181`. The JSON can state “APK sha256 X” while the process under test is an older install.
- `apps/android/app/src/main/AndroidManifest.xml:4-5` — `versionCode=1`, `versionName="0.1-p0"` are constants; `scripts/build-apk.sh` never rewrites them (`grep versionCode` → none), so two different builds are indistinguishable by package metadata.
- Probe output: `final_report.apk.build.current_tree -> PASS` for the on-disk APK with `dirty_paths=183`; the APK's own entries carry the build wall clock (`K-RELEASE-GATE/observed.txt` §3, §6).

**Fix direction.** Record `git rev-parse HEAD` + `git status --porcelain` + a hash of the build inputs in a build manifest written by `build-apk.sh`; embed it (asset) and require `sha256(build_manifest) == sha256(apk)` and `build.source_sha == audited_sha` in `final_report`/`device_cycle`; hash `/data/app/.../base.apk` after install and compare with the local artifact; bump `versionCode` from the commit count or a counter.

### K-02 — P2 — stale-native guard in `build-apk.sh` is mtime-only, misses vendored runtime code and `Cargo.lock`, and wedges on `examples/` — CONFIRMED

`scripts/build-apk.sh:69-76`:
```sh
stale_rs="$(find crates apps -name '*.rs' -newer "$MAIN_LIB" -print -quit …)"
stale_toml="$(find . -maxdepth 3 -name 'Cargo.toml' -newer "$MAIN_LIB" …)"
```
- **Misses** `third_party/wgpu-hal/**` (runtime code via `[patch.crates-io]`, root `Cargo.toml:44-46`) and `Cargo.lock`; the fixture in `K-STALE-GUARD/observed.txt` §2 shows a newer vendored file is not seen.
- **False positive**: any newer `*.rs` under `crates/` or `apps/`, including `examples/`, blocks packaging. Live: `crates/wc3-sim/examples/audit_i_dup.rs` is newer than `target/release/libmain.so` (14:02:45) → `build-apk.sh` exits 2 now, and `cargo build -p wc3rust-android --release` will not relink the cdylib (an example is not an input of the `[lib]`), so the gate stays red until a library file is touched.
- The guard is not a provenance check (mtime is copyable); a restored/`touch`ed artifact passes.

**Fix direction.** Hash the build inputs (source tree + `Cargo.lock` + vendored tree + rustc version) into a stamp file consumed by `build-apk.sh`; restrict the source set to the cdylib's package graph (`cargo build --build-plan`/`cargo metadata`, not a filesystem walk).

### K-03 — P2 — APK bytes are not reproducible from the same tree — CONFIRMED

- `scripts/build-apk.sh:120-143`: `cp` (mtime = now) then `zip -q -r` stores each entry's mtime. Probe (`K-REPRO-APK/observed.txt` §2-3): same payload, two runs 2 s apart → `a.zip 4959ca0b…` vs `b.zip 76617562…`; `libmain.so` entry timestamp `14:06:20` vs `14:06:24`. The shipped APK shows the same mechanism: `classes.dex`/`libmain.so` at `2026-11-18 14:02`, aapt2 resources at `1980-01-01`.
- `.so` across checkout paths: `strings target/release/libmain.so` contains `/data/data/com.termux/files/home/src/wc3rust/crates/wc3-game` and `/data/data/com.termux/files/home/.cargo/registry/…` (`K-REPRO-APK` §1); no `--remap-path-prefix`.
- **Not** a source: zipalign and apksigner are deterministic (identical hashes twice, `K-REPRO-APK` §4); d8 output is independent of class order (identical dex hashes, §5 — hypothesis disproved).
- Consequence: the TZ §3.2 gate “rebuild from a clean checkout of this SHA and compare APK SHA256” cannot succeed as written.

**Fix direction.** `SOURCE_DATE_EPOCH` + `zip -X`, `cp -p`/`touch -d @$SOURCE_DATE_EPOCH`, `-C strip=debuginfo`, `--remap-path-prefix=$PWD=/build`, and build metadata in a manifest.

### K-04 — P2 — campaign matches are never checkpointed on pause/background/process death — CONFIRMED (device reproducer pending)

- `apps/android-native/src/lib.rs:1032-1034`:
  ```rust
  if !game.melee { return; }
  ```
  and `crates/wc3-game/src/game.rs:906` `let melee = !campaign_path && …`.
- The lifecycle watcher's save path is only `save_checkpoint` (`lib.rs:196-210`); there is no other autosave.
- The acceptance basis is the Orc01 campaign chapter (`lib.rs:1656-1677`). Any Android process kill while backgrounded therefore loses progress since the last manual save.
- Reproducer (TL/device): start Orc01, play, HOME, `am kill org.wc3rust.p0` (or wait for the LMK), relaunch → no `GAME_RESUME`/autosave for the campaign.

**Fix direction.** Serialize the campaign state with the same `CheckpointMeta` contract (map stamp + phase + tick), or refuse to run a campaign in a process without a save path.

### K-05 — P3 — autosave map stamp is cached once per process, not per match — NEEDS_REPRO

- `lib.rs:996-1009`: `static STAMP: OnceLock<Option<MapStamp>>` computed from the first `map=` line of `game.assets.debug`.
- `crates/wc3-game/src/game.rs:4111,5230-5257`: `load_match`/`load_map` can replace the map in the same process from the menu.
- After a switch, the autosave is written with the **first** map's stamp; `restore_checkpoint` rejects it (MapStamp mismatch) → the autosave is silently discarded. Reproducer: menu → map A → background (autosave) → menu → map B → background → relaunch.

### K-06 — P2 — a failed surface recovery terminates the game instead of retrying — NEEDS_REPRO (device)

- `lib.rs:2023-2037` (lifecycle/resize), `2088-2102` (watchdog), `2125-2139` (lost/outdated): `recover_surface(…)?` propagates to `run()`, which returns `Err`; `SDL_main` logs `GAME_FATAL` and returns 1 (`lib.rs:2275-2281`).
- `current_native_window` gives up after **20 × 50 ms** (`lib.rs:1381-1397`); `create_android_surface` errors on a NULL window (`lib.rs:1401-1417`). On resume, the new `ANativeWindow` is normally published before `DID_ENTER_FOREGROUND`, but a 1 s budget plus a transient `configure` failure is enough to lose the session.
- Upstream SDL destroys the process on activity recreation (`SDLActivity.java:401` and `:474` `System.exit(0)`, hinted by `lib.rs:1499-1509`), and `onDestroy` calls `nativeQuit()` after `join(1000)` (`SDLActivity.java:752-769`) while the vendored acquire can block for 2 s (`native.rs:433-435`) — a narrow concurrent-teardown window.
- Reproducer: `scripts/device-lifecycle-stress.sh` (host adb; adb is present) or manual HOME→resume ×10 with `logcat -s wc3rust | grep P0_SURFACE/GAME_FATAL`.

**Fix direction.** Treat `recover_surface` failure as retryable: keep the loop alive with the old surface omitted, retry with backoff on the next frame/exposed event; only exit after N consecutive failures or an explicit quit.

### K-07 — P2 — QA gates PASS on source substrings, and the final report contains a static `rc=0` command list — CONFIRMED

- `scripts/qa/parity.py:552-580`: `jass.frame_loop` → `PASS` if the text `JassBridge` occurs in `crates/wc3-game/src/game.rs`; `582-617`: `hud.resources` → `PASS` if `ResourcePill::new` occurs ≥3 times and `"GOLD"/"LUM"/"SUP"` appear; `619-626`: `audio.wired` → `PASS` if `"wc3-audio"` is in `Cargo.toml` and `GameAudio`/`wc3_audio` appear. No frame, no sim state, no audio device is exercised.
- `scripts/qa/final_report.py:345-355`: the `commands` array is a literal with `"rc": 0` for `cargo check`, `cargo build`, `cargo test` — the script never runs them. This violates TZ §11 (“do not treat registry presence as implementation”; “do not swallow errors in QA scripts”) and produces acceptance evidence that cannot be trusted.
- Probe: `K-QA-TRUTH/observed.txt` §1-2.

**Fix direction.** Gates must execute the harness (`wc3-parity --gate …`) and parse its JSON; delete the static command list or generate it from actual `subprocess` results with captured rc.

### K-08 — P3 — `perf-gpu.sh` can benchmark a stale binary after a failed build — CONFIRMED

- `scripts/perf-gpu.sh:8` `set -u` (no pipefail), `:27` `if cargo build … | tee … | tail -5; then`, `:28` `grep -q '^error'` only. Probe: `false | tee /dev/null | tail -5` → status 0 and the `if` branch is taken (`K-QA-TRUTH` §3). A link/OOM failure whose log has no line starting with `error` is accepted; `:48` then runs the previous `target/debug/wc3rust-gpu-bench`.
- `scripts/perf-release.sh:44-49`: prints `rc=$?` per run but never aggregates; the script exits with the final `grep|tee` status, so two crashed benches still exit 0.
- `scripts/robustness-verify.sh:8,19-25`: hardcodes the checkout path, records `STEPn_RC=` in the log, then ends without `exit $rc` — the script always returns 0 and a red step is invisible to a caller.

### K-09 — P3 — the build gates required by TZ §5.1 are red and there is no CI — CONFIRMED

- `rustfmt --edition 2024 --check apps/android-native/src/lib.rs` → rc=1; the **HEAD blob** also fails (`K-QA-TRUTH` §7), so this is not only the dirty worktree.
- `H2_CARGO_WAIT_MAX_S=5 scripts/h2-cargo clippy -p wc3-core -p wc3-path --offline -- -D warnings` → **rc=101** (`crates/wc3-path/src/lib.rs:120` `collapsible_if`); the app subset also surfaces `wc3-mpq` lints (`K-BUILD-GATES/observed.txt`, logs).
- No `.github/` directory at all (`ls -d .github` → absent).
- Consequence: `cargo fmt --check` and `cargo clippy -D warnings` cannot be used as release conditions; formatting/lint regressions land silently.

### K-10 — P3 — `h2-cargo` FIFO is not strict and the memory floor is bypassed after 900 s — CONFIRMED (code)

- `scripts/h2-cargo:93-96`: `if ! flock -n 9; then queue_turn; …` — a job that finds the lock free takes it immediately, even if older tickets are waiting in `queue_turn` (each waiter sleeps 2 s at `:85`). `H2_CARGO_PRIORITY=1` only sorts ticket names (`:54-58`); it cannot beat a fresh `flock -n`.
- `:34-46` `wait_memory` logs “proceeding” and returns 0 after 900 s even when `MemAvailable < 3_200_000 kB`, contradicting the header contract at `:16` (“wait until MemAvailable >= MIN_AVAIL_KB before starting”) that the OOM fix depends on.
- Not exercised against the live queue (would compete with the critical-path APK); the control flow is deterministic.

### K-11 — P3 — `tools/storm_probe` path traversal and unbounded allocation on a hostile MPQ — CONFIRMED

- `tools/storm_probe.cpp:14-22` `sanitize()` strips only leading `.`/`/` and replaces `: * ? " < > |`; internal `..` survives. `:44-46` `fs::path out = outdir / sanitize(name); fs::create_directories(out.parent_path());` → a listfile name `a/../../x` writes outside `outdir`.
- `:36` `std::vector<unsigned char> data(size)` with `size` from MPQ metadata (up to 4 GiB) → OOM; `--limit` only affects listing.
- `tools/storm_dbg.cpp:31` has the same unbounded `vector(size)` and ignores `SFileGetFileInfo` return codes.

### K-12 — P3 — device evidence can attach a screenshot from another run/app — CONFIRMED

- `scripts/game-selftest.sh:66-72` `shoot()`: `screencap` failure returns 1, but the `cp` and `chown` return codes are ignored (`>/dev/null 2>&1` without `|| return 1`); `visual_check.py` then evaluates whatever old `analysis/device-N-*.png` exists.
- `scripts/qa/device_evidence.sh:82-86`: `newest=$(ls -t /storage/emulated/0/Pictures/Screenshots/*… | head -1)` with no `-newermt` window against `started`; the JSON records it as this run's screenshot.
- `K-QA-TRUTH/observed.txt` §5-6.

### K-13 — P3 — Turnip setup can silently ship a driver that is not used, and hardcodes the package — CONFIRMED

- `scripts/setup-turnip.sh:47` `patchelf --set-rpath '$ORIGIN' "$TMP/$name" || true` — a failure leaves a system lib without the rpath and the script still prints “installed”.
- `:11-13,52-61` hardcode `org.wc3rust.p0`; `apps/android-native/src/lib.rs:684-700` also hardcodes `/data/data/org.wc3rust.p0/files/turnip/...` while the rest of the app has a legacy-package fallback (`lib.rs:775-800`). An install under the legacy id silently falls back to the vendor driver (`GAME_VULKAN turnip=0`); only `game-selftest.sh --allow-vendor` makes that non-fatal (`:59-64`).

### K-14 — P4 — the vendored wgpu-hal patch is minimal/sound but undocumented, untested, and a no-op below API 30 — CONFIRMED

- Coverage: `85/86` vendored files are **byte-identical** to `~/.cargo/registry/src/…/wgpu-hal-30.0.1` (`.cargo_vcs_info.json`: `40f4a34ebaf56f9a046231f54125ad046239d3f3`); the only difference is `src/vulkan/swapchain/native.rs` (`K-COVERAGE/gen.py`, `cmp`).
- Diff (9 lines): cap `timeout_ns` at 2 s when `cfg!(target_os = "android")` (`native.rs:427-435`).
- Semantics verified: `VK_TIMEOUT` → `hal::SurfaceError::Timeout` (`native.rs:496-506`); wgpu-core maps it to `Status::Timeout` (`wgpu-core-30.0.1/src/present.rs:364`); the app handles `CurrentSurfaceTexture::Timeout` (`lib.rs:2075-2116`). The acquire semaphore index only advances after success (`native.rs:524-527,676-684`), so a timed-out acquire retries the same semaphore — correct.
- Limits: `native.rs:446-448` resets the timeout to `u64::MAX` for `android_sdk_version < 30`, so minSdk 24–29 devices keep the infinite hang the patch was written for. Fence wait (`:469-473`) plus acquire can consume 2 × 2 s, not 2 s.
- `Cargo.toml:44-46` says “see `third_party/wgpu-hal/README.md`”, but that README is upstream and never mentions the patch (`grep -i acquire/README` → generic text only). No test/CI covers the patch (TZ §5.1 requires Android coverage).

### K-15 — P3 — numeric/grid input validation gaps — CONFIRMED

- `crates/wc3-core/src/lib.rs:64-66` `Fixed::from_int`: `v << 16`; release builds have `overflow-checks = false` (root `Cargo.toml:50-54`), so `Fixed::from_int(32768)` wraps to `i32::MIN` instead of saturating/rejecting (map/world coordinates near the +32767 boundary). `from_f32` (`:68-70`) maps NaN→0 and ±Inf→i32::MAX/MIN silently.
- `crates/wc3-path/src/lib.rs:17-24` `Grid::new`: `vec![true; width*height]` with no bound; `world_to_cell` divides by `cell_size` without rejecting 0/NaN (`:50-55`). A map dimension near `u32::MAX` or `cell_size=0` aborts/allocates unbounded.
- These are the §5.2 “map dimensions and multiplication overflow” and “finite/NaN policy” items.

### K-16 — P2 — the release depends on untracked binaries with no checksum or provenance — CONFIRMED

- `scripts/build-apk.sh:53` `SDL_LIB="$ROOT/third_party/sdl3-android-build/libSDL3.so"` and `:67` requires it; `:131-132` copies `third_party/vulkan-loader/libvulkan.so` if present; `:147-151` generates `apps/android/debug.keystore` on demand.
- None of these is tracked: `.gitignore:14-26` ignores `*.so`/`*.apk` and `third_party/sdl3*`/`turnip`; `git ls-files third_party` lists only `horizon`, `vulkan-loader` (LICENSE/README), `wgpu-hal`.
- Observed hashes (no expected value recorded anywhere): `libSDL3.so` `d63caf9e2ce401815550f101d714dbb7d318e38588d3732744e9778069ac29c5` (2 087 792 B, 2026-11-12), `libvulkan.so` `13d6b4eb5ea3ae6870425d8fe27f99ca88c21e2272cb7b23c94ea5caeca8cf4e` (601 520 B), `debug.keystore` `3042798b…`.
- The Rust side never calls `SDL_GetVersion`, so a different prebuilt SDL (the manual `SdlEvent` offsets assume SDL 3.5.0 `4ca1b1c`) is not detected; `grep SDL_GetVersion apps/android-native/src/lib.rs` → no match.
- Consequence: a fresh clone cannot reproduce the audited APK, and the SDL binary's identity is a comment, not evidence.

## Coverage

`analysis/full-audit-2026-11-18/FILE_COVERAGE.csv` — **228 rows** for this lane: `AUDITED 217`, `NOT_APPLICABLE 11` (launcher PNGs, licences/docs), `BLOCKED 0`.
Vendored wgpu-hal files are `AUDITED` by byte-identity to upstream 30.0.1 (the only risk surface is the one patched file, reviewed line-by-line). Java files are `AUDITED` as unmodified SDL 3.5.0 imports (single import commit, no local markers). Prebuilt `libvulkan.so` is untracked and therefore absent from the tree inventory; it is covered by `K-16`.

Subsystems: `android-native` (2), `android-java` (16), `apps-tools` (13), `build-scripts` (24), `qa-scripts` (34), `dev-tools` (27), `vendor-wgpu-hal` (86), `wc3-core` (2), `wc3-path` (2), `wc3-adrenotools*` (19), `build` (2).

## False positives / disproved hypotheses (TZ §11)

1. **“Java class order makes the DEX differ.”** Disproved: `d8` produced the same `classes.dex` hash for forward and reversed input order (`K-REPRO-APK` §5).
2. **“Signing is nondeterministic.”** Disproved: `zipalign` twice and `apksigner sign` twice produced identical hashes (`K-REPRO-APK` §4). The nondeterminism is the `zip` mtimes, not RSA.
3. **“The 176-byte camera fix is missing on HEAD.”** Disproved for source: `crates/wc3-game/src/render.rs` contains the 176-byte assert and `min_binding_size`; the device-QA crash was a stale-APK artifact (lane J owns the render detail; K owns the gate that allowed it).
4. **“The vendored wgpu-hal patch is a large fork.”** Disproved: 1 file, 9 lines, 85/86 files byte-identical to upstream.
5. **“The APK is malformed for 16 KB pages.”** Disproved: `readelf -lW libmain.so` shows `LOAD … Align 0x4000`; the device page size is 4096, SDK 36.
6. **“The build script packages a stale `libmain.so` silently.”** Not silently: `build-apk.sh:69-76` refuses on newer `.rs`/`Cargo.toml`. The defect is that the predicate is both too broad (`examples/`) and too narrow (`third_party/wgpu-hal`, `Cargo.lock`) — `K-02`.

## Blocked / limits

- **Device evidence** (install identity, first frame, menu, in-game, lifecycle stress) is blocked to this lane by assignment rule 6; reproducers: `K-01`/`K-06`/`K-04` text above, `scripts/device-lifecycle-stress.sh` (adb present), `scripts/game-selftest.sh` (H2_DEVICE_SLOT).
- **`cargo test --workspace`** not run (shared slot, ~30–70 min, product lanes hold the build). `scripts/test-all.sh` itself was audited and is the right runner.
- **Prebuilt `libSDL3.so`/`libvulkan.so`** have no source in the tree → `K-16`; code-level audit impossible, identity recorded by hash.

## Red-team notes for the tech-lead

1. **Do not accept any device PASS produced by `device_cycle.py`/`device_p0_cycle.py` until K-01 is fixed** — those scripts hash a file they did not install.
2. The 2026-11-17 report's own evidence (`REPORT.md:19-26`) is enough to invalidate that run; the same hole exists today at `final_report.py:232`.
3. Re-run the APK build at HEAD before the next verdict: `cargo build -p wc3rust-android --release` **and** `touch target/release/libmain.so` or remove the audit `examples/*.rs` from the guard's scope — right now `crates/wc3-sim/examples/audit_i_dup.rs` makes `build-apk.sh` exit 2.
4. If a fresh APK is built, record `sha256 + HEAD + dirty list` in the handoff; without it the next lane cannot tell whether it audited the same artifact.
5. The vendored wgpu-hal README claim in `Cargo.toml` is false today; either document the patch there or drop the pointer (K-14).
6. `clippy -D warnings` is red in `wc3-path`/`wc3-mpq`; if the release process claims a lint gate, it is not being run (K-09).
7. Campaign acceptance (Orc01) has no pause autosave (K-04) — device process death during a long mission loses the run.

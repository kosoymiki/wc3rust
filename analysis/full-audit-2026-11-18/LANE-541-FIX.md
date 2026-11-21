# Lane 541 — release gate + platform fixes (K-01, K-02, K-03, K-04, K-06, K-07, K-08, K-12, K-16)

**Input:** `analysis/full-audit-2026-11-18/lane-K-platform-build.md` (16 findings, probes `probes/K-*`).
**Owner paths:** `scripts/**`, `apps/android-native/**`, `tools/**`, `.cargo/**`, `Cargo.toml`.
**Rules honoured:** no new tests, no device access (no `am`/`input`/`pm`/`screencap`/`logcat -c`), cargo only through `scripts/h2-cargo`.
**Evidence:** `probes/LANE-541/run.sh` output (host-side; device statuses stay `NOT_RUN` by rule).

## What changed

### K-01 (P1) — release/QA gate could not bind the APK to the audited source

- `scripts/build-apk.sh` now writes **`build/android-p0/release/build-manifest.json`** with:
  - `source.head` = `git rev-parse HEAD`,
  - `source.source_dirty_digest` = sha256 over tracked modifications/deletions **and untracked build inputs** (contents, not mtimes),
  - `inputs.native_sha256` / `inputs.packaging_sha256` / `inputs.libmain_sha256`,
  - `artifact.sha256` (the signed APK), per-`.so` sha256 for every packaged native library,
  - `artifact.install` = the verified installed identity, written by `scripts/qa/device.py` after `pm install` (never by the build itself).
- The same identity (head, source digest, input digests, epoch) is **embedded in the APK as `assets/build-identity.json`** (aapt2 `-A`), so the installed package carries its own provenance.
- **`scripts/qa/identity.py`** verifies: manifest ⇄ current tree ⇄ APK file ⇄ embedded identity. Any mismatch is a `FAIL` with the exact reason (e.g. “HEAD moved since the build”, “source content changed since the build”).
- `scripts/qa/final_report.py` replaces the existence-only `apk.build.current_tree` gate with **`apk.build.identity`** and adds **separate statuses** `device.install_identity`, `device.first_frame`, `device.menu_visual`, `device.in_game_visual`, `device.gameplay_smoke`. A status is only `PASS` when the device evidence names the same APK **and** records an installed `base.apk` with the same sha256. Evidence from another APK is discarded as stale (listed in `stale_evidence`), never counted as PASS.
- **`scripts/qa/device.py`** hashes `/data/app/.../base.apk` via `pm path` + `sha256sum`, refuses `install`/`launch` on a mismatch, and records the identity into the manifest.
- `device_cycle.py` / `device_p0_cycle.py` refuse to run a cycle when the installed package differs from the `--apk` artifact, and record `apk.installed_sha256` + `install_matches_built` in their JSON.
- `device_evidence.sh` records `installed_sha256` / `install_identity_match` and fails on mismatch.
- `final_report.py --release-only` prints HEAD, dirty count/digest, apk sha256, installed sha256 and the per-status breakdown; exit code 1 on any mismatch.

### K-02 — stale-native guard was mtime-only, missed the vendored patch + lockfile, and wedged on `examples/`

- **`scripts/build_inputs.py`** parses the path-dependency closure of `wc3rust-android` from the manifests (root `[patch.crates-io]` included) and hashes the real input set: workspace `src/**`, `Cargo.toml`/`build.rs`, `Cargo.lock`, `.cargo/config.toml`, `third_party/wgpu-hal/**`. `examples/`, `tests/`, `benches/` are excluded because the cdylib never compiles them.
- **`scripts/build-native.sh`** builds the cdylib through `scripts/h2-cargo`, hashes the inventory **before and after** (a source edited mid-build forces a retry), and writes `target/release/.libmain-inputs.json`.
- **`scripts/build-apk.sh`** requires the stamp: `inputs_sha256` must equal the current inventory and `libmain_sha256` must equal the file. Without a stamp it falls back to a **precise** mtime check over the same inventory (never the old `find crates apps` walk), and says so.
- Live tree: `crates/wc3-sim/examples/audit_i_dup.rs` no longer blocks packaging.

### K-03 — APK not reproducible

- `.cargo/config.toml` remaps every device path (`$ROOT`, `$HOME/.cargo`, `$PREFIX`, `$HOME`) to stable `/build/...` prefixes. The rules live under `[target.aarch64-linux-android]` because cargo lets target-specific rustflags override `[build] rustflags` (the device's global config already sets a target rustflags array, and arrays are not merged; the environment's mold flag is repeated so linking is unchanged). This is the single source so every lane compiles with one RUSTFLAGS fingerprint (no cache thrash).
- `build-native.sh` requires the remap to have landed: it fails if no `/build/cargo-registry` string is present or if any device diagnostic `*.rs` path survives in `libmain.so`. It reports (not fails) the remaining `env!("CARGO_MANIFEST_DIR")` literals, which rustc does not remap: `crates/wc3-game/src/assets.rs:4506`, `hud.rs:1918` are host asset-fallback strings in crate code (crate source is owned by other lanes).
- **`scripts/apk_pack.py`** rebuilds the archive deterministically: fixed entry order, fixed timestamp from `SOURCE_DATE_EPOCH` (HEAD commit time), fixed permissions, preserved per-entry compression (`resources.arsc` stays stored). `zipalign`/`apksigner` were already deterministic per the audit.
- Two consecutive `scripts/build-apk.sh` runs from the same tree produce the same APK sha256 (see evidence).

### I-006 — restore gives up after the first corrupt checkpoint

- `apps/android-native/src/checkpoint_fs.rs` (new, pure std) provides `restore_candidates`: each candidate is a transaction (metadata → verdict → digest → deserialize → commit); a rejection logs `GAME_CHECKPOINT_REJECT path=… reason=…` and continues to the next file. When every candidate fails it logs `GAME_CHECKPOINT_REJECT_ALL count=N`.
- `restore_checkpoint` now feeds all `resume_candidates()` through it; `Finished` clears only that pair (`clear_candidate`), never the fallback.

### I-007 — migration never read the old package sandbox

- `migrate_legacy_checkpoint` now uses the real channels in order: this package's own pre-rename files, the old package's **external** files dir, then `org.herofolends.p0`'s **private sandbox** (which Android denies to a different applicationId). `checkpoint_fs::migrate_pair` logs `GAME_CHECKPOINT_MIGRATE_UNREADABLE` for EACCES/EIO instead of treating the source as absent; `GAME_CHECKPOINT_MIGRATE_LEGACY_SANDBOX access={readable|denied|absent|error}` states the private-sandbox status explicitly.

### I-008 — checkpoint callback masked a failed save

- `save_checkpoint` returns `SaveOutcome::{Saved, Skipped(reason), Failed(error)}`. A pair is published through temp file + fsync + rename (`publish_pair`); a failed write leaves the previous pair untouched, logs `GAME_CHECKPOINT_FAILED path=… err=…`, and does **not** record the tick.
- `on_sdl_event` sets `WATCHER_SAVED` only for `Saved`/`Skipped`; `Failed` leaves it false so the main loop retries in the same transition and logs `GAME_CHECKPOINT_FAILED callback=background retry=1`.
- Probe: `analysis/full-audit-2026-11-18/probes/I-006/run.sh` (also I-007/I-008 wrappers) compiles the real module with `rustc` and proves corrupt→fallback, read-only→honest error with the old pair preserved, migration, and unreadable legacy source.

### K-06 — a failed surface recovery killed the game

`apps/android-native/src/lib.rs`:
- new `SurfaceRetry` (30 attempts, 250 ms→2 s backoff). `recover_surface` failures at the lifecycle/resize, watchdog and lost/outdated sites are retried instead of `?`-propagated to `GAME_FATAL`; the sim keeps advancing while the `ANativeWindow` is re-published (the 1 s publication window in `current_native_window` is now used as a retry, not a death sentence). Only the retry budget or an explicit quit ends the match. The `surface.is_none()` render path also retries.

### K-04 / K-05 — campaign autosave and per-map stamp

- `save_checkpoint` no longer returns for non-melee games: a campaign chapter writes the same `autosave.sav` + `CheckpointMeta` on Home, with `GAME_CHECKPOINT campaign=1 note=sim-only` in the log.
- `restore_checkpoint` accepts campaign saves and **parks the JASS engine/bridge** (`script=parked-campaign`) instead of letting the freshly booted `war3map.j` replay over the restored board (the game crate itself refuses campaign saves for the same reason).
- `cached_map_stamp` (once-per-process `OnceLock`) replaced by `current_map_stamp`, a one-entry cache keyed by the current `map=` line, so an in-process map switch stamps the autosave with the map that is actually loaded (K-05).

### K-07 — QA truths

- `scripts/qa/final_report.py` no longer carries a static `{"rc": 0}` command list; `commands` is derived from the saved logs the gate parsed (with an explicit note that the report does not execute them).
- `scripts/qa/parity.py`: `jass.frame_loop`, `hud.resources`, `audio.wired` are source-presence checks, so they are now `STATIC` (with `method: static-source`) instead of `PASS`; `overall()` cannot return `PASS` for a run whose remaining gates are static, and the summary counts `STATIC` separately.

### K-08 — build pipelines that masked failure

- `perf-gpu.sh`: uses `scripts/h2-cargo`, checks cargo's **exit status** (no `tee|tail` masking), requires the binary to exist, and requires the bench JSON to exist before returning 0.
- `perf-release.sh`: uses `scripts/h2-cargo`, aggregates build + per-run rcs, fails on a missing JSON.
- `robustness-verify.sh`: derives the checkout path from `BASH_SOURCE`, records every step, exits with the worst step rc.

### K-12 — screenshots/evidence that could belong to another run

- `game-selftest.sh`: `shoot()` deletes the destination first, checks `screencap` and the transfer, validates PNG magic, and requires the file mtime to be after the run start; prints `SELFTEST_APK sha=… installed=…`.
- `device_evidence.sh`: only screenshots captured after `start_epoch` are attached (`find -newermt @epoch`); the JSON records `installed_sha256` and fails on an install-identity mismatch.

### K-16 — untracked binaries with no checksum/provenance

- **`scripts/build-deps.json`** pins sha256 + bytes + provider for `libSDL3.so`, `third_party/vulkan-loader/libvulkan.so`, `apps/android/debug.keystore`, Termux `libiconv.so` / `libc++_shared.so` (and the optional `libturnip_vendor.so`).
- **`scripts/build_deps.py verify`** is a hard gate in `build-apk.sh`: missing/mismatched bytes fail with the expected/actual hashes (a fresh clone without the audited artifacts cannot silently ship a different APK).
- Every packaged `.so` is recorded by sha256 in the build manifest (`artifact.libs`).
- `apps/android-native/src/lib.rs` calls `SDL_GetVersion` and logs `GAME_SDL version=…`, so the SDL identity is runtime evidence (not a comment).

## Verification

Host-side evidence is produced by `analysis/full-audit-2026-11-18/probes/LANE-541/accept2.sh` (builds + gate demo) and the checkpoint probe (`probes/I-006/run.sh`, wrappers under `I-007/`, `I-008/`).

Acceptance chain (shared tree; lane retried through other lanes' in-flight compile errors):

```
ROUND 5 2026-11-18T17:57:40+03:00
round 5: sha_a=cdef59f90d1b9a813bfa3cc67207dddb67cd298b8ee408acc33c36fd455a5cf7
round 5: sha_b=cdef59f90d1b9a813bfa3cc67207dddb67cd298b8ee408acc33c36fd455a5cf7
ACCEPTANCE_BUILDS_IDENTICAL sha256=cdef59f90d1b9a813bfa3cc67207dddb67cd298b8ee408acc33c36fd455a5cf7
```

Release gate (`--release-only`):

```
fresh : rc=0 verdict=INCOMPLETE source=PASS apk=cdef59f90d1b9a81 installed=None
stale : rc=1 verdict=FAIL     source=FAIL reasons:
          source content changed since the build (manifest=c6a2f8914035 tree=4ad2ab055eb5)
          libmain.so was built from different native sources (manifest=4980b50cde66 tree=6817a60bb16b)
revert: rc=0 verdict=INCOMPLETE source=PASS apk=cdef59f90d1b9a81 installed=None
```

Stale run = a new `apps/android-native/src/lane541_stale_probe.rs`; revert = moved out of the tree (no source edit to another lane).

Build manifest (`build/android-p0/release/build-manifest.json`):

```
schema wc3rust-build-manifest-v2
head d7af459daf5c507d46c8a63741385e5d1091af26
dirty_count 261 digest c6a2f89140353d93
native_inputs 4980b50cde662843 packaging 1dd0ea47b6d2c7c3 libmain 979d21f4baad2ab0
apk cdef59f90d1b9a813bfa3cc67207dddb67cd298b8ee408acc33c36fd455a5cf7 bytes 7714218
install None (device evidence is NOT_RUN by operator rule)
libs libSDL3/libc++_shared/libdrm/libiconv/libmain/libvulkan/libz/libzstd
epoch 1794970638 zip_entries 17
```

Entries share one normalized timestamp `2026-11-18 02:57:18Z` (HEAD commit time); the APK embeds `assets/build-identity.json` with head/digests/epoch.

Guard inventory:

```
files=398 digest=4980b50cde662843 roots=.cargo,Cargo.lock,Cargo.toml,apps,crates,third_party
examples_tests= []  lockfile=True  cargo_config=True  vendored_wgpu_hal=74
stamp inputs=4980b50cde662843 lib=979d21f4baad2ab0 remap=True source=.cargo/config.toml
newer_than_libmain=[]
```

Remap in the artifact: 204 `/build/` strings, 0 `/…/.cargo` device strings, 0 device `*.rs` diagnostic paths. One non-diagnostic `CARGO_MANIFEST_DIR` literal remains (`crates/wc3-game/src/assets.rs:4506`), reported by the build as crate-source-owned.

Checkpoint probe:

```
GAME_CHECKPOINT_REJECT path=…/first.sav reason=deserialize: bad payload
RESTORE_ACCEPTED=…/second.sav tick=Some(42)
GAME_CHECKPOINT_REJECT_ALL count=2
RESTORE_ALL_CORRUPT_NONE=true
GAME_CHECKPOINT_REJECT path=…/first.sav reason=finished
RESTORE_AFTER_FINISHED=…/second.sav
READONLY_SAVE=Err("serialize: Permission denied (os error 13)")
READONLY_PREVIOUS_PAIR_PRESERVED=true
GAME_CHECKPOINT_MIGRATED from=…/legacy/herofolends-resume.sav to=…/autosave-migrated.sav
MIGRATED=Some(…) payload_and_meta_ok=true
GAME_CHECKPOINT_MIGRATE_UNREADABLE path=…/blocked-legacy/herofolends-resume.sav err=Permission denied (os error 13)
BLOCKED_MIGRATED=None target_exists=false
PROBE_RESULT=PASS
```

Device statuses (`install_identity`, `first_frame`, menu/in-game visual, gameplay smoke) remain `NOT_RUN` by operator rule; the gate refuses to infer them, and a device cycle on an older APK is discarded as `stale_evidence`.

## Limits / notes (recorded, not hidden)

1. **Campaign resume parks the JASS world.** The sim payload is the board; the engine's script state is not serializable from the host. On `boot.select=resume` the restored campaign continues with `GAME_RESUME … script=parked-campaign` (units/economy/tick preserved, scripted waves/quests stop). The game crate itself refuses campaign saves (`campaign-engine-state-unsupported`), so the menu CONTINUE still refuses the campaign autosave; only the host resume path consumes it. Proper fix (engine-state serialization) belongs to a `crates/wc3-game` lane.
2. **Two `env!(CARGO_MANIFEST_DIR)` host-fallback literals remain in crate code** (`crates/wc3-game/src/assets.rs:4506`, `hud.rs:1918`); rustc cannot remap compile-time literals, so cross-checkout byte-identity still needs a crate-side change (runtime path). The build reports them as a NOTE and refuses only diagnostic `*.rs` device paths.
3. **K-10 (h2-cargo FIFO/memory floor) was observed** during acceptance (waiters starved by a fresh `flock -n`, 25-minute holds) but is outside this lane's assigned items; `H2_CARGO_PRIORITY=1` was used for the release build.
4. **Shared tree:** rounds 1–4 failed on other lanes' in-flight compile errors (`crates/wc3-game/src/hudskin/pack_build.rs`) and mid-build edits; round 5 produced the stable snapshot above. Logs: `$PREFIX/tmp/opencode/lane-541/{accept2,native-r5,apk-a-r5,apk-b-r5}.log`.
5. **Post-acceptance state:** the acceptance snapshot is the 17:58 build. Other lanes edited `crates/wc3-game/src/hudskin/*` at 18:02+; `--release-only` then correctly reports `FAIL` ("source content changed", "libmain was built from different native sources"). Re-run `probes/LANE-541/accept2.sh` at integration time for a green snapshot; the gate is supposed to fail until then.

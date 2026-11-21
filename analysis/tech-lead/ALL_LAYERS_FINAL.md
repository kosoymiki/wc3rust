# All-layer push — final gate report (2026-11-21)

Repo `~/src/herofolends-rs` (`~/src/wc3rust`) · HEAD `d7af459daf5c507d46c8a63741385e5d1091af26` ·
15-agent push (`analysis/tech-lead/all-layers-20261121/ORCHESTRATION_MASTER.md`) · infra owner ledger **611**.
Oracle for every parity claim: the shipped tree `analysis/war3_full3` (10 523 files), `analysis/war3_mpq_listing.txt`,
SLK/MDX/BLP data, 44 campaign maps and `Scripts/common.j`.

The tree is a live construction site: at build time it carried **354 dirty paths** (133 tracked modifications,
221 untracked) and lanes kept editing native sources through the run. Every number below is what a command
printed, not an expectation; the snapshot time is named wherever the tree can have moved since.

## 1. Gate results (exact)

| Gate | Command | rc | Evidence |
|---|---|---:|---|
| Script syntax | `bash -n` over `scripts/*.sh`, `scripts/qa/*.sh`, `tools/*.sh`, `tests/gates/*.sh` | 0 | no output, `BASH_N_RC=0` |
| Port-parity hook present | `grep -n port-parity-all.sh scripts/test-all.sh` | 0 | `scripts/test-all.sh:460-486` (phase `port_parity`, PASS/FAIL line in the JSONL) |
| test-all skip honesty | `scripts/test-all.sh --check-only --no-check --no-tests --no-targets` | 0 | three SKIP records (menu, orc01, port_parity) with reasons + `members:32` summary |
| Deterministic packer | `scripts/apk_pack.py` twice on identical inputs, `--epoch 1794970638` | 0 | both `sha256=1d109415a4a99e43d7752947ac6b1fad986cb5a644c9bd5b3aaf98e821e293fc`, 17 entries, 7 726 002 B |
| Native library | `scripts/build-native.sh` (release, aarch64-linux-android) | 0 | `LIB_SHA256 1c76a2fa26c52fda3002ae4209870049c7ffc2e3141bc1679fec537d49f65c22`, 13 252 528 B, stamp inputs `c61b8bcd9b0f832a…`, rustc 1.99.1 |
| Debug APK, run 1 | `ANDROID_JAR=~/src/xray-mgr-android/android-35/android.jar scripts/build-apk.sh --debug` | 0 | `sha256=01ae8fc30dd1d65f545d39163aca3b0bd67c8d0fa5eb6a67cf08ad52776c13a1`, 7 738 794 B, 17 zip entries, package `org.wc3rust.p0` 0.1-p0, v2+v3 verified, 8 native libs, signer `cd2e73f6…` |
| Debug APK, run 2 | same command | 0 | identical `sha256=01ae8fc3…`, identical source state `354420c2e01bf38bf51c36fc539ca2438b23bc7a0bea1782e873333a0df67e4e`, identical libmain `1c76a2fa…` → **byte-identical rebuild** |
| Workspace check | `PORT_PARITY=0 scripts/test-all.sh --check-only` | 0 | `PASS check:workspace 152s`, `members=32`, `test-all-infra-20261121.jsonl` (menu/orc01/parity recorded as SKIP with reasons) |
| Port parity matrix (snapshot) | `scripts/port-parity-all.sh` territory (agent N; observed `last-gate.json`) | 9/11 ok | at 11:21–11:22Z: `ability-class` parity 0/1 + `cargo_check rc=101`; `anim-geoset` `cargo_check rc=101`; the rest green |
| Original parity | `python3 scripts/original_parity.py` (agent N's live tool) | not re-run | killed after 600 s in the cargo queue; last recorded `applicable=10 failures=0` (`analysis/tech-lead/PORT_PARITY_ALL_LAYERS.md`) |
| Docs EN/RU | heading parity + content sync | 0 | README 14/14 headings mirrored; build/test sections updated in both languages |

The APK was built at **16:38** against source state `354420c2…`; lanes continued editing after that, so the
artifact proves the build path for that state, not for the final post-push tree. Re-running the two commands
after all lanes close reproduces the same result by construction.

## 2. Per-agent table (snapshot 16:42)

| # | Agent | Ledger | Layer | State (observed) | Evidence on disk |
|---|---|---|---:|---|---|
| A | `render-terrain` | 597 | terrain/water/lighting | phase 5, waiting on slide | water-edge seam fix (`band/interior delta 0.53`, seam 2.79 = legacy), QA clear-band 43.5%→2.2%, fade A/B 17.1% far-half pixels; tests `art14_water_edge_continuity`, `art14_terrain_shoulder_mesh`, `art14_terrain_frame_2414` in `crates/wc3-game/src/terrain_render.rs:4822-4986,6276`; 2414×1080 A/B frames in `analysis/agent-world-art/all-layers-20261121/` |
| B | `hud-ghost` | 598 | HUD / icons / build ghost | in progress | `analysis/agent-hud/ALL_LAYERS_20261121.md` (11 259 B, 16:38); `art7_army_icons` / `art10_ghost_visible` present; latest `art7_army_panel_headless_render` run at 16:41 (in flight) |
| C | `assets-formats` | 599 | formats / assetdb / mpq | done | `analysis/agent-assets/ALL_LAYERS_20261121.md` (13 689 B): `cargo test -p wc3-formats -p wc3-assetdb -p wc3-mpq` **rc=0, 209 tests** (assetdb 47, formats 116, mpq 46); `catalog-20261121/asset-catalog.md` |
| D | `animation` | 600 | anim / geoset | completed | `analysis/agent-anim/ALL_LAYERS_20261121.md` (8 738 B): 2 048 models / 6 512 sequences / 33 354 alpha slots, 0 mismatches; `wc3-anim` suite rc=0 |
| E | `audio` | 601 | audio / voices | closed | `analysis/agent-audio/ALL_LAYERS_20261121.md` (9 330 B): SLK-oracle drift 214→0; `artE_perdef_voices` rc=0 |
| F | `sim` | 602 | sim / gameplay | completed | `analysis/tech-lead/mechanics-20261121/ART_F_REPORT.md` + `ALL_LAYERS.md`: `artF_crossmap_move` |
| G | `qa-device` | 603 | QA / device evidence | done | `analysis/agent-qa/ALL_LAYERS_20261121/QA_REPORT.md` (15 143 B): no device slot; 95/96 readable frames pink=0; installed `e74d79…` ≠ tree APK (honest boundary) |
| H | `ui-screens` | 604 | UI screens / hudskin | running | `ui_screens` lib test ran from `$PREFIX/tmp/opencode/h-ui-sandbox-1795262940` with the live `CARGO_TARGET_DIR`; no report yet |
| I | `campaign-jass` | 605 | campaign / JASS | running | `analysis/agent-jass/MAP_BOOT_20261121.md` (3 827 B) + `map_boot_matrix_20261121.json`, `native_coverage_20261121.json` |
| J | `input` | 606 | input / touch | succeeded | `analysis/agent-touch/ALL_LAYERS_20261121.md` (11 467 B) + `PARITY_TOUCH.md`: 7 root-cause fixes, 73 lib + 13 integration tests rc=0, 15/15 traces `--check` rc=0 |
| K | `data` | 607 | gamedata | registered | `analysis/agent-data/ALL_LAYERS_20261121.md` (12 750 B, 16:26) |
| L | `guest-re` | 608 | guest PE / RE | completed | `analysis/agent-re/ALL_LAYERS_20261121.md` (13 588 B) + `pe_census_20261121.md`: 71 guest tests pass, census matches rabin2 on 4 shipped PE32 |
| M | `replay-bink` | 609 | replay / media | completed | `analysis/agent-media/ALL_LAYERS_20261121.md` (8 441 B): `-p wc3-replay -p wc3-bink` rc=0, 86 passed / 0 failed / 6 ignored |
| N | `parity-byte` | 610 | parity / original cross-check | running | `analysis/tech-lead/PORT_PARITY_ALL_LAYERS.md` (11 units) + `analysis/port-parity/*/last-gate.json` (9/11 ok at snapshot) |
| O | `infra-gates` | 611 | infra / gates / release | done | this report; scripts/docs changes in §3; APK + rebuild + workspace check in §1 |

## 3. Infra changes landed in this push (mechanism, root cause)

1. **`scripts/test-all.sh` — skipped phases were silent.** The port-parity block ran on `--check-only`
   (contradicting the documented "only `cargo check --workspace`") and every narrow run (`--check-only`,
   `--no-tests`, `--only`, `PORT_PARITY=0`) skipped the matrix without a JSONL trace. Root cause: the phase
   was appended after the flags but never wired to them. Fix: `DO_PARITY`, `--no-parity`, automatic off for
   `--check-only`/`--no-tests`/`--only`, and a SKIP record with the reason for menu, Orc01 and port parity.
   Evidence: the skip-path run (§1) and `analysis/test-all-infra-20261121.jsonl`.
2. **`scripts/build-native.sh` — digest sampled before the cargo queue.** The script sampled the native input
   digest and then waited in the single-slot FIFO; observed waits of 3–15 min made every attempt stale before
   the compile started (digests `4a5bed45 → 7a165234 → a314a22b → 7d3b7a33` were all rejected by the
   after-compile guard — correctly: a mixed `.so` is never shipped). Three successive fixes, each measured:
   (a) wait for the slot before taking the digest (lost the race against queued waiters, rc=6 after 30 min);
   (b) observe the `h2-cargo begin … build -p wc3rust-android` line in the shared log and take the digest at
   **compile start** (the final mechanism). Evidence: the successful attempt logged
   `inputs at compile start: c61b8bcd…` and stamped `lib_sha256=1c76a2fa…` after a 5-minute compile.
3. **`scripts/build-apk.sh` — android.jar discovery.** The auto-detect only knew two `$PREFIX/tmp` paths, so
   the documented `~/src/xray-mgr-android/android-35/android.jar` needed the env var. Added it as a third
   candidate (checked with `-f`, harmless on other hosts); the documented `ANDROID_JAR=…` path still wins.
4. **`scripts/build-native.sh --help`** printed `set -euo pipefail` as part of the usage block (sed range
   included line 20). Fixed range, same byte length (a running build was mid-file).
5. **Docs synced to the scripts**: `README.md` + `README.ru.md` (32 workspace members, `build-native.sh` →
   `build-apk.sh` release/debug steps, test-all acceptance phases incl. Orc01/menu/port parity, honest
   parity-skip semantics, `wc3-guest` is a workspace member and `wc3-re` dependency), `docs/TESTING.md`
   (workspace gate section: phase table, exit codes 0/1/2/3, narrow-run semantics, single-slot note),
   `docs/ENGINE.md` (verification commands, MP3 boundary corrected to the shipped `probe_mp3`/`play_mp3`
   path), `docs/audit/status-infra.md` (AUD-030 phases 8–9, current 32 members; local-only file,
   `/docs/audit/` is gitignored by design).

## 4. Known boundaries (explicit, not failures)

- **No device slot**: `H2_DEVICE_SLOT` unset and `adb` empty, so this push makes **zero current-tree device
  claims**; the only device frame is the 11:56 ready screen from installed `e74d79…` (agent G). The APK from
  §1 was not installed.
- **Port parity is a moving target while lanes edit crates**: green 11/11 at 10:51–10:52Z, 9/11 at the
  11:21–11:22Z snapshot; `ability-class` carries a real 0/1 parity mismatch and two `cargo_check rc=101`
  rows came from concurrent edits. A fresh full sweep belongs to agent N/tech lead after the lanes close.
- **Android target checks**: only `aarch64-linux-android` has a linker on this host; armv7/i686/x86_64 are
  SKIP with the missing tool named (test-all phase 3) — never PASS.
- **Debug APK packages `target/release/libmain.so` by design**: the build type selects aapt2 `--debug-mode`
  and the manifest flag; it is not a debug-symbol native build.
- **`docs/` is English-only**; the Russian mirror is `README.ru.md` (14/14 headings aligned).

## 5. Leftover-risk ledger (named owner, exact next step)

| # | Risk | Evidence | Owner | Next step |
|---|---|---|---|---|
| 1 | Current-tree device evidence missing | G: `H2_DEVICE_SLOT` unset, adb empty; installed `e74d79…` vs tree APK `01ae8fc3…` | tech lead + G | grant `H2_DEVICE_SLOT=1`, install `build/android-p0/debug/wc3rust-p0.apk`, run `scripts/qa-all.sh --no-build` |
| 2 | `ability-class` parity mismatch 0/1 | `analysis/port-parity/ability-class/last-gate.json` (11:21Z) | N | replay after the sim lane lands; fix the diff, never edit `cases.jsonl` |
| 3 | `anim-geoset` `cargo_check rc=101` at snapshot | `analysis/port-parity/anim-geoset/last-gate.json` (11:22Z) | N + D | rerun `scripts/port-parity.sh gate anim-geoset` on the green tree |
| 4 | Port-parity matrix not re-swept after the push | 9/11 snapshot; C's formats fix (rc=0, 209 tests) landed after it | N | full `scripts/port-parity-all.sh` once lanes close; then the full `scripts/test-all.sh` |
| 5 | B's `art7` render test in flight | `h2-cargo.log` 16:41 `art7_army_panel_headless_render` | B | green the test, then the full gate |
| 6 | H's sandbox build shares the live target | pid sandbox `$PREFIX/tmp/opencode/h-ui-sandbox-…` with `CARGO_TARGET_DIR=<repo>/target` | H + tech lead | give sandbox builds a private `CARGO_TARGET_DIR`; rerun `ui_screens` there |
| 7 | Parity units blocked on data/GPU | `PORT_PARITY_ALL_LAYERS.md`: render/terrain (GPU adapter), data/coverage (single `War3.mpq`), HUD layout gate | A, K, B/H + N | headless GPU runner / point at the operator's `War3.mpq` / green the HUD gate |
| 8 | FULL_PORT_CRITERIA P0-12 still FAIL | `analysis/tech-lead/FULL_PORT_CRITERIA.md` | tech lead + A + G | device render-parity rows need a device run |
| 9 | `scripts/build-native.sh` is untracked | `git status` → `?? scripts/build-native.sh` | tech lead (commit) | add it to the release commit; a fresh clone must build libmain through it |
| 10 | `analysis/test-all.jsonl` is 0 B | `ls -la` after an interrupted run; infra run wrote `analysis/test-all-infra-20261121.jsonl` instead | tech lead | run the full `scripts/test-all.sh` once after all lanes close |
| 11 | The APK predates later lane edits | built at 16:38 from state `354420c2…`; lanes continued | tech lead | after the push quiets, rerun `scripts/build-native.sh && scripts/build-apk.sh --debug` (the same two commands; expect a new sha for the new state) |

## 6. H2 persistence

- Corpus preflight: `harness2_corpus_pack` (debug/weak) at session start.
- Ledger 611: registered → checkpoints phase 1–5 → close with this report.
- Capture/experience IDs: capture **1264**; experiences **4922** (test-all phase flags + SKIP records), **4923** (digest at compile start), **4924** (sandbox target-dir isolation), **4925** (APK reproducibility proof).


---

# FINAL VERIFICATION — 2026-11-21 19:57Z (all 15 agents closed)

Run by the tech lead after every lane closed; commands and exact results:

| Gate | Command | rc | Result |
|---|---|---:|---|
| Workspace check | `scripts/h2-cargo check --workspace --offline` | **0** | whole workspace compiles |
| Port parity (11 units) | `bash scripts/port-parity-all.sh` | **0** | `PORT_PARITY_ALL_PASS units=11` (incl. `original-parity`) |
| Original-game parity | `python3 scripts/original_parity.py` | **0** | `ORIG_PARITY_SUMMARY applicable=17 failures=0 not_applicable=3` |

## Agents (15/15 closed)

A 597 render · B 598 hud/ghost · C 599 assets · D 600 anim · E 601 audio · F 602 sim ·
G 603 QA · H 604 ui · I 605 campaign · J 606 input · K 607/613 data · L 608+615 guest/re ·
M 609 replay · N 610 parity · O 611 infra.

## Highlighted outcomes (all with named tests, exact rc in the lane reports)

- **Render**: water seam 9.09→2.79, clipped luma 12499→0, shimmer far-delta −12.9%, doodad grounding 0.188% px.
- **HUD/ghost**: shipped BLP icons (5 distinct), ghost alpha 0.50 + ring 216 + red/blocked flip; hud_layout gate PASS, menu 5/5 viewports.
- **Assets**: 3935 BLP / 2048 MDX / 33 SLK 0 mismatches; listfile 10523=10523; MDX signed keyframe drift fixed.
- **Anim**: 2048 models, 0 mismatches; 2 real fixes (end<start panic, copied-sequence fallback).
- **Audio**: 8 defs → 8 distinct voice sets; SLK oracle drift 214→0.
- **Sim**: 6 shipped-oracle fixes (queue 7, refund 75%, decay 10 hp/s, victory one-shot, osld price, neutral owner).
- **Campaign/JASS**: 44/44 chapters, 44/44 scripts boot, 791/791 natives 0 mismatches; wc3-re 13→0 unimplemented.
- **Data**: 10/10 tables count-exact; abilities max_level drift fixed (357/749 over-reports).
- **Input**: 7 root-cause fixes; 15/15 traces.
- **Infra**: native+APK reproducible byte-identical (`01ae8fc3…`), test-all parity hook honest.

## Honest boundaries (unchanged)

- No device slot: all visual artifacts are headless/GPU renders labelled NOT device; device pass is owner-side.
- No single `War3.mpq` on Termux (split A..Y): archive-payload SHA audit needs a desktop install.
- No `war3.exe` on Termux: PE census uses the extracted copies (rabin2-verified).
- Parity recording replacement is audited via `retire` with reasons, never edits to hide mismatches.

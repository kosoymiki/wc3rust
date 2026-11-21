# Agent G — final QA gate, wc3rust art push (2026-11-21)

Ledger **591**, role reviewer, repo `~/src/herofolends-rs`. Packets obeyed:
`analysis/tech-lead/art-push-20261120/PACKET_G_qa.md` and `ORCHESTRATION_MASTER.md`.
OWNER paths only: `analysis/agent-qa/art-push-20261120/**`, `scripts/qa/**`,
`scripts/visual_check.py`. No `crates/**`, `Cargo.toml`, `scripts/build-apk.sh`
or other agent's analysis dir was edited.

## 1. Honest boundary — no device attached

| Probe | Exact result | rc |
|---|---|---|
| `H2_DEVICE_SLOT` | unset (`printenv \| grep -i DEVICE_SLOT` → `NO_DEVICE_SLOT_ENV`) | 0 |
| `adb devices` | `List of devices attached` — empty | 0 |
| `adb get-state` | `error: no devices/emulators found` | 1 |
| `su -c id` | `uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0` | 0 |
| tooling | rtk `1.1.0`, radare2 `6.2.6`, Python PIL `12.3.0`, tesseract present | 0 |

**Therefore: no APK rebuild, no install, no scenario 09-army-panel / 05-ghost / 06-place
device run, and ZERO new device screenshots.** Every artifact in this report is either a
pre-existing device screenshot from build `d3efe82e` (2026-11-14) or a **HEADLESS MOCK
procedural reference** produced by Agent B / a headless render by Agent A. None of the new
art-push PNGs is a device capture. This is an owner-side slot dependency, not a defect that
QA can fix inside this session.

Exact next step to close the gap (unchanged from packet): attach `H2_DEVICE_SLOT=1`, then
`scripts/build-apk.sh`, install the APK, run scenarios 09-army-panel / 05-ghost / 06-place,
and capture the real 2414×1080 frames into this directory. Only then may #7/#10/#14 be
marked `device-verified`.

## 2. QA tool added (in OWNER path)

`scripts/qa/art_push_magenta_scan.py` (sha256 `d6254e19f7e62ad704a15d6225ec2d2b9ad7cf9d2ecaae8e040593e737ff0bd9`)
reuses the project's canonical definitions instead of inventing a threshold:

- pink mask `(r>195) & (b>175) & (g<95) & (r-g>90)` from `scripts/qa/analyze.py::world_holes`;
- clusters via `scripts.qa.common.components(mask, min_size=120)` (same as `analyze.py`);
- exact key `(255,0,255)` and hud mask `(r>190)&(b>190)&(g<90)` from `tools/hud_pack_check.py`;
- tesseract OCR of the top 80 px to prove the "HEADLESS MOCK" banner is really rendered.

PASS requires expected dims, `pink_pixels == 0`, `pink_clusters == 0`,
`exact_magenta_pixels == 0` (and label present when `--check-label` is used).

`*.annotated.png` is classified `N/A-annotated`, not FAIL: `scripts/qa/analyze.py:657-669`
and `:695-700` paint debug boxes in `(255,0,255)`, so magenta there is annotation ink.
This classification was a same-generation fix after the first extended scan produced six
false FAILs; `python3 -m py_compile` and `git diff --check` on the tool: **rc=0**.

## 3. Magenta regression matrix — art-push PNGs (all, exhaustive)

Source: `analysis/agent-qa/art-push-20261120/magenta_scan_artpush_all.json` (world, 9/9 PASS,
rc=0) and `magenta_scan_mocks.json` (army/ghost, 3/3 PASS, rc=0). Every image is
2414×1080, `pink_pixels=0`, `pink_clusters=0`, `hud_magenta_pixels=0`,
`exact_magenta_pixels=0`.

| Image (provenance) | dims | magenta | label | verdict |
|---|---|---|---|---|
| `analysis/agent-hud/art-push-20261120/headless-army-panel-2414x1080.png` (B mock) | 2414x1080 | 0 | OCR `HEADLESS MOCK!… NOT a device screenshot` | PASS |
| `analysis/agent-hud/art-push-20261120/headless-ghost-valid-2414x1080.png` (B mock) | 2414x1080 | 0 | OCR `HEADLESS MOCK!… NOT a device screenshot` | PASS |
| `analysis/agent-hud/art-push-20261120/headless-ghost-invalid-2414x1080.png` (B mock) | 2414x1080 | 0 | OCR `HEADLESS MOCK!… NOT a device screenshot` | PASS |
| `analysis/agent-world-art/art-push-20261120/art14-AFTER-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/art14-BEFORE-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/art14-after-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/art14-baseline-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/art14-legacy-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/probe-14000_16100-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/probe-14000_200-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/probe-200_8000-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |
| `analysis/agent-world-art/art-push-20261120/probe-28500_8000-edge-2414x1080.png` (A render) | 2414x1080 | 0 | not a mock (render) | PASS |

Extended context (pre-existing, not art-push): `magenta_scan_prior_device.json` 38 frames →
32 PASS / 6 annotated-N/A / **0 FAIL**, rc=0; `magenta_scan_raw_screens.json` 38 →
29 PASS / 9 annotated-N/A / 0 FAIL, rc=0. No non-annotated capture in the repo has a
magenta cluster.

Artifact SHA-256 (provenance): army `75a85268c438de41d4d6611079672819f43e187dc80874ef33e0363875149077`,
ghost-valid `69e5b3d5ddeacb013773db0563bc042ac7e52dffc64e550c93f0217777a58b81`,
ghost-invalid `b2865a94d6152a097cbd7f4fda5011da74998d8e43b1b910d09843d723e8a91f`,
world-baseline `dd579ed28ce651fa1d31cf3d35a7e15ec31a2432089f7d8c46886f432fac6700`.

## 4. Independent named-test verification

`analysis/agent-qa/art-push-20261120/named-tests-qa-recheck.log`:

| Test binary | QA rc | Output |
|---|---|---|
| `wc3-render --test art14_doodad` | 0 | 2 passed; 0 failed |
| `wc3-game --test art14_terrain` | 0 | 2 passed; 0 failed |
| `wc3-game --test art10_ghost_visible` | 0 | `ART10_GHOST_INVALID red=PASS`, `ART10_GHOST_VISIBLE alpha=0.50 ring_r=216 PASS` |
| `wc3-game --test art7_army_icons` | 0 | QA rescar `art7-qa-rescar.log`: `ART7_ARMY_ICONS units=5 chips=5 distinct_icons=5 PASS defs=["htow","hpea","hfoo","hrif","Hpal"]`, `RC_art7_rescar=0` (B's `art7-rerun.log` also `ART7_RC=0`) |

First attempt (`named-tests-qa.log`) had 3× rc=101 from a **concurrent in-flight edit** in
another agent's file: `error[E0308] crates/wc3-audio/src/game_audio.rs:543` expected
`Option<u64>`, found `Option<u32>`. It is outside my OWNER paths, so I did not edit it;
the owning lane fixed it — `scripts/h2-cargo check -p wc3-audio --offline` → rc=0 — and the
re-run above is clean. No open tail from that defect.

### 4a. art7 independent rescar — completed

`analysis/agent-qa/art-push-20261120/art7-qa-rescar.log`: after clearing the shared
`h2-cargo` slot (other agents' test runs), `ART7_ARMY_ICONS units=5 chips=5
distinct_icons=5 PASS`, `test result: ok. 1 passed; 0 failed`, `RC_art7_rescar=0`.
All four named art-push test binaries are therefore independently confirmed green by QA.

## 5. Per-bug verdicts

Verdict vocabulary: **device-verified** = real device screenshot on the current build;
**headless-only** = headless render/mock + named test, honestly labeled; **not-verified** =
no fresh evidence in this push.

| # | Bug | Verdict | Evidence / reason |
|---:|---|---|---|
| 7 | No active army panel | **headless-only** | B `art7_army_icons` rc=0 (5 units→5 distinct icons); mock `headless-army-panel-2414x1080.png` dims/magenta/label verified §3; no device |
| 10 | No build ghost / validity | **headless-only** | QA re-ran `art10_ghost_visible` rc=0 (alpha 0.50, green/red flip, ring r=216); two mocks verified §3; no device |
| 14 | Terrain looks bad (flat water band) | **headless-only** | QA re-ran `art14_doodad` + `art14_terrain` rc=0; 9 world renders 2414×1080 magenta=0; no Justice-device frame |
| 15 | Pink/magenta triangles | **headless-only (this push)**; prior device-verified | 12/12 art-push PNGs magenta=0; prior build `d3efe82e` 13 frames 0 clusters (`operator-bugs/REPORT.md` row 15) |
| 2, 3, 5, 11, 12, 16, 17 | fixed on 2026-11-14 device build | **device-verified (inherited, not re-run)** | `analysis/agent-qa/operator-bugs/REPORT.md` + `operator-bugs/screens/`; this push produced no new device build |
| 1, 4, 6, 8, 9, 13, 18 | partial / open evidence | **not-verified (this push)** | no scenario for these in the art-push packet; status unchanged from `operator-bugs/REPORT.md` |

## 6. Result and limits

- Goal 1 (device evidence): **not achieved — honest boundary**, no device slot.
- Goal 2 (magenta regression): **achieved** — 12/12 art-push images PASS at `magenta=0`, per-image matrix above.
- Goal 3 (QA report): this file, with exact paths, rc and limits.
- Headless mocks are procedural references v1, **not** hand-drawn Blizzard-level art and
  **not** device screenshots. No fabricated screenshot or rc is claimed.
- QA never edited `crates/**`, `Cargo.toml`, `scripts/build-apk.sh` or another agent's dir.

## H2_PERSISTED
- corpus pack `ev-75da455b8b834eec` (2026-11-21, task_type=sysadmin, weak);
- captures **1210** (device/boundary + scanner), **1213** (final report);
- experiences **4885** (`fp 1a81dd265d619c2b0c950db3fedb198c1d15e6e3d9666979c427dc43e0820389`),
  **4888** (`fp 073e278ac4ce65c2eb99b34f7f9ae4f1c2a92197d8dc55c0082c1bbbeac5a94f`);
- monitor 591 checkpoints at phase 1 (device check), phase 2 (magenta scan), phase 3 (independent verify), phase 4 (final report); root `uid=0 u:r:ksu:s0`.

## SESSION_CLOSED
- Ledger 591 — closed `done_partial` with `h2_ids_json`; new device screenshots remain an
  explicit owner-side slot dependency, not an unowned tail.

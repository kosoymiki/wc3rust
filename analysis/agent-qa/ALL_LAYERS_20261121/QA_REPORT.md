# Agent G — QA / device evidence gate, all-layers push (2026-11-21)

Ledger **603** · repo `~/src/herofolends-rs` (`~/src/wc3rust`) · packet
`analysis/tech-lead/all-layers-20261121/PACKET_G_qa-device.md`.
Owner paths only: `analysis/agent-qa/**`, `scripts/qa/**`. No `crates/**`, other agents'
analysis dirs, `Cargo.toml` or `scripts/build-apk.sh` were edited.

**Goal.** Prove every closed bug against real device evidence where a slot exists,
otherwise verify honest headless artifacts and state the boundary; run the magenta
regression matrix over every art-push PNG (pink = 0).

**Result in one line.** No device slot is attached, so no bug is claimed
`device-verified` on the current tree; all readable art-push frames are pink-free
(95/96 frames, 1 file is a 0-byte artifact, 1 source atlas is N/A); the only fresh
device frame is the 11:56 ready screen from the installed debug build `e74d79`.

---

## 1. Device / slot probe (observed, exact)

| Probe | Exact result | rc |
|---|---|---:|
| `printenv \| grep -i DEVICE_SLOT` | `NO_DEVICE_SLOT_ENV` (H2_DEVICE_SLOT unset) | 0 |
| `adb devices` | `List of devices attached` — empty | 0 |
| `adb get-state` | `error: no devices/emulators found` | 1 |
| `su -c 'id'` | `uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0` | 0 |
| `python3 -m scripts.qa.device check` | `installed=true version=0.1-p0 last_update=2026-11-21 12:01:28`, display 1080×2414 rot 0, `installed_sha256=e74d79ee…` | 0 |

The phone is reachable only through KernelSU root (`scripts/qa/device.py` drives
`su -c screencap/input/am`, not adb). Serial ownership (`H2_DEVICE_SLOT=1`) is not
granted to this session, so **no install, no launch, no new scenario capture was
performed**; every device statement below is either the pre-existing 11:56 frame or
inherited 2026-11-18 evidence.

### 1a. Build ↔ device sha facts

| Artifact | sha256 | Meaning |
|---|---|---|
| installed `org.wc3rust.p0` | `e74d79ee7c16…3238` | what the 11:56 run installed (`scenarios.json` apk sha = same) |
| `build/android-p0/debug/wc3rust-p0.apk` (mtime 12:34) | `3e5a2361e555…cd23` | rebuilt after the last install → **not** on the device |
| `build/android-p0/release/wc3rust-p0.apk` | `cdef59f90d1b…5cf7` | release build manifest from 2026-11-18 |
| `scenarios.json` apk sha | `e74d79ee7c16…3238` | **MATCH** with installed package |

So `device_run sha match` holds for the 11:56 run, but that run predates the current
tree: a current-tree device claim requires a slot, a fresh build/install and a re-run.

---

## 2. Fresh device evidence available (not produced by this session)

`analysis/agent-qa/ALL_LAYERS_20261121/device-frames/2026-11-21-1156-01-ready.png`

- 2414×1080 RGBA, 1 177 870 B, mtime 11:56:56, sha256 `732c9a830f16…3524` (byte copy
  of `analysis/agent-qa/operator-bugs/screens/01-ready.png`; same sha verified).
- Provenance: `device-frames/2026-11-21-1156-scenarios.json` (copy of
  `operator-bugs/scenarios.json`) records `apk.sha256=e74d79…`, `install=Success`,
  `display 2414×1080 rotation 1`, and
  `GAME_READY source=user-mpq map="Ордк01" models=145 roster=823 scope=map
  textures=870 world=28672x16384 terrain_atlas=true minimap=true`.
- **Honest limit 1:** that run ended `FAIL` *after* the ready screenshot because
  `rel(ready)` raised `ValueError: 'analysis/agent-qa/operator-bugs/screens/01-ready.png'
  is not in the subpath of '…/src/wc3rust'` — a relative `--out` leaked into
  `Path.relative_to(ROOT)`. Fixed in this generation (§4).
- **Honest limit 2:** the build is the 11:56 debug APK `e74d79`, not the current tree,
  and the map is the user-MPQ `Ордк01` (not the Melee.w3x `Justice` run of 2026-11-18).

All older device frames (`02-select-worker.png` … `10-card.png`, mtime 2026-11-18
17:33–17:39) belong to build `d3efe82e` per `operator-bugs/REPORT.md:3` and are
**inherited**, not re-shot.

---

## 3. Magenta regression matrix — every art-push PNG (2026-11-20 18:00 → now)

Tool: `scripts/qa/art_push_magenta_scan.py` (canonical masks from `scripts/qa/analyze.py`
and `tools/hud_pack_check.py`; cluster min size 120). Raw evidence:

- `magenta_scan_frames.json` — **96 frames: 95 PASS, 1 FAIL** (rc=1)
- `magenta_scan_source_atlas.json` — **1 N/A-source-atlas, exact magenta 0** (rc=0)
- `png_inventory.txt` — 97 PNGs (96 frames + atlas), mtimes and sizes
- `raw/magenta_scan_all93.json` — superseded first pass (before the 14:12 world-art
  frames existed and before the `--source-atlas` class); kept for audit, not the gate

PASS = `pink_pixels=0 ∧ pink_clusters=0 ∧ exact_magenta_pixels=0` (dims recorded).

| Group | Files | Verdict |
|---|---:|---|
| `analysis/hudskin/screens/**` | 59 | 58 PASS · **1 FAIL (0-byte)** |
| `analysis/hudskin/composition-*.png` | 2 | PASS |
| `analysis/agent-world-art/all-layers-20261121/*` (new, 14:12–14:13) | 4 | PASS |
| `analysis/agent-world-art/art-push-20261120/*` | 9 | PASS |
| `analysis/agent-hud/mock-menu-*` + `card-fix/*` + `art-push-20261120/headless-*` | 10 | PASS |
| `analysis/agent-fdf/mocks/*` (11 menu/dialog mocks) | 11 | PASS |
| `analysis/agent-qa/operator-bugs/screens/01-ready.png` | 1 | PASS |
| `analysis/agent-hud/icons/commandbuttons_atlas.png` | — | N/A-source-atlas (exact magenta 0) |

- **FAIL (real open defect, not a render):**
  `analysis/hudskin/screens/portrait-1080x2340-tooltip-1080x2340.png` is **0 bytes**
  (mtime 12:58), `UnidentifiedImageError`. Owner: **ui-screens / `hud_layout`**
  (`crates/wc3-game/examples/hud_layout.rs:110` writes this dir). Exact next step:
  rerun the example on the portrait-tooltip state after the tooltip draw path is fixed,
  require `hud_layout` rc=0 and a non-zero PNG, then re-run this scanner. QA cannot
  edit that lane's files.
- **N/A-source-atlas:** `commandbuttons_atlas.png` (1024×2816) is the extracted retail
  icon sheet — `analysis/agent-hud/INTEGRATION.md:70`: “all 691 files from
  `ReplaceableTextures/CommandButtons`”. Its 2 561 pink pixels are legitimate icon
  artwork (largest cluster 527,2584–562,2610); **exact (255,0,255) = 0**, so no
  chroma-key placeholder. Input texture, not a rendered frame — classified explicitly
  by the new `--source-atlas` mode, never silently dropped.

---

## 4. QA tool fixes in this generation (owner paths, tested)

1. **`scripts/qa/operator_scenarios.py` — device-run crash fixed.**
   Root cause (observed in the 11:56 `scenarios.json` traceback): `--out` was compared
   with `.resolve()` but the *unresolved* path was kept, so a relative `--out` produced
   relative screenshot paths and `str(path.relative_to(ROOT))` raised `ValueError`
   before `steps.ready` was recorded. Fix: `canonical_out()` always returns the absolute
   `OUT`, and `rel()` is `scripts.qa.common.rel` (resolves both ROOT spellings, accepts
   relative paths, keeps foreign paths verbatim). Docstring corrected (ten scenarios).
2. **`scripts/qa/art_push_magenta_scan.py` — explicit `--source-atlas` class.**
   Source atlases are input textures, not frames: `N/A-source-atlas`, but any exact
   (255,0,255) or unreadable file is still FAIL. Summary gains `source_atlas_na`.
3. **Tests (new, offline, no device/cargo):**
   `scripts/qa/tests/test_operator_scenarios_paths.py` (5 tests: canonical out, the
   exact relative-path crash, symlink spelling, foreign path, slot refusal rc=3) and
   `scripts/qa/tests/test_magenta_scan.py` (5 tests: clean/pink frame, atlas pink art,
   atlas chroma-key FAIL, unreadable FAIL).
   `python3 -m pytest scripts/qa/tests -q` → **18 passed in 0.99 s**; `py_compile` of
   both edited tools → **rc=0**.

---

## 5. Honest artifact classification (headless vs device)

| Artifact family | Real device capture? | Provenance |
|---|---|---|
| `operator-bugs/screens/01-ready.png` (11:56) | **YES** — fresh, build `e74d79` | `device.py screenshot` via `su screencap`; `scenarios.json` |
| `operator-bugs/screens/02…10*.png` | YES — inherited 2026-11-18, build `d3efe82e` | `operator-bugs/REPORT.md:3` |
| `analysis/hudskin/screens/*` (59) and `composition-*.png` | **NO — headless** | `crates/wc3-game/examples/hud_layout.rs:110-111` creates/writes the dir; no device code. `device-2414x1080-*` is a **viewport profile name**, not a capture |
| `analysis/agent-world-art/**` (13) | **NO — headless** | `world_frame` binary (`target/release/examples/world_frame`, built 03:53, sha `a3a2c8bb…`) via `all-layers-20261121/run_frames.sh`; `rc=0` per tag |
| `agent-hud/art-push-20261120/headless-*.png` (3) | **NO — labelled mock** | OCR reads `HEADLESS MOCK! procedural reference v1 — NOT a device screenshot` (3/3) |
| `agent-hud/mock-menu-*`, `card-fix/*`, `agent-fdf/mocks/*` (18) | **NO — headless mocks** | HUD mock generator / FDF renderer (`agent-fdf/MENUS_DIALOGS.md` “PNG (1920×1080)” table). **No on-image banner** (OCR 0/18) → label gap, owners H/FDF |

No headless mock is presented as a device screenshot anywhere in this report.

---

## 6. Per-bug verdicts (device-verified | headless-only | not-verified)

Verbatim JSON: `per_bug_verdicts.json`. `inherited` = real device evidence from an
older build, explicitly not re-run in this push.

| # | Bug | Verdict | Evidence / scope |
|---:|---|---|---|
| 1 | Units cannot move across the map | **not-verified** | device order ≥800 px + arrival still missing; headless `melee_map_paths` 10/10 is 2026-11-16, lane F in flight |
| 2 | Take the ready map from the library | **device-verified** | fresh 11:56 frame `732c9a83` + `GAME_READY source=user-mpq map=Ордк01 models=145 roster=823`; scope: user-MPQ map, debug `e74d79` |
| 3 | Units stuck in textures | headless-only | inherited device 2026-11-18; new headless `hud_layout` worker states |
| 4 | Peasant sounds like an orc | not-verified | audio lane E in flight; no fresh device audio log |
| 5 | No build menu for the worker | headless-only | `hud_layout` command-card + menu mocks; no device |
| 6 | Ugly dialogs; empty top strip | headless-only | 11 FDF menu/dialog mocks from shipped FDF/war3skins |
| 7 | No active army panel | headless-only | OCR-labelled army-panel mock 2414×1080 + `art7` rc=0 (04:06); no device `09-army-panel` |
| 8 | Hotkeys / abilities incomplete | not-verified | no fresh evidence in this push |
| 9 | HUD is bad | headless-only | 59 `hud_layout` screens over 12 viewports; no device |
| 10 | No build ghost / validity | headless-only | OCR-labelled ghost valid/invalid 2414×1080 + `art10` rc=0 (03:55); no device `05-ghost` |
| 11 | Hall foundation raised, stretched | device-verified (inherited 2026-11-18) | `operator-bugs/screens` 01/09/06, build `d3efe82e` |
| 12 | All buildings share one texture | device-verified (inherited 2026-11-18) | screens 01/06, build `d3efe82e` |
| 13 | Every unit its own voice | not-verified | lane E in flight |
| 14 | Terrain looks bad | headless-only | 4 new `world_frame` renders rc=0 (14:12–14:13) + 9 prior; no device |
| 15 | Pink/magenta triangles | headless-only this push; device-verified (inherited) | 95/96 readable frames pink=0; build `d3efe82e` device scan had 0 clusters |
| 16 | Trees/doodads absent | device-verified (inherited 2026-11-18); headless-only this push | screens 01/05/06 `d3efe82e`; new doodad frames are headless |
| 17 | Fog/overlay washes the map | device-verified (inherited 2026-11-18) | screens `d3efe82e`; no fresh frame |
| 18 | Card labels truncated | headless-only | `hud_layout` card12/hero screens; inherited device 10-card |
| 19 | Three halls stacked | device-verified (inherited 2026-11-15) | `place_matrix` STACK_CHECK + device 06-place `d3efe82e` |
| 20 | Other buildings did not place | device-verified (inherited 2026-11-15) | device GHOST/DENY log + `place_matrix`; no fresh place frame |

Scenario coverage requested by the packet:

| Scenario | This push |
|---|---|
| 09-army-panel | headless-only (mock + test); no device slot |
| 05-ghost | headless-only (mocks + test); no device slot |
| 06-place | headless-only (inherited device 06-place from `d3efe82e`); no new device frame |
| menu | headless-only (H `mock-menu-main` 2414×1080, FDF `menu_main` 1920×1080); no device frame |

---

## 7. Verification commands (rc pasted)

| Command | rc | Output |
|---|---:|---|
| `python3 -m py_compile scripts/qa/operator_scenarios.py scripts/qa/art_push_magenta_scan.py` | 0 | — |
| `python3 -m pytest scripts/qa/tests -q` | 0 | `18 passed in 0.99s` |
| `python3 -m scripts.qa.art_push_magenta_scan --json-out …/magenta_scan_frames.json $(cat frames_list.txt)` | 1 | 96 frames: 95 PASS, 1 FAIL (0-byte) |
| `python3 -m scripts.qa.art_push_magenta_scan --source-atlas --json-out …/magenta_scan_source_atlas.json commandbuttons_atlas.png` | 0 | 1 N/A-source-atlas, exact magenta 0 |
| `python3 -m scripts.qa.device check` | 0 | installed `e74d79…`, display 1080×2414, last_update 12:01:28 |
| `sha256sum` installed vs `scenarios.json` | — | both `e74d79ee7c16…3238` (match) |

---

## 8. Result, limits, next steps

**Result.** Matrix gate green for every readable art-push frame (pink = 0, exact
magenta = 0); 1 open artifact defect (0-byte hudskin tooltip); device evidence for
the current tree is **not available** without the slot — verdicts above say exactly
what was and was not proved.

**Limits (evidence-backed, not excuses).**
1. Device slot absent (`H2_DEVICE_SLOT` unset, `adb` empty); the local phone is
   root-drivable but serial ownership was not granted to this session.
2. Installed build `e74d79` is older than the current debug APK `3e5a23`; no
   current-tree device run exists.
3. 18 headless mock PNGs (H menu/card, FDF menus/dialogs) carry no on-image
   `HEADLESS MOCK` banner; provenance is documented but not self-labelled.

**Next steps (named owners, exact).**
1. **Tech lead:** grant `H2_DEVICE_SLOT=1` to agent G, then
   `scripts/build-apk.sh`, `python3 -m scripts.qa.device install/launch`, and
   `H2_DEVICE_SLOT=1 python3 scripts/qa/operator_scenarios.py` (now fixed) to capture
   real `09-army-panel` / `05-ghost` / `06-place` frames; the menu needs either a
   scenario step or H's on-device frame.
2. **ui-screens (H):** regenerate
   `analysis/hudskin/screens/portrait-1080x2340-tooltip-1080x2340.png` (0 bytes) and
   require non-zero output in `hud_layout`'s gate.
3. **HUD/FDF lanes:** add a visible `HEADLESS MOCK` banner to the 18 unlabelled
   mocks, or declare them provenance-only in their reports.

No fabricated evidence: every claim above is a path + sha/rc in this repository.

---

## 9. H2 persistence

| Item | ID / evidence |
|---|---|
| corpus preflight | `ev-a421a6a67c0eab40` (preflight ok, task_type=debug) |
| capture | **1235** (`alllayer-qa-device-20261121`, ok=true, delivered=true) |
| experiences | **4897** (relative `--out` / `relative_to` crash + fix), **4898** (magenta matrix provenance classes) |
| monitor ledger | **603** `alllayer-qa-device-20261121`, state **done**, h2_ids_json contains this table |
| dream validation | rc=0, report `~/.local/share/opencode/harness2/reports/dream-20261121-112108.json` (4897 promotable; 4898 flagged unverified until linked) |


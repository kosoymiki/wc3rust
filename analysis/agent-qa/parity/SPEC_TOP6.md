# PARITY TOP-6 — harness spec (QA)

Mission: per-feature parity gates for the six highest-leverage increments in
`analysis/tech-lead/PARITY_MATRIX.md` §7. Each gate is stable, matrix-mapped,
headless where possible, and device-measured only in the single coordinated
device cycle. Gate ids are authoritative for the tech-lead scorecard.

Snapshots: matrix `2026-11-13T07:34Z`; live-tree probe runs are stamped in
`analysis/agent-qa/parity/report.json`. Installed green build for the device
phase: `0.1-p0`, `lastUpdateTime 2026-11-13 08:04:52`, sha256
`78508c9c18addcb6e29b7009b5edcfd8792e9b5f7aac653d9fa866faf54ed232`.

## 1. Gate contract

| Gate id | Matrix row / increment | Mechanism under test | Headless acceptance | Device acceptance |
|---|---|---|---|---|
| `models.per_def` | R1/R2, §7 #1 | `wc3-game` loads a mesh per `UnitDef.model`, not per `UnitKind` | `MODEL_PATHS` count ≥ distinct roster models with `model_present=true`; report rendered/required | Human04 + one map per race renders ≥ N distinct unit meshes; screenshot set |
| `models.budget` | §7 #2 | model cache fits phone memory | n/a (no GPU in probe) | PSS ≤ 1536 MB with four-race models resident or evicted; `assets_models_min` raised from 4 to the roster count |
| `economy.lumber` | G2 | tree → chop → carry → hall; lumber debits on build/train | `harvest_lumber` deposits ≥10; zero-lumber build → `not enough lumber`; funded build debits exact `lumber_cost` | HUD lumber > 0 after one chop cycle; building card shows lumber cost |
| `map.units.spawn` | G22, §7 #4 | `war3mapUnits.doo` → sim units, count equality | `MapBundle.units(Human04).len() > 0`; `Sim::spawn_map_units` spawns entries | in-game unit count == parsed `war3mapUnits.doo` count (Human04) |
| `creeps.camps` | G18, §7 #5 | neutral camp spawn, aggro ≤500, leash home | 2 owner-2 creeps; intruder at 300 → `Order::Attack`; teleport to 2000 → return, max distance ≤950 | camp visible on map, aggro/leash log line |
| `jass.frame_loop` | G21, §7 #6 | map script executes in the game frame loop | `wc3re scan` map `main` 44/44; static check that `game.rs` constructs a script runtime | Human04 `main` runs in-game; ≥1 timer action fires; sim state mutates via script |

Status vocabulary: **PASS** = mechanism present and every measured case
succeeds; **FAIL** = mechanism present but a measured case breaks (parser/API
defect); **GAP** = WC3 rule/feature absent (matrix row still open); **SKIP** =
device-only or uncoordinated.

## 2. Phase plan

1. **Spec + headless gates** — implement `models.per_def`, `models.budget`
   (SKIP), `economy.lumber`, `map.units.spawn`, `creeps.camps`,
   `jass.frame_loop`; run `python3 scripts/qa/parity.py`; artifact
   `analysis/agent-qa/parity/report.json`. Checkpoint ledger 349.
2. **One device cycle per green build** — run
   `python3 scripts/qa/device_cycle.py --apk build/android-p0/herofolends-p0.apk
   --out analysis/agent-qa/parity/device-cycle` exactly once for the installed
   build: build a hall, train one unit, capture `getevent -lt` during an
   injected box drag, tap a known button to prove tap mapping, screenshots,
   stop app, return Termux. No retry; a failed step is recorded as evidence.
   Checkpoint ledger 349.
3. **Report** — fold device results into `report.json`/`REPORT.md`, classify
   box-select as `harness_boundary` **only with getevent evidence**, write
   rc/artifacts/H2 ids, close ledger 349.

## 3. Box-select getevent protocol

`input motionevent` (InputManager injection) may never appear in
`/dev/input/event*`; that is the boundary this proof must establish:

1. start `su -c 'timeout 8 getevent -lt /dev/input/event*' > getevent.log 2>&1 &`
2. inject `input motionevent DOWN`, 6–8 `MOVE` events, `UP` over the world;
3. wait 1.5 s, stop capture, read `getevent.log`;
4. tap the known `BASE` button and assert `BUTTON BASE` in logcat;
5. classify: box-select is a **harness boundary** when (a) tap mapping works,
   (b) injected drag produces no selection, and record whether getevent saw
   the injected events (`getevent_seen=true|false`). A physical-finger-only
   limitation is recorded as `physical_finger_boundary`, never product FAIL.

## 4. Device-cycle guardrails

- Exactly one cycle for the installed build; no blind retries.
- Only one device user at a time: check the current focus before launch; if it
  is not Termux, record it and proceed only under this authorized packet.
- Stop the app (`force-stop`), start Termux, verify
  `mCurrentFocus=com.termux/com.termux.app.TermuxActivity`.
- Artifacts: `start.png`, `after-build.png`, `after-train.png`, `box-before.png`,
  `box-after.png`, `getevent.log`, `cycle.json`, `REPORT.md`.

## 5. Rollback

Probe and runner live only in `scripts/qa/**`; no engine file is edited, so
rollback is deleting the parity crate/runner and the generated report. The
device cycle changes only app state (fresh scenario on next launch).

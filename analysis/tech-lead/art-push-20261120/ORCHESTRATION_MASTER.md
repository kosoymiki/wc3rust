# ORCHESTRATION MASTER — wc3rust all-layer art push (2026-11-21)

Parent ledger 584. Tech lead owns supervision loop; every agent has a ledger row, heartbeat,
deadline, exclusive OWNER_PATHS, and must close with rc/artifact/screenshot + h2_ids.

## Layer ownership map (NO overlaps — verify against this before any dispatch)

| Agent | Ledger | Layer(s) | EXCLUSIVE OWNER_PATHS | Status |
|---|---|---|---|---|
| A | 585 | Render terrain/water | `crates/wc3-render/src/{mesh,texture,terrain}.rs`, `crates/wc3-game/src/terrain_render.rs`, `art14_*` tests, `analysis/agent-world-art/**` | running (steer: checkpoint) |
| B | 586 | HUD / ghost / icons | `crates/wc3-game/src/{game,hud,mobile_hud,assets}.rs`, `art7_*`/`art10_*` tests, `analysis/agent-hud/art-push-20261120/**` | in_progress (steer: checkpoint) |
| C | TBD | Assets / formats | `crates/wc3-formats/**`, `crates/wc3-assetdb/**`, `crates/wc3-mpq/**`, `analysis/agent-assets/art-push-20261120/**` | to dispatch |
| D | TBD | Animation | `crates/wc3-anim/**`, `analysis/agent-anim/**` | to dispatch |
| E | TBD | Audio | `crates/wc3-audio/**`, `analysis/agent-audio/**` | to dispatch |
| F | TBD | Sim / gameplay | `crates/wc3-sim/**`, `analysis/tech-lead/mechanics-20261121/**` | to dispatch |
| G | TBD | QA / device evidence | `analysis/agent-qa/art-push-20261120/**`, `scripts/qa/**`, `scripts/visual_check.py` | to dispatch |

## READ_ONLY for ALL agents (outside every OWNER_PATHS)
- `Cargo.toml`, `Cargo.lock`, `scripts/h2-cargo`, `scripts/build-apk.sh`, `scripts/pickup-screenshots.sh`
- `analysis/tech-lead/FULL_PORT_CRITERIA.md`, `TEST_POLICY.md`, `OPERATOR_BUGS.json`, `PLAYABILITY_GAP_2026_11_16.md`
- Other agents' OWNER_PATHS (never edit or overwrite — several have uncommitted work in the tree)

## Global invariants
1. HONEST ART STANDARD: rendering/lighting/filter fixes are NOT hand-drawn Blizzard art — label honestly.
2. No stubs/TODO-as-deferred, no fake evidence, no fabricated screenshots.
3. Per-change named tests; `scripts/h2-cargo` for builds; capture exact rc/output.
4. Device slot H2_DEVICE_SLOT=1 is SERIAL — agent G owns device capture; others produce headless evidence and label it, or coordinate with G via tech lead.
5. Persist every meaningful step: harness2_capture + experience + monitor checkpoint; close with h2_ids_json.
6. Stale heartbeat >180 s is a violation — checkpoint at every phase change. Tech lead sweeps <=60 s.

## Completion reconciliation
A finished agent turn = event to reconcile within one sweep: check ledger state + evidence,
or capture missing report and relaunch with continuation packet. Notification-only is invalid.

## Evidence contract (final acceptance)
- Each agent: ART_PUSH_REPORT.md in its owner analysis dir with named tests (rc=0), before/after
  artifacts (PNG + numeric means where applicable), and h2_ids.
- Tech lead: merged report `analysis/tech-lead/art-push-20261120/FINAL_REPORT.md` with per-layer
  table, `cargo check --workspace --offline` rc, fresh-clone/static gate, and leftover-risk ledger.

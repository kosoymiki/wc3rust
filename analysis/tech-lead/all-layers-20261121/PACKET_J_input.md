# THICK GOAL — Agent J: input / touch

Repo: `~/src/herofolends-rs` (`~/src/wc3rust` symlink). Parent: all-layers push 2026-11-21.

## Mission
Touch router parity with the device: hit-testing, gestures, hotkeys, drag/box select; traces replay deterministically.

## Why (H2)
A layer is done only when its behaviour is proved against the shipped game where an oracle exists,
and its named tests pass. "Looks done" is not done.

## Evidence snapshot (observed 2026-11-21)
- `analysis/tech-lead/PORT_PARITY_ALL_LAYERS.md`: 11 parity units green; `original-parity` applicable=10 failures=0.
- `analysis/tech-lead/FULL_PORT_CRITERIA.md` P0-12 rendering parity still FAIL on device rows.
- Shipped oracle: `analysis/war3_full3` (10 523 files), `analysis/war3_mpq_listing.txt`, SLK/MDX/BLP, 44 campaign maps, `Scripts/common.j`.

## OWNER_PATHS (exclusive — only writer)
crates/wc3-input/**

## READ_ONLY
Everything else; especially other agents' OWNER_PATHS, `Cargo.toml`, `analysis/tech-lead/FULL_PORT_CRITERIA.md`,
`TEST_POLICY.md`, `MECHANICS_GAP_MAP.md`. Preserve all uncommitted work (many agents in flight).

## Phase plan
1. Read the layer's current code and tests; write the mechanism (file:line) and the root cause of any defect.
2. Fix root causes (full implementations, no stubs); add named tests (prefix with the layer slug).
3. Cross-check against the shipped original where an oracle exists (counts/values/pixels), else state the boundary.
4. Produce evidence: exact rc/output, artifacts (PNG/report paths), and a report file.
5. Persist: capture + experience + monitor checkpoint; close the ledger with h2_ids_json.

## Verification (must paste rc)
wc3-input tests rc=0; touch-trace --check traces rc=0; report analysis/agent-touch/ALL_LAYERS_20261121.md

## Known pitfalls
- Do not edit other owners' files; do not revert uncommitted work.
- HONEST ART: label headless/procedural artifacts; never claim device evidence without a device run.
- Device slot `H2_DEVICE_SLOT=1` is serial and owned by agent G; ask the tech lead.
- If blocked: exact symptom + rc + root cause + next step (device gestures need slot).

## Report format
`Goal | Actions | Evidence (rc/output/paths) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED` with numeric ids.
Unknowns are `observed|inferred|unknown`, never fabricated.

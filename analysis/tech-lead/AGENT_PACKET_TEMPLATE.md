# AGENT PACKET TEMPLATE — THICK GOAL (mandatory shape)

Every dispatch to a layer agent uses this packet. A thin prompt is an H2 violation.
The tech-lead fills every section; "n/a" is allowed only with a reason.

## 0. Header
- task_ref / ledger id / session id
- layer / owner agent
- phase (1 of N) and this packet's single objective
- deadline and checkpoint cadence (<=90 s heartbeat, phase checkpoint, escalation >10 min blocked)

## 1. Mission (why this matters)
One paragraph: the player-visible outcome this layer must produce, tied to `PARITY_MATRIX.md` row ids.
State the exact parity increment number(s) and what "done" looks like in the product.

## 2. Current evidence snapshot (observed, not remembered)
- BASE.md / PARITY_MATRIX.md row numbers and current percentages for this layer
- last green rc/tests and their exact commands
- known artifacts and their paths (screenshots/galleries/manifests)
- known broken state and exact file:line (do not re-discover from scratch)
- device status: installed APK build id/sha, lastUpdateTime, QA report path

## 3. Invariants (must never break)
- one implementation per concern; consume the canonical base, never add a second reader/parser/sim/renderer
- owner paths only; read-only paths listed; game.rs ownership explicitly stated
- original game data only (MPQ/SLK/FDF/W3E/MDX); no invented config/values
- no stubs/TODO/placeholder/fake evidence; changed functions rewritten whole
- no device run unless this packet grants one; no repo-wide builds during another build
- H2: capture/experience/checkpoint/close with numeric IDs

## 4. Scope and paths
- OWNER_PATHS: exact files/dirs this agent may write
- READ_ONLY_PATHS: files it may read but not edit
- PATCH_ONLY: changes to other owners delivered as `analysis/tech-lead/patches/<name>.patch` with exact lines
- FORBIDDEN: files/dirs and anti-patterns for this phase

## 5. Phase plan (each phase = one checkpoint)
For each phase:
- input (files/data read)
- action (specific functions to implement/rewrite, with current file:line)
- verification (exact command, expected rc/output/count/screenshot)
- artifact (path)
- rollback (how to revert if verification fails)
Phases must be independent enough that a relaunch can resume at the next one.

## 6. Hints and known pitfalls (from prior failures)
- exact previous bugs and their root causes with file:line (e.g. GPU palette bind pose, TerrainVertex layer interpolation, TouchEnd enum)
- canonical helpers to reuse (functions/modules)
- measurements that falsify a wrong approach
- performance/memory ceilings for this layer

## 7. Acceptance gates (hard)
Numbered gates with commands and pass criteria; include at least one product-level artifact
(screenshot/gallery/manifest/scan JSON). Tests alone are not acceptance for visual work.
List the exact `cargo test`/`wc3re scan`/QA commands the tech-lead will re-run.

## 8. Intervention protocol
- checkpoint ledger at every phase boundary and before/after any batch >2 min
- if a gate fails: quote exact error, root-cause, ONE fix, re-verify; no blind retry
- if blocked >10 min: checkpoint + `analysis/tech-lead/BLOCKED-<layer>.md` with symptom/evidence/next step
- never declare completion without rc/artifact; close ledger only with `h2_ids_json`
- the tech-lead will steer mid-flight with corrections; read them before the next tool call

## 9. Report format (final)
Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2_PERSISTED (capture/experience/ledger) | SESSION_CLOSED

## 10. Escalation contacts
- canonical base/merge conflicts: merge dispatcher session + BASE.md
- HUD/layout conflicts: HUD owner; game.rs owner noted per phase
- device slot: QA gate owner; only one device cycle at a time

## 11. TEST DISCIPLINE (added 2026-11-13)
Run only the gates named in this packet or in `analysis/tech-lead/TEST_GATES.md`.
No workspace-wide `cargo test`, no exploratory suites, no duplicate runs. Each gate reports the exact
command + rc + counts. Add a new gate to TEST_GATES.md before running it.
- Device: no launch/install/cycle without an explicit tech-lead grant (`H2_DEVICE_SLOT=1`); the guarded scripts exit 3 otherwise. One device cycle at a time; force-stop the app after it.


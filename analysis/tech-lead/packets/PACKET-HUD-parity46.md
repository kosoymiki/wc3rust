# THICK PACKET — HUD applies PARITY #4/#6 game-runtime patch

## 0. Header
- task_ref: `r2-re` · parent ledger **350** (agent-re, round 4) · HUD ledger **347**
- layer: HUD (`crates/wc3-game/src/game.rs` owner) · phase 4 of 4
- objective: apply the already-verified `analysis/tech-lead/patch-game-parity46.diff` and re-run the game gates on the applied file.
- checkpoint cadence: report after apply, after check, after tests; escalation only if a gate fails.

## 1. Mission
PARITY #4/#6: a loaded W3M/W3X must spawn its `war3mapUnits.doo` placements in the
game and run `war3map.j` main + `InitCustomTriggers` inside `Game::update`, so map
state (units, timers, trigger actions) drives the canonical `wc3-sim`.
Done = patch applied to `game.rs`, game lib rc 0, `map_units_spawn` passes
(90/90 Human04 placements, `main_ok`, executed triggers > 0, `world.game_time`
advances in `Game::update`), full game lib suite green.

## 2. Current evidence (observed)
- Patch: `analysis/tech-lead/patch-game-parity46.diff` (235 lines, `-p0`, only `game.rs`).
- Dry-run against current tree: rc 0.
- Temp-applied verification already done by RE:
  - `cargo check -p wc3-game --lib` rc **0** (`analysis/agent-re/check-game-patch.log`)
  - `cargo test -p wc3-game --lib map_units_spawn` **1 passed**
    (`analysis/agent-re/game-map-units-spawn.log`): 90 placements,
    `spawned+matched == 90`, `main_ok`, executed triggers > 0,
    `world.game_time` advances, sim holds map units.
- Current `game.rs` SHA-256: `7e3c0b9d65e9215ea0b232bc1f7a6862a91e507068d0ec5dc915e1045aaa7ba1`
  (unpatched; the patch was reverted byte-identically after verification).
- Full game lib suite on the unpatched tree: 91 passed / 18 ignored (base-v0-game.log).

## 3. Invariants
- Edit **only** `crates/wc3-game/src/game.rs`, and only via the patch. Do not
  rewrite/reformat other HUD code, do not touch `wc3-re`/`wc3-jass`/`wc3-sim`/
  `sim_bridge.rs`.
- The patch is the contract; do not redesign it. If it conflicts, stop and report
  the exact hunk instead of forcing.
- No stubs/TODO; no device run.

## 4. Scope
- OWNER: `crates/wc3-game/src/game.rs` (yours).
- READ-ONLY: `analysis/tech-lead/{patch-game-parity46.diff,PATCH-GAME-PARITY46.md}`,
  `crates/wc3-game/src/sim_bridge.rs`, `crates/wc3-re/**`.
- PATCH-ONLY: nothing else.

## 5. Phase plan
1. **Backup + apply**
   - `cp crates/wc3-game/src/game.rs /data/data/com.termux/files/usr/tmp/opencode/game.rs.pre-parity46`
   - `patch -p0 < analysis/tech-lead/patch-game-parity46.diff`
   - verify: `grep -n "attach_map_runtime\|JassBridge" crates/wc3-game/src/game.rs` shows
     the new fields/methods; `sha256sum` differs from the pre-patch hash.
   - artifact: `analysis/agent-re/round4-patch-apply.log`
2. **Compile**
   - `cargo check -p wc3-game --lib` (bounded env: `CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0`)
   - pass: rc 0; artifact `analysis/agent-re/round4-check.log`
3. **Acceptance test**
   - `cargo test -p wc3-game --lib map_units_spawn -- --nocapture`
   - pass: `1 passed`; artifact `analysis/agent-re/round4-map-units-spawn.log`
4. **Full game lib regression**
   - `cargo test -p wc3-game --lib`
   - pass: 0 failed (expect ≥92 passed; 18 ignored); artifact `analysis/agent-re/round4-game-lib.log`
5. **Report** the rc/artifacts/hash and the `GAME_MAP_RUNTIME` log line; do not
   revert on success. Rollback only if a gate fails:
   `patch -p0 -R < analysis/tech-lead/patch-game-parity46.diff` and restore the backup.

## 6. Hints
- The patch boots `Engine` from `Assets` map provenance, re-enters the map in
  `reset_scenario` (fixed demo scenario skipped only when a map runtime exists),
  and swaps the canonical `Sim` through `JassBridge::step_engine` each tick.
- Expected Human04 line: `GAME_MAP_RUNTIME … units=90 … world_units=92` — the two
  extra world units are script-only cinematics; the 90 placements are all present.
- Do not assert `world_units == 90`; assert `spawned + matched == 90` and per-placement presence.

## 7. Acceptance gates (hard)
1. `cargo check -p wc3-game --lib` rc 0.
2. `cargo test -p wc3-game --lib map_units_spawn` passes.
3. `cargo test -p wc3-game --lib` 0 failures.
4. `game.rs` contains `Engine`/`JassBridge` wiring and no other file changed.

## 8. Intervention
- If the patch does not apply: stop, report `patch --dry-run` output + `sha256sum`, do not hand-merge.
- Checkpoint and report after each phase; close with rc + artifact paths.

## 9. Report format
`Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2 (capture/experience/ledger)`

## 10. Escalation
- RE/patch content: agent-re ledger 350.
- Other HUD conflicts: this session (HUD owner).

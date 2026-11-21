# THICK GOAL PACKET — Agent B: army icons (#7) + build ghost (#10)

## Mission
Close operator bugs **#7 (No active army panel — selection icon row empty)** and **#10 (No build ghost / validity)** in wc3rust, then prove them on device screenshots. Repo: `~/src/herofolends-rs`.

## Parity row
Parent ledger **584** (`wc3rust-art-blizzard-20261120`). You are **Agent B**. Sibling Agent A owns terrain/water/mesh — do NOT touch its files.

## Evidence snapshot (observed, 2026-11-20)
- `analysis/agent-qa/operator-bugs/REPORT.md` row #7: "09-army-panel.png: 5 units selected, six icon slots visible but EMPTY (no unit icons)" — partial. Missing: icons rendering unit icons for the selection.
- Row #10: "05-ghost.png: hold frame shows no visible translucent building (region diff 30.8 from units/camera)" — partial. Missing: a frame where ghost model + footprint ring is visibly drawn under the finger.
- `crates/wc3-game/src/game.rs`: `icon_name_for_def`, `GhostState` (line ~509), `ARMY_TARGET_BASE=200`, `ghost_cell`.
- `crates/wc3-game/src/mobile_hud.rs`: army row's slot count depends on selection (`layout_army`), resource strip.
- Several agents' UNCOMMITTED work is in the tree (88 modified files) — do not revert or overwrite it.

## Goal + done-criteria
1. Army panel: selected units show **real unit icons** (from the game's BLP/icon assets via `Assets`), not empty slots. Labels readable at shipped short names, no truncation. Named test `art7_army_icons` (e.g. 5 selected -> 5 icon chips with def ids and distinct icon keys).
2. Build ghost: while placing, a translucent building preview (green when valid / red when invalid) with a footprint ring is visibly drawn under the pointer. Named test `art10_ghost_visible`, plus a device screenshot showing the ghost.
3. Device screenshot evidence at real size (2414x1080) for army panel and ghost; named tests pass; `cargo test -p wc3-game --lib` and `cargo check` rc=0.

## Invariants
- HONEST ART STANDARD: any procedural placeholder icon/ghost must be labelled as such; real game assets preferred; no false "Blizzard-level hand-drawn" claim.
- No stubs/TODO-as-deferred; full implementation; tests per change.
- Do NOT touch files outside OWNER_PATHS. Preserve uncommitted work from prior agents.

## OWNER_PATHS (exclusive — you are the only writer)
- `crates/wc3-game/src/game.rs`
- `crates/wc3-game/src/hud.rs`
- `crates/wc3-game/src/mobile_hud.rs`
- `crates/wc3-game/src/assets.rs`
- `crates/wc3-game/tests/**` (new `art7_*` / `art10_*` tests only)
- `analysis/agent-hud/art-push-20261120/**` (your report dir)

## READ_ONLY (do not edit)
- `crates/wc3-game/src/terrain_render.rs`, `crates/wc3-render/src/{mesh,texture,terrain}.rs` (Agent A)
- `analysis/agent-qa/operator-bugs/**`, `FULL_PORT_CRITERIA.md`
- `Cargo.toml`, `scripts/**`, `apps/**`

## Phase plan + verification
1. Read `icon_name_for_def`, army chip build path in `hud.rs`/`mobile_hud.rs`, and `Assets` icon accessors. Confirm why slots are empty (icon key resolution vs no texture vs draw skip).
2. Implement real icon resolution from `Assets` using each selected unit's def id; add test `art7_army_icons`. Verify rc=0.
3. Read `GhostState` + placement draw path in `game.rs`. Confirm why ghost invisible (tint alpha vs no draw vs footprint ring missing). Implement visible translucent ghost + green/red validity + footprint ring; test `art10_ghost_visible`.
4. Capture device screenshots (APK build via `scripts/build-apk.sh`, install to device slot H2_DEVICE_SLOT=1 if needed) at 2414x1080; save to `analysis/agent-hud/art-push-20261120/`.
5. Write `analysis/agent-hud/art-push-20261120/ART_PUSH_REPORT.md` with before/after evidence.

## Hints/known pitfalls
- Icon textures are BLP with magenta chroma key — use alpha, never colour key (`agent-hud/CONSOLE_UI_SPEC.md` §3.6).
- `ARMY_TARGET_BASE=200` maps touch index to n-th selected unit.
- Ghost MDX comes from unit def; `with_contact` / footprint ring pattern is in `mesh.rs` (Agent A owns it — read only).
- HUD tests: `crates/wc3-game/tests/hud_layout.rs` exists — extend with `art7_*` cases.
- Do not reintroduce magenta console (regression test checks magenta pixels = 0).

## Intervention protocol
Tech lead may steer via `subagent(sessionID=...)`. Report blockers with exact rc/output. Checkpoint each phase in ledger.

## Report format
`Goal | Actions | Evidence (rc/output/screenshot paths) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED`. Include your ledger id.

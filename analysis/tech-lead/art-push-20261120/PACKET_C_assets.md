# THICK GOAL — Agent C: Assets/formats layer (ledger 587)

## Mission
Supply the real game assets the HUD/ghost agents need: icon keys and BLP textures for unit army
chips (#7) and building ghost (#10). Ensure the asset DB exposes them; do NOT edit game.rs/hud.rs
(Agent B owns those).

## Evidence snapshot
- `analysis/agent-qa/operator-bugs/REPORT.md` #7: army icon row EMPTY because icons not resolved.
- `analysis/agent-assets/texture-catalog.tsv` exists (icon keys + BLP paths). `crates/wc3-game/src/assets.rs` builds models; `icon_name_for_def` in game.rs uses def id -> icon key.
- `analysis/agent-hud/CONSOLE_UI_SPEC.md` §3.6: BLP magenta is a colour key, alpha plane is authoritative.

## Goal + done-criteria
1. Catalog/verify icon keys for >=8 common unit defs (peon, footman, grunt, peasant, hall, etc.) and expose an accessor or mapping that Agent B can consume (document the exact path in your report — Agent B owns assets.rs, so provide the data/table, not the edit).
2. Ghost model BLP/alpha pipeline verified: translucent building texture has real alpha (not magenta-keyed). Named tests `artC_icon_keys`, `artC_ghost_alpha`.
3. Report table: def id -> icon key -> BLP path -> verified alpha. `cargo test -p wc3-assetdb -p wc3-formats --offline` rc=0.

## Invariants / READ_ONLY
- OWNER: `crates/wc3-formats/**`, `crates/wc3-assetdb/**`, `crates/wc3-mpq/**`, `analysis/agent-assets/art-push-20261120/**`.
- NEVER edit: `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-audio/**`, `Cargo.toml`, `scripts/**`. Preserve uncommitted work.
- HONEST ART: catalog/extraction is not "hand-drawn Blizzard art"; label honestly. No stubs.

## Verification
Named tests rc=0; report with exact asset paths; `h2oc monitor-checkpoint 587` each phase, close 587 at end with h2_ids_json. Reply H2_PERSISTED + SESSION_CLOSED with ids.

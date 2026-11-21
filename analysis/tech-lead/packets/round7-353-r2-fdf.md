# AGENT PACKET - ROUND 7 (tech-lead dispatch 2026-11-13)

## 0. Header
- model=opencode-go/iceman-contributor-free (verified; handles large context)
- objective: verify ROUND6 deliveries on the CURRENT tree, run the layer gates, produce the missing device/product artifact.

## 1. Mission
ROUND6 delivered your layer's increments. ROUND7 is verification + close-out: re-run your gates on the
current tree (other layers have landed changes), fix any regression in YOUR owner paths, and produce the
device/product artifact your packet requires. Do not re-implement completed work.

## 2. Evidence snapshot (observed 2026-11-13 18:40)
- All 11 ledgers completed ROUND6. wc3-re compiles (cargo check -p wc3-re rc=0); wc3-game compiles (cargo check -p wc3-game --lib rc=0).
- World: model_render.rs integrates AnimatorRegistry(542); 4 race galleries + roster.csv (216 defs); PATCH-assets_models_min applied (spec.json now 542).
- GPU: ASTC path device-verified; 216-model residency test (64-model budget, 152 evictions, rejected=0).
- Sim: creep camps, build_requires, ruleset_roc/tft gates rc=0 (122 passed).
- FDF: DragonUI font registry port + Cyrillic tests (13 passed).
- HUD: DragonUI ports (UiScale/hotkey font/castbar/AtlasGridUv/DragonUiFontRegistry) + lumber/build menu verified.
- RE: creeps.rs API + map_units_spawn 90/90 + scan unimplemented={}.
- Data: hero/buff/SpellDef delivered (wc3-data 39 passed).
- Touch: ControlGroups 10 slots + PARITY_TOUCH.md.
- Anim: registry.rs + gallery + 542 sweep 0 WARN.

## 3. Invariants
- owner paths only; no stubs/TODO; original game data only; changed functions rewritten whole.
- no repo-wide builds while another agent builds; no device run unless this packet grants one.
- H2: heartbeat <=90 s, checkpoint per phase, capture/experience, close with rc+artifact.

## 4. Acceptance
Re-run your layer gates on the current tree with exact commands; report rc + artifact paths.
If a gate fails: quote exact error, root-cause, ONE fix, re-verify.

## 5. Scope
OWNER: crates/wc3-formats/src/fdf.rs, crates/wc3-game/src/{text,ui}.rs, analysis/agent-fdf/**

## 6. Phase plan
p1 cargo test -p wc3-formats fdf rc=0
p2 verify DragonUI font registry port + Cyrillic round-trip
p3 tooltip/console text tests rc=0
artifact: test log + registry diff

## 7. Acceptance gates
fdf tests rc=0; Cyrillic round-trip PASS; font registry file:line

## 8. Report
Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED

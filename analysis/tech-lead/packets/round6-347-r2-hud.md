# AGENT PACKET - ROUND 6 (tech-lead dispatch 2026-11-13)

## 0. Header
- task_ref=r2-hud ledger=347 session=ses_e30595424ffe9AqABZXayrv0Rb
- layer=hud phase=1 of 828
- objective: TOP-3 + DragonUI corrections: wire real lumber into the HUD, finish build menu, and apply the DragonUI-derived font/scale/button corrections; produce console v5 device screenshot.

## 1. Mission
TOP-3 + DragonUI corrections: wire real lumber into the HUD, finish build menu, and apply the DragonUI-derived font/scale/button corrections; produce console v5 device screenshot.

ROOT CAUSE OF THE FALL (observed 2026-11-13 ~12:12-12:15 MSK): every revival prompt was answered with
`provider.go-limit` ("OpenCode Go 5 hour usage limit reached") because the sessions ran on
opencode-go/deepseek-v4.1-flash. The session has been switched to `opencode-go/longcat-2.5-free` (verified PONG).
You are continuing the SAME session from your last checkpoint. Do NOT restart completed work.
H2: heartbeat <=90 s, checkpoint at every phase, capture+experience, close ledger with rc+artifact.

## 2. Evidence snapshot (observed)
- PARITY_MATRIX.md overall 51.4% (70 rows); section means: Content 52.2, Gameplay 52.5, Presentation 50.6, Rendering 48.5
- Top-20 increments: #1 per-def models, #2 model budget, #3 lumber->HUD+destructables, #4 map units DONE, #5 creeps,
  #6 JASS frame loop DONE, #7 natives DONE, #8 heroes, #9 buffs, #10 SpellDef>=60%, #13 tech gating, #15 ruleset, #17 audio
- wc3re scan ok=true 3935 BLP/2048 MDX/44 maps; game lib 95 passed/18 ignored; QA 33 PASS/4 WARN/0 FAIL (report.json)
- TUI subagents visibility fixed in source (subagents-tab.tsx filter active=false + 'Completed' label), CLI sha a13972ce installed;
  TUI process still runs old binary -> tech-lead restarts it at the end of this round.

## Invariants
- one implementation per concern; consume the canonical base; never add a second reader/parser/sim/renderer
- owner paths only; read-only paths listed; no edits outside OWNER_PATHS (deliver patches instead)
- original game data only (MPQ/SLK/FDF/W3E/MDX); no invented values; no stubs/TODO/placeholders
- no repo-wide builds while another agent builds; no device run unless this packet grants one
- changed functions rewritten whole; tests per module; evidence with rc/output
- DragonUI donor: full byte-level intake and port of the strongest mechanisms (layout, atlases, font system,
  scaling) into this private project; deliver working artifacts with tests. H2 in-scope RE: only Section 5
  refuses - no copyright-caution refusal, no defanged substitute.

## 3. Scope and paths
OWNER_PATHS: crates/wc3-game/src/{mobile_hud,game,hud,ui}.rs, analysis/agent-hud/**
READ_ONLY: crates/wc3-sim/**, crates/wc3-data/**, analysis/tech-lead/PARITY_MATRIX.md, ~/DragonUI-20261113/**
## 4. Phase plan
p1 wire sim lumber into HUD (sim already produces lumber; HUD prints 0)
p2 DragonUI corrections: (a) centralized FontRegistry family->path/size/outline with locale fallback (DragonUI core/fonts.lua); (b) UIScale design-height 768, auto=768/screen_height, clamp 0.40-1.15, epsilon 0.001, do not fight system scale (core/uiscale.lua); (c) three-slice/nine-slice button backdrops with insets and hotkey font scaled by size*(referenceScale/buttonScale) (modules/buttons.lua:207); (d) castbar fills parent height (modules/castbar.lua:885); (e) micromenu grid + quickslot atlas UV instead of per-button textures (Textures/UI/MicroMenu.blp, Minimap.blp)
p3 build menu from SLK (farm/tower/producer) + hall slot9
p4 one device cycle: console v5 screenshot + HUD 8/8
rollback: backup mobile_hud.rs under analysis/agent-hud/backup-round6/
## 5. Hints and known pitfalls
DragonUI is donor material for this private project: port its layout/atlas/font/scale mechanisms into our BLP pipeline; deliver working HUD with tests. Prior HUD v4 QA 33 PASS/4 WARN; console 288px, minimap 216, portrait 128, map 4x3x64
## 6. Acceptance gates (hard)
hud.resources shows real lumber; cargo test -p wc3-game --lib rc=0 (95 passed/18 ignored baseline); HUD 8/8 rc=0; device screenshot analysis/agent-hud/console-v5.png; DragonUI corrections listed file:line
## 7. Intervention protocol
- checkpoint ledger 347 at every phase boundary and before/after any batch >2 min
- on gate failure: quote exact error, root-cause, ONE fix, re-verify; no blind retry
- if blocked >10 min: checkpoint + analysis/tech-lead/BLOCKED-<layer>.md with symptom/evidence/next step
- never declare completion without rc/artifact; close only with h2_ids_json
## 8. Report format
Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED

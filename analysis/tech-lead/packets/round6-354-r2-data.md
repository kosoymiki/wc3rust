# AGENT PACKET - ROUND 6 (tech-lead dispatch 2026-11-13)

## 0. Header
- task_ref=r2-data ledger=354 session=ses_e3059541fffejS1YGmXDWBw12Q
- layer=data phase=1 of 329
- objective: TOP-8/#9/#10: hero leveling/attributes/skill points/revive, buff/aura framework, and data-driven SpellDef from AbilityData (>=60% melee IDs).

## 1. Mission
TOP-8/#9/#10: hero leveling/attributes/skill points/revive, buff/aura framework, and data-driven SpellDef from AbilityData (>=60% melee IDs).

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
OWNER_PATHS: crates/wc3-data/**, crates/wc3-formats/src/gamedata.rs, analysis/agent-data/**
READ_ONLY: crates/wc3-sim/**, analysis/war3_data/**, analysis/war3x_data/**
## 4. Phase plan
p1 hero attrs/level from UnitBalance/UnitData (STR/INT/AGI/plus), skill points, revive at Altar from MiscGame
p2 buff/aura framework (154 buffs) - armor aura changes apply_damage; slow/burn durations tick and expire
p3 SpellDef generator from AbilityData SLK (levels 1-3, area/cost/cooldown columns)
rollback: backup edited files
## 5. Hints and known pitfalls
data is 48/50 units, 47/49 buildings, 273/273 items, 55/55 upgrades; abilities 140->11 melee; use real SLK columns, not invented values
## 6. Acceptance gates (hard)
cargo test -p wc3-data rc=0; qa.heroes.leveling (kill->level2, +1 skill point, HP/mana per UnitBalance, revive cost); qa.buffs (Devotion Aura armor, slow/burn expiry); qa.abilities.coverage >=60%
## 7. Intervention protocol
- checkpoint ledger 354 at every phase boundary and before/after any batch >2 min
- on gate failure: quote exact error, root-cause, ONE fix, re-verify; no blind retry
- if blocked >10 min: checkpoint + analysis/tech-lead/BLOCKED-<layer>.md with symptom/evidence/next step
- never declare completion without rc/artifact; close only with h2_ids_json
## 8. Report format
Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED

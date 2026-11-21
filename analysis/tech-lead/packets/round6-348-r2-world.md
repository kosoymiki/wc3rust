# AGENT PACKET - ROUND 6 (tech-lead dispatch 2026-11-13)

## 0. Header
- task_ref=r2-world ledger=348 session=ses_e30595423ffdGkcohq9P8MIxiL
- layer=world phase=1 of 323
- objective: TOP-1/#2: verify per-definition model loading (>=216 four-race MDX parse+upload) and the model budget/eviction (<=1536 MB) with the AnimatorRegistry from anim.

## 1. Mission
TOP-1/#2: verify per-definition model loading (>=216 four-race MDX parse+upload) and the model budget/eviction (<=1536 MB) with the AnimatorRegistry from anim.

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
OWNER_PATHS: crates/wc3-game/src/{terrain_render,render,model_render,doodads}.rs, analysis/agent-world/**
READ_ONLY: crates/wc3-anim/**, crates/wc3-gpu/**, crates/wc3-formats/src/mdx.rs
## 4. Phase plan
p1 verify analysis/agent-world/parity/models-per-def.ppm and parity logs exist (they do - re-stat)
p2 MeshAsset keyed by UnitDef.model with LRU eviction; reuse AnimatorRegistry from anim
p3 memory ceiling test with all race models resident-or-evicted
p4 gallery per race + roster CSV
rollback: backup render/model_render.rs
## 5. Hints and known pitfalls
models-per-def.ppm already exists (verified); the missing piece was the registry/budget; CPU skinning already fixed bind pose; Adreno palette path in wc3-gpu
## 6. Acceptance gates (hard)
cargo test -p wc3-game --lib models_per_def rc=0 (>=216 MDX parse+upload); perf.memory <=1536 MB on Human04; assets_models_min raised 4->542; gallery PNG + roster CSV
## 7. Intervention protocol
- checkpoint ledger 348 at every phase boundary and before/after any batch >2 min
- on gate failure: quote exact error, root-cause, ONE fix, re-verify; no blind retry
- if blocked >10 min: checkpoint + analysis/tech-lead/BLOCKED-<layer>.md with symptom/evidence/next step
- never declare completion without rc/artifact; close only with h2_ids_json
## 8. Report format
Goal | Actions (file:line) | Evidence (commands+rc+counts) | Result | Risks/Next | H2_PERSISTED | SESSION_CLOSED

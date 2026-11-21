# PARITY_MATRIX deltas — ROUND7 (QA)

| Matrix row | Previous | Now | Evidence |
|---|---|---|---|
| §7 #1 per-def models | GAP 4/88 | **PASS 216/216** (214 loaded + 2 meshless, 0 missing) | parity `models.per_def` |
| §7 #2 model budget | SKIP | **PASS** peak 73.4/1536 MB, LRU 98 evictions | `analysis/agent-qa/parity/models-per-def.log` |
| §7 #3 lumber economy | PASS 3/3 | PASS 3/3 | parity `economy.lumber` |
| §7 #4 map placed units | FAIL 0/44 | **PASS 44/44** | `analysis/agent-qa/r2/report.json` |
| §7 #5 creep camps | PASS 3/3 | PASS 3/3 | parity `creeps.camps` |
| §7 #6 JASS frame loop | GAP | **PASS** | parity `jass.frame_loop` |
| G5 tech requires | GAP | **PASS 2/2** | parity `tech.requires` |
| G13 ability coverage | 11/147 | **PASS 154/154** data-backed (115 active castable, 11 hand-written) | parity `cast.coverage` |
| G15 hero leveling | GAP | **PASS 12/12** XP->level, attribute growth, set_level | parity `hero.leveling` |
| G16 item gaps | GAP | **PASS 4/4** (240/240 ability items, shop Requires gating) | parity `items.gaps` |
| G21 JASS scripts | FAIL 39/45 | **PASS 45/45** (+ return 3/3, elseif 1/1) | parity `triggers.*` |
| P15/P16 audio wiring | GAP | **PASS** (wc3-audio dependency + 11 call sites) | parity `audio.wired` |
| C2 train links | FAIL 117/119 | **PASS 121/121** | parity `train.menu_linkage` |
| P10 resource bar | GAP | **PASS** | parity `hud.resources` |
| Device P0 cycle | PASS (ROUND6 APK) | **NOT_RUN** (operator using the phone) | `analysis/agent-qa/parity/p0-device/DEVICE-NOT-RUN.md` |

Full status: `analysis/agent-qa/parity/report.json`; human report: `analysis/agent-qa/parity/ROUND7-QA.md`.

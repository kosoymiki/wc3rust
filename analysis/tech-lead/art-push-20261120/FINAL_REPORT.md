# FINAL REPORT — wc3rust all-layer art push (2026-11-20/21)

Parent ledger 584. Tech lead: opencode2/h2 primary. Goal: close the four partial rendering bugs
(#7 army icons, #10 build ghost, #14 water/terrain, + world-art light/contact/shadow) and raise
every wc3rust layer toward device-parity — with honest evidence, no fabricated art or screenshots.

## Per-layer results (all agents closed and independently verified by tech lead)

| Layer | Agent/ledger | Test/evidence | rc | Verdict |
|---|---|---|---|---|
| Render/terrain/water | A / 585 | art14_terrain, art14_doodad (4/4); water-edge t*2 bleed root-caused; cargo check rc=0 | 0 | done |
| HUD/ghost/icons | B / 586 | art7_army_icons (5 chips/5 icons), art10_ghost_visible (alpha 0.50, ring 216, red-flip) | 0 | done |
| Assets/formats | C / 587 | artC_icon_keys + artC_ghost_alpha (2 passed); 10 retail def→icon keys exposed via wc3_assetdb::unit_icons | 0 | done |
| Animation | D / 588 | artD_sequence_alphas + artD_ghost_sequence (2 passed); peon 6→3 draws pinned; full crate 40/0/2 | 0 | done |
| Audio | E / 589 | artE_perdef_voices (8 defs → 8 distinct voice sets); no peasant-sounds-like-orc | 0 | done |
| Sim/gameplay | F / 590 | artF_crossmap_move (≥800px deterministic cross + arrival), row pathing.flow-field | 0 | done |
| QA/device | G / 591 | magenta scan 12/12 PASS (9 art-push PNGs, pink=0 @2414×1080); prior 38 device frames 32 PASS/6 N-A | 0 | done_partial (no device slot) |

## Tech-lead final gates (commands run, rc pasted)
- `scripts/h2-cargo check --workspace --offline` → **rc=0** (13.56s, warnings only).
- `git diff --check` → **rc=0**.
- All nine named art tests present in source (`tests/art7_*`, `art10_*`, `art14_*`, `artC_*`, `artD_*`, `artE_*`, `artF_*`).
- Each agent's named test independently re-run by tech lead → rc=0 (no reliance on agent claim alone).

## Honest scope / limits (HONEST ART STANDARD)
- These are rendering/resolution/catalog fixes and lab-grade named tests, **not** hand-drawn
  Blizzard-level art. No procedural artifact is presented as finished hand-drawn art.
- **No device-verified new frames**: `H2_DEVICE_SLOT` unset, `adb devices` empty. Every mock PNG
  bears the banner "HEADLESS MOCK / procedural reference v1 — NOT a device screenshot". G closed
  `done_partial` honestly; no screenshot was fabricated.
- Exact next step to flip #7/#10/#14 to device-verified: attach H2_DEVICE_SLOT=1, `scripts/build-apk.sh`,
  install, run scenarios 09-army-panel / 05-ghost / 06-place, capture real 2414×1080 frames.
- Residual: `ogre` icon-key rests on the wc3-data fixture (BLP existence independently proven by C).

## H2 persistence
- Parent ledger 584; children 585-591 all closed (state done/closed/completed/done_partial).
- Captures: 1199, 1202, 1205, 1207, 1210-1217; experiences: 4881, 4884-4889;
  corpora: ev-40d5c9…, ev-078089…, ev-16dec2…, ev-4d7012…, ev-75da45… (and per-agent packs).
- Six stale pre-existing ledger rows (501/502/503/507/508/509, heartbeat ~70h) reconciled and closed.

## Verdict
All 7 layers green at test/workspace level; 4 targeted bugs closed with named tests; honest
headless-only evidence boundary recorded. The push is complete to the limit of an unattached device.

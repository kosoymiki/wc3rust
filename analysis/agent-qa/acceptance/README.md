# Acceptance harness — single base, 1:1 console

This directory is the QA layer for `analysis/tech-lead/BASE.md` §12. It measures
the **canonical base** (console HUD, real terrain/model assets, original data)
and reports machine-readable PASS/FAIL/BLOCKED evidence.

No runtime code lives here. The harness reuses `scripts/qa/` for device control
and image metrics, and reads the game's own log lines plus `wc3re` scan/map
audits. It never re-implements a reader, parser, renderer or sim.

## Files

| Path | Purpose |
|---|---|
| `spec.json` | machine-readable gate list, budgets, expected counts, conflicts |
| `run_acceptance.py` | orchestrator: freeze guard → launch → capture → gates → report |
| `console_layout.py` | facade to `scripts/qa/layout.py` (no second geometry copy) |
| `patches/0001-app-hud-rects-log.patch` | app logs exact `GAME_HUD` / `GAME_HUD_ART` (read-only patch; dispatcher applies) |
| `patches/0002-terrain-source-log.patch` | terrain logs `source=terrainart\|fallback` |
| `patches/0003-anim-acceptance-test.patch` | headless 542-MDX Animator fixture test |
| `patches/0004-audio-acceptance-test.patch` | headless real-SoundInfo/GameAudio fixture test |
| `fixtures/` | `anim_report.json`, `audio_report.json` produced after freeze |
| `runs/<UTC>/` | `report.json`, `summary.txt`, screenshots, logs, fixtures |

## Modes

```bash
# Safe: writes a plan, no device/build/test
python3 analysis/agent-qa/acceptance/run_acceptance.py --plan

# After the dispatcher announces BASE.md freeze (Status: **FROZEN**) and install:
BASE_FROZEN=1 python3 analysis/agent-qa/acceptance/run_acceptance.py --execute \
  --apk build/android-p0/herofolends-p0.apk

# One device acceptance after HUD/world land (screenshots only; no install/build)
BASE_FROZEN=1 QA_DEVICE_ONLY=1 python3 analysis/agent-qa/acceptance/run_acceptance.py \
  --execute --device-only --apk build/android-p0/herofolends-p0.apk

# Generate the post-freeze headless fixtures (tests are opt-in)
BASE_FROZEN=1 python3 analysis/agent-qa/acceptance/run_acceptance.py --execute --run-headless
```

`--execute` exits 3 and does nothing when the base is not frozen. Device runs,
install and tests are forbidden until then. `--device-only` additionally
requires `QA_DEVICE_ONLY=1` (dispatcher's explicit go) and refuses to install or
build; it captures screenshots + logs only.

Device-only gate set (`spec.json:device_once`): console grid, no joystick,
selection + placement, animation frame motion, terrain layers/seams/holes/
doodads, FPS/tick. Screenshots: start, start annotated, input-select(+annotated),
input-build(+annotated), frame-1/frame-2.

## Gate groups

1. **launch** — package/activity, `GAME_FIRST_FRAME` 2414x1080 fullscreen,
   launch ≤20 s, Vulkan/Turnip, no `GAME_FATAL`/abort/tombstone.
2. **map_load** — `GAME_READY source=user-mpq` (not fallback), Human04,
   16384x16384, assets, `wc3re scan` exact BASE counts, `wc3re map` terrain/
   object/script evidence.
3. **hud_grid** — 1:1 console: 288 px console (current source is 300 → FAIL),
   216 minimap, 128 portrait, 4×3 64 px card with 4 px gaps, resources inside
   the bar, 32 px top strip, no joystick, real `HumanUITile01..04` +
   `CommandButtonBorder` + resource BLP indices (`GAME_HUD_ART`), text ≥10 px
   and ≥3:1.
4. **terrain_layers** — `GAME_TERRAIN layers≥4 source=terrainart`, W3E/asset
   tile evidence, no fallback, seam amplitude/ratio, flat patches, black/white/
   pink holes, `GAME_DOODADS` parsed/placed with 0 failures.
5. **model_animation** — headless fixture: all 542 unit/building MDX parse,
   Stand playback, skin indices in range, matrices move; device: two frames
   0.6 s apart differ in unit tint boxes.
6. **input** — select, move, pan, pinch, train (hall → card slot 3), build
   (worker → slot 7, cost charged), attack-move (ARMY → slot 4), each proven by
   `GAME_STATE`/`ORDER`/`BUTTON` logs and screenshots.
7. **audio** — device `GAME_AUDIO` + `wc3-audio` dependency (currently absent →
   FAIL), headless `GameAudio` with real SoundInfo/unitUI/WAV fixture.
8. **performance** — FPS min ≥30/avg ≥45, tick advance, PSS ≤1.5 GB, APK size.

## Threshold conflicts recorded in `spec.json`

- `mobile_hud.rs:20 CONSOLE_HEIGHT=300` vs `ACCEPTANCE_1TO1.md:5` 288 → gate
  fails until HUD owner lands 288 (or dispatcher updates target).
- `ASSET_COVERAGE.md` 48 dp targets vs 1:1 64 px cells (21.3 dp at 480 dpi) →
  48 dp is advisory only; console geometry is authoritative.
- Previous mobile text floor 24 px vs console Tokens 11/13/20/22 px → 1:1 sizes
  + contrast are authoritative.
- `GAME_READY models=4` vs 542 unit/building MDX → device gate allows ≥4;
  headless fixture must prove 542.
- `crates/wc3-game/Cargo.toml` has no `wc3-audio`; audio gate is expected FAIL
  until integration lands.

## Evidence rules

Every gate names its source `file:line` and artifact path. `artifact.completeness`
fails if a PASS gate's artifact is missing. Blocked gates (missing headless
fixture or read-only log patch) make the overall result PARTIAL, never PASS.

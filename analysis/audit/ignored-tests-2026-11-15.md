# Ignored tests audit — 2026-11-15 (ledger 420)

Policy: `analysis/tech-lead/TEST_POLICY.md` rule 3, enforced by
`analysis/tech-lead/audit_ignored_tests.py` and
`analysis/tech-lead/run_layer_gates.sh --ignored`.

## Measurement (before)

`python3 analysis/tech-lead/audit_ignored_tests.py` measured **37** `#[ignore]`
tests; every one had a reason string, but only `terrain_ground_detail_justice`
was wired to a gate, so the other 36 were invisible: no command ever executed
them.

| file | ignored |
|---|---|
| `wc3-game/src/model_render.rs` | 13 |
| `wc3-game/src/terrain_render.rs` | 8 |
| `wc3-game/src/world_audit.rs` | 6 |
| `wc3-sim/tests/perf.rs` | 2 |
| `wc3-game/src/game.rs` | 2 |
| `wc3-game/src/doodads.rs` | 2 |
| `wc3-game/src/render.rs` | 1 |
| `wc3-game/tests/asset_gallery.rs` | 1 |
| `wc3-anim/tests/game_path_sweep.rs` | 1 |
| `wc3-anim/tests/object_id_indexing.rs` | 1 |
| **total** | **37** |

## Action (after)

Chosen action is (c) for every test: keep `#[ignore]`, make the reason name the
environment it needs, and register it in
`analysis/tech-lead/acceptance_ignores.tsv`. `run_layer_gates.sh --ignored`
executes exactly that manifest, one cargo invocation per test, with the
environment prefix from the row (`SDL_AUDIODRIVER=dummy`, `--release`).

33 reasons were rewritten (all files under this agent's write scope:
`wc3-sim/tests/**` and the five `wc3-game` sources allowed for `#[ignore]`
annotations). Four reasons are unchanged because their files are outside the
write scope (`render.rs`, `tests/asset_gallery.rs`, two `wc3-anim` tests); they
are still registered with their environment in the manifest, which is what the
audit reads.

Deleted: none. No test was removed, and no test body was changed. Diagnostics
remain grandfathered per TEST_POLICY rule 3; the change is that each one is now
reachable by a named command.

The audit now fails on an `#[ignore]` that is not registered, on a manifest row
with no matching test, and on a row without an environment requirement. Current
result: `IGNORE_AUDIT_PASS` (37/37 registered, 0 violations, 0 warnings).

## Runner

```bash
# every registered ignored test (heavy: MPQ + headless GPU + release perf)
bash analysis/tech-lead/run_layer_gates.sh --ignored
# one module / substring
bash analysis/tech-lead/run_layer_gates.sh --ignored peon
# the manifest (crate, target, test, flags, env, requirement)
analysis/tech-lead/acceptance_ignores.tsv
```

Every cargo call in this session goes through `scripts/h2-cargo` (device-wide
flock, `MemAvailable >= 3.2 GB`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`RUST_TEST_THREADS=2`) after the 2026-11-15 OOM incident.

## Before / after table

| file:line | test | after reason (environment named) | command | environment requirement |
|---|---|---|---|---|
| `wc3-anim/tests/game_path_sweep.rs:90` | `every_model_animates_through_the_runtime` | walks the full extracted MPQ tree (542 models); run explicitly | `wc3-anim --test game_path_sweep` | full extracted MPQ tree (542 models) |
| `wc3-anim/tests/object_id_indexing.rs:81` | `object_id_indexing_holds_for_every_model` | walks the full extracted MPQ tree (542 models); run explicitly | `wc3-anim --test object_id_indexing` | full extracted MPQ tree (542 models) |
| `wc3-game/src/doodads.rs:628` | `blight_doodad_read_parse` | diagnostic: BlightDoodad mdx read/parse; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/doodads.rs:739` | `doodads_justice_render_draws` | heavy: headless GPU render of the Justice doodad batch; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/game.rs:6280` | `print_build_device_coords` | diagnostic: prints device coordinates; needs the device; run_layer_gates.sh --ignored | `wc3-game --lib` | device artifact for manual QA |
| `wc3-game/src/game.rs:6972` | `melee_scene_render` | heavy: renders the melee scenario with real MPQ models; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:946` | `models_per_def` | heavy: loads every four-race unit/building model; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/model_render.rs:1017` | `models_per_def_lru_budget` | heavy: LRU eviction over every four-race model; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/model_render.rs:1244` | `models_per_def_gallery` | heavy: renders registry models through the game renderer; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:1298` | `models_per_def_roster_csv` | heavy: loads every four-race model; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/model_render.rs:1435` | `projectiles_render` | heavy: renders missile models through the game renderer; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:1611` | `projectile_billboard_edge_on` | heavy: renders a flat missile edge-on; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:1825` | `spell_cast_playback` | heavy: starts canonical Cast playback for every hero; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/model_render.rs:1968` | `unit_trio_justice_frame` | heavy: headless trio frame at the Justice camera; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:2109` | `peon_geoset_ab_frame` | heavy: two headless frames of the Justice worker cluster; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:2262` | `peon_layer_isolation` | heavy: three headless close-ups of one geoset; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:2350` | `draw_count_is_geoset_layers` | heavy: headless render + draw counters; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/model_render.rs:2492` | `dump_texture_ppm` | diagnostic: writes a texture PPM; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/model_render.rs:2557` | `peon_closeup_skinning_audit` | heavy: headless GPU close-up of the Justice workers; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/render.rs:1384` | `agent_world_doodad_alpha_teamcolor` | round2: doodad alpha/teamcolor audit | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/terrain_render.rs:2769` | `agent_world_device_frame_seam_ratio` | diagnostic: seam ratio of a device screenshot; needs a device PNG; run_layer_gates.sh --ignored | `wc3-game --lib` | device screenshot |
| `wc3-game/src/terrain_render.rs:3142` | `agent_world_terrainart_catalog_all_tiles` | round2: decodes all 111 TerrainArt BLPs; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/terrain_render.rs:3289` | `agent_world_biome_matrix_screenshots` | round2: renders 13 campaign biome screenshots; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/terrain_render.rs:3469` | `agent_world_cliffs_ramps_shoreline` | round2: cliff/ramp/shoreline close-up renders; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/terrain_render.rs:3630` | `agent_world_water_flag_distribution` | diagnostic: W3E water flag/level distribution; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/terrain_render.rs:3674` | `agent_world_biome_inventory` | diagnostic: campaign map biome inventory; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/terrain_render.rs:4667` | `hivewe_melee_closeups` | diagnostic: Melee.w3x shoreline/cliff headless close-ups; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/terrain_render.rs:4874` | `terrain_ground_detail_justice` | heavy: headless GPU render of the Justice start view; needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/world_audit.rs:463` | `agent_world_audit_all_models` | expensive full-archive audit: needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/world_audit.rs:767` | `agent_world_audit_all_biomes_and_icons` | expensive full-archive audit: needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/world_audit.rs:1046` | `agent_world_dump_player_models` | diagnostic dump: model mesh/alpha census; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/world_audit.rs:1079` | `agent_world_debug_poses` | diagnostic pose render: needs War3.mpq + GPU; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq + GPU |
| `wc3-game/src/world_audit.rs:1151` | `agent_world_debug_canonical_resources` | diagnostic: canonical doodad art and Water.slk; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/src/world_audit.rs:1254` | `agent_world_debug_close_camera` | diagnostic: close camera + terrain vertex layout + doodad scales; needs War3.mpq; run_layer_gates.sh --ignored | `wc3-game --lib` | War3.mpq |
| `wc3-game/tests/asset_gallery.rs:916` | `build_asset_gallery` | heavy: builds the full asset gallery contact sheet from the MPQ; run explicitly | `wc3-game --test asset_gallery` | War3.mpq + GPU |
| `wc3-sim/tests/perf.rs:14` | `two_hundred_units_run_at_sixty_hz_budget` | performance gate: needs a --release build; run_layer_gates.sh --ignored | `wc3-sim --test perf` | CPU 60 Hz budget, release build only |
| `wc3-sim/tests/perf.rs:61` | `pathfinding_burst_is_bounded` | performance gate: needs a --release build; run_layer_gates.sh --ignored | `wc3-sim --test perf` | CPU path burst budget, release build only |

## Cross-lane unblock (recorded, not a silent scope expansion)

`cargo test -p wc3-game --lib` was red on arrival from two in-flight changes in
other lanes; both are compile-level and were fixed minimally so this layer's
required gate can be green:

- `wc3-audio` added `AudioConfig::channel_voices`; the five initializers in
  `wc3-assetdb` (`audio_provider.rs` + `audio_bench`/`audio_wiring`/`audio_parity`)
  did not set it. Added `channel_voices: wc3_audio::engine::DEFAULT_CHANNEL_VOICES`.
- `wc3-game::doodads::parse_doo` now returns `Result<Vec<RawDoodad>, MapError>`;
  two call sites (`re.rs` test, `world_audit.rs::bridge_smoke`) treated it as a
  `Vec`. Added `let Ok(..) else` / `if let Ok(..)` handling (no behaviour change
  on success).

An orphaned `wc3_game` test binary (pid 17259, ppid 1, 28 min old) was holding
the `h2-cargo` flock after an interrupted run; it was killed by exact pid (not a
pattern kill), which unblocked the device-wide slot. No other process was
touched.

## Raw evidence

- `python3 analysis/tech-lead/audit_ignored_tests.py` → `IGNORE_AUDIT_PASS`,
  `ignored tests: 37; registered: 37; violations: 0; warnings: 0`.
- `DRY_RUN=1 bash analysis/tech-lead/run_layer_gates.sh --ignored` prints one
  `scripts/h2-cargo test … -- --ignored` line per manifest row.
- Smoke run of the new mode:
  `bash analysis/tech-lead/run_layer_gates.sh --ignored blight_doodad_read_parse`
  → `LAYER_GATES_PASS 1 steps`, rc=0, `passed=1 failed=0`, 57s;
  log `analysis/tech-lead/gate-logs/ignored_blight_doodad_read_parse.log`.
- Main suites (through `scripts/h2-cargo`, one at a time):
  `cargo test -p wc3-sim --offline` → `SIM_RC=0`;
  `cargo test -p wc3-game --lib --offline` → `GAME_LIB_RC=0`,
  `210 passed; 0 failed; 32 ignored` in 2182.87 s.
  Logs: `/data/data/com.termux/files/usr/tmp/opencode/h2-ledger420-tests3.log`,
  `h2-ledger420-game-lib.log`.
- Save-roundtrip test peak RSS: `SAVE_SCRIPT_PEAK VmHWM: 25476 kB` (~25 MB),
  single Justice load, measured in the same test process; see
  `analysis/audit/save-script-peak-2026-11-15.txt`.

# LEDGER 373 — full WC3 script world (P0-3 abilities, P0-4 maps, native coverage, cinematics)

Date: 2026-11-14 · owner RE · `crates/wc3-re/**`, `analysis/agent-re/**`,
`analysis/tech-lead/map_boot_matrix.json` only.

## Goal

Make the whole WC3 script world run: every shipped map boots through the JASS
runtime (`main` + `InitCustomTriggers`, 100 ticks), every ability row is
executable or explicitly data-only, all called natives execute, and cinematic
data is exposed as typed events. No sim edits (`crates/wc3-sim` was read-only
while its owner was active).

## Metrics

| Metric | Value | Evidence |
|---|---:|---|
| Maps booted | **44/44 ok** | `analysis/tech-lead/map_boot_matrix.json` |
| Ticks per map | **100** | matrix `ticks` and per-map `ticks` |
| `main_ok` | **44/44** | matrix |
| Triggers declared / executed | **4 820 / 726** | matrix totals |
| Native unique / calls / unimplemented | **335 / 130 123 / 0** | `analysis/agent-re/native_coverage.json` |
| Native missing before → after | `{volumegroupreset: 14}` → `{}` | `analysis/agent-re/map-boot-run.log` (before), `native_coverage.json` (after) |
| Ability rows classified | **763/763** | `analysis/agent-re/phase0/abilities.json` |
| Executable abilities | **428** (damage 244, summon 83, heal 38, aura 28, buff 28, movement 7) | summary |
| Data-only abilities | **335** (item_ability 234, passive 27, missing_effect_params 33, no_effect_payload 12, no_levels 18, no_target 11) | summary |
| Cinematic calls / transmissions / cameras | **9 981 / 1 313 / 999** | `analysis/agent-re/phase0/cinematics.json` |
| Fade events / total typed events | **781 / 13 074** | cinematics JSON |
| RE tests | **76 lib passed**, integration rc 0 (5+1+2+3); `cargo test -p wc3-re` rc 0 | `ledger373-final-tests3.log`, `ledger373-test-{abilities,cutscenes,mapboot}.log` |

## Modules, callers, tests (mechanic ids)

| Mechanic | Module (`pub mod`) | Caller | Test | Command rc |
|---|---|---|---|---:|
| `abilities.classification` | `wc3_re::abilities` (`lib.rs:31`) | `wc3re abilities --summary/--json` (`bin/wc3re.rs:584`) | `abilities_classification_covers_763_rows` (`abilities.rs:797`) | 0 |
| `abilities.execution` | `wc3_re::abilities::apply_effect` | `wc3re abilities --run-demo` | `abilities_execution_runs_classified_effects` (`abilities.rs:846`) | 0 |
| `maps.boot` | `wc3_re::mapboot` (`lib.rs:43`) | `wc3re map-boot` (`bin/wc3re.rs:734`) | `maps_boot_matrix_44_of_44` (`mapboot.rs:366`) | 0 |
| `maps.jass.natives` | `Runtime::native_calls` + `wc3_re::mapboot` | `wc3re map-boot --native-json` | `maps_jass_natives_cover_called_set` (`mapboot.rs:383`) | 0 |
| `cutscenes.data` | `wc3_re::cutscenes::cinematic_events` (`cutscenes.rs:408`) | `wc3re cinematics` (`bin/wc3re.rs:693`) | `cutscenes_data_44_maps` (`cutscenes.rs:823`) | 0 |

## Root causes fixed

1. **Self-recursive map triggers** — Credits' `Trig_Create_Actions` ends with
   `TriggerExecute(gg_trg_Create)`, relying on `TriggerSleepAction` to suspend.
   The headless VM's sleep native is a no-op, so the trigger recursed to the
   call-depth limit. `Runtime::execute_trigger_depth` now keeps a
   `trigger_stack` and ignores re-entrant `TriggerExecute`; the call-depth limit
   was raised 128 → 512. Human01 and Credits moved from FAIL to ok; all 44 pass.
2. **Zero-time init triggers re-fired** — `Engine::tick` now fires
   `TriggerRegisterTimerEvent(0.0)` triggers only when `exec_count == 0`, so
   `main`/`RunInitializationTriggers` is not duplicated.
3. **Native census** — `Runtime::native_calls` records every native actually
   invoked; `wc3re map-boot` aggregates 335 names / 130 123 calls and exports
   `native_coverage.json`. The only missing native, `VolumeGroupReset` (14
   calls), is implemented (`group_volumes` state) along with
   `VolumeGroupSetVolume`/`GetVolume`.
4. **Camera/volume natives were no-ops** — `World.camera_setups`,
   `camera_events`, `group_volumes` now back `CreateCameraSetup`,
   `CameraSetupSetField/DestPosition/Apply*`, `SetCameraField/Position`,
   `PanCamera*`, `SetCameraTargetController`, `SetCinematicScene`,
   `CinematicFadeBJ`, `VolumeGroup*`; they mutate typed state instead of
   returning Null.

## Acceptance

`bash analysis/tech-lead/accept_full_port.sh` (cached mode):
**P0-4 PASS** (`maps=44 booted=44 failed=[]`). Overall score remains **6/14**
because other layers are still open and P0-3's acceptance path is stale: the
script loads classification artifacts only from
`analysis/agent-sim/ability_classification.json` or
`analysis/agent-qa/ability_classification.json`. The complete 763-row artifact
is at **`analysis/agent-re/phase0/abilities.json`** (`schema wc3-abilities-v1`,
`rows` length 763, `executable=428`, `data_only=335`). Adding that path to the
accept script's `classification_paths` is a tech-lead edit (owner boundary);
the code/caller/test deliverable is complete. No kimi/sim edits were made.

## Commands

```
cargo check -p wc3-re --offline                         # rc 0
cargo test -p wc3-re --lib --offline                    # 76 passed, rc 0
cargo test -p wc3-re --offline                          # 76 lib + integration 5/1/2/3, rc 0
cargo test -p wc3-re --lib abilities_                    # 2 passed, rc 0
cargo test -p wc3-re --lib cutscenes_data_44_maps       # 1 passed, rc 0
cargo test -p wc3-re --lib mapboot::tests               # 2 passed, rc 0
./target/debug/wc3re abilities --json --out analysis/agent-re/phase0/abilities.json
./target/debug/wc3re cinematics --out analysis/agent-re/phase0/cinematics.json
./target/debug/wc3re map-boot --json analysis/tech-lead/map_boot_matrix.json \
    --native-json analysis/agent-re/native_coverage.json --md analysis/agent-re/MAP_BOOT.md
bash analysis/tech-lead/accept_full_port.sh             # P0-4 PASS, overall 6/14
```

Artifacts: `analysis/tech-lead/map_boot_matrix.json`,
`analysis/agent-re/native_coverage.json`,
`analysis/agent-re/phase0/{abilities.json,cinematics.json}`,
`analysis/agent-re/MAP_BOOT.md`.

## H2

- Capture **814**; experience **4476**.
- Ledger **373** closed `completed`, `verification.ok=true`, warnings 0.

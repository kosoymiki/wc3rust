# Simulation layer, lane 3 (`crates/wc3-sim`) — audit status (LEDGER 491)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`, base revision `dea209d` (2026-11-17).
Items: **AUD-016, AUD-018, AUD-019, AUD-023, AUD-024, AUD-027, AUD-028**.

Landing commit: **`716a8c2`** — the shared-worktree index was committed by the
parallel AUD-012/013/017/021/022 lane, which therefore carries this lane's code
and its 24 tests (its message does not list AUD-016/018/019/023/024/027/028).
The commit that records this lane's status/evidence is the docs commit that adds
this file's final revision.

Evidence artifacts `analysis/audit/sim3_handler_scan.py`,
`analysis/audit/sim3_usage_search.py` and `analysis/audit/native_status.json`
live on disk but are **not** in git: the repository ignores `/analysis/**` by
policy (operator game data and everything derived from it). The generated
`native_status.json` is reproduced by
`scripts/h2-cargo run -p wc3-sim --offline --example native_status`.

Every cargo invocation went through `scripts/h2-cargo` (single-slot wrapper: one
cargo at a time, memory gate) because other audit lanes build on the same
device. Two sibling lanes were editing `crates/wc3-sim` in the same window
(AUD-017 sleep, unit-type classification, argument validation); the statuses
below are taken from the merged tree and say so where they depend on it.

## Status

| ID | P | Status | Where | Regression tests |
|----|----|--------|-------|------------------|
| AUD-016 | P3 | **FIXED** | `crates/wc3-sim/src/jass/native_status.rs` (new), `jass/registry.rs` (`NativeDef::dispatch/status/status_reason`, `RegistryStats` per-status, `is_implemented`), `examples/native_status.rs` | `aud016_every_registry_row_carries_one_status_and_a_reason_iff_it_is_a_gap`, `aud016_a_handler_alone_is_not_an_implementation`, `aud016_stats_and_the_status_module_agree`, `aud016_json_report_lists_every_native_with_its_status` |
| AUD-018 | P3 | **FIXED** | `jass/host.rs` (`Host::clear_display_messages`), `jass/natives/unit.rs` (`clear_text_messages`), plus new finding AUD-SIM3-01 below (`DisplayTextToPlayer` argument index) | `aud018_clear_text_messages_clears_the_display_model_and_keeps_the_transcript`, `aud018_clear_text_messages_keeps_messages_addressed_to_another_player` |
| AUD-019 | P3 | **FIXED** | `jass/natives/core.rs` (`is_fog_enabled`, `is_fog_mask_enabled`, `is_player_observer`, `player_state_observer`), `jass/natives.rs` (mask mapping) | `aud019_fog_queries_read_the_host_state_the_setters_write`, `aud019_observer_flag_is_one_value_shared_by_the_state_native` |
| AUD-023 | P3 | **FIXED** | `abilities.rs` (`SpellNumberTable`, `SpellDatasetId`, `SpellInstall`, `SpellDatasetMismatch`), caller `crates/wc3-game/src/assets.rs` (reports `dataset_conflict`) | `aud023_two_tables_keep_independent_datasets_and_reject_the_other_one`, `aud023_repeated_identical_load_is_idempotent`, `aud023_identity_is_stable_and_covers_every_numeric_field`, `aud023_uninstalled_table_answers_from_the_fallback_table`, `aud023_process_table_accessors_agree` |
| AUD-024 | P3 | **FIXED** (parity checked first; behaviour changed) | `jass/natives/core.rs` (`number_prefix`, `s2i`, `s2r`) | `aud024_s2i_matches_the_documented_engine_semantics`, `aud024_s2r_matches_the_documented_engine_semantics`, `aud024_junk_suffix_is_the_documented_divergence`, `aud024_registry_marks_the_conversions_implemented` |
| AUD-027 | P4 | **FIXED** (inventory classified; placeholders marked) | `jass/native_status.rs` (95 explicit entries), `analysis/audit/sim3_handler_scan.py`, `analysis/audit/sim3_usage_search.py`, `analysis/audit/native_status.json` | `aud027_every_null_body_handler_is_classified_as_a_no_op`, `aud027_no_op_entries_really_drop_their_effect`, `aud027_the_audit_list_is_classified_row_by_row`, `aud027_each_gap_class_is_represented_and_carries_a_reason` |
| AUD-028 | P4 | **REPRODUCED_NOT_A_BUG** + cleanup | `jass/natives/core.rs` (`remove_saved_cell`), `jass/natives.rs` (mapping + comment) | `aud028_each_typed_remove_deletes_its_own_cell_and_leave_no_trace`, `aud028_remove_is_a_key_deletion_not_a_typed_deletion`, `aud028_the_five_typed_removes_share_one_handler` |

24 new tests, all passing (see `Gates`).

## AUD-016 — registry: semantic status instead of "a handler exists"

The old row carried `handled: bool = natives::is_handled(name)`, so a row with a
placeholder handler and a row with a complete implementation were the same
number. After the change a row carries

```rust
pub dispatch: bool,                 // handler present (what the VM needs)
pub status: NativeStatus,           // Implemented|Partial|RecordedOnly|NoOp|Unsupported
pub status_reason: &'static str,    // empty iff Implemented
```

`NativeStatus` lives in the new `jass/native_status.rs`; `resolve(name, dispatch)`
is the single decision point, and it *forces* `Unsupported` when there is no
handler, whatever the table says. `RegistryStats` now reports the per-status
counts next to `dispatch`, and `registry::is_implemented()` is separate from
`is_handled()`.

Measured on this tree (`native_status.json`, generated):

```
registry rows            1665
dispatch                  666   (was reported as "handled")
implemented               582   (engine-registered 424 of 1096)
partial                    16
recorded_only              57
no_op                      12
unsupported               998
```

The difference `666 - 582 = 84` is exactly the overcount the audit describes:
84 natives had a handler and no implementation worth the name. The JSON report
`analysis/audit/native_status.json` is generated by
`cargo run -p wc3-sim --example native_status` and lists every row with its
status and reason; `header_absent` carries the 11 Reforged-era handlers that no
header in the tree declares (10 of them placeholders).

## AUD-018 — `ClearTextMessages`

The display model is `Host::display_log: Vec<(Option<u8>, String)>`, written by
`DisplayTextToPlayer` (argument 3) and `DisplayTimedTextToPlayer` (argument 4).
`ClearTextMessages` takes no player (`common.j:1451`), and `Blizzard.j:1334`
(`ClearTextMessagesBJ`) gates the call per force — so the native is the *local
client's* clear. `Host::clear_display_messages(viewer)` removes broadcasts and
messages addressed to `viewer`; `JassVm::text_log`/`output` (the transcript) is
untouched. Two tests: global clear (2 messages → 0, transcript still 2, new
messages still append) and player scope (`SetPlayerController(Player(1), 2)`
declares the local player; the clear removes "mine"/"mine again" and keeps
"theirs").

**New finding AUD-SIM3-01 (found by this test, FIXED):** `display_text_to_player`
read `s(a, 2)` — the `y` coordinate — as the message. `common.j:1448` is
`DisplayTextToPlayer(player, x, y, message)`, so every `DisplayTextToPlayer`
call logged `"0.000"` instead of its text. Reproduced by the first run of
`aud018_clear_text_messages_keeps_messages_addressed_to_another_player`:
`left: [(Some(0), "0.000")]` vs expected `"theirs"`. Fixed to `s(a, 3)` and
pinned by the same tests.

## AUD-019 — fog / observer queries

`FogEnable`/`FogMaskEnable` already wrote `Host::fog_enabled`/`fog_mask_enabled`
(and the save round-trips them), but both getters returned `false` and
`IsFogMaskEnabled` was even mapped to the same handler. Now:

* `IsFogEnabled()` → `host.fog_enabled`, default `true` (melee), so
  `Blizzard.j:4553` cinematic mode saves and restores the real value;
* `IsFogMaskEnabled()` → `host.fog_mask_enabled` through its own handler;
* `IsPlayerObserver(p)` → the per-player flag stored under
  `PLAYER_STATE_OBSERVER = ConvertPlayerState(11)` (`common.j:294`), the same
  cell `GetPlayerState`/`SetPlayerState` read and write (and the player-state
  trigger watches), so the two cannot disagree; `null` answers `false`.

Registry status: fog natives `Implemented`; `IsPlayerObserver` is `Partial`
because a headless boot has no lobby — only a script-set flag is visible.

## AUD-023 — spell numbers are instance-owned + dataset identity

`INSTALLED: OnceLock<Vec<SpellDef>>` became a `SpellNumberTable` (OnceLock
inside), so the table is instance-owned data: `spell_defs()` reads the process
static instance, and tests/tools can own their own. `install_spell_numbers`
returns `Result<SpellInstall, SpellDatasetMismatch>`:

* `SpellDatasetId` = FNV-1a over the canonical applied rows (sorted by id,
  exact `f32::to_bits`), so it is stable across platforms and row order;
* first install → `Installed(id)`;
* identical dataset (any row order, unknown ids ignored) → `AlreadyInstalled(id)`
  — idempotent, not `false`;
* a different dataset → `Err(SpellDatasetMismatch { installed, incoming })` with
  both ids in `Display`; live gameplay keeps the first dataset.

The one caller (`crates/wc3-game/src/assets.rs`, outside `wc3-sim`) now logs
`spell_numbers=N installed dataset=…` or `spell_numbers=N dataset_conflict: …`.
Five tests prove two independent tables, a repeated identical load, identity
coverage of all five numeric fields + row presence, the fallback table, and the
process accessors.

## AUD-024 — `S2I`/`S2R` parity with the engine

Parity source (primary, observed **2026-11-17**):
`https://raw.githubusercontent.com/lep/jassdoc/master/common.j` — the JassDoc
annotation of the 1.32 `common.j`, blocks above `native S2I` / `native S2R`.
Quoted behaviour:

* `S2I`: "For values too big or too small, returns max/min integer
  respectively. For an empty string or text that doesn't start with a number,
  returns 0." Examples `S2I("") == 0`, `S2I("-123") == -123`,
  `S2I("123abc") == 123`, `S2I("abc123") == 0`.
* Both: "The parser stops at the first non-number character `[0-9.]`",
  "does not support comma `,` as a decimal point"; `S2R(".123asd") == 0.123`.
* `S2R`: "Returns 0 for: values too big or too small, an empty string or text
  that doesn't start with a number."

Result: the engine is a **prefix parser**, the old code was a strict parser with
`unwrap_or(0)`, so the answers differed wherever a string had a numeric prefix
followed by anything (`123abc`, `1.5abc`, `1,5`, `1e3`). Behaviour fixed
(`number_prefix`), golden cases pinned:

| input | engine | old runtime | now |
|---|---|---|---|
| `S2I("")` | 0 | 0 | 0 |
| `S2I("abc123")` | 0 | 0 | 0 |
| `S2I("123abc")` | 123 | 0 | 123 |
| `S2I("-123")` | -123 | -123 | -123 |
| `S2I("99999999999999999999")` | 2147483647 | 0 | 2147483647 |
| `S2I("-99999999999999999999")` | -2147483648 | 0 | -2147483648 |
| `S2I(" 42 ")` | (inferred: 42) | 42 | 42 |
| `S2I("12.9")` | 12 (integer part) | 0 | 12 |
| `S2I("1e3")` | 1 (scan stops at `e`) | 0 | 1 |
| `S2I(".5")` | 0 | 0 (`.` parses in Rust) | 0 |
| `S2R(".123asd")` | 0.123 | 0 | 0.123 |
| `S2R("1e3")` | 1.0 | 1000.0 | 1.0 |
| `S2R("1,5")` | 1.0 | 0 | 1.0 |
| `S2R("1.5.5")` | 1.5 (inferred) | 0 | 1.5 |
| `S2R(huge)` | 0 (too big) | inf/0 | 0 |

Two cells are marked *inferred* where the source documents the scan rule but
not the exact example: leading whitespace (the engine links the C runtime's
numeric parsers, which skip it) and the second decimal point (the scan treats it
as the end of the number, as `strtod` does). Everything else is quoted.

## AUD-027 — full handler inventory, classified

Two reproducible scans, both committed:

* `analysis/audit/sim3_handler_scan.py` — extracts every handler body from
  `src/jass/natives*.rs` and flags the ones that write no engine state or drop a
  value. This is what found `UnitAddType`/`UnitRemoveType` (now implemented by
  the unit-type lane as `UnitMeta::types`) and confirmed the audit's list.
* `analysis/audit/sim3_usage_search.py` — call counts over the 1.32.9
  `common.j`+`Blizzard.j`+`Cheats.j`, the other header copies in the tree, the
  96-map corpus (`analysis/agent-jass/extracted/*.j`) and the Rust tests:

```
name                                common.j+Blizzard.j  other-headers  maps(96)  tests
SetTerrainFogExV                    0                    0              1         2
BlzSetTerrainFogMaxLinearDensity    0                    0              1         2
BlzSetTerrainFogDrawOverSky         0                    0              1         2
SetHDWaterParamsEx                  0                    0              2         2
SetSoundParamsFromLabel             0                    0              97        3
SetSoundDistances                   0                    0              15        3
SetSoundDistanceCutoff              1                    2              10        4
SetSoundConeAngles                  1                    2              15        2
SetSoundConeOrientation             0                    0              2         2
SetSoundFacialAnimationLabel        0                    0              27        2
SetSoundFacialAnimationGroupLabel   0                    0              27        2
SetSoundFacialAnimationSetFilepath  0                    0              27        2
SetDialogueSpeakerNameKey           0                    0              27        2
SetDialogueTextKey                  0                    0              27        2
PlayDialogueFromSpeakerEx           0                    0              27        2
AddCameraBlocker                    0                    0              1         2
BlzCameraSetupSetCameraType         0                    0              2         2
SetItemColor                        0                    0              2         2
SetDestructableVertexColor          0                    0              2         2
SetPlayerRaceSkin                   0                    0              3         2
ClearTextMessages                   2                    4              0         3
IsPlayerObserver                    3                    6              0         2
IsFogEnabled                        2                    4              0         3
IsFogMaskEnabled                    2                    4              0         3
GetTriggerEvalCount                 0                    0              0         3
GetTriggerExecCount                 0                    0              0         4
TriggerSleepAction                  5                    14             139       4
UnitAddType                         0                    0              0         3
UnitRemoveType                      0                    0              0         2
SetUnitFog                          0                    0              15        2
CreateTextTag                       0                    2              0         3
GetCameraMargin                     4                    8              560       3
S2I                                 0                    0              0         4
S2R                                 5                    10             0         3
RemoveSavedBoolean                  0                    0              0         2
RemoveSavedInteger                  0                    0              0         2
RemoveSavedReal                     0                    0              0         2
RemoveSavedString                   0                    0              0         2
RemoveSavedHandle                   0                    0              0         2
GetStartLocPrioSlot                 0                    0              0         2
SetStartLocPrio                     0                    0              1696      2
IsUnitType                          18                   36             8         3
SetWidgetLife                       1                    2              0         2
SetUnitX                            0                    0              8         3
SetUnitY                            0                    0              8         3
```

Two structural findings fell out of the same scan and are fixed here:
`SetTextTagLifespan` (the registry spelling) had its handler registered under
the capitalisation `SetTextTagLifeSpan`, which the exact-name dispatch index can
never match (AUD-SIM3-06); and `DisplayTextToPlayer` read the `y` argument as its
message (AUD-SIM3-01, found by the AUD-018 test).

What the corpus shows: the Reforged visual/no-op fleet is *rarely* called
(1–3 maps for fog/water/camera-blocker/item colour), the sound-label and facial
setters are called by 27–97 maps but only set presentation state, while
`GetCameraMargin` (560 calls) and `TriggerSleepAction` (139) are the two
high-traffic natives that were degraded.

Classification fed into AUD-016 (full machine list:
`analysis/audit/native_status.json`):

| class | count | names |
|---|---|---|
| NoOp (registry) | 12 | SetDialogueSpeakerNameKey, SetDialogueTextKey, SetSoundConeAngles, SetSoundConeOrientation, SetSoundDistanceCutoff, SetSoundDistances, SetSoundFacialAnimationGroupLabel, SetSoundFacialAnimationLabel, SetSoundFacialAnimationSetFilepath, SetSoundParamsFromLabel, SetUnitFog, UnitMakeAbilityPermanent |
| NoOp (header-absent) | 10 | SetTerrainFogExV, BlzSetTerrainFogMaxLinearDensity, BlzSetTerrainFogDrawOverSky, SetHDWaterParamsEx, AddCameraBlocker, BlzCameraSetupSetCameraType, SetItemColor, SetDestructableVertexColor, SetPlayerRaceSkin, PlayDialogueFromSpeakerEx |
| Partial | 16 | TriggerSleepAction→(implemented by AUD-017 lane), GetTriggerEvalCount, GetTriggerExecCount, GetGameSpeed, GetCameraMargin, IsPlayerObserver, Player, GetPlayerSlotState, GetStartLocPrioSlot, SetStartLocPrio, And, Or, SetUnitX, SetUnitY, SetWidgetLife, AddHeroXP, CreateTextTag |
| RecordedOnly | 57 | preloader/slot caps, sound playback (StartSound, StopSound, KillSoundWhenDone, AttachSoundToUnit), text tags, effects/weather, cine filter + cinematic scene, dialogs + timer dialogs, selection, fog modifiers, trackables, quest-dialog refresh |

(After the sibling lane's AUD-017 fix and the unit-type implementation landed in
the same window, `TriggerSleepAction`, `UnitAddType`, `UnitRemoveType` and
`IsUnitType` are `Implemented` — the table above is the pre-merge picture; the
JSON artifact is generated from the merged tree and is authoritative.)

The suite also enforces the classification: `aud027_every_null_body_handler_is_classified_as_a_no_op`
reads the handler sources and fails when a `Value::Null`-only body is not a
`NoOp` row, and `aud027_no_op_entries_really_drop_their_effect` fails when a
`NoOp` row points at a body with a real effect. A new placeholder therefore
cannot enter the tree unnoticed.

## AUD-028 — `RemoveSaved*` parity: not a bug

All five typed removes have the same signature
`(hashtable, integer, integer) -> nothing`, and the engine deletes by
`(parent, child)` key without reading the stored cell type. Three tests pin it:
one cell of each type is saved, removed through its own native, and the
`Load*`/`HaveSaved*` pair confirms the cell is gone; a cross-typed removal
(`SaveInteger` + `RemoveSavedReal`) removes the same cell; a neighbouring key is
untouched. Mapping simplified to one `remove_saved_cell` handler for all five
names, with the reason in `natives.rs` and the handler doc comment. Verdict:
`REPRODUCED_NOT_A_BUG`, behaviour unchanged, misleading mapping removed.

## New findings (for the single registry)

| ID | P | Status | Where | What |
|----|----|--------|-------|------|
| AUD-SIM3-01 | P3 | **FIXED** | `jass/natives/unit.rs` (`display_text_to_player`) | `DisplayTextToPlayer` read argument 2 (`y`) as the message; `common.j:1448` puts it at 3. Every message logged as `"0.000"`. Fixed + covered by the AUD-018 tests. |
| AUD-SIM3-02 | P4 | **OPEN — next sim pass** | `jass/natives/world.rs` (`set_start_loc_prio`, `get_start_loc_prio_slot`), `jass/host.rs` (`Host::start_prio`), `save.rs` | `SetStartLocPrio(whichStartLoc, prioSlotIndex, otherStartLocIndex, priority)` stores only `priority` (the 3rd argument is dropped), and `GetStartLocPrioSlot(whichStartLoc, prioSlotIndex)` returns that priority instead of `otherStartLocIndex`. Needs the host map value to become `(otherStartLocIndex, priority)`, a save-format bump, and two tests. Corpus usage: `SetStartLocPrio` 1696 calls, `GetStartLocPrioSlot` 0 — the wrong value is currently unobservable in the shipped maps, which is why it is recorded rather than patched inside this lane's save-format scope. |
| AUD-SIM3-03 | P4 | **OPEN — next sim pass** | `jass/natives/world.rs` (`get_camera_margin`) | Returns `0.0` for every side; the viewport/camera-bound margin model does not exist. 560 corpus calls read it. Registry status `Partial`. |
| AUD-SIM3-04 | P4 | **OPEN — next sim pass** | `jass/natives/core.rs` (`get_player_slot_state`) | Answers playing (1) or empty (0); `PLAYER_SLOT_STATE_LEFT` (2) is never produced. Registry status `Partial`. |
| AUD-SIM3-06 | P3 | **FIXED** | `jass/natives.rs` (`SetTextTagLifespan` dispatch row), `jass/native_status.rs` | The handler was registered as `SetTextTagLifeSpan` while the registry row is `SetTextTagLifespan`; the dispatch index is keyed by the exact name, so the handler was unreachable and `SetTextTagLifespan` counted as unresolved. Renamed; `every_handler_name_is_a_registry_native_or_a_known_header_gap` now compares case-sensitively so a future mismatch fails. |
| AUD-SIM3-05 | P4 | **OPEN — next sim pass** | `jass/natives/unit.rs` (`set_unit_x`, `set_unit_y`, `set_widget_life`, `add_hero_xp`) | The simulation call's `Result` is dropped (`let _ =`), so a refused move/life/XP change is silent. Registry status `Partial`. |

## Gates

```
scripts/h2-cargo check -p wc3-sim --offline                         rc=0
scripts/h2-cargo check -p wc3-game --offline                        rc=0
scripts/h2-cargo test  -p wc3-sim --offline --test aud016_native_status    4 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud018_clear_text       2 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud019_host_queries     2 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud023_spell_numbers    5 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud024_string_number_parity 4 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud027_handler_inventory 4 passed / 0 failed
scripts/h2-cargo test  -p wc3-sim --offline --test aud028_saved_cells      3 passed / 0 failed
scripts/h2-cargo run   -p wc3-sim --offline --example native_status        rc=0 (analysis/audit/native_status.json)
scripts/h2-cargo test  -p wc3-sim --offline --no-fail-fast          see "Full suite" below
```

Full suite: `scripts/h2-cargo test -p wc3-sim --offline --no-fail-fast` →
**rc=0, 422 passed / 0 failed** over 64 lib tests and every integration target
(including the AUD-017 sleep test that was red while that lane was in flight).

## Determinism and contracts

No new simulation state in `Sim` (the display model, fog flags, per-player
states and the spell table all live in the script host/data layer), so
`Sim::state_hash` is unchanged by this work. `SpellDatasetId` uses exact bit
patterns. `DisplayTextToPlayer` now consumes the argument `common.j` declares,
which changes what the transcript contains for maps that use it (the previous
content was the `y` coordinate).

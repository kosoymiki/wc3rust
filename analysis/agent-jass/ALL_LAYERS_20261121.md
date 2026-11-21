# Agent I — campaign / JASS layer, all-layers push 2026-11-21

Ledger 605. Owners: `crates/wc3-jass/**`, `crates/wc3-game/src/campaign.rs`.
Shipped oracle: `analysis/war3_full3` (44 campaign maps, `Scripts/common.j`,
`Scripts/Blizzard.j`), `analysis/war3_mpq_listing.txt`.

## Goal

Campaign flow over the 44 shipped chapters and JASS semantics:

1. every native declared in the shipped `Scripts/common.j` resolves with the
   shipped signature (`scripts/native_sig_compare.py` → 0 missing / 0 mismatches);
2. the 44 chapter scripts boot on the real maps;
3. no unsupported path is silent — an unimplemented native or a failed trigger
   action is measured, never dropped.

## Actions

### 1. `crates/wc3-jass/src/lib.rs` — the legacy VM no longer swallows unsupported calls

Root cause (observed): a script-declared native with no host handler returned
`Ok(Value::Null)` and trigger/timer action failures were discarded with
`let _ =`. A map could therefore "boot" while a call that drives its outcome did
nothing, and nothing recorded it.

- `JassRuntime::unimplemented: BTreeMap<String, u64>`, `errors: Vec<String>`,
  `strict_natives: bool` (`lib.rs:704-708`); `unimplemented_calls()`,
  `take_errors()`, `set_strict_natives()` (`lib.rs:987-1005`).
- `call_function` routes a declared-but-unhosted native through
  `unimplemented_native` (`lib.rs:969`), which counts it and answers with the
  declared type's default (`default_for_type`, `lib.rs:1222`) instead of a
  type-less `Null`; strict mode turns the call into an error naming the shipped
  signature.
- `tick` (`lib.rs:1211`) and `TriggerExecute` record action failures in `errors`
  instead of dropping them.
- Four named tests: `unimplemented_native_is_recorded_with_typed_default`,
  `strict_natives_turn_unimplemented_into_an_error`,
  `timer_action_failure_is_recorded_not_dropped`,
  `trigger_execute_failure_is_recorded_not_dropped` (`lib.rs:1460+`).

### 2. `crates/wc3-jass/tests/campaign_boot.rs` (new) — 44/44 boot on the real maps

Boots every chapter through the canonical engine
(`wc3_re::mapboot::boot_all`: `Engine::boot` → `common.j` + `Blizzard.j` +
`war3map.j` → 100 ticks at 30 Hz, 32 MB script stack), then gates:

- `ok = 44`, `main_ok = 44`, no script errors;
- every map registers triggers (`>= 5`) and executes at least one init trigger;
- totals pinned to the measurement (`triggers >= 4700`, `executed_triggers >= 650`);
- no outcome-critical native (quests, victory/defeat, `GetLocalPlayer`,
  `GetHandleId`, hashtables, `GetUnitRace`, `IsQuestDiscovered`) falls through
  **unless** it is in the dated `KNOWN_OUTCOME_GAPS` table (`campaign_boot.rs:77`)
  with its call sites and the owner of the fix;
- the full unimplemented ledger and the top-called natives are printed.

### 3. `crates/wc3-jass/examples/campaign_boot_ledger.rs` (new)

Per-chapter attribution of the unimplemented ledger (which map calls which
missing native), so the cross-owner fix has exact call sites.

### 4. `crates/wc3-game/src/campaign.rs` — campaign flow over 44 chapters (tests only)

No production change; three tests in the module's own test block:

- `campaign_registry_lists_all_44_shipped_chapters` (`campaign.rs:3866`) — the
  registry equals the shipped tree: 44 files = 44 rows, unique ids, retail order
  (prologue first, credits last, interludes after their mission), interlude and
  chapter-number flags derived, no melee map admitted.
- `campaign_flow_clears_all_44_chapters_in_retail_order` (`campaign.rs:3953`) —
  a fresh save opens one mission per campaign (Prologue, Human, Undead, Orc,
  Night Elf + the Extra credits group); each `complete()` opens exactly the next
  chapter; 44/44 chapters are reachable; the campaign ends with every row
  `Cleared` and nothing left to unlock. `save_path` is forced to `None` so the
  operator's `campaign.sav` is never written by the sweep.
- `campaign_every_chapter_parses_into_a_mission` (`campaign.rs:4021`) — each
  chapter resolves to a real map and parses into a mission with a non-empty
  `war3map.j`; prints `GATE campaign.chapter-parse`.

## Evidence (rc pasted)

| Gate | Command | rc | Output |
|---|---|---|---|
| jass suite | `scripts/h2-cargo test -q -p wc3-jass --offline` | **0** | `15 passed` lib; `campaign_boot` `1 passed` (127.60 s, re-run 21:12); `triggers_gates` `1 passed` (5.03 s) |
| wc3-game build | `scripts/h2-cargo check -q -p wc3-game --offline` | **0** | clean check after the `.then(\|\| …)` closure fix at `campaign.rs:3833-3849` |
| campaign flow | `scripts/h2-cargo test -q -p wc3-game --lib --offline -- campaign::tests::campaign` | **0** | `5 passed` (5.14 s); `GATE campaign.chapter-parse PASS chapters=44 script_bytes=12586474 objectives=82 cinematics=691` |
| campaign report | `scripts/h2-cargo run -q -p wc3-game --example campaign_report --offline -- --mission Human01 --frames 0` | **0** | `CAMPAIGN_ROOT … chapters=44`; 44 chapter rows; `MISSION id=Human01 … objectives=3 cinematics=25 triggers=130`; `CAMPAIGN_MISSION_STATE id=Human01 script_booted=true` |
| native signatures | `python3 scripts/native_sig_compare.py --json` | **0** | `{"ok": true, "original": 791, "registry": 1665, "missing": [], "mismatches": []}` |
| map-boot matrix | `scripts/h2-cargo run -q -p wc3-re --bin wc3re --offline -- map-boot --json analysis/agent-jass/map_boot_matrix_20261121.json --native-json analysis/agent-jass/native_coverage_20261121.json` | **0** | `map-boot maps=44 ok=44 main_ok=44 executed_triggers=682 unique_natives=325 unimplemented=13` |

Measured boot numbers (`analysis/agent-jass/map_boot_matrix_20261121.json`):
`triggers=4794`, `executed_triggers=682`, `unique_natives=325`,
`calls=120666`, `errors=[]` on all 44 maps.

### Finalization after the serve restart (21:09-21:13)

The serve restarted mid-close; every gate was re-run on the live tree and is
green: `cargo check -p wc3-game --offline` **rc=0**, `cargo test -p wc3-jass
--offline` **rc=0** (15 lib, `campaign_boot` 127.60 s, `triggers_gates` 5.03 s),
`campaign::tests::campaign` **rc=0** (5 passed, 5.14 s), `campaign_report`
**rc=0** (`chapters=44`, `Human01 script_booted=true`), `native_sig_compare`
**rc=0** (`791/1665`, `missing=[]`, `mismatches=[]`).

One compile defect of this lane was fixed on the way: the new
`shipped_map_stems` helper used `?` inside a `bool::then(|| …)` closure
(`campaign.rs:3841`, `E0277`); it is now an explicit extension check plus
`file_stem()` (`campaign.rs:3833-3849`), which is what the green runs compile.

## Measured unimplemented ledger (13 natives / 1360 calls)

| native | calls | where | classification |
|---|---|---|---|
| `definestartlocation` | 257 | every map's melee init | **typo arm** (`natives.rs:2698` matches `"define startlocation"`, never the lowercase name) |
| `setplayerracepreference` | 257 | lobby init | lobby-only |
| `setplayerraceselectable` | 257 | lobby init | lobby-only |
| `forceplayerstartlocation` | 250 | lobby init | lobby-only |
| `setstartlocprio` | 197 | lobby init | lobby-only |
| `setstartlocpriocount` | 94 | lobby init | lobby-only |
| `getunitrace` | 16 | boot calls in `Undead08`; static sites `Undead07.j:3881,3920,4079,4118` (`Shield01/02` filters) | **outcome**: `RACE_UNDEAD == GetUnitRace(GetEnteringUnit())` decides whether the shield triggers run |
| `pausecompai` | 15 | Blizzard.j:2501 | no AI in this VM (revisit with the AI lane) |
| `isquestdiscovered` | 6 | boot calls in `Orc03`; static sites Human03/Orc03/Undead05 | **outcome**: quest-visibility gates |
| `unitremovebuffs` | 6 | Blizzard.j `UnitRemoveBuffsBJ` | no buff subsystem in this VM |
| `groupenumunitsselected` | 2 | Blizzard.j:2970,3069 | interactive selection only |
| `syncselections` | 2 | Blizzard.j:2969,3068 | multiplayer selection sync |
| `getgametypeselected` | 1 | Blizzard.j:6414 | lobby query |

Exact fix (arms + constant values + verification) is written up for the
`wc3-re` owner in `analysis/agent-jass/PATCH_wc3-re_natives_campaign.md`. The two
outcome-critical rows are tracked in the test's `KNOWN_OUTCOME_GAPS` table with
their owner and next step; a *new* outcome gap fails the gate.

## Result

All four packet gates pass with rc=0 on the shipped tree: 44/44 chapters in the
registry and in the flow, 44/44 scripts boot on the real maps, 791/791 shipped
native declarations match their signatures, and every unsupported call in the
JASS layer is measured rather than silent.

## Risks / next (named owners)

1. **`wc3-re/src/jass/natives.rs`** (agent L): the 13-native ledger above.
   `PATCH_wc3-re_natives_campaign.md` gives the exact arms; §1–§3 close 279 of
   1360 calls; §4 needs a decision (implement as no-ops or dated allowlist) because
   `wc3-re/src/mapboot.rs:383 maps_jass_natives_cover_called_set` is red on the
   same set and attributes it here.
2. **`crates/wc3-game/src/game.rs:10119`** (agent B): `game::tests::
   map_runtime_campaign_script_boots` fails with `Engine ticks inside
   Game::update: 0 -> 0` — the engine is not advanced by `Game::update`, so a
   campaign mission stops its script clock after boot. Observed in this lane's
   run; outside the campaign-jass owner paths.
3. **Legacy VM contract**: `JassRuntime` answers an unhosted handle native with
   `Handle(0)`; callers must treat 0 as null. `strict_natives` is the opt-in that
   makes the same call an error.
4. **Campaign flow pins**: the flow test pins the RoC order (Human, Undead, Orc,
   Night Elf) and the Extra credits group; adding TFT chapters to
   `Maps/Campaign` changes the expected chain and must update the test in the
   same change.

## H2_PERSISTED

- `harness2_capture` **1265** — task_ref `alllayer-campaign-jass-20261121`.
- `harness2_experience` **4926** (never answer an unhosted declared native with
  `Null`; count + typed default + error ledger + strict mode), **4927** (prove a
  script layer by booting the corpus, not by parse counts), **4928** (campaign
  chapter tests are chain tests; `save_path=None`), **4929** (retail order comes
  from `race_order`, not memory), **4930** (`verify_20261121.sh --wait` while
  shared crates are mid-refactor).
- `harness2_monitor` **605** — running → **done** (checkpoints at baseline,
  phase 2, phase 2b measurement, post-restart finalization; closed with
  `h2_ids_json {"capture_ids":[1265],"experience_ids":[4926,4927,4928,4929,4930],"monitor_id":605}`).
- Brain: `~/.local/share/mimocode/memory/vault/brain/Key Decisions.md` and
  `Patterns.md`, section "2026-11-21 — campaign/JASS".

## SESSION_CLOSED

Ledger **605** closed `done` with this report, the five rc=0 gates
(`cargo check -p wc3-game`, `cargo test -p wc3-jass`, `campaign::tests::campaign`,
`campaign_report`, `native_sig_compare`), the map-boot matrix and the patch
artifact for the cross-owner natives gap.

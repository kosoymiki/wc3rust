# LANE H — data, map loading, JASS (ledger 528) — audit report
2026-11-18 · repo `~/src/wc3rust` · audited worktree **`d7af459`** + uncommitted
`crates/wc3-re/src/{engine.rs,jass/natives.rs,jass/vm.rs}` (the in-flight
`TriggerSleepAction` work) and the three untracked `crates/wc3-re/tests/*` files.
`crates/wc3-formats/**` and `crates/wc3-mpq/**` are lane G's files; where a finding
crosses that boundary it is stated explicitly.

Scope: `crates/wc3-data/**` (5.6), `crates/wc3-re/**` (5.7 map loading/map content/JASS
parser+AST, 5.9 VM/natives). No new tests were written; every reproducer is a one-shot
`cargo run --example` probe copied under `analysis/full-audit-2026-11-18/probes/`.

## 0. Executive result

- **69/69 production source files** under `crates/wc3-data/src` + `crates/wc3-re/src`
  reviewed; status per file in `FILE_COVERAGE.csv` (`AUDITED` with one concrete
  observation each). 2 manifests (`Cargo.toml`) also reviewed.
- **3 P1** findings, all on the live mission path: `war3map.j` `config` is never run;
  death/region/state/dialog/unit trigger events are registered but never dispatched;
  `TriggerSleepAction` inside a called function does not suspend the caller.
- **6 P2**, **8 P3**, **5 P4** (22 findings registered below). Four are `NEEDS_REPRO`
  (mechanism proven by code, crafted input not executed): WC3-MAP-002, WC3-JASS-010,
  WC3-MAP-003, WC3-JASS-011. No finding is FALSE_POSITIVE after self-review.
- **No P0**: no memory corruption/UB found; the parser crash is a process abort from a
  crafted script (P2 by the TZ severity model), not an exploitable write.
- **Mission context:** the live campaign path runs `Game::reset_map_runtime` →
  `Engine::enter_map` → `Runtime::tick_script` (32 MB scoped thread,
  `crates/wc3-game/src/game.rs:1759-1777`). The `wc3-sim` JASS machine
  (`Sim::attach_script`) has **no non-test caller**; it is not the VM that plays the
  mission. Prologue01 (mounted from `War3.mpq` as `Maps\Campaign\Prologue01.w3m`,
  305 448 B, w3i v18)
  boots with `main_ok=true`, 183 triggers, 65 `TriggerRegisterEnterRect`, 23
  player-unit registrations (9 `EVENT_PLAYER_UNIT_DEATH`) and only **15 of 183 triggers
  ever execute in 600 ticks**. That is the measured symptom behind "the mission plays
  wrong".

Urgent hand-off written to `analysis/tech-lead/AUDIT-H-URGENT-2026-11-18.md`.

## 1. Method and evidence

- Source of truth for retail boot order: `analysis/agent-re/formats/ENGINE_MAP.md`
  §4.1 (observed Game.dll disassembly `0x6f2ccf00 RunConfig` / `0x6f2ccf30 RunMain`,
  callers `0x6f28dc58` / `0x6f28ec7c`, sequence "run config → start-up → run main").
- Probes are Rust examples in `crates/wc3-re/examples/audit_h_*.rs`; sources and captured
  outputs are copied to `analysis/full-audit-2026-11-18/probes/<ID>/` with a `run.sh`.
- Prologue01 was mounted read-only from the operator's own `War3.mpq`
  (`Maps\Campaign\Prologue01.w3m`, 305 448 B, w3i v18), extracted to
  `$TMPDIR/opencode/audit_h_prologue01.w3m`; no install file was modified.
- `scripts/h2-cargo` was used for every build/run (FIFO slot).

Probe summary (exact outputs in `probes/`):

| ID | command | observed |
|---|---|---|
| H-config | `cargo run -q -p wc3-re --example audit_h_config` | Human04 boot: `config_calls=None`; all 6 players `controller=1 start=0`; after manual `config`: slot 1 `controller=0 start=1 team=0`, slot 0 `controller=3`, slot 5 `team=1` |
| H-sleep | `… --example audit_h_sleep` | `after main: stage=3 parked=1`; `after advance(2.0): stage=2` (retail order would end at 3) |
| H-array | `… --example audit_h_array` | `oob writes accepted: len=3 a[-5]=7 a[9000]=9`; `growth: len=200000 steps=2400010` |
| H-parser | `… --example audit_h_parser` | `unterminated function ACCEPTED: functions=1 body=1`; empty body rejected; UTF-8 literal `Str("Ð\u{9f}Ñ\u{80}Ð¸Ð²ÐµÑ\u{82}")` |
| H-steps | `… --example audit_h_steps` | boot `steps=15335`, after 600 ticks `24600` (`+2306`), `max_steps=50000000`; after a spent budget every later entry is `Err(step limit exceeded)` |
| H-depth | `… --example audit_h_depth -- 20000` | `SIGSEGV`, rc `139` (also at 50 000/100 000; `ulimit -s` = 8192 KiB) |
| H-substring | `… --example audit_h_substring` | `StringLength("Привет")=24`; `SubString("Привет",0,1)` panics: `end byte index 1 is not a char boundary` |
| H-data | `… --example audit_h_data` | Prologue01 `war3map.doo` 5321 records: **3852 destructable rows, 1469 doodad rows**; `gold_cost(3)` with `goldmod=i32::MAX` panics `attempt to multiply with overflow` at `upgrades.rs:295`; `DirSource::resolve("../secret.txt")` returns a path outside the root |
| H-prologue | `… --example audit_h_prologue` | Prologue01 boot/ticks/event counts in §0; `config_calls=None`; `player1_controller=1`; manual `config` makes slot 0 `controller=0` (the user seat) |

## 2. Findings — P1

### WC3-JASS-001 — `war3map.j` `config` is never executed
Priority: **P1** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459` + worktree `crates/wc3-re/src/engine.rs`
File(s): `crates/wc3-re/src/engine.rs:305`, `crates/wc3-re/src/jass/vm.rs:1116-1118`
Preconditions: any map boot (`Engine::enter_map`), including campaign chapters.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_config`
(Human04) and `audit_h_prologue` (Prologue01).
Observed behavior: `call_counts["config"] = None` after boot; all players keep the
`World::default` values (`name="Player N"`, `controller=1`, `start=0`, `team=0`).
Running `config` manually afterwards sets the map's real slots (Human04 slot 1 =
`MAP_CONTROL_USER`; Prologue01 slot 0 = user).
Expected behavior: retail `RunConfig` calls `config` before `main` (ENGINE_MAP §4.1),
so `InitCustomPlayerSlots`, `InitCustomTeams`, `SetMapName`, `DefineStartLocation`,
`SetPlayerController(...,MAP_CONTROL_USER)` take effect before the mission starts.
Root cause: `enter_map` calls only `runtime.run_main()`; `run_main` is
`run_function("main")`. No code path runs `config` (`wc3re jass` CLI does, but the
engine and the game do not).
Impact: script-visible player names/races/controllers/teams/start locations are
defaults for every map; the documented seat fallback
`crates/wc3-game/src/campaign.rs:3329-3333` ("slot the script marked
MAP_CONTROL_USER") is unreachable, forcing the `sloc`/camera heuristic or the
`war3map.w3i` path (`Prologue01` is w3i **v18**, which carries no player records — see
H-prologue), and campaign.rs falls back to `set_local_player_any(true)`.
Why not covered by excluded backlog: the old backlog item is "JASS native/event
semantics and coverage"; this is the engine entry-point order (`config` vs `main`),
an independent root cause proven against Game.dll.
Fix specification: in `Engine::enter_map`, after compiling the merged program and
before `run_main`, call `runtime.run_function("config")` and surface its error like
`main` (a map without `config` is legal — treat NotFound as absent, not as an error).
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: H-config probe;
assert `call_counts["config"] == Some(1)` and slot 1 `controller == 0` for Human04.
Verification commands and exact result: H-config output above.
Related: WC3-JASS-002, WC3-JASS-007.

### WC3-JASS-002 — non-spell trigger events are registered but never dispatched
Priority: **P1** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459` + worktree `jass/{natives,vm}.rs`
File(s): `crates/wc3-re/src/jass/natives.rs:1412` (`Death`), `:1420` (`EnterRegion`),
`:1428` (`LeaveRegion`), `:1465` (`GameState`), `:1475` (`Variable`), `:1483/:1492`
(`Dialog`), `:3034/:3043/:3058`; `crates/wc3-re/src/jass/vm.rs:1748-1798`
(`fire_unit_event` matches only `PlayerUnit`/`UnitEvent`/`Player`); `natives.rs:1277-1278`
(`IsPointInRegion`/`IsUnitInRegion` hardcoded false).
Preconditions: any trigger registered for death, region enter/leave, game state,
variable, dialog or unit-specific events — the standard campaign quest structure.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_prologue`.
Observed behavior: Prologue01 registers 65 `EnterRegion`, 23 `PlayerUnit` (9 of them
`EVENT_PLAYER_UNIT_DEATH`), 5 `UnitEvent`, 9 timer, 26 other events. In 600 ticks only
15 of 183 triggers execute (`exec_total=33`), all timer-driven; no region/death event
is ever raised. `fire_unit_event` has only spell/item call sites
(`vm.rs:1811,1931,1975-1976`); no `wc3-game` call site exists.
Expected behavior: a unit crossing a region fires `TriggerRegisterEnterRegion`; a unit
death fires `TriggerRegisterPlayerUnitEvent(..., EVENT_PLAYER_UNIT_DEATH)`; game-state,
variable and dialog events fire on their state changes.
Root cause: the event producer side was never implemented. `TriggerEvent` variants exist
and registrations push them, but the only dispatcher (`fire_unit_event`) matches three
variants, and nothing in the engine/game layer calls it for death or region movement.
Impact: every campaign quest/defeat trigger based on region enter or unit death is
inert. This is the strongest measured explanation of "the mission plays wrong" after
the opening: quests never advance and defeat/victory checks never run.
Why not covered by excluded backlog: old item is a generic native/event coverage count;
this finding is the specific missing producer for already-registered events, with the
map-level count (65+23) and the 15/183 execution evidence.
Fix specification: add one dispatcher in the engine tick (or bridge sync):
(a) death diff over `runtime.world.units` alive→dead → `fire_unit_event` for
`EVENT_PLAYER_UNIT_DEATH`/`EVENT_UNIT_DEATH` with the unit owner; (b) position diff over
units vs `world.regions.rects/cells` → a new `fire_region_event(region, unit, entering)`
matching `EnterRegion`/`LeaveRegion`; (c) match the remaining variants in
`fire_unit_event` (or separate functions) and call them from the state/dialog setters;
(d) implement `IsPointInRegion`/`IsUnitInRegion` against the same geometry.
Regression test(s): BLOCKED: operator test freeze. Manual: H-prologue probe must show
non-zero executed triggers among the region/death set after moving a unit/killing it.
Verification commands and exact result: H-prologue output above.
Related: WC3-JASS-001, WC3-MAP-001.

### WC3-JASS-003 — `TriggerSleepAction` in a called function does not suspend the caller
Priority: **P1** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: worktree `crates/wc3-re/src/jass/vm.rs` (uncommitted)
File(s): `vm.rs:1321-1340` (`Op::Call`), `:1180-1197` (`run_entry_index` creates a new
`Thread` per user-function call), `:1437-1441` (runtime-wide `pending_sleep` is consumed
by whichever thread runs next), `:1577-1580` (`request_sleep`).
Preconditions: `TriggerSleepAction`/`PolledWait` inside a user function that is called
from `main` or from a trigger action/helper.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_sleep`.
Observed behavior: `main` sets stage 1, calls `inner` (which waits 2 s), then sets
stage 3 immediately; after `advance(2.0)` `inner`'s post-wait write sets stage 2 last.
Printed: `after main: stage=3 parked=1`; `after advance(2.0): stage=2`.
Expected behavior: one JASS thread per activation — the whole call chain waits; the
caller's post-call statements run after `inner` resumes, so the final stage is 3.
Root cause: every user-function call is executed on a fresh `Thread`; the native sets a
single `Runtime::pending_sleep`, which the inner thread's interpreter loop consumes and
parks. The outer thread continues because `pending_sleep` is already `None`.
Impact: cutscene/quest helpers that wait and are called from another script function
return early; later writes land in the wrong order. Prologue01 has 152
`TriggerSleepAction` calls; the opening is a chain of helpers.
Why not covered by excluded backlog: this is code added in the current worktree (the
`request_sleep`/`tick_script` rework); no old item covers thread suspension.
Fix specification: either run user-function calls on the *same* thread/Frames stack,
or make `pending_sleep` a per-thread field that the innermost loop propagates to the
outermost entry (e.g. return a `Suspended` status through `call_function` and let
`run_thread_inner` stop the whole chain). Do not consume the sleep in an inner thread.
Regression test(s): BLOCKED: operator test freeze. Manual: H-sleep probe must end at
stage 3 with one parked thread.
Verification commands and exact result: H-sleep output above.
Related: WC3-JASS-002.

## 3. Findings — P2

### WC3-JASS-004 — non-ASCII strings: mojibake, wrong `StringLength`, `SubString` panic
Priority: **P2** (P1 for a localized map on the release APK: `[profile.release] panic = "abort"`, `Cargo.toml:50-54`) · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459` (lexer) + worktree `natives.rs`
File(s): `crates/wc3-re/src/jass/lexer.rs:105-107` (`value.push(ch as char)` per byte);
`crates/wc3-re/src/jass/natives.rs:150` (`stringlength` = `String::len` bytes),
`:151-158` (`text[start..end]` byte slice), `:143-147` (`r2sw` width/precision).
Preconditions: a map script or `TRIGSTR`/`GetLocalizedString` value contains non-ASCII
text; any localized map (RU/DE/PL/CN) or a map-authored UTF-8 string.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_substring`.
Observed behavior: `StringLength("Привет") = 24` (6 characters); `SubString("Привет",0,1)`
panics at `natives.rs:157`: `end byte index 1 is not a char boundary; it is inside 'Ð'`.
The lexer already turned the UTF-8 literal into `"Ð\u{9f}Ñ\u{80}Ð¸Ð²ÐµÑ\u{82}"` (H-parser).
Expected behavior: JASS strings are byte/character strings; `StringLength` returns the
number of characters, `SubString` offsets are character indices and never panic.
Root cause: byte→`char` (Latin-1) conversion in the lexer and byte offsets in the native;
`String::len()` is UTF-8 bytes.
Impact: wrong text, wrong hashes/lengths, and a deterministic panic when a localized
string is sliced. In debug the panic unwinds through the script thread; in the release
APK `panic = "abort"` (`Cargo.toml:50-54`) turns the same slice into SIGABRT, i.e. the
app dies on a valid localized map. `StringLength` is also wrong for CP1251/UTF-8 map
strings even without slicing.
Why not covered by excluded backlog: old item is native coverage; this is the concrete
encoding/indexing contract of three natives plus the lexer.
Fix specification: decode the byte string in the lexer as UTF-8 when valid, otherwise
CP1251/Latin-1 (declare the map encoding), and implement `StringLength`/`SubString` on
`char_indices` (clamp indices, no panic); `r2sw` clamp width/precision to a sane max.
Regression test(s): BLOCKED: operator test freeze. Manual: H-substring probe; assert
length 6 and no panic.
Verification commands and exact result: H-substring output above; H-parser AST dump.
Related: WC3-MAP-003 (war3map.j read path).

### WC3-JASS-005 — recursive-descent parser has no depth limit (SIGSEGV)
Priority: **P2** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-re/src/jass/parser.rs:377-379` (`parse_expr` → `parse_or`),
`:464-512` (`parse_primary` → `parse_expr`), `:217-230` (`parse_block` → `parse_stmt`
recursion), `:282-323` (`debug` recursion).
Preconditions: untrusted `war3map.j` with deeply nested parentheses/if/loop.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_depth -- 20000`;
also 50 000/100 000.
Observed behavior: `SIGSEGV`, rc 139, at 20 000 nested parentheses on the 8 MiB main
thread (`ulimit -s 8192`). The game already works around this with a 32 MiB scoped
thread (`crates/wc3-game/src/game.rs:1759-1777`, comment "overflow the 2 MB default
thread stack … Human01 is the worst map").
Expected behavior: a parser limit (e.g. 256 nesting) with a diagnostic, independent of
the host thread stack.
Root cause: unbounded recursion; no depth counter.
Impact: a hostile or generator-broken map aborts the app thread on load (DoS); the
32 MiB workaround only raises the bar, it does not bound the input.
Why not covered by excluded backlog: new code path; the old backlog has no parser
recursion limit item.
Fix specification: add `depth: u32` to `Parser`, increment in `parse_expr`/`parse_block`
and return `ParseError` beyond a fixed limit (e.g. 256); add the same to the
`Compiler::expr`/`stmt` recursion.
Regression test(s): BLOCKED: operator test freeze. Manual: H-depth probe must return a
parse error, not SIGSEGV.
Verification commands and exact result: H-depth output above.
Related: none.

### WC3-JASS-006 — JASS array index is unbounded (negative and >8191 accepted)
Priority: **P2** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-re/src/jass/vm.rs:2032-2045` (`get_array`),
`:2064-2090` (`set_array`), `natives.rs:1294-1306` (`LoadArray`/`StoreArray`).
Preconditions: any script write to an array with an out-of-range index.
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_array`.
Observed behavior: `set g_a[-5]=7`, `g_a[9000]=9`, `g_a[100000]=11` are accepted;
the map holds 3 entries. A 200 000-iteration loop creates 200 000 entries
(`steps=2400010`) with no error.
Expected behavior: JASS arrays have 8192 elements; an out-of-range index is a script
error (or is ignored), never grows an unbounded map.
Root cause: `BTreeMap<i64, Value>` with no 0..=8191 check.
Impact: a map can exhaust device memory (`200k × ~40 B` per array; the loop guard
allows 5 M iterations per loop) and the negative-index behaviour diverges from retail.
Why not covered by excluded backlog: concrete array contract, not native coverage.
Fix specification: reject `index < 0 || index >= 8192` with the VM's error channel
(record once per site) and keep the map bounded.
Regression test(s): BLOCKED: operator test freeze. Manual: H-array probe must report
rejected indices.
Verification commands and exact result: H-array output above.
Related: WC3-JASS-005.

### WC3-JASS-007 — no-op native arms count as "handled" (alliances, unit queries, tech)
Priority: **P2** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459` + worktree
File(s): `crates/wc3-re/src/jass/natives.rs:323-326` (`SetPlayerAlliance` and 8 more =
`Value::Null`, no state), `:327-332` (`GetPlayerTechCount` etc. = 0),
`:852-858` (`IsUnit`/`IsUnitType`/`IsUnitAlly`/`IsUnitEnemy`/`IsUnitOwnedByPlayer` all
collapse to a constant; `isunit` returns true, the rest false), `:1277-1278`.
Preconditions: any map using alliances/unit queries/tech levels.
Trigger / minimal reproduction: H-prologue script counts + H-config player dump;
Human04 calls `SetPlayerAllianceBJ` **39×**, Prologue01 13×; `IsUnitType` 1×.
Observed behavior: `SetPlayerAlliance` returns without storing anything; `IsUnitAlly`
and `IsUnitOwnedByPlayer` are always false; `IsUnitType` is always false; `IsUnit` is
always true; `GetPlayerTechCount` is 0. These arms are not in `unimplemented`, so the
`map_boot` "unimplemented.is_empty()" assertion is a false green for them.
Expected behavior: per-pair alliance state, unit-type tests (`UNIT_TYPE_HERO`, …), and
tech levels set by `SetPlayerTechResearched`.
Root cause: deliberate "handled" fallbacks that return a typed default without state.
Impact: `InitCustomTeams`/map `SetPlayerAllianceBJ` have no effect; triggers that gate
on `IsUnitType`/`IsUnitOwnedByPlayer` always take the false branch; script tech
upgrades never affect anything script-visible.
Why not covered by excluded backlog: the old item is coverage; this is the "handled flag
vs actual semantics" defect explicitly named in TZ §5.9. **Possible dedup with the old
JASS-native item — tech-lead to confirm; not a duplicate if the old item was only a
count.**
Fix specification: implement `Player.alliances: BTreeMap<(u8,i32), bool>`, set from
`SetPlayerAlliance`, answer `GetPlayerAlliance`/`IsUnitAlly`/`IsUnitEnemy`; implement
`IsUnitType` from `Unit` state (`hero_level > 0`, building flag, etc.); implement
`IsUnitOwnedByPlayer` from `owner`; store `SetPlayerTechResearched` in
`Player.states`/a tech map and answer `GetPlayerTechCount`; remove the no-op arms.
Regression test(s): BLOCKED: operator test freeze. Manual: H-prologue/H-config; add a
trigger with `IsUnitType(GetTriggerUnit(), UNIT_TYPE_HERO)` and verify it fires.
Verification commands and exact result: H-prologue counts above.
Related: WC3-JASS-001, WC3-JASS-012.

### WC3-MAP-001 — every `war3map.doo` record becomes a destructable (1469/5321 are decorations)
Priority: **P2** · Status: **CONFIRMED** · Confidence: HIGH
Affected commit: `d7af459`
File(s): `crates/wc3-re/src/engine.rs:518-549` (`spawn_map_entities` inserts every
`PlacedDoodad` into `world.destructables`); `crates/wc3-re/src/mapfiles.rs:147-152`
(`war3map.doo` parsed as one flat list); cross-lane twin
`crates/wc3-game/src/sim_bridge.rs:377-404` (`spawn_map_destructables`).
Preconditions: any map with `war3map.doo` (every shipped map).
Trigger / minimal reproduction: `cargo run -q -p wc3-re --example audit_h_data`.
Observed behavior: Prologue01 `war3map.doo` has 5321 records; joined against the TFT
data, **3852 are `DestructableData.slk` rows and 1469 are `Doodads.slk` rows**
(decorations), 0 unknown. The engine spawns all 5321 as `Destructable { hp, alive: true }`
(`spawned_doodads=5321`). The sim bridge does the same at its layer.
Expected behavior: only `DestructableData` rows become attackable/killable destructables;
`Doodads.slk` rows are decorations with no life/targeting.
Root cause: the `.doo` file does not classify its records; classification must go
through the object data (`wc3-data::{destructable,doodad}`), which the loader has but
`spawn_map_entities` does not consult.
Impact: 1469 fake destructables per Prologue-class map in the script world and in
`Sim::destructables` (pathing/selection/destructable enumeration in maps that use it);
memory bloat (5321 objects) and wrong `EnumDestructablesInRect` results once that
native lands. Lane J/I own the sim side; the root is the flat map record.
Why not covered by excluded backlog: map-content classification, not a format byte bug.
Fix specification: split `PlacedDoodad` into destructable vs doodad at load (using the
map's object data + install `Doodads.slk`/`DestructableData.slk`), or filter in
`spawn_map_entities`/`spawn_map_destructables` via `GameDatabase::destructable()`.
Regression test(s): BLOCKED: operator test freeze. Manual: H-data probe counts.
Verification commands and exact result: H-data output above.
Related: WC3-JASS-002.

### WC3-MAP-002 — `MapBundle::open` eagerly reads/decompresses every listfile entry
Priority: **P2** · Status: **NEEDS_REPRO** · Confidence: HIGH (mechanism) / MEDIUM (impact)
Affected commit: `d7af459`
File(s): `crates/wc3-re/src/mapfiles.rs:98-117` (names = fixed list + every listfile
entry; `archive.read(&normalized)` for each), `crates/wc3-mpq/src/lib.rs:117`
(`MAX_FILE_SIZE = 512 MiB`).
Preconditions: a crafted or bloated map whose listfile names large files (or a map with
thousands of entries).
Trigger / minimal reproduction: not built (needs a crafted MPQ). Code path: `read` is
called for every name before the extension is known; unknown entries are read and
discarded.
Observed behavior: a map can force N × up to 512 MiB decompression at load; no cap on
the number of entries (MPQ `MAX_INDEX_ENTRIES` = 4 M at the archive layer).
Expected behavior: read only the fixed inner names that are actually consumed; listfile
names only for the resource scanner, not the map loader.
Root cause: blanket loop over `names` without an extension/known-name filter.
Impact: resource exhaustion (CPU/RAM) from a hostile map; device OOM is reachable
(11 GB device, 512 MiB per entry).
Why not covered by excluded backlog: old item was MPQ decompression limits inside the
codec; this is the map loader's eager extraction policy.
Fix specification: split `MAP_INNER_FILES` (read) from listfile enumeration (metadata
only), or cap unknown-entry reads (size + count) and record skipped entries.
Regression test(s): BLOCKED: operator test freeze. Manual: synthetic map with a large
unknown entry; measure peak RSS.
Verification commands and exact result: code inspection; no crafted MPQ executed.
Related: WC3-MAP-003.

## 4. Findings — P3

### WC3-DATA-001 — upgrade cost arithmetic overflows on map-controlled i32 cells
Priority: **P3** · Status: **CONFIRMED** · Confidence: HIGH
File(s): `crates/wc3-data/src/upgrades.rs:288-296` (`base + per_level * (level-1)`).
Trigger: `cargo run -q -p wc3-re --example audit_h_data` (synthetic UpgradeData row
`goldmod=2147483647`, `gold_cost(3)`).
Observed: debug panic `attempt to multiply with overflow` at `upgrades.rs:295`; a release
build wraps to a negative/incorrect cost. Maps can set these cells via `war3map.w3q`.
Expected: saturating/checked arithmetic, error on overflow.
Impact: malformed map object data can panic debug builds and produce negative costs in
release; no memory unsafety.
Fix: `checked_mul`/`checked_add` + `DataError::BadCell` (or saturating with a diagnostic).
Regression test(s): BLOCKED: operator test freeze; H-data probe is the reproducer.

### WC3-JASS-008 — `max_steps` is a runtime-lifetime counter (permanent script death)
Priority: **P3** · Status: **CONFIRMED**
File(s): `crates/wc3-re/src/jass/vm.rs:1015` (default 20 M), `engine.rs:304` (50 M),
`vm.rs:1129-1132`, `:1282-1285`.
Trigger: `… --example audit_h_steps`.
Observed: Human04 boot `steps=15335`; 600 ticks (20 s) `+2306`; after a spent budget every
later entry returns `Err("step limit exceeded")` until the cap is raised (probe).
Expected: per-tick/per-call budget, or a budget that resets each tick.
Impact: the budget is ~120 h at the measured idle rate (115 ops/s), but any script that
spends millions of ops per tick (the loop guard allows 5 M iterations per loop) exhausts
it in seconds; after that every timer/trigger action fails permanently and only the
`errors` vector records it.
Fix: reset a per-tick step counter in `tick_script` (keep a separate total diagnostic).
Regression test(s): BLOCKED: operator test freeze; H-steps probe.

### WC3-JASS-009 — unterminated `function` is accepted when the body is non-empty
Priority: **P3** · Status: **CONFIRMED**
File(s): `crates/wc3-re/src/jass/parser.rs:188-206`.
Trigger: `… --example audit_h_parser` → `unterminated function ACCEPTED: functions=1 body=1`.
Expected: `ParseError("unterminated function")` for any missing `endfunction`.
Impact: a truncated `war3map.j` (partial download/import) compiles to a partial program
and runs; the error is silently hidden.
Fix: track `endfunction` consumption with a flag; error at EOF unconditionally.
Regression test(s): BLOCKED: operator test freeze; H-parser probe.

### WC3-JASS-010 — `TimerStart` does not reset `elapsed`
Priority: **P3** · Status: **NEEDS_REPRO** (code-clear)
File(s): `crates/wc3-re/src/jass/natives.rs:1568-1579`; `Timer.elapsed` is only
incremented in `vm.rs:1467`.
Observed: restarting a timer keeps the old `elapsed`; `TimerGetElapsed` returns the
lifetime total, not the time since the last start.
Impact: scripts that use `TimerGetElapsed` after a restart (periodic restarts) see a
wrong value; low frequency in campaign scripts.
Fix: `timer.elapsed = 0.0` in `timerstart`.
Regression test(s): BLOCKED: operator test freeze.

### WC3-MAP-003 — missing `common.j`/`Blizzard.j` is silently ignored
Priority: **P3** · Status: **NEEDS_REPRO** (broken install)
File(s): `crates/wc3-re/src/engine.rs:281-299`: `if let Some(source) = source { ... }`;
no `else` error. `run_main` then executes a program without `InitBlizzard`/BJ helpers;
unknown calls are counted as `unimplemented` and return typed defaults.
Impact: on an install where both the loose file and the MPQ entry are missing (or the
archive cannot be opened), the mission starts with a hollow script and no error.
Fix: error when a required script is absent from both sources; at minimum a boot-step
failure and a first-class diagnostic.
Regression test(s): BLOCKED: operator test freeze.

### WC3-JASS-011 — a missing boolexpr handle evaluates to `true`
Priority: **P3** · Status: **NEEDS_REPRO**
File(s): `crates/wc3-re/src/jass/vm.rs:1627-1642` (`None => Ok(true)`).
Impact: a null/invalid condition handle makes a trigger pass; retail errors on a null
condition. Only reachable with a script that passes a stale handle.
Fix: `None => Ok(false)` + diagnostic, or return an error for a null condition.
Regression test(s): BLOCKED: operator test freeze.

### WC3-DATA-002 — `DirSource::resolve` follows `..` outside the root
Priority: **P3** · Status: **CONFIRMED** (no product caller found)
File(s): `crates/wc3-data/src/source.rs:58-80` (`root.join(normalized)`; no component
rejection).
Trigger: `… --example audit_h_data` → `Some(".../root/../secret.txt")`.
Impact: a `Source` path containing `..` (e.g. an object-data `Art=`/model string if it
ever reaches `DirSource::read`) escapes the extracted tree. Current callers pass fixed
table paths, so no live map path was proven.
Fix: reject `..`/absolute components in `normalize_wc3_path` or in `resolve`.
Regression test(s): BLOCKED: operator test freeze; H-data probe.

### WC3-JASS-012 — `GetLocalizedString` returns its key (and `GetLocalizedHotkey` 0)
Priority: **P3** · Status: **CONFIRMED**
File(s): `crates/wc3-re/src/jass/natives.rs:177-178`.
Observed: `Value::Str(a_str(args, 0))` — no table lookup. The engine does load
`Localizer`/`war3map.wts` for object data, but the native does not use it.
Impact: any map that displays `GetLocalizedString("WESTRING_…")` shows the key.
Fix: route through the runtime's string table (the engine already has
`Localizer`/`WTS` in `wc3-game`; pass it into `World` or a lookup callback).
Regression test(s): BLOCKED: operator test freeze.

## 5. Findings — P4 (engineering debt / diagnostics)

- **WC3-DATA-003** — `unwrap_or(false)` on object flags hides malformed cells:
  `crates/wc3-data/src/defs.rs:315,343-349,436-437,498-515`. A cell like `isbldg=q`
  becomes "false" without a diagnostic; `validate` may report it, but the typed path
  does not. Convert to `Result<bool>` propagation or count the defaulted cells.
- **WC3-MAP-004** — `parse_start_locations` returns `Vec::new()` on any layout mismatch
  (`crates/wc3-re/src/mapcontent.rs:150-230`); a v31/TFT tail bug becomes "0 starts"
  instead of an error. Report per-map diagnostics.
- **WC3-JASS-013** — native name typos leave AI arms dead: `"getnextxxpansion"`/
  `"getnext expansion"`/`"getmega target"`/`"setexpansiò n"`/`"gettow nunitcount"`/
  `"geta iplayer"` (`natives.rs:3005-3019,3034`). `common.ai` declares
  `GetNextExpansion`/`GetMegaTarget`/`SetExpansion`; the AI runtime (`ai.rs`) is not on
  the live mission path today, but the census is wrong and future AI use will fail.
  Fix the strings and add a test that every `common.ai` native name resolves.
- **WC3-JASS-014** — `TriggerAddCondition` allocates and returns a new handle
  (`natives.rs:1315-1322`) although `common.j` declares `returns nothing`; every added
  condition leaks a handle. Return `Value::Null`.
- **WC3-DATA-004** — `ObjectDb::field_by_id` resolves through the merged
  `metadata: BTreeMap<String, FieldMeta>` (`objectdata.rs:360-378,386-401`), but field
  ids are only unique per object kind (the file's own comment). A unit/ability id
  collision returns the first kind's column. Resolve through `metas[kind]` using the
  record's `kind`; add a collision report.

## 6. Coverage and per-file observations

`FILE_COVERAGE.csv` — 102 lane-H rows (69 production source files + 2 manifests +
14 examples + 12 tests/examples labelled `NOT_APPLICABLE` where the operator test
freeze applies). Status is `AUDITED` for every production source file. The shared
`FILE_COVERAGE.csv` in this directory is the assembled multi-lane file (773 rows at
close: K 228, I 203, H 102, J 149, G 91).
Representative observations by subsystem:

- **wc3-data table/localizer** — `Table` keeps the raw cell and offers strict vs
  engine-`atof` reads (`table.rs:270-380`), `Localizer` applies overlay → `*Func` →
  `*Strings` and resolves `TRIGSTR` (`localization.rs:208-300`); no parser duplication
  (both delegate to `wc3-formats`). `GameDatabase::load` errors on a present-but-broken
  table and lists missing optional tables (`lib.rs:203-241`).
- **wc3-data object merge** — `ObjectDb::apply_map_objects` delegates to
  `wc3_formats::gamedata::merge_overlay`, keeps base/custom ids and refreshes records
  (`objectdata.rs:412-492`); the only semantic gap found is `field_by_id` (P4).
- **wc3-data views** — units/items/abilities/upgrades/doodads/destructables expose typed
  `Result<Option<…>>` accessors over `Table`; a bad cell is a `DataError`, not a silent
  zero, except the `unwrap_or(false)` flag paths (P4).
- **wc3-re map loading** — `MapBundle` probes the fixed inner names, parses w3e/w3i/wpm/
  wts/w3u/w3t/w3a/w3d/w3b/w3h/w3q/doo/units-doo and records per-file errors
  (`mapfiles.rs:86-174`). `war3map.j` is `from_utf8_lossy` (`:127`) — part of
  WC3-JASS-004. `MapBundle::open` reads every listfile entry (WC3-MAP-002).
- **wc3-re engine** — boot order, object data, `enter_map`, `spawn_map_entities`,
  `tick` verified. `enter_map` runs only `main` (WC3-JASS-001), does not error on
  missing scripts (WC3-MAP-003), spawns all doodads as destructables (WC3-MAP-001).
- **wc3-re JASS** — lexer/parser/AST/compiler/VM/native dispatch read end-to-end;
  findings above. Determinism: `BTreeMap` world/handles, fixed-seed RNG
  (`vm.rs:125-164`), no `HashMap` iteration in the world path (runtime `globals` is a
  `HashMap` but only keyed lookups are used).
- **wc3-re scanners/reports** — `scan`, `manifest`, `consumption`, `cutscenes`,
  `mapboot`, `art`, `icons`, `catalog`, `terrain`, `skin`, `trigger`, `slk`, `txt`,
  `wts`, `blp`, `mdx`, `mpqex`, `guest_host`, `host`, `storm_api`, `gameplay`,
  `abilities` are report/tool layers; byte parsers are delegated to lane G. Bounds are
  checked (`Reader` uses `get`, BLP probe has a 4-byte guard, MDX delegates to
  `wc3-formats::mdx`).

## 7. Cross-lane observations (not counted in lane H findings)

1. **`wc3-sim` runs a second JASS implementation**, but `Sim::attach_script` has no
   non-test caller (`rg attach_script` = only `save.rs` tests), so the live mission is
   `wc3-re`. Two VMs with different semantics is an architectural risk (P4 for the
   tech-lead): any fix must be applied to the VM that actually plays.
2. **`map_player_slots` only parses w3i versions 23..=26**
   (`crates/wc3-game/src/sim_bridge.rs:506-564`). Prologue01 is **v18** (H-prologue),
   so no player records and no `human_slot()`; campaign.rs then takes the
   `set_local_player_any(true)` wildcard path. The retail source of the seat for these
   maps is `config`'s `SetPlayerController` — which is never run (WC3-JASS-001).
3. **Owner squashing** is in `sim_bridge::adopt`
   (`owner_of_player.get(owner.max(0)).unwrap_or(0)`, `sim_bridge.rs:1183-1187`):
   every engine player index outside the bridge map lands on owner 0. That is lane J/I,
   but it composes with the v18 seat inference above.
4. **Doodad classification** also exists in `sim_bridge::spawn_map_destructables`
   (`sim_bridge.rs:377-404`); the same 1469/5321 decoration split applies to
   `Sim::destructables`. Lane J/I owns the sim path; the data evidence is H-data.

## 8. Blocked / NEEDS_REPRO

- No crafted hostile MPQ was executed for WC3-MAP-002 (would need a synthetic archive);
  mechanism is code-proven, impact is inferred from the 512 MiB cap.
- WC3-JASS-010/011/012 are code-proven but not exercised by a dedicated probe (012 is
  also absent from Prologue01; it is a native contract gap).
- Operator test freeze: no test file was added or modified; no `tests/` file was created.
- Lane G cross-check pending: `crates/wc3-formats` WC3-JASS-004's `war3map.j`/WTS
  encoding split and the w3i tail parser are G-owned; lane G should confirm the
  CP1251/UTF-8 policy from its side.

## 9. Evidence paths

- Report: this file. CSV: `analysis/full-audit-2026-11-18/FILE_COVERAGE.csv`.
- Probes: `analysis/full-audit-2026-11-18/probes/{H-config,H-sleep,H-array,H-parser,
  H-steps,H-depth,H-substring,H-data,H-prologue}/` (source + `run.sh` + `output.txt`).
- Probe sources also live at `crates/wc3-re/examples/audit_h_*.rs` (examples, not tests).
- Urgent hand-off: `analysis/tech-lead/AUDIT-H-URGENT-2026-11-18.md`.
- Internal evidence for the boot order: `analysis/agent-re/formats/ENGINE_MAP.md:470-510`.

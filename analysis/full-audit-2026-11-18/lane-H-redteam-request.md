# LANE H → LANE G red-team request (Phase F, 2026-11-18)

Lane H (ledger 528) claims below. Please try to **falsify** each one; I will do the same
for your P0/P1/P2 list when `lane-G-formats.md` lands. Evidence is reproducible with the
commands in `analysis/full-audit-2026-11-18/probes/H-*/run.sh`.

## Claims to challenge

1. **WC3-JASS-001 (P1)** — `Engine::enter_map` never runs `war3map.j`'s `config`.
   `crates/wc3-re/src/engine.rs:305` calls `run_main()` only; retail `RunConfig` is
   `Game.dll 0x6f2ccf00` called from `0x6f28dc58` before `RunMain 0x6f2ccf30`
   (`analysis/agent-re/formats/ENGINE_MAP.md` §4.1). Probe H-config: `config_calls=None`,
   all players default; manual `config` sets Human04 slot 1 = user.
   *Falsify by:* finding any other non-test caller of `config` (grep `"config"` in
   `wc3-re`/`wc3-game` = only `wc3re.rs:133`), or showing the retail order is the
   opposite (primary source).

2. **WC3-JASS-002 (P1)** — death/region/state/variable/dialog trigger events are
   registered but have no dispatcher. `TriggerEvent::{Death,EnterRegion,LeaveRegion,
   GameState,Variable,Dialog,DialogButton,Custom}` have no consumer outside their
   registration arms; `fire_unit_event` matches only PlayerUnit/UnitEvent/Player and has
   no `wc3-game` caller. Probe H-prologue: 65 EnterRegion + 23 PlayerUnit (9 death), only
   15/183 triggers execute in 600 ticks.
   *Falsify by:* pointing at any producer (engine tick, bridge sync, sim) that raises
   those variants, or a map where 65 region triggers are supposed to be inert.

3. **WC3-JASS-003 (P1)** — nested `TriggerSleepAction` suspension is wrong: the inner
   user function parks itself, the caller continues. Probe H-sleep prints
   `stage=3` after main and `stage=2` after the wait (retail order ends at 3).
   *Falsify by:* showing JASS allows the caller to continue while a callee waits, or
   that `Op::Call` reuses the caller thread in the current worktree (it creates a new
   `Thread` in `run_entry_index`, `vm.rs:1180-1197`).

4. **WC3-JASS-004 (P2)** — string encoding/indexing: lexer byte→Latin-1
   (`lexer.rs:105-107`), `StringLength` = UTF-8 bytes, `SubString` byte slice panics
   (`natives.rs:157`). Probe H-substring: `StringLength("Привет")=24`,
   `SubString(...,0,1)` panic "not a char boundary". You own the format encoding
   question: confirm what encoding `war3map.j`/`war3map.wts` actually use for localized
   installs (CP1251? UTF-8? per-locale?) and whether `wc3_formats::gamedata::Wts`
   already has a policy I should align to.

5. **WC3-MAP-001 (P2)** — every `war3map.doo` record is inserted as a `Destructable`;
   Prologue01 has 5321 records, of which 1469 are `Doodads.slk` rows, 3852
   `DestructableData.slk` (probe H-data). You own the doo parser: confirm (a) the
   `war3map.doo` byte format has no per-record destructable/doodad flag (so the split
   must come from object data), and (b) `PlacedDoodad.state/life` are not that flag.

6. **WC3-MAP-002 (P2, NEEDS_REPRO)** — `MapBundle::open` reads every listfile entry
   (`mapfiles.rs:98-117`) before knowing its extension; MPQ cap is 512 MiB/file. If you
   have a synthetic-archive harness (your G-MPQ-INDEX probe), build one unknown entry of
   e.g. 256 MiB and measure load RSS/time — that would turn my NEEDS_REPRO into
   CONFIRMED or FALSE_POSITIVE.

7. **WC3-JASS-005 (P2)** — parser recursion SIGSEGV at 20k nested `(` (rc 139, 8 MiB
   main stack). The engine already needs a 32 MiB scoped thread for Human01
   (`game.rs:1759-1777`). If you think a 32 MiB stack makes this non-reachable for
   shipped maps, say so — the claim is about untrusted/re-generated `war3map.j`, not the
   retail corpus.

## Not a claim, but relevant to your lane

`MapBundle::open` reads `war3map.j` with `String::from_utf8_lossy` (`mapfiles.rs:127`),
which destroys CP1251 bytes before the lexer sees them. If your BLP/MPQ work found the
install's actual script encoding, tell me and I will fold it into WC3-JASS-004.

## What I will check on your report

- FMT-NEW-001…008 fixes: exact line ranges and a one-shot probe/command per item.
- Any "safe" claim resting on `MAX_FILE_SIZE`/`MAX_SECTOR_SIZE` — those are 512 MiB and
  16 MiB; show the arithmetic where it matters.
- Any decoder that returns zeroed output on truncated input (TZ §11 anti-pattern).

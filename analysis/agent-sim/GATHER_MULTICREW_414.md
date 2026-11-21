# Ledger 414 — multi-crew gather root cause, save v3, JASS tick hook, script roster

Repo: `/data/data/com.termux/files/home/src/herofolends-rs`
Date: 2026-11-15 · root: `su -c 'id'` → `uid=0(root) gid=0(root) context=u:r:ksu:s0`
Map: `(2)Justice.w3x` (War3Net test fixture) · install: `/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq`

## 1. The measured failure has two independent causes

`crates/wc3-game/tests/playable_loop.rs` reports `gold 1000 -> 1000` after 2400
`game.update(1/30)` calls with five workers ordered onto one mine. The failure is
not in the harvest economy: a sim-level run of the same command pays (28
deliveries / 500 gold in 2400 ticks before any fix, 29 after). Two separate
defects combine on the game path.

### 1a. The game never steps the sim at all (game-side, file outside this lane)

`Game::new` boots with `boot_menu = melee` (`crates/wc3-game/src/game.rs:672`).
The first `Game::update` consumes that flag and, when no input has been seen,
calls `open_main_menu()` and returns before the tick loop
(`crates/wc3-game/src/game.rs:1975-1984`). `Game::issue`
(`crates/wc3-game/src/game.rs:1858-1868`) pushes the command into `pending` but
does **not** mark `input_seen`, and nothing drains `pending` while the screen is
not `Screen::Playing`.

Evidence (probe `analysis/agent-sim/probe`, `/tmp/probe.log`):

```
PROBE setup melee=true boot_screen=Playing tick=0 workers=5 mine=Vec2(14576.0, 1808.0) gold=1000
PROBE after_2400 screen=MainMenu tick=0 gold=1000->1000
```

`tick=0` after 2400 updates: zero simulation ticks ran, so no worker can pay.
Raising the 600-tick budget to 2400 changes nothing, exactly as measured.

The documented host contract is "a host that injects input before the first
frame drives the battle directly" (`game.rs:7487` test). With one pointer press
before the first update the same 2400-update loop pays 1290 gold / 129 deliveries
(see 1c), and `touch_playability.rs` proves the input route works.

**Exact one-line fix for the game lane (owner 402):** mark host input in
`Game::issue`:

```rust
pub fn issue(&mut self, command: Command) {
    self.input_seen = true;   // host API = injected input; skip the glue screens
    ...
}
```

Nothing in this lane edits `game.rs`.

### 1b. Sim defects found by the game-path investigation (fixed here)

Reproduced at sim level with the exact game geometry
(`crates/wc3-sim/tests/gather_multicrew.rs`): hall (14288,2000), the five worker
ring positions, mine (14576,1808).

1. **Buildings were teleported off their own footprint every match.**
   `Sim::unstick_units` moved any unit whose *centre cell* was unwalkable. A
   building's own footprint always blocks that cell, so on the first tick every
   hall was relocated to the nearest walkable cell centre while its four blocked
   cells stayed behind — a 2x2 phantom wall that later wedged a worker. Evidence
   before the fix (`COARSE setup/after1`):

   ```
   COARSE setup hall_pos=Some(Vec2(14256.0, 1968.0)) blocked=[(111,15),(112,15),(111,16),(112,16)] cell=(111,15) walkable=false
   COARSE after1 hall_pos=Some(Vec2(14144.0, 1856.0))     <- moved by unstick
   ```

   after the fix the hall stays at its spawn position and keeps its footprint.

   Fix: `crates/wc3-sim/src/lib.rs:2150` (`unstick_units`): skip buildings and
   use the movement-grade `can_stand(pos, radius)` + `nearest_standable` for
   mobile units instead of a centre-cell test.

2. **Return trips ran on the straight line only.** The harvest phase machine
   cleared `unit.path` when it switched gathering→ToHall and deposit→ToMine but
   never requested a new path (`advance_economy`). A worker whose line crossed
   the mine footprint walked into it, accumulated `stuck_ticks`, and dropped the
   order. Fix: `request_gather_path` (`crates/wc3-sim/src/lib.rs:5465`) called at
   all six phase transitions (gold and lumber: gather→hall, deposit→resource,
   no-hall fallback); call sites `lib.rs:5528,5538,5566,5662,5671,5698`.

3. **A transient stall dropped a gather order to `Idle` forever.** The stuck
   branch retried once and then called `finish_order`, while `Attack` kept
   trying. Fix: `crates/wc3-sim/src/lib.rs:2611` — harvest/lumber orders re-path
   to their live phase target up to three times before the explicit
   `path blocked` error, exactly like attack.

### 1c. Before / after on the real game path (probe, 2400 updates, input primed)

Before (probe4.log):

```
PROBE hall id=Some(UnitId(1)) pos=Some(Vec2(14288.0, 2000.0)) blocked=[(446,62),(447,62),(446,63),(447,63)] cell_walkable=false cell_size=32 grid=512x384
PROBE hall_after tick=2400 pos=Some(Vec2(14256.0, 1968.0))   <- building teleported
PROBE live frame=1200 screen=Playing tick=1201 gold=1480
PROBE live tick=1201 unit=2 ... phase=ToHall   carry=10
PROBE live tick=1201 unit=3 ... phase=Gathering timer=41
PROBE live tick=1201 unit=4 pos=(14259,1994) order=Idle phase=ToMine goal=None carry=0
PROBE live tick=1201 unit=5 ... phase=Gathering timer=23
PROBE live tick=1201 unit=6 ... phase=ToHall   carry=10
PROBE live_2400 gold=1000->2010 deliveries=101 errors=[(264, "path blocked")]
PROBE final unit=4 pos=(14259,1994) order=Idle goal=None
```

After (probe5.log):

```
PROBE hall pos=Some(Vec2(14288.0, 2000.0)) blocked=[(446,62),(447,62),(446,63),(447,63)]
PROBE hall_after tick=2400 pos=Some(Vec2(14288.0, 2000.0))  <- stays on its footprint
PROBE live frame=1200 screen=Playing tick=1201 gold=1620
PROBE live tick=1201 unit=2 ... phase=Gathering timer=40
PROBE live tick=1201 unit=3 ... phase=ToHall   carry=10
PROBE live tick=1201 unit=4 ... phase=Gathering timer=51
PROBE live tick=1201 unit=5 ... phase=Gathering timer=14
PROBE live tick=1201 unit=6 ... phase=ToMine  path=5
PROBE live_2400 gold=1000->2290 deliveries=129 errors=[]
PROBE final tick=2400: units 2,3,4,5,6 all Order::Harvest (Gathering/ToMine/ToHall)
```

Per-worker sim table (Justice melee opening, `MULTIDBG` every 60 ticks,
`crates/wc3-sim/tests/gather_multicrew.rs`) shows all five in `ToMine` at tick
61 and `Gathering`/`ToHall` afterwards; the summary is
`gold=1000->1500 gained=500 deliveries_seen=29` (28 before the fixes).

Additional sim gates added (`crates/wc3-sim/tests/gather_multicrew.rs`):

* `economy_gold_five_workers_one_mine_on_justice_all_pay` — w3i melee opening, real WPM grid.
* `economy_gold_game_opening_five_workers_all_pay` — exact game geometry, 25/25/24/25/25 trips.
* `economy_gold_coarse_game_grid_five_workers_all_pay` — 128-unit terrain-tile grid,
  38 trips per worker, hall keeps its footprint.

## 2. Save format v3 (`ethereal`)

`crates/wc3-sim/src/save.rs`:

* `SAVE_FORMAT_VERSION = 3` (`save.rs:89`); `open_envelope` accepts `1..=3`
  (`save.rs:2499`), everything else is `SaveError::UnknownVersion`.
* `write_unit(.., version)` appends the `ethereal` bool only for `version >= 3`
  (`save.rs:1136`); `read_unit(.., version)` reads it only for `v>=3`
  (`save.rs:1279`), otherwise `false`.
* New `decode_v3` arm (`save.rs:2537`); `from_save` dispatches 1/2/3.
* `analysis`: `encode_payload_version` + `seal_version` exist so a genuine v2
  file can be produced in tests (`#[cfg(test)]` unit test in `save.rs`).

Evidence:

```
crates/wc3-sim/tests/save_load.rs: 10 passed (v3 accepted, v4 rejected, v1/v2 migrate)
crates/wc3-sim --lib: 42 passed
test: version_2_file_loads_with_ethereal_false
  v2 header version = 2, unit loads with ethereal=false
  v3 round trip preserves ethereal=true and state_hash equality
```

## 3. JASS tick hook on `Sim::step` (addendum item 1)

`crates/wc3-sim/src/lib.rs`:

* `script: Option<Box<JassVm>>` field (`lib.rs:881`), `None` by default.
* `Sim::attach_script(bundle, seed)` builds the VM around a scratch `Sim`, runs
  the map entry points with the live state swapped in, returns `EntryReport`.
* `Sim::with_script` swaps the live `Sim` into the VM for the duration of a
  script call; `Sim::step` calls `advance_script()` once per tick
  (`lib.rs:1054`, call in `step`), which is inert (`is_none` check) without a
  bundle.
* One VM tick per sim tick: `dispatch_pending_events` → `advance(1/30)` →
  `track_regions`/`track_unit_ranges`; 300 000-instruction budget and seed come
  from `JassVm::new`.

Evidence: `crates/wc3-sim/tests/jass_tick_hook.rs` — `2 passed`:
timer trigger damages a real unit through `Sim::step` (2 fires / 30 ticks, hp
dropped, script clock 1.0 s) and a sim without a bundle does no script work.
`crates/wc3-sim/tests/jass_trigger_fires.rs` and the rest of the JASS suite stay
green (full run below).

## 4. Script roster coverage (addendum item 2)

`analysis/agent-jass/coverage_report.json` does not contain the per-id roster
gap (its `corpus_missing` is about natives and is empty), so the id list was
measured from the 96 extracted scripts (the lane's own corpus) and is now a
test: `crates/wc3-sim/tests/script_roster.rs`.

```
SCRIPT_ROSTER files=96 distinct_ids=176 resolved=167 unreachable=9 calls=11425
```

* Fixed (real table rows, race `demon` filtered out before):
  `nbal` Doom Guard hp 1350, `nfel` Felhound hp 750, `ninf` Infernal hp 1500.
  Generator `analysis/agent-sim/gen_creeps.py` accepts race `demon` and appends
  those ids **after** the sorted set so no saved `def_index` shifts
  (`crates/wc3-sim/src/creeps_gen.rs`: 324 -> 327 defs, only three appends).
* Unreachable with evidence (asserted to stay unresolved):
  `nfro`, `nrac`, `necr`, `nfly`, `nfed` — no `UnitData.slk` / `UnitBalance.slk`
  / `UnitWeapons.slk` row in `War3.mpq` or `War3x.mpq` (read directly through
  `wc3-mpq`); only `UnitUI.slk` still lists them, and `nrac` is a map-only unit
  of `(2)Justice.w3x`. `o000`, `o002`, `o003`, `o006` — custom rawcodes of the
  map "Narabotka ot Tokhi" (`war3map.w3u` object data, not shipped tables).

## 5. Verification commands

```
cargo test -p wc3-sim --offline        rc=0  (51 test binaries, all ok, 0 failed)
cargo test -p wc3-sim --offline --lib  rc=0  (42 passed)
cargo check -p wc3-game --offline      rc=0
```

`crates/wc3-game/tests/playable_loop.rs` still fails on this tree because of
1a (game.rs, owner 402). With the documented pre-frame input the same game path
pays 129 deliveries (1c). No `wc3-game` source or test file was modified here.

## Risks / next

* The unstick fix changes the position of every building by ≤1 cell relative to
  the old (buggy) behavior; any golden positions captured before this change
  shift accordingly.
* `game.rs` one-liner for 402 (see 1a) is the last step to make `playable_loop`
  green; it is outside this lane's owner paths.
* The wyrm splash test ally was changed from a footman to a sheep
  (`crates/wc3-sim/tests/combat_depth.rs`) because a building now keeps its
  footprint: an armed ally inside tower range killed the tower before the wyrm
  could walk into range, which measured nothing. Splash assertions unchanged.

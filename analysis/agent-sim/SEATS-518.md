# SEATS-518 — 8-seat sim capacity and `bind_human_seat`

Lane A, ledger 518, 2026-11-18. Scope: `crates/wc3-sim`. No tests were added
(operator order 2026-11-18); the proof artifacts are `scripts/h2-cargo check`
and the headless probe `crates/wc3-sim/examples/seat_binding_probe.rs`.

## Defect

Device evidence before the fix:

```
CAMPAIGN_HUMAN_SLOT idx=0 name=Prologue01 slot=4 sim_bound=false
crates/wc3-game/src/campaign.rs:1123  pub const SIM_PLAYER_SLOTS: usize = 2;
```

Two causes, both in `wc3-sim`:

1. Every per-seat array was two wide (`resources`, `lumber`, `explored`,
   `last_seen`, `upgrades`) and the fog bitmask queries treated any owner above
   1 as omniscient, so seat 4 had no bank, no fog and no ownership.
2. **The spawn path clamped the owner**: `spawn_def_index_at` built the unit
   with `owner: owner.min(2)` (`lib.rs:1487` before the fix). A unit spawned for
   seat 4 therefore became owner 2, and even a widened sim would report
   `units=0` for the bound seat. This was the actual reason `sim_bound=false`
   never turned into a working seat.

## Fix (files, line anchors after the change)

`crates/wc3-sim/src/lib.rs`

| what | where |
|---|---|
| `MAX_PLAYERS = 8`, `NEUTRAL_OWNER = 12` (`common.j` `PLAYER_NEUTRAL_AGGRESSIVE`), `NEUTRAL_PASSIVE_OWNER = 27`, `seat_row(owner)` | 118–136 |
| seat banks: public `resources`/`lumber` stay `[i32; 2]` (55 shipped call sites assign `[a, b]`), seats 2..8 live in `seat_gold`/`seat_lumber`; `human_seat: Option<u8>` | 926–942 |
| `explored: [Vec<u8>; MAX_PLAYERS]`, `last_seen: [BTreeMap; MAX_PLAYERS]`, `upgrades: [BTreeMap; MAX_PLAYERS]` | 945–976 |
| `gold/set_gold/add_gold/lumber_of/set_lumber/add_lumber` (the single read/write path for a seat) | 1174–1240 |
| `local_player()` (bound seat, else seat 0) | 1249 |
| `bind_human_seat(seat) -> Result<SeatBinding, SeatError>` — validates the seat, stamps `update_fog` immediately, returns `units/buildings/visible_units/visible_cells/gold/lumber` | 1265–1320 |
| spawn owner clamp removed (`owner.min(2)` → `owner`) | 1487 |
| fog: stamp only real seats, marker stride `tick * MAX_PLAYERS` (was `tick * 4`, aliased above seat 3), `last_seen` over every seat + `NEUTRAL_OWNER`, `is_visible/is_explored/visibility/explored_*/fog_mask/last_seen_*` keyed by `seat_row`, `is_visible_at` follows `local_player()` | 6752–6870 |
| `state_hash` feeds all 8 banks (the human seat is a view parameter and is *not* hashed) | 7074 |
| `set_unit_owner` no longer clamps to 0/1; `upgrades_of/upgrades_mut` replace `owner.min(1)` | 5183–5200, 6978 |
| `spawn_camp`/`cluster_neutral_camps` use `NEUTRAL_OWNER`; `is_critter`/`grant_kill_bounty` use `seat_row` | 3890–3920, 475, 3666 |

`crates/wc3-sim/src/save.rs` — **format version 9**

- `SAVE_FORMAT_VERSION = 9` (`save.rs:147`).
- `seats_for_version(v)`: 2 for v1..v8, `MAX_PLAYERS` for v9.
- `SEC_RESOURCES` / `SEC_FOG` / `SEC_UPGRADES` records hold 8 seats at v9; the
  version gate keeps every older layout byte-identical.
- New self-describing `SEC_SEATS` frame (`0x5431`, `save.rs:207`): explicit seat
  count (`MAX_PLAYERS as u8`) + `opt_u8` human seat. The reader rejects a seat
  count it does not carry and a human seat outside the band.
- `decode_v8` now decodes with version 8 (it used `SAVE_FORMAT_VERSION`, which
  would have read a v8 file with the v9 layout); `decode_v9` is the current arm.
- Old files still load: seats 2..7 restore to the fresh-sim value (1000 gold /
  1000 lumber, empty fog and upgrades). A v8 2-seat save round-trips its
  `save_digest` unchanged.

`crates/wc3-sim/src/items.rs` — shop buy/sell used `owner >= resources.len()`
(two seats) and indexed the legacy array; now they use `seat_row` + `gold` /
`add_gold`, so a seat above 1 can trade.

## Evidence

```
$ scripts/h2-cargo check -p wc3-sim --offline
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.49s
rc=0

$ scripts/h2-cargo run -p wc3-sim --example seat_binding_probe --offline
SEAT_PROBE spawn-seat-owner ok=true unit=1 owner=Some(4)
SEAT_PROBE bind-seat-4 ok=true seat=4 units=1 buildings=0 visible_units=1 visible_cells=357 local=4
SEAT_PROBE fog-ownership ok=true seat4_sees_own=true seat0_sees_seat4=false seat4_sees_seat0=false render_local=4
SEAT_PROBE neutral-band ok=true neutral_owner=12 camp_owners=[12, 12]
SEAT_PROBE banks ok=true seats=8 seat4_gold=104 seat4_lumber=204 seat0_gold=100
SEAT_PROBE save-load ok=true format_version=9 save_bytes=39068 human_seat=Some(4) state_hash=0xb99116d77f69753b fog_before=Visible fog_after=Visible reencode_identical=true
SEAT_PROBE result=PASS checks=6
rc=0
```

Real binary, same tree (lane B's campaign session + this sim):

```
$ scripts/h2-cargo run -p wc3-game --example campaign_report --offline -- --mission Prologue01 --frames 30
CAMPAIGN_HUMAN_SLOT idx=0 name=Prologue01 slot=4 sim_bound=true units=0 visible=0
CAMPAIGN_LOAD source=mpq path=Maps\Campaign\Prologue01.w3m bytes=305448 units=60 quests=1 script=true title=В погоне за видением sim_bound=true seat_units=0 seat_visible=0
rc=0
```

Before the fix the same run printed `sim_bound=false`. The bind now succeeds (no
`CAMPAIGN_SEAT_UNBOUND`, title resolved), and `units=0` is exactly the remaining
game-side seam below: the map's seat‑4 units are still spawned as owner 2, so
the sim has nothing owned by seat 4 to show.

## Remaining seam (owner: tech-lead, scene wiring)

`bind_human_seat` is already called by `crates/wc3-game/src/campaign.rs:1469`
(`bind_campaign_seat`). The mission still spawns zero seat-4 units because
`crates/wc3-game/src/sim_bridge.rs:241` keeps the two-seat owner squash:

```rust
let owner = if unit.player <= 1 { unit.player as u8 } else { 2 };
```

Required in the same merge (tech-lead's `game.rs`/scene slice):

```rust
let owner = if (unit.player as usize) < wc3_sim::MAX_PLAYERS {
    unit.player as u8
} else {
    wc3_sim::NEUTRAL_OWNER
};
// and at sim_bridge.rs:250 the neutral bucket test becomes
if owner == wc3_sim::NEUTRAL_OWNER { neutrals.push((id, pos)); }
```

`sim_bridge.rs:1110` already calls `sim.cluster_neutral_camps()`, which now
clusters `NEUTRAL_OWNER`, so camps form again once the mapping above lands.

## Notes

- `crates/wc3-sim/tests/{merge,melee_spawn,melee_matrix,creeps}.rs`: four
  existing assertions that encoded the old neutral id 2 now compare against
  `wc3_sim::NEUTRAL_OWNER`. No new tests.
- `squad.rs` filters creeps with `unit.owner != 2`; camps now sit at 12, so a
  squad scan may target a creep camp (WC3-correct, previously excluded by the
  magic constant). Left for the squad lane; no test depends on it.
- Pre-existing `jass_native_coverage` failures (3) are untouched and unrelated.

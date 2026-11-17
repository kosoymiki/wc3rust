# agent-sim — WC3-like RTS simulation layer

**Project:** `/data/data/com.termux/files/home/src/herofolends-rs`
**Owner paths:** `crates/wc3-sim/src/**`, `crates/wc3-sim/tests/**`, `analysis/agent-sim/**`
**Date:** 2026-11-12
**Toolchain:** Termux aarch64, rustc/cargo (workspace edition 2024), `ulimit -v 4000000`, `CARGO_BUILD_JOBS=1`, `CARGO_PROFILE_DEV_DEBUG=0`.

---

## Goal

Replace the 946-line simulation slice with a credible WC3-like RTS simulation that
feels good on a phone:

- pathfinding with dynamic avoidance, formations, no stuck units, bounded cost;
- attack-move / patrol / stop / hold with correct transitions and queueable orders;
- full resource lifecycle (mine → carry → return → deposit), supply and upkeep;
- construction sites, production queues, rally points;
- ability framework with unit/point/no-target spells, mana, cooldowns, ranges;
- WC3 damage/armor tables, per-player supply;
- deterministic AI that rebuilds an army, expands, attacks in waves and retreats;
- deterministic seeded simulation with tick-based tests;
- 200+ units at a 60 Hz budget on the phone CPU;
- data-driven definitions so real WC3 tables feed the sim (four-race roster).

The game crate is **read-only** in this task: `game.rs` was not edited. All existing
public API used by `wc3-game`, `wc3-replay`, `herofolends-bench` and
`herofolends-headless` is preserved (`Sim`, `Unit`, `Command`, `Ability`, `Event`,
`Order`, `UnitKind`, field names and semantics); new functionality is additive.

---

## Actions

### Source layout (new modules, ~9.4k lines incl. generated roster)

| File | Lines | Contents |
|---|---|---|
| `crates/wc3-sim/src/lib.rs` | 2556 | `Sim`, tick pipeline, `Unit`, orders, movement/steering, combat, economy, supply/upkeep, fog, construction, production, state hash, legacy tests |
| `crates/wc3-sim/src/data.rs` | 500 | `ArmorType`/`AttackType`, WC3 damage matrix, armor formula, `UnitStats`, `UnitDef`, roster lookups |
| `crates/wc3-sim/src/roster_gen.rs` | 3731 | **generated** 98-definition four-race roster from real WC3 tables |
| `crates/wc3-sim/src/nav.rs` | 377 | bounded A* with reused scratch, nearest-walkable ring search, flow-field tracing, formation slots |
| `crates/wc3-sim/src/spatial.rs` | 101 | uniform spatial hash (bounded neighbour queries) |
| `crates/wc3-sim/src/abilities.rs` | 480 | spell defs from `AbilityData.slk`, targeting/validation, delayed Blizzard waves |
| `crates/wc3-sim/src/ai.rs` | 875 | AI config/state, workers, supply, tech, army, waves, defence, retreat, expansion |
| `crates/wc3-sim/tests/*.rs` | 1754 | 11 integration suites (93 tests total incl. unit tests) |
| `analysis/agent-sim/gen_roster.py` | — | SLK/INI parser + curated tech tree → `roster_gen.rs` |
| `analysis/agent-sim/RESEARCH.md` | — | evidence ledger (URLs, dates, quotes, donor `file:line`) |
| `analysis/agent-sim/ROSTER.md` | — | generated grounding table of all 98 defs |
| `analysis/agent-sim/REPORT.md` | — | this report |

### Systems delivered

1. **Pathfinding / movement**
   - `nav::PathFinder`: A* with 8-neighbour movement, no corner cutting,
     generation-stamped array reuse, `MAX_ASTAR_NODES = 20_000`, 8 path requests per tick.
   - `Sim::nearest_walkable_cell`: deterministic concentric-ring nearest-pathable point
     when an order lands on a blocked cell (WC3 mechanism, see RESEARCH §3).
   - Group moves: cached flow field (`240` tick TTL, max 16 goals) traced per unit +
     deterministic formation slots (`Box`/`Line`/`Column`) assigned by nearest slot in
     entity order.
   - Steering: separation from allies/buildings (bounded to 24 neighbours via spatial
     hash), axis slide when blocked, footprint-aware standing checks, spawn relocation
     out of blocked cells, escape from building footprints, 150-tick bounded stuck
     recovery (two repaths then release with `Event::Error("path blocked")`).
2. **Orders**
   - `Order`: `Idle`, `Move`, `Attack`, `MoveAttack`, `Hold`, `Patrol{a,b,forward}`,
     `Harvest{mine,phase}`, `Build(site)`.
   - `OrderSpec` + `Sim::issue`, `Sim::queue_order` (max 8), `move_group`, `attack_move`,
     `patrol`, `hold`, `stop`.
   - Patrol remembers endpoints while engaging (`Unit::patrol`) and resumes patrol after a
     kill; attack-move resumes its destination; `Stop` clears the queue.
3. **Economy**
   - Gold mines (`add_gold_mine`, finite `remaining`), worker cycle
     `ToMine → Gathering(60 ticks) → ToHall → deposit`, 10 gold per load,
     deposit threshold footprint-aware, local fallback when no hall exists.
   - Passive stipend (`passive_income`, toggleable; tests disable it for exact deltas).
   - Upkeep 100/70/40% at 0-50/51-80/81+ supply (Liquipedia + `MiscGame.txt`).
4. **Production / construction**
   - Production queue max 5, full refund on cancel, rally points, exit position search.
   - Supply gate at completion with a `blocked` flag and `"supply blocked"` event.
   - `train_def` enforces real `trained_at` + `requires` tech; legacy `Command::Train`
     stays permissive for the scripted game HUD.
   - `place_building` creates a construction site (10% HP), blocks its footprint, requires
     a nearby builder, channels `build_ticks`, raises HP with progress, raises supply on
     completion; `cancel_construction` refunds and frees cells.
5. **Combat**
   - WC3 damage matrix in permille (`DamageBonus*` rows from real data) → armor rating
     reduction `100/(100+6a)`, negative armor amplification table, minimum 1 damage.
   - Weapon dice rolls (`dmgplus + NdS`) through the seeded RNG.
   - Acquisition via spatial hash (never O(n²)), melee chase-stop at range, ranged hold
     distance, Hold never chases, armed buildings (towers) auto-fire.
   - Death clears paths/queues/production and frees building footprints.
6. **Abilities (6 real WC3 spells)**
   - Framework: `SpellDef { target: Unit|Point|None, mana, cooldown, range, radius, power,
     duration, waves, waves interval, jumps, decay }`, per-unit `spell_cd`, mana regen.
   - Holy Light (`AHhb`, 75 mana/5 s/800 range/heal 200), Heal (`Ahea`, 5/1 s/250/25),
     Blizzard (`AHbz`, 6 waves × 30 magic, 200 radius), Entangling Roots (`AEer`, root +
     damage), Chain Lightning (`AOcl`, 100 + 4 bounces at 85%), Divine Shield (`AHds`,
     no-target, 10 s invulnerability, 125 mana/75 s).
7. **Supply / fog**
   - Real WC3 food values in the roster; hall 12, farm 6, orc burrow 10, ziggurat 10,
     moon well 10; cap 100; supply used/cap computed from live units.
   - Per-player `visibility` bits and persistent `explored` maps with cached disc offsets.
8. **AI (deterministic, no RNG)**
   - Workers (target count, queue-aware), harvest assignment, supply buildings, one-level
     tech prerequisites, barracks/production construction, army composition cycle,
     wave attacks, home defence, retreat at <35% average HP, expansion to a far mine with
     a dedicated worker, spell usage (heals/shield/damage/Blizzard).
9. **Roster (four races, real data)**
   - 98 defs: Human 26, Orc 24, Undead 24, Night Elf 24. Every def has HP/armor/damage
     dice/range/speed/cost/supply/mana/build times/footprint/model/ability ids from
     `UnitBalance.slk`, `UnitWeapons.slk`, `UnitData.slk`, `UnitAbilities.slk`.
   - Training producers and requirements curated to the classic tech tree; all 44 building
     types are placeable/constructible and all units trainable in tests.
   - 89/98 model paths exist in the local RoC MPQ; the 9 TFT-only models carry
     `model_present = false` (no fake availability).
10. **Determinism**
    - Sorted vectors / `BTreeMap` for caches; stable entity order; integer combat tables;
      FNV-1a hash covering units, orders, queues, production, construction, spells, mines,
      AI state and RNG. Same seed + command log → identical hashes (tested with and
      without AI).

### Compatibility / API

Preserved for `wc3-game`/HUD: `Sim::{new, spawn, unit, unit_mut, alive_units, step,
resources, grid, tick, seed, events, state_hash, player}`, all `Unit` fields read by the
game (`id, owner, kind, pos, hp, max_hp, order, path, cooldown, facing, production,
alive`), `Command` variants and `Event` variants (only added, never removed), `Order`
variants (`Idle/Move/Attack/MoveAttack/Hold` still match), `Ability::{Heal, Fireball}`,
`UnitKind::{Footman, Archer, Peasant, TownHall}`, `TICKS_PER_SECOND`.

New sim APIs available to the game layer: `issue`, `queue_order`, `move_group`,
`attack_move`, `patrol`, `hold`, `stop`, `cast`, `train_def`, `place_building`,
`cancel_construction`, `set_rally`, `add_gold_mine`, `enable_ai`,
`enable_ai_for_race`, `supply_used`, `supply_cap`, `is_visible`, `is_explored`,
`spawn_def`, `find_build_site`, `army_units`, `idle_workers`.

---

## Evidence

### Build and test (rc / output)

Command:

```
ulimit -v 4000000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo check -p wc3-sim --all-targets
```

Result: `wc3_sim_check_rc=0` (no warnings).

Command:

```
ulimit -v 4000000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p wc3-sim
```

Result (`test result` lines, all suites):

```
lib          24 passed; 0 failed
abilities     9 passed; 0 failed
ai            6 passed; 0 failed
combat       10 passed; 0 failed
determinism   3 passed; 0 failed
economy      10 passed; 0 failed
game_compat   1 passed; 0 failed
movement     14 passed; 0 failed
perf          0 passed; 2 ignored (release gate)
production   10 passed; 0 failed
roster        6 passed; 0 failed
Total: 93 passed, 0 failed
```

Notable suite coverage:

- `movement`: wall routing (never enters blocked cell), concave pocket escape,
  unreachable goal terminates, 24-unit formation arrival + separation, 32-unit narrow
  gate (≥24 pass), patrol both endpoints, patrol resume after combat, queue executes
  second order, stop clears queue, hold no chase, blocked-goal nearest walkable,
  footprint free-on-death, blocked spawn relocation.
- `economy`: 10-gold round trips and mine depletion, worker-only harvest, supply cap
  blocks at 6 footmen, farm raises cap, upkeep 70%, no-hall fallback, visibility/explored.
- `production`: queue cap 5, refunds, rally, tech requirements (Blacksmith before
  Rifleman), construction completion/supply, cancel + footprint free, legacy instant
  hall build, spawn on walkable ground.
- `combat`: WC3 armor/attack typing, exact typed-damage numbers (Normal vs medium armor 2 =
  133 of 100), pierce vs light/fortified, negative armor, death clears production,
  ranged stand-off, melee kill, invulnerability, tower fire, dead units inert.
- `abilities`: mana/cooldown contract, wrong ally/range/target errors, mana regen,
  6-wave Blizzard exact damage, roots immobilise then move, chain lightning decay,
  Divine Shield duration, spell kills.
- `roster`: 98 defs / real counts, all 98 units trainable from their real producer, all
  44 buildings constructible by a race worker, 89 model paths present.
- `ai`: barracks + army by tick 4200, wave damages the player hall, base defence,
  retreat branch, second-mine expansion, same-seed determinism.
- `determinism`: 1200-tick scripted log is bit-identical; different seed changes rolls;
  AI run reproducible.
- `game_compat`: compiles every field/variant the game/HUD read, assigns `sim.grid`
  after `Sim::new` like the game does.

### Performance gate (release, phone CPU)

Command:

```
ulimit -v 4000000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 \
cargo test -p wc3-sim --release --test perf -- --ignored --nocapture
```

Output:

```
PERF burst_path_ms=32.45
PERF units=400 ticks=600 total_ms=318.73 per_tick_ms=0.531 hash=0c025f8d530afe31
test result: ok. 2 passed; 0 failed
```

(An earlier identical run on the same binary measured 0.427 ms/tick; both are well
inside budget and the log is kept in `analysis/agent-sim/final-perf.log`.)

Interpretation: 400 simultaneous units (200 v 200 attack-move across the map, plus
separation/combat/fog) at **0.531 ms per tick**, i.e. **31× headroom** inside the
16.67 ms 60 Hz budget; a 200-unit group move through a forced-to-path wall settles in
32 ms total including 120 ticks of movement.

### Game-data grounding

- `analysis/war3_full/Units/UnitBalance.slk`: hfoo HP 420 / armor 2 / speed 270 /
  sight 1400 / gold 160 / food 2 / bldtm 20 s; hrif 520/0/270/240/3/26 s; hpea
  220/0/190/90/1/15 s; htow 1500/5/450/food 12/180 s; hkni 800/6.
- `analysis/war3_full/Units/UnitWeapons.slk`: hfoo normal 11+1d2 1.35 s range 90
  acquire 500; hrif pierce 16+2d4 1.6 s range 400; hpea 4+1d2 2 s.
- `analysis/war3x_local/Units/MiscGame.txt:103,106-112`: `DefenseArmor=0.06` and the
  complete `DamageBonus*` matrix used verbatim.
- `analysis/war3_full/Units/AbilityData.slk`: AHhb/AHds/AHbz/AEer/AOcl/Ahea mana,
  cooldown, range, area, duration, wave data (see RESEARCH §2).
- `analysis/list-War3-clean.txt`: 2048 `.mdx` paths used to verify 89 roster models.

### Web research (stealth-browser MCP, access date 2026-11-12)

- `https://liquipedia.net/warcraft/Armor` — armor reduction formula; integer table
  cross-checked against the published values.
- `https://liquipedia.net/warcraft/Upkeep` — 100/70/40% at 0-50/51-80/81+.
- `https://waywardstrategy.com/2018/11/14/why-cc-rivals-is-really-fun/` — mobile RTS
  3-4 minute target vs ~15 minute desktop default (reported-unverified, used to justify
  a single-base skirmish scale, not to change WC3 stats).
- One Liquipedia page (`Armor_and_Attack_types`) returned a Cloudflare "Verify you are
  human" wall; no bypass was attempted, the real `MiscGame.txt` data was used instead.

### Donor mechanisms (read, not executed)

- `donors/W3AndroidEngine/.../CPathfindingProcessor.java:50-63` — nearest-pathable
  32/1024 ring search, bounded search iterations (4096/16), slide 4×64, detour ×16.
- `donors/OpenRA/OpenRA.Mods.Common/Pathfinder/PathSearch.cs:30,72` — search-layer pool
  and weighted heuristic.

### Cross-crate status (observed)

- `cargo check -p wc3-sim --all-targets` → rc 0.
- `cargo check -p wc3-game` still fails in **pre-existing, unrelated** `ui.rs` code
  (`crate::text` missing; `current_layer` moved) owned by the UI layer; no error points
  at `wc3_sim`. `tests/game_compat.rs` pins the entire sim surface the game reads.
- `cargo test -p wc3-replay` fails on a **pre-existing** non-exhaustive match:
  `encode_command` has no arms for `Command::Harvest` / `Command::CancelTrain`, variants
  that already existed before this task (the old sim and its gameplay tests used them).
  Replay is a data crate outside this task's owner paths, so it was not edited; the fix
  is two encode arms (and matching decode tags) in `crates/wc3-replay/src/lib.rs`.

---

## Result

**Done.** The simulation layer is a complete, tested WC3-like RTS core:

- 93/93 tick-based tests pass, deterministic with and without AI;
- 400 units simulate at 0.531 ms/tick in release (60 Hz budget: 16.67 ms, 31× headroom);
  full test log `analysis/agent-sim/final-test.log`, perf log `analysis/agent-sim/final-perf.log`;
- 98 data-grounded definitions across all four races; every unit trainable, every
  building constructible in tests;
- existing game/replay command and unit surface preserved (compile-probed);
- research and roster provenance recorded with URLs/dates and `file:line` donor evidence.

## Risks / Next

1. **Supply cap and the current game HUD.** The sim now enforces WC3 supply: a town hall
   provides 12 food, so the game starts with room for six footmen. The HUD has no farm
   button (the game can only build another town hall for +12). This is intentional WC3
   accuracy + mobile pacing, and training shows the `supply blocked` toast, but the game
   owner may want to add a farm button or start with two halls.
2. **Nine TFT models** (`Keep`, `Castle`, `Kodo Beast`, `Stronghold`, `Fortress`,
   `Halls of the Dead`, `Black Citadel`, `Tree of Ages`, `Tree of Eternity`) are not in
   the local RoC MPQ. Definitions carry canonical paths with `model_present=false`; the
   data layer should merge the TFT archive or map substitutes. Nothing is faked.
3. **Replay crate compile break** described above is external and pre-existing; replay
   encoding should add Harvest/CancelTrain arms if replays are meant to record them.
4. **Hero leveling / items / auras** are out of scope: heroes exist as level-1 casters
   with their real mana and spells; ability ids for all units are preserved in
   `UnitDef::ability_ids` for a future buff/aura system.
5. **AI defaults.** The built-in AI is opt-in (`enable_ai` / `enable_ai_for_race`); the
   game still runs its own wave script. Switching the game to the sim AI is a one-line
   change for the game owner and is covered by six AI tests.

## H2_PERSISTED

- capture: **552** (implementation), **556** (final verification)
- experience: **4261**, **4262**, **4263**
- monitor ledger: **302** (register → checkpoint → closed)
- dream report: `~/.local/share/opencode/harness2/reports/dream-20261112-130746.json`
  (3 experiences scanned, 0 duplicates/promotions, wiki/core refreshed)
- corpus preflight at start: `ae_corpus_discipline+evidence` (must gates honoured)
- REPORT: this file
- SESSION_CLOSED: **monitor 302** closed with `state=closed`, `last_status=DONE`,
  `h2_ids_json={"capture":[552,556],"experience":[4261,4262,4263],"ledger":302}`
  (2026-11-12). Final logs: `analysis/agent-sim/final-test.log`,
  `analysis/agent-sim/final-perf.log`.

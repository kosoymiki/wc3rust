# Simulation / gameplay research ledger

Date of all web access below: **2026-11-12** (server clock, MSK). Method for
web sources: **stealth-browser MCP** (`spawn_browser` → `navigate` →
`execute_script` → `close_instance`, headless Chromium, Termux). Local game
data was read from the repository extraction of `War3.mpq`; no network.

Claim labels: `observed` = directly printed/read in the source, `inferred` =
derived from observed data, `reported-unverified` = a secondary source states
it, `unknown` = not established.

---

## 1. Armor formula and upkeep (primary/secondary web source)

**Observed** — https://liquipedia.net/warcraft/Armor (accessed 2026-11-12,
stealth-browser, `domcontentloaded`, content extracted from
`.mw-parser-output`; page loaded completely, no bypass attempted):

> "Damage reduction (positive armor): Reduction = armor * 0.06 / 1 + 0.06 * armor"
> "Amplification = 2 - 0.94armor"
> Armor 1 → 5.66% reduction, armor 5 → 23.08%, armor 10 → 37.50%.

**Observed** — https://liquipedia.net/warcraft/Upkeep (accessed 2026-11-12):

> "No Upkeep 0 - 50 100% income"
> "Low Upkeep 51 - 80 70%"
> "High Upkeep 81+ 40%"

**Observed (primary game data, better than the wiki)** —
`analysis/war3x_local/Units/MiscGame.txt:103`:

```
DefenseArmor=0.06
```

Implementation: `data::armor_reduction` uses `damage * 100 / (100 + 6*armor)`,
the integer-exact rearrangement of the formula. Negative armor uses a
precomputed permille table (`2 - 0.94^armor`, values match the Liquipedia table
-1 → 1060‰ … -20 → 1710‰), so no `powf` is needed and replay hashes cannot
depend on libm.

**Observed** — `analysis/war3x_local/Units/MiscGame.txt:106-112` (real
`DamageBonus*` rows, columns SMALL/MEDIUM/LARGE/FORT/NORMAL/HERO/DIVINE/NONE):

```
DamageBonusNormal=1.00,1.50,1.00,0.70,1.00,1.00,0.05,1.00
DamageBonusPierce=2.00,0.75,1.00,0.35,1.00,0.50,0.05,1.50
DamageBonusSiege=1.00,0.50,1.00,1.50,1.00,0.50,0.05,1.50
DamageBonusMagic=1.25,0.75,2.00,0.35,1.00,0.50,0.05,1.00
DamageBonusChaos=1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00
DamageBonusHero=1.00,1.00,1.00,0.50,1.00,1.00,0.05,1.00
```

Note: the wiki page for the attack/armor matrix was behind a Cloudflare
"Verify you are human" wall on the second Liquipedia request; no bypass was
attempted. The local game data is the authoritative source and was used
instead (`observed`, complete).

## 2. Unit, weapon and ability parameters (primary game data)

**Observed** — `analysis/war3_full/Units/UnitBalance.slk`,
`UnitWeapons.slk`, `UnitData.slk`, `UnitAbilities.slk` (parsed by
`analysis/agent-sim/sylk.py`):

- hfoo: HP 420, armor 2, speed 270, sight 1400, gold 160, food 2, bldtm 20 s,
  defType `medium`, weapon `normal` 11+1d2, cooldown 1.35 s, range 90,
  acquire 500.
- hrif: HP 520, armor 0, speed 270, sight 1400, gold 240, food 3, bldtm 26 s,
  defType `small`, weapon `pierce` 16+2d4, cooldown 1.6 s, range 400.
- hpea: HP 220, speed 190, sight 800, gold 90, food 1, bldtm 15 s, range 90.
- htow: HP 1500, armor 5, defType `fort`, sight 900, gold 450, food provided
  12, bldtm 180 s.
- hkni: HP 800, armor 6, defType `large`, 19+2d5, 1.36 s.
- hmpr: HP 290…, mana 200, regen 0.5/s, Heal `Ahea` cost 5, cooldown 1 s,
  range 250, Data11 = 25.
- Hpal: max mana `realM` 255, regen 0.01/s, `AHhb` Holy Light cost 75,
  cooldown 5 s, range 800, Data11 = 200; `AHds` Divine Shield cost 125,
  cooldown 75 s, duration 10 s.
- Hamg: mana 285, `AHbz` Blizzard cost 75, cooldown 6 s, area 200, range 800,
  Data11 = 6 waves, Data12 = 30 damage, Data14 = 0.5 s.
- Ekee: mana 270, `AEer` Entangling Roots cost 75, cooldown 8 s, range 800,
  Data11 = 15.
- Ofar: mana 285, `AOcl` Chain Lightning cost 120, cooldown 9 s, range 700,
  Data11 = 100, Data12 = 4 jumps, Data13 = 0.15 falloff.

Hero max mana lives in `realM`, not `manaN` (first generator pass produced 0
mana and the ability tests caught it; `observed`, fixed in
`gen_roster.py`). All of these values are emitted into
`crates/wc3-sim/src/roster_gen.rs` by `analysis/agent-sim/gen_roster.py`.

**Observed** — `analysis/list-War3-clean.txt`: 2048 model paths. The generator
matches each of the 98 curated roster entries against this listing; 89 model
paths exist in the local RoC archive. Nine TFT-only models (`hkee`, `hcas`,
`okod`, `ostr`, `ofrt`, `unp1`, `unp2`, `etoa`, `etoe`) are emitted with their
canonical path and `model_present = false`; they are not faked as present.

## 3. Pathfinding and movement (donor mechanisms)

**Observed** — `donors/W3AndroidEngine/core/src/com/etheller/warsmash/viewer5/handlers/w3x/simulation/pathing/CPathfindingProcessor.java`:

- `:50-51` `NEAREST_PATHABLE_STEP = 32f`, `NEAREST_PATHABLE_RADIUS = 1024f` —
  an order landing on blocked ground stops at the nearest pathable point
  searched in rings.
- `:52` `MIN_SEARCH_ITERATIONS = 4096`; `:59` `SHORT_SEARCH_ITERATIONS = 16` —
  searches are incremental and **bounded per update**, not run to completion.
- `:60-61` `SLIDE_STEP = 4f`, `SLIDE_STEPS = 64` — units slide along
  obstacles when the direct line is blocked.
- `:63` `DETOUR_ITERATION_FACTOR = 16` — detour searches get a bounded budget.

Adopted in `nav.rs`: bounded A* (`MAX_ASTAR_NODES = 20_000`, reused
generation-stamped scratch arrays), `nearest_walkable` ring search, an
8-per-tick path-request budget, and slide-by-axis in `advance_movement`.
Stuck handling retries a bounded number of times and then releases the unit
instead of spinning (tests `unreachable_goal_terminates_without_hanging`,
`unit_escapes_a_concave_pocket`, `many_units_do_not_stick_at_a_narrow_gate`).

**Observed** — `donors/OpenRA/OpenRA.Mods.Common/Pathfinder/PathSearch.cs:30`
("Maintain a pool of layers used for paths searches … to avoid the high cost
of initializing a new search space") and `:72` (`heuristicWeightPercentage`).
Adopted: reused search scratch in `nav::PathFinder` and a cached flow field
(`Sim::flow_cache`, 240-tick TTL, max 16 entries) for group moves.

## 4. Mobile RTS pacing (secondary source, reported-unverified)

**Observed** — https://waywardstrategy.com/2018/11/14/why-cc-rivals-is-really-fun/
(accessed 2026-11-12, stealth-browser, full article text):

> "The game length of RTS has remained mostly stagnant over the decades, with
> an average game length of about 15 minutes."
> "Rivals as a mobile game needed to have short game lengths of 3-4 minutes to
> make it accessible for on-the-go play sessions such as commuting."
> "no macro cycle, no building supply depots or rallying workers."

**Claim (reported-unverified):** mobile RTS matches should be shorter than the
desktop 15-minute default and favour quick, readable engagements.

**Inference for this slice:** keep WC3-accurate stats and tech but target a
single-base skirmish: one town hall provides 12 food (WC3 value), max supply
100, and the AI fields waves of 4-8 units rather than 50-unit armies. This
also keeps the 60 Hz budget on a phone comfortable (400 units measured at
0.427 ms/tick, see REPORT.md).

## 5. What was not adopted and why

- `CPathfindingProcessor`'s asynchronous shared-graph node pool is a
  multi-tick search scheduler; this sim needs deterministic same-tick path
  assignment for replays, so it uses a per-tick request budget instead
  (`inferred` design decision; deterministic order is testable and tested).
- OpenRA's hierarchical path graph is overkill for our 64×64/128×128 maps;
  A* with a node cap and cached flow fields is enough (`inferred`, measured
  in the perf gate).
- No donor source code was executed; mechanisms were re-implemented in Rust
  with the constants above.

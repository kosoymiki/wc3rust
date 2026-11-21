# WC3 ESSENCE — what Warcraft III *is*, mechanically

Technical reference for the port. Every number below is either read from the
operator's install (`/data/data/org.herofolends.p0/files/*.mpq`, extracted to
`analysis/war3x_data/`), taken from the decompiled script corpus
(`analysis/agent-re/phase0/`), or marked **inferred** where the engine constant
is not in the data tables. The goal is one page that answers "what must the port
do to be WC3" without article prose.

Sources (observed, this install):

| Layer | Evidence |
|---|---|
| Archives | `War3.mpq`, `War3x.mpq`, `War3xlocal.mpq` — 7,451 named files / 543,237,719 B, 0 read failures (`wc3_re::manifest`) |
| Object data | 812 units, 812 weapons, 763 abilities, 273 items, 79 misc tables (`wc3_re::gameplay`) |
| Maps | 44 maps, 19,420 script functions, 4,560 triggers, 9,981 cinematic calls, 1,313 transmissions, 999 cameras, 4,603 regions, 1,763 sounds, 4,428 WTS strings (`wc3_re::cutscenes`) |
| Constants | `Units/MiscGame.txt` (`analysis/war3x_local/Melee_V0/Units/MiscGame.txt`), `Units/MiscData.txt`, `Units/UnitBalance.slk`, `Units/UnitWeapons.slk`, `Units/AbilityData.slk`, `UpgradeData.slk` |

---

## 1. The loop

WC3 is a **deterministic, fixed-step lockstep simulation**, not a physics game.
The port runs `TICKS_PER_SECOND = 30` (`crates/wc3-sim/src/lib.rs:44`); WC3's own
logic tick is ~30 Hz (**inferred** — not exposed in the data tables; the value is
consistent with unit durations like a 1.5 s attack cooldown landing on 45 ticks).

Per tick: advance cooldowns → acquire/validate targets → update orders
(move/attack/patrol/harvest/build/cast) → move with turn rate and collision →
resolve attacks and projectiles → production/research → economy payouts →
fog/visibility → victory check. All randomness is seeded; `Sim::state_hash`
(`crates/wc3-sim/src/lib.rs`) makes divergence a test failure
(`crates/wc3-sim/tests/determinism.rs`).

**Consequence for the port:** order handling, damage, and economy must never
read wall-clock time or unordered maps. This is why mechanics are tested in the
sim crate, not in the renderer.

## 2. Economy

- **Gold.** A mine holds **12,500** gold (`GOLD_MINE_GOLD`, `lib.rs:60`). A worker
  walks to the mine, channels for its harvest duration, carries a load, returns,
  and deposits. Two resources, two return trips — gold from the mine, lumber from
  destructable trees.
- **Lumber.** Trees are destructables; harvesting removes them and pays lumber per
  trip. Wood is the expansion-limiting resource: tech and towers cost lumber that
  gold cannot substitute.
- **Supply (food).** Units cost food and buildings provide it. The hard ceiling is
  **100** (`lib.rs:77`, "matching WC3's 100 supply cap"). Production that would
  exceed it is *blocked*, not cancelled — it resumes when supply frees
  (`Production.blocked`, `crates/wc3-sim/tests/production.rs`).
- **Upkeep.** This is the mechanic that defines WC3's mid-game: above **50** food
  income is taxed (low upkeep), above **80** it is taxed harder (high upkeep), and
  the tax is shown in the HUD. The constants live in `MiscGame.txt`; the port
  currently has the HUD label only — **this is a real gap** (see
  `MECHANICS_GAP_MAP.md` row `economy.upkeep`). Without upkeep, a 100-supply army
  is economically free and the classic tech-vs-army tension disappears.

## 3. Tech tree

Three tiers per race, unlocked by the main hall upgrade; every building and unit
declares `Requires=` in `UnitData.slk`, and every producer declares `Trains=`.
The port gates production on those two fields (`enqueue_production` →
`def.requires` / `trained_at`), which is why the roster is data-driven rather than
hardcoded (`crates/wc3-sim/tests/roster.rs`, `production.rs`).

A worker builds by channelling a construction site; the building has an
under-construction state with partial HP and no function until complete
(`construct`, `build_instant`, `OrderSpec::Build`). Buildings occupy pathing
cells while alive and free them on death (`spawn_def_index_at`,
`apply_damage`) — this is what makes bases physically defensible.

## 4. Combat math

Damage is **integer permille** math, not floats
(`crates/wc3-sim/src/data.rs:142-205`):

1. `DAMAGE_MULT_PERMILLE[attack_type][armor_type]` — a 24-cell table (normal,
   pierce, siege, magic, chaos vs light, medium, heavy, fortified, hero, unarmored).
2. Armor reduction: **60 permille per armor point** (`DEFENSE_ARMOR_PERMILLE`),
   applied with diminishing returns; negative armor uses its own table
   (`NEG_ARMOR_MULT_PERMILLE`).
3. `typed_damage()` is the only path damage takes (`apply_damage`,
   `lib.rs:2168`), so a balance table change propagates to every attack.

Attack cadence is a per-weapon cooldown in ticks with a damage point; the port
schedules it at `lib.rs:1800` and covers it in `tests/combat.rs` (10 tests).
Projectiles carry speed and homing; splash and arcs are partial.

**Why permille:** WC3's UI numbers and replay determinism both depend on exact
integer results. Float damage drifts across platforms.

## 5. Movement, pathing, collision

- **Speed clamp:** `MinUnitSpeed=10`, `MaxUnitSpeed=400`, `MinBldgSpeed=10`,
  `MaxBldgSpeed=400` (`MiscGame.txt:32-36`). Frost effects halve movement and
  quarter attack speed (`FrostMoveSpeedDecrease=0.5`,
  `FrostAttackSpeedDecrease=0.25`) — values the sim consumes through
  `GameplayConfig`.
- **Turn rate:** units rotate toward their heading with a per-unit turn rate and
  a floor; a unit cannot strafe. This is why WC3 armies feel heavy and why
  chokepoints work.
- **Pathing:** the map ships a `war3map.wpm` grid at **32 world units per cell**
  (512×512 for a 16,384-unit map). The port applies it at native resolution
  (`sim_bridge::apply_assets_pathing` → `Sim::set_pathing`,
  `PATHING_CELL_SIZE=32`) instead of downsampling, then layers the terrain water
  mask on top (`mask_water`) — deep water is not walkable for ground units.
- **Collision:** `UnitBalance.slk` `collision` converts to footprint cells
  (`footprint_for`); `can_stand` samples the centre plus four offsets, and units
  push apart instead of stacking.
- **Patrol** walks between two points and engages on both legs
  (`OrderSpec::Patrol`, `Order::Patrol`); **hold** stops and keeps attacking in
  range; **attack-move** engages anything met on the way.

## 6. Buildings: spawn, exit, rally

A building's trained unit appears at a **building exit**, not at its centre. The
port's `exit_position` (`crates/wc3-sim/src/lib.rs`) scans 24 directions at a
radius that clears the pathing footprint plus 0.75 cell (model rim), then expands
in half-cell rings, and finally snaps to the nearest walkable cell. This is the
fix for units appearing inside the town-hall model. Rally points
(`Unit.rally`) send the new unit to a chosen world point after it exits; the
default rally is the exit itself.

## 7. Heroes

- **XP:** `NeedHeroXP = [200, 500, 900, 1400, 2000, 2700, 3500, 4400, 5400, 5400]`
  and `GrantHeroXP = [80, 100, 120, 140, 160, 200, 240, 280, 320, 400]`
  (`MiscData.txt:108-109`, `lib.rs:46-48`). Shared XP range is **1200**
  (`HeroExpRange`).
- **Attributes:** strength/agility/intelligence drive HP, armor, damage and mana;
  the primary attribute adds damage. Agility→armor is tracked in milli-units to
  stay integer-exact (`agi_armor_milli`).
- **Levels:** 1–10, one skill point per level, ultimate at 6 (**inferred** from
  the standard ability level requirements — the exact requirement rows are in
  `AbilityData.slk` and are not yet fully consumed).
- **Revive:** `HeroMaxReviveCostGold=700`, `HeroMaxReviveTime=150`
  (`MiscGame.txt`) — **not implemented** in the port (gap row
  `heroes.revive`).

Hero-centric play is the core of WC3's identity: a level-5 hero with two items
beats a larger unheroic army. Any port that treats heroes as strong units misses
the design.

## 8. Creeps

Neutral camps guard resources and chokepoints. A camp has an **anchor**, an
aggro radius (600 in the port's melee opening), and a leash; `GuardReturnTime=5.0`
(`MiscGame.txt:120`) is how long a creep chases before turning home, and
`MaxGuardDistance` bounds the chase. Killing creeps gives XP and gold and can
drop items — "creep jacking" (pulling a camp while it is being fought) is a
legitimate tactic, so leash and return must be exact.

The melee opening now fields **five creeps per side** around each gold mine,
registered as one camp (`spawn_melee_camp` → `register_camp`), with a test
(`tests/melee_spawn.rs`).

## 9. Abilities

763 ability rows in `AbilityData.slk`; each has targeting (none/unit/point),
mana cost, cooldown, levels, and an effect id. The port executes 13 hand-ported
spells (`SPELL_DEFS` in `crates/wc3-sim/src/abilities.rs`) with a generic
targeting pipeline (`Command::Cast`, `TargetKind`) and 9 tests. The remaining
~750 rows are decompiled and queryable but have no executable effect — the single
largest gap in the port.

## 10. Items

273 item rows: cost, charges, ability, drop tables. The port has the item table,
carrying, shops and buy/sell paths (`Item`, `Shop`, `ShopSlot`;
`tests/items_round9.rs`, `tests/projectiles_items.rs`); drop tables and restock
timing are missing.

## 11. Fog, vision, day/night

Per-player `visibility` and `explored` buffers sized to the pathing grid, with
circular reveal offsets precomputed by radius (`fog_offsets`) and per-unit sight
from `UnitBalance`. Day/night changes lighting
(`render.rs::day_night_light`) and, in WC3, night reduces unit sight — the port
has the visual cycle but not yet the sight modifier (partial row `daynight`).

## 12. UI: the 12-slot card and the letters

The command card is a 3×4 grid. The army card's first five commands are
**Attack A / Move M / Stop S / Hold H / Patrol P**; the worker card swaps Attack
for Gather G and puts build entries in the Patrol/ARMY slots. Control groups are
1–9 (`Ctrl+N` assign, `N` recall, double-tap centres). The port now derives the
letter drawn on the button and the letter the keyboard matches from one function
(`game::hotkey_label` / `hotkey_letter`), so the card and keys cannot drift —
this is the fix for "no normal hotkeys".

Selection: tap, box marquee, shift-add, control groups. Missing: double-click
type-select and per-object extended tooltips.

## 13. Audio

Positional SFX for attacks, deaths, spells and UI clicks, plus race music. The
port's mixer reports `underruns=0`, `backend_errors=0`, buffer 107–128 ms on
device (`GAME_AUDIO_STATS`), with 29 audio tests. Sound must be tied to the
simulation tick, not to render frames, or fast-forward breaks it.

## 14. Art (honest boundary)

MDX models (geosets, materials, sequences), BLP textures with mips, W3E terrain
tiles, cliffs and water. The port parses and renders all of these from the
operator's own files. **The redraw HUD pack is `procedural reference v1`, not
hand-drawn Blizzard-level art**, and it is labelled as such in code and reports.
A finished claim requires real hand-drawn source layers plus device evidence.

## 15. Maps and cutscenes

44 shipped maps, each with `war3map.j` (JASS), triggers, cameras, regions,
sounds and WTS strings: 19,420 functions / 4,560 triggers / 9,981 cinematic
calls / 999 cameras / 4,603 regions. The port decompiles and exposes them
(`wc3_re::cutscenes`, 3 tests) and runs a JASS bridge for unit/group natives, but
does not yet play cinematic sequences (`cutscenes.playback` is missing). Campaign
flow, quests and mission chaining are missing as a layer.

## 16. Design invariants the port must not break

1. **Heroes decide fights**, not unit count.
2. **Upkeep makes big armies expensive** — without it the economy has no ceiling.
3. **Creeps are contested resources** — leash and return must be exact.
4. **Terrain is a unit**: chokepoints, high ground, water and trees shape fights.
5. **Buildings are physical**: footprint, exit, rally, construction state.
6. **Every number is integer and deterministic**; replays must hash identically.
7. **The card is the interface**: A/M/S/H/P and control groups are muscle memory.

## 17. How this maps to code

`analysis/tech-lead/MECHANICS_GAP_MAP.md` (generated by `gen_gap_map.py`) lists
53 mechanics with source of truth, implementation `file:line`, the test that
covers it, and a status. Current totals: **26 PORTED, 18 PARTIAL, 1 DATA-ONLY,
8 MISSING** (2026-11-13). A mechanic without a named test is PARTIAL by
definition, so the table cannot overstate coverage.

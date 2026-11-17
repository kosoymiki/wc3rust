# WC3 SIM — combat depth (ledger 401)

Layer: SIM / combat. Owner path: `crates/wc3-sim/**`. Measured 2026-11-15.

Goal: combat must read like Warcraft 3 — the shipped attack/armour matrix,
per-hit dice rolls, targetability, bounty, splash falloff and projectile
impacts — instead of a flat "damage minus armour" exchange.

## What was added

| Piece | Where | Source |
|---|---|---|
| Attack/armour matrix (7 attack rows × 8 armour classes) | `src/data.rs:150` `DAMAGE_MULT_PERMILLE` | `analysis/war3x_local/Units/MiscGame.txt:106-112` |
| Armour formula `100/(100+6a)`, negative armour `2-0.94^|a|` | `src/data.rs:181` `armor_reduction` | `Units/MiscData.txt` `DefenseArmor=0.06` |
| Table applied to every hit | `src/lib.rs:3045` `apply_damage` → `data::typed_damage` | — |
| Unit target classes, weapon target masks, splash profiles | `src/combat_gen.rs` (526 profiles, binary search by id) | `Units/UnitWeapons.slk` `targs1/targs2/Farea1/Harea1/Qarea1/Hfact1/Qfact1/splashTargs1/targCount1`, `Units/UnitData.slk` `targType` |
| Profile generator | `tools/gen_combat.py`, `tools/slkread.py` | regenerate with `python3 crates/wc3-sim/tools/gen_combat.py` |
| Damage roll `dmgplus + dice d sides` (+ researched damage upgrades) | `src/lib.rs:2815` → `combat::roll_weapon_damage` | `Units/UnitWeapons.slk` `dmgplus1/dice1/sides1` |
| Ground↔air miss policy 5% / 10% | `src/combat.rs` `miss_chance_permille`, applied `src/lib.rs:2819` | simulation policy (see "Open items") |
| Weapon targetability (no ground unit chasing air) | `src/lib.rs:2770` `can_attack`, used by acquisition and `attack_if_in_range` | `Units/UnitWeapons.slk` `targsN` × `Units/UnitData.slk` `targType` |
| Splash falloff (full/half/quarter radii + factors, `enemy` mask, friendly fire) | `src/lib.rs:2858` `resolve_weapon_hit` → `combat::splash_factor` | `Units/UnitWeapons.slk` splash columns |
| Splash on projectile impact (mortar shells, ballistae, wyrms) | `src/lib.rs:3024` (`advance_projectiles` → `resolve_weapon_hit`) | — |
| Kill bounty (`bountyplus + bountydice d bountysides`), paid once | `src/lib.rs` `grant_creep_bounty` + `src/bounty_gen.rs` (ledger 393 table) | `Units/UnitBalance.slk` |
| Kill XP to heroes in `HeroExpRange` | `src/lib.rs` `apply_damage` death branch | `Units/MiscGame.txt` `GrantHeroXP` |

### Defect found and fixed in the shipped table

`MiscGame.txt` ships `DamageBonusPierce=2.00,0.75,1.00,0.35,1.00,0.50,0.05,1.50`
and `DamageBonusSiege=1.00,0.50,1.00,1.50,1.00,0.50,0.05,1.50`. The previous
`data.rs` table carried `1.50` in the NORMAL-armour column for both rows, i.e.
pierce and siege over-damaged unarmoured units by 50%. Fixed to the shipped
`1.00`; `combat_attack_armor_table_matches_shipped_matrix` now pins every cell of
the matrix verbatim.

### Behaviour that changed with the table

* Auto-attacks now use `unit.damage` (base `dmgplus1` **plus researched smith
  upgrades**) instead of `def.stats.damage`; upgrade damage was silently
  dropped before.
* A ground-only weapon (footman, grunt, tauren) no longer acquires, chases or
  damages an air unit; `targs1|targs2` decides (the union lets dual-weapon units
  such as the mortar team hit structures with their second weapon).
* Siege/splash weapons now hit every unit inside `Qarea1` with the shipped
  falloff, including allies when `splashTargs1` has no `enemy` bit (WC3 mortar
  friendly fire), and excluding allies when it does (`hmtm` vs `ufro`/`esen`).

## Measured numbers (this device, 2026-11-15)

`CARGO_BUILD_JOBS=2 cargo test -p wc3-sim --offline --test combat_depth`

| Fight | Roll | Table result | Measured on Justice |
|---|---|---|---|
| footman `hfoo` → grunt `ogru` | 11+1d2 = 12..13 | normal × medium 1.5, armour 1 → **16..17** | 16..17 over 22 hits |
| rifleman `hrif` → tauren `otau` | 16+2d4 = 18..24 | pierce × heavy 1.0, armour 3 → **15..20** | 15..20 over 19 hits |
| mortar `hmtm` → guard tower `hgtw` | 51+1d13 = 52..64 | siege × fortified 1.5, armour 5 → **60..73** | 60..73 over 9 hits |
| footman vs grunt with +2 armour (rating 4) | 12..13 | normal × medium, armour 4 → **14..15** | 14..15 over 30 hits |

Splash (mortar vs towers, `Farea1=25`, `Harea1=150` at `Hfact1=0.4`,
`Qarea1=250` at `Qfact1=0.1`):

| Distance from impact | Expected | Measured |
|---|---|---|
| 0 (primary) | 60..73 | 71 |
| 128 (inside `Harea1`) | 23..28 | 27 |
| 128, allied footman (friendly fire) | 8..10 | 10 |
| 224 (inside `Qarea1`) | 5..6 | 6 |
| 672 (outside) | 0 | 0 |

Miss policy, 500 held shots each (`attack_ticks=1`, seeded):
ground→air **23/499 = 4.6%** (policy 5%), air→ground **55/495 = 11.1%**
(policy 10%). A footman dealt **0** damage to a gryphon over 400 ticks.

Bounty: forest troll `nftr` = 5+1d3 = 6..8 gold, paid **once** (a second lethal
strike on the corpse pays nothing and emits no event); the level-2 kill granted
`GrantHeroXP[1]` to the hero in range. Neutral-on-neutral kills pay no player.

10-unit skirmish on the Justice grid: 5 footmen vs 5 grunts, **786 ticks**,
185 damage events, orcs 5/5 alive, humans 0/5 — resolves inside the 4000-tick
budget.

## Open items / cross-layer needs

1. **Armour upgrade amount is not in the shipped data.** Every `rarm` row
   (`Rhar`, `Ruar`, `Rhac`, ...) carries `base1='-'`, `mod1='-'` in
   `analysis/war3_full`, `war3_data` **and** `war3x_data` `UpgradeData.slk`; the
   localized `*UpgradeStrings.txt` tooltip says only "повышение защиты" with no
   number. `upgrades_gen.rs` therefore emits `UpgradeEffect { kind: Armor,
   base: 0, step: 0 }` and researching Iron Plating currently adds **0 armour**.
   The per-level amount is engine-hardcoded, so it must be supplied by the
   upgrades layer (owner: upgrades ledger) — this layer proves the damage path
   responds to armour rating but cannot invent the constant.
   Next step: recover the constant from the `rarm` handler in `Game.dll`
   (`analysis/game_files/Files100_Game.dll`, r2) or a verified community
   reference, then fill `base/step` in the upgrade table.
2. **PvP bounty decision.** `grant_creep_bounty` (`src/lib.rs`) deliberately
   pays only neutral victims "so the melee player-vs-player economy is
   untouched" (ledger 393 wording). WC3 pays bounty for enemy player units too.
   Enabling it changes AI income, so it belongs to the economy owner.
3. **HUD**: `Event::Bounty { owner, unit, gold }` is not surfaced by
   `wc3-game` (no floating gold text); read-only here.
4. **`Spells` vs armour.** WC3 spell damage bypasses the armour *rating*; the
   sim still routes `AttackType::Magic` through `armor_reduction`. The matrix row
   `Spells` exists; switching spell casts to it is the next combat-depth step.
5. **Mechanic map / runner**: `analysis/tech-lead/MECHANICS_GAP_MAP.md` has
   `combat.damage-table`, `combat.attack-speed`, `combat.projectiles`; the new
   tests are named `combat.attack-armor-table`, `combat.damage-roll`,
   `combat.bounty-xp`, `combat.splash-falloff`, `combat.justice-1v1-table`
   (`crates/wc3-sim/tests/combat_depth.rs`). Gap-map rows and a
   `run_mechanic_tests.sh` entry are read-only here.
6. **Concurrent edit**: another agent edited `crates/wc3-sim/src/lib.rs` (the
   `grant_creep_bounty`/`bounty_gen.rs` work) while this layer held the file.
   The two changes are merged (this layer consumes the economy bounty table and
   event instead of duplicating them), but the ledger should serialize owner
   paths for the next round.

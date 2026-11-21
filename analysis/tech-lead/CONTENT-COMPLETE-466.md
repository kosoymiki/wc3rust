# LEDGER 466 — content completeness (all four races)

Date: 2026-11-17 (MSK). Repo: `/data/data/com.termux/files/home/src/herofolends-rs`.
Cargo: `scripts/h2-cargo` (single-slot, offline).

## Goal

Every building reachable and in the worker build menu, every researched upgrade
carries a working effect or an explicit data-only reason, every building has a
real model, and the Lumber Mill / War Mill / Hunter's Hall research menus reach
unit stats. Measured, not claimed.

## Before (census at task start)

```
RACE Human    buildings=14 buildable=14 model_present=12 menu=15 entries
RACE Orc      buildings=11 buildable=11 model_present=9  menu=12 entries
RACE Undead   buildings=13 buildable=12 model_present=9  menu=13 entries
RACE NightElf buildings=12 buildable=12 model_present=10 menu=12 entries
buildings total=50 · upgrades=40 printed · 16 printed rows with ZERO effects
non-buildable: uzg1 Ziggurat Upgrade (req ugrv)
```

Root causes measured, not guessed:
- `uzg1` had `buildable=false` and no upgrade path in the roster even though
  `analysis/war3_full/Units/UndeadUnitFunc.txt:195` ships `[uzig] Upgrade=uzg1`.
- The build menu and the building scan used different sets: the scan read
  `ROSTER` only, the menu appended `SHOP_DEFS` and dropped zero-cost `egol`;
  hall tiers were printed as menu cards instead of upgrade paths.
- 33 four-race upgrade rows had `effects=[]` because their effect is an
  ability grant (`Adef`, `Acmg`, `AInv`, ...), which `gen_upgrades.py` did not
  model. 15 further rows mapped an unmapped effect code to `Other`, a no-op.
- 10 buildings pointed at MDX paths the archive does not ship. The hall lines
  reuse one model with `Upgrade First`/`Upgrade Second` sequences; the melee
  shops had `model: ""` hard-coded in `gen_shops.py`.

## After (census, full output in `analysis/agent-sim/content-census-2026-11-17.txt`)

```
RACE Human    buildings=15 (roster 14) buildable=15 worker_buildable=11 menu=11 model_present=15/15 roster_models=14/14
  menu_check missing=[] extra=[] ok=true
  reach: unreachable=[] upgrade_paths=[hcas<-hkee hctw<-hwtw hgtw<-hwtw hkee<-htow] ability_placed=[]
RACE Orc      buildings=12 (roster 11) buildable=12 worker_buildable=10 menu=10 model_present=12/12 roster_models=11/11
  menu_check missing=[] extra=[] ok=true
  reach: unreachable=[] upgrade_paths=[ofrt<-ostr ostr<-ogre] ability_placed=[]
RACE Undead   buildings=14 (roster 13) buildable=13 worker_buildable=11 menu=11 model_present=14/14 roster_models=13/13
  menu_check missing=[] extra=[] ok=true
  reach: unreachable=[] upgrade_paths=[unp1<-unpl unp2<-unp1 uzg1<-uzig] ability_placed=[]
RACE NightElf buildings=13 (roster 12) buildable=13 worker_buildable=10 menu=10 model_present=13/13 roster_models=12/12
  menu_check missing=[] extra=[] ok=true
  reach: unreachable=[] upgrade_paths=[etoa<-etol etoe<-etoa] ability_placed=[egol<-Aent]
UPGRADES all=89 zero_effect=0 applied_effects=112 data_only_effects=16 unlock_ability_grants=34
TOTALS roster_buildings=50 roster_models=50/50 all_buildings=54 all_models=54/54
```

Build menu == worker-placeable set for all four races (0 missing, 0 extra).
Every non-menu building is reachable by an explicit `Upgrade=` path or a
placement ability; `unreachable=[]` for every race.

## The effect-less upgrade table (census list → now)

| upgrade | name | before | after |
|---|---|---|---|
| Rhde | human footman defend | effects=0 | `UnlockAbility(Adef)` |
| Rhse | human magical sentinel | effects=0 | `UnlockAbility(Adts)` |
| Rhfl | human flare | effects=0 | `UnlockAbility(Afla)` |
| Rhss | human control magic | effects=0 | `UnlockAbility(Acmg)` |
| Rhpm | human backpack | effects=0 | `UnlockAbility(AInv)` |
| Rhfc | human flak cannon | effects=0 | `UnlockAbility(Aflk)` |
| Rhfs | human frag shards | effects=0 | `UnlockAbility(Afsh)` |
| Rhcd | human cloud research | effects=0 | `UnlockAbility(Aclf)` |
| Ropg | orc pillage | effects=0 | `UnlockAbility(Asal)` |
| Rows | orc tauren smash | effects=0 | `UnlockAbility(Awar)` |
| Roen | orc raider ensnare | effects=0 | `UnlockAbility(Aens)` |
| Rovs | orc wyvern venom spear | effects=0 | `UnlockAbility(Aven)` |
| Rolf | orc liquid fire | effects=0 | `UnlockAbility(Aliq)` |
| Ropm | orc backpack | effects=0 | `UnlockAbility(AInv)` |
| Robf | orc naptha | effects=0 | `DataOnly` (no ability id in AbilityData.slk) |
| Ruac | undead ghoul cannibalize | effects=0 | `UnlockAbility(Acan)` |
| Ruwb | undead fiend web | effects=0 | `UnlockAbility(Aweb)` |
| Rusf | undead gargoyle stone | effects=0 | `UnlockAbility(Astn)` |
| Rufb | undead frost wyrm breath | effects=0 | `UnlockAbility(Afrb)` |
| Rupc | undead plague cloud | effects=0 | `UnlockAbility(Aap1)` |
| Rubu | undead burrowing | effects=0 | `UnlockAbility(Abur)` |
| Rusp | undead avenger transform | effects=0 | `UnlockAbility(Aave)` |
| Ruex | undead exhume corpses | effects=0 | `UnlockAbility(Aexh)` |
| Rupm | undead backpack | effects=0 | `UnlockAbility(AInv)` |
| Reuv | nightelf ultravision | effects=0 | `UnlockAbility(Ault)` |
| Resc | nightelf scout | effects=0 | `UnlockAbility(Aesn)` |
| Resi | nightelf abolish magic | effects=0 | `UnlockAbility(Aadm)` |
| Reht | nightelf hippogryph taming | effects=0 | `UnlockAbility(Acoh)` |
| Rers | nightelf resistant skin | effects=0 | `UnlockAbility(Arsk)` |
| Rehs | nightelf hardened skin | effects=0 | `UnlockAbility(Assk)` |
| Reeb | nightelf enchanted bears | effects=0 | `UnlockAbility(Aroa)` |
| Reec | nightelf enchanted crows | effects=0 | `UnlockAbility(Afae)` |
| Repm | nightelf backpack | effects=0 | `UnlockAbility(AInv)` |
| Rnen | naga myrmidon ensnare | effects=0 | `UnlockAbility(Aens)` |
| Rnsi | naga abolish magic | effects=0 | `UnlockAbility(Aadm)` |

`zero_effect=0`. 34 `UnlockAbility` effects. The grant is wired at runtime in
`Sim::unit_ability_unlocked`: a statically listed ability is gated by its
research, and an ability the unit does not list statically becomes available
once the granting research is complete for the owner (e.g. `Rhfc` -> `Aflk` on
the gyrocopter). `Sim::cast_data_ability` refuses locked abilities
(`ability not researched`), so a grant is not cosmetic.

Remaining `DataOnly` effects (16 effect rows, 8 distinct four-race upgrades:
Rhgb, Rhhb, Rhrt, Robk, Robf, Rusl, Rusm, Recb, Remg, Repb) are engine
weapon/summon flags the sim has no model for; every row prints its reason in
the census. Nothing is silently a no-op.

Stat codes that used to collapse to `Other` now apply for real:
`ratd` damage, `rarm` armor (per-unit `defUp` = 2), `rhpx` HP, `rmvx` speed,
`ratr` range, `rmnx` mana, `rmnr` mana regen, `rlum` lumber per trip,
`rspi` spiked-barricade return damage, `rhpo` HP%, `rhpr` HP regen,
`raud` damage%, `rats` attack speed, `ratx` flat damage, `rart` armor class.

Research now applies only to units whose `upgrades` column lists it
(`UnitBalance.slk`, RoC ∪ TFT) instead of every unit of the race, and it
applies to buildings too (`Rhac` architecture reaches town halls).

## Research reaches stats (test output)

`content::research::applies_to_stats` (footman, blacksmith, real 60 s research):

- footman armor `2` → `4` after `Rhar` (per-unit `defUp=2`);
- `typed_damage(Normal, Medium, armor, 30)` drops `26 → 24`;
- `Event::UpgradeResearched { id: "Rhar" }` emitted, gold charged;
- `Rhme` raises the same footman's `damage` by 1.

`content::lumber_mill::research_menu`: `hlum=[Rhac Rhlh]`, `ofor=[Rome Roar
Rora Rosp Rorb]` (5), `edob=[Resm Rema Resw Rerh Reuv Rews]` (6); every id
resolves to a real upgrade with a non-empty effect and reaches at least one
unit (or is an explicit grant). `Rhlh` is `LumberHarvest +5` on `hpea`;
`Rhac` is `affected_by(htow)`.

## Tests (all green)

```
content::buildings::all_reachable     ok
content::upgrades::all_have_effects   ok
content::research::applies_to_stats   ok
content::models::all_present          ok
content::lumber_mill::research_menu   ok
content::census::prints_table         ok
```

Command: `scripts/h2-cargo test -p wc3-sim --test content --offline` → rc=0,
5 passed (plus the census printer).

## Files changed

- `crates/wc3-sim/src/data.rs` — `UpgradeEffectKind` (damage%, HP%, regen,
  lumber, sight, attack speed, armor class, unlock, data-only), effect
  `ability`/`note`, `UpgradeBonus`, `building_defs`, `worker_buildable`,
  `upgrade_path`, `ability_unlock_upgrade`, `unit_upgrades`,
  `unit_armor_upgrade`, `is_worker_buildable`, `affected_by`.
- `crates/wc3-sim/src/lib.rs` — upgrades scoped by `UnitBalance.slk`
  `upgrades`, armor via `defUp`, percent/regen/lumber/sight/attack-speed/armor
  effects, spiked return damage, life-regen tick, ability gating.
- `crates/wc3-sim/src/buffs.rs` — attack interval includes `rats` upgrades.
- `crates/wc3-sim/src/save.rs` — format v5 appends `hp_milli`/`hp_regen_milli`.
- `crates/wc3-sim/src/roster_gen.rs` (generated) — `UNIT_UPGRADES`,
  `UNIT_UPGRADE_OF`, `UNIT_PLACED_BY_ABILITY`, `UNIT_ARMOR_UPGRADE`; TFT
  `UnitBalance` union for the `upgrades` column.
- `crates/wc3-sim/src/upgrades_gen.rs` (generated) — 89 rows, 0 empty effect
  lists, 34 ability grants, explicit data-only reasons.
- `crates/wc3-sim/src/shops_gen.rs` (generated) — real shop model paths.
- `analysis/agent-sim/gen_roster.py`, `gen_upgrades.py`, `gen_shops.py`.
- `crates/wc3-sim/tests/content.rs` — the five mechanic tests + census printer.
- `crates/wc3-game/examples/content_probe.rs` — extended census (models,
  menu equality, reachability, effect/data-only counts).
- `analysis/agent-sim/content-census-2026-11-17.txt` — saved full census.

## Risks / next

- `Rhss` (Control Magic) maps to `Acmg`, but its WC3 target unit (Spell Breaker
  `hspt`) is not in the curated 113-def roster, so the grant currently has no
  roster unit that lists `Rhss`. The upgrade data and the unlock are correct;
  adding `hspt` to the unit roster is a separate unit-content task.
- The 8 distinct four-race `DataOnly` upgrades are engine weapon/summon flags
  (splash radius, air/ground retarget, DoT breath, skeleton lifetime/level,
  glaive bounce). They are visible with reasons in the census but do not change
  combat numbers yet; modelling them is a combat-system expansion, not a data
  fix.
- Save format v5 is append-only; v1..v4 saves still decode (new fields restore
  to 0). The two `save_script` assertions that hardcoded v4 were updated to
  `SAVE_FORMAT_VERSION`.

## External observation

The updated `crates/wc3-game/examples/content_probe.rs` ran to completion
(`PROBE_RC=0`) and its 153-line output is the census saved at
`analysis/agent-sim/content-census-2026-11-17.txt`; the `UPGRADES`/`TOTALS`
lines above are taken from that run (four-race view: 102 applied effects, 13
data-only effect rows, 0 zero-effect).

After that run, an active edit in another lane (`wc3-re/src/jass/{natives,vm}.rs`,
modified 02:24-02:25) temporarily broke a transitive dependency
(`type_name_by_rawcode` / `RawCode` errors), so a later recompile of the sim
tests could not start. The five content tests and the sim regressions were
already run green against this exact data; the probe artifact is preserved.
No wc3-audio or wc3-re file was touched from this lane.

# Reforged 2.x balance diff (vs the local TFT roster baseline)

Applied at roster generation by `analysis/agent-sim/gen_roster.py`; the source
of truth is `analysis/balance/reforged-2.x.json`. The generator aborts when an
id/field does not exist or when the recorded `tft` value does not match the
generated baseline, and `balance.overlay.applies` re-checks the applied result
plus orphan-freedom in Rust.

## Sources (official Blizzard patch notes)

| ID | Patch | URL | Published | Accessed | Method |
|---|---|---|---|---|---|
| S1 | Warcraft III: Reforged — Version 2.0.2 | https://us.forums.blizzard.com/en/warcraft3/t/35355 | 2025-04-15 | 2026-11-16 | stealth-browser-mcp (spawn → navigate `.json` → execute_script → close) |
| S2 | Warcraft III: Reforged — Version 2.0.4 | https://us.forums.blizzard.com/en/warcraft3/t/36567 | 2026-01-26 | 2026-11-16 | stealth-browser-mcp (spawn → navigate `.json` → get_page_content → close) |

Verbatim excerpts: `analysis/balance/sources/reforged-2.0.2-2025-04-15.md` and
`analysis/balance/sources/reforged-2.0.4-2026-01-26.md`.
2.0.1 (https://us.forums.blizzard.com/en/warcraft3/t/33148, 2024-11-13) was
also retrieved and contains **no** balance section (engine/UI/lighting only).

## Applied rows

`TFT` is the value the generator emits from the local extract
(`analysis/war3_full` first, `analysis/war3x_data` fallback); `Reforged 2.x` is
the final value stated by the patch notes. Every row below is present in
`crates/wc3-sim/src/roster_gen.rs` and in the generated
`ROSTER_OVERLAY`/`ABILITY_OVERLAY`/`UPGRADE_OVERLAY` tables.

| Section | Id | Field | TFT | Reforged 2.x | Source |
|---|---|---|---:|---:|---|
| roster | `earc` | `stats.hp` | 260 | 275 | S2 |
| roster | `ehpr` | `stats.hp` | 780 | 800 | S2 |
| roster | `hmil` | `stats.armor` | 4 | 3 | S1 |
| roster | `esen` | `stats.speed` | 350.0 | 340.0 | S1 |
| roster | `Ekee` | `stats.hero_strength_gain` | 1.8 | 2.0 | S1 |
| roster | `ufro` | `stats.range` | 300.0 | 375.0 | S1 |
| roster | `ogre` | `stats.build_ticks` | 4500 | 4050 | S1 |
| abilities | `AHhb` | `cooldown_ticks` | 150 | 165 | S1 |
| abilities | `Ahea` | `range` | 250.0 | 350.0 | S1 |
| abilities | `Aslo` | `duration_ticks` | 1800 | 1350 | S1 |
| abilities | `Aivs` | `range` | 300.0 | 400.0 | S1 |
| abilities | `AUfn` | `range` | 700.0 | 750.0 | S2 |
| abilities | `AUcs` | `power_l2` | 125 | 135 | S1 |
| abilities | `Acri` | `mana` | 100 | 90 | S1 |
| abilities | `AUan` | `mana` | 250 | 150 | S1 |
| abilities | `AUan` | `duration_ticks` | 3600 | 1200 | S1 |
| abilities | `AUan` | `cooldown_ticks` | 7200 | 5400 | S1 |
| upgrades | `Rhde` | `gold_base` | 150 | 125 | S1 |
| upgrades | `Rhde` | `time_base` | 1350 | 1200 | S1 |
| upgrades | `Remg` | `gold_base` | 100 | 125 | S1 |
| upgrades | `Remg` | `time_base` | 1050 | 1500 | S2 |
| upgrades | `Reuv` | `time_base` | 1350 | 900 | S1 |
| upgrades | `Rubu` | `time_base` | 1350 | 900 | S1 |

Row notes:

- `earc.stats.hp`: patch-note baseline is 255, the local extract carries 260;
  the final 275 is applied and the discrepancy is recorded in the JSON.
- `ogre.stats.build_ticks`: patch-note baseline is 140 s, the local extract
  carries 150 s (4500 ticks); the final 135 s (4050 ticks) is applied.
- `AUfn.range`: 2.0.2 first reduced 800 → 700; the sim base already carries
  700, so 2.0.4's 750 is the applied value.
- `AUcs.power_l2`: level-2 damage only; the sim implements level 1 (75,
  unchanged), and the row is a data-table entry.
- `Aslo`/`Aivs` exist in the Sorceress `ability_ids` but have no implemented
  `SpellDef`; their rows are data-table entries (still id-validated).
- `Acri` (Necromancer Cripple) and `AUan` (Death Knight Animate Dead) likewise
  exist in their unit `ability_ids`; their rows are data-table entries. `AUan`
  carries the three numbers the note changed: mana, duration and cooldown.
- Upgrade rows are validated against `upgrades_gen.rs` (`gold_base`,
  `time_base`). The roster generator cannot rewrite `upgrades_gen.rs`
  (`gen_upgrades.py` is a different owner), so these rows are emitted as
  `UPGRADE_OVERLAY` for the upgrade layer and asserted by
  `balance.overlay.applies`.

## Coverage statement

The table covers every category the Reforged 2.0.2/2.0.4 notes changed for
four-race melee data: hero base growth (`Ekee`), ability mana/cooldown/range/
damage, unit hp/armor/speed/range/build time, and upgrade cost/time effects.
The two patches changed **no four-race melee unit's base damage**; the damage
category is represented by the `AUcs` level-2 ability row and the notes below.

## Not applied (no ROSTER def or no owner field)

- 2.0.4 changed Piercing-vs-Heavy damage from 100% to 90%: a combat-table
  constant (`data.rs`/damage matrix), not a per-def field; follow-up for the
  combat owner.
- Neutral heroes: Dark Ranger (Drain Life mana 50→25, friendly transfer), Tinker
  (Pocket Factory, Cluster Rockets), Brewmaster (Breath of Fire damage/AoE/DoT),
  Pit Lord (Cleave targets Wards). No ROSTER def.
- Creeps: Poison Treant hp 290→350, Spitting Spider 400→350 Heavy→Medium, Giant
  Spider 550→500, Satyr Hellcaller level 9→8, Satyr Shadow Dancer 200→190,
  Immolation 10→5, Entangle 10(3)→6(3)/range 800→600, Frost Nova slow 4→2,
  Unholy Frenzy 4→2, Brilliance Aura 1→0.5, Polymorph 225→200/25→20 s, Healing
  Wave 215→130/4→3 jumps, Cold Arrows 3(0.75)→3(1)/0.25→1, Diseased Cloud
  120→75, Animate Dead 120→40/250→150/240→180, Parasite 30→15, Blizzard/Rain of
  Fire cast time 1→0.599, Skink 200→100, Apprentice Wizard 180→200/+2 damage,
  Fallen Priest +2 damage, Lightning Lizard 280→300 Heavy→Medium, Salamander
  Hatchling −1 damage, Arachnathid Earth-borer 400→450, Murgul Snarecaster
  375→400, Skeletal Orc Champion ability swap, Infernal Machine Unholy Aura.
- Items: Sentry/Healing/Stasis ward numbers, Ring of Protection +4→+3, Orb of
  Fire→Orb of Slow, Orb of Lightning 150→200 vs summons, Orb of Venom 9→8,
  Ancient Janggo/Legion of Doom-Horn, Wand of Negation, Ritual Dagger, Backpack
  requirement.
- Melee units whose patch-note baseline contradicts the local extract in the
  opposite direction (applying the note's absolute final value would invert the
  change): Flying Machine speed (local 320 vs 400→375), Wind Rider speed (local
  330 vs 320→310), Gargoyle speed (local 270 vs 375→350), Chimaera speed (local
  220 vs 250→270), Tauren gold cost (local 325 vs 280→300). These need an
  extract-version decision before they can be applied without a lie.
- Ward-target rules (ground AoE, Pulverize, Burning Oil), Mountain King Bash vs
  magic-immune air, Hippogryph/Huntress glaive behaviour, Animate Dead ability
  retention, Gargoyle Prioritize, Wisp lumber interval 8→7→7.5 s: behavioural,
  no numeric def field.

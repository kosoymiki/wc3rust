# GAMEPLAY CONSTANTS — physics, balance, weapons, abilities, items

Generated (unix): 1794598721 · **ok=true**

## Sources
- `ability_data`: `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-re/../../analysis/war3x_data/Units/AbilityData.slk`
- `gameplay_constants`: `absent-in-this-install`
- `item_data`: `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-re/../../analysis/war3x_data/Units/ItemData.slk`
- `misc_game`: `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-re/../../analysis/war3x_local/Units/MiscGame.txt`
- `unit_balance`: `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-re/../../analysis/war3x_data/Units/UnitBalance.slk`
- `unit_weapons`: `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-re/../../analysis/war3x_data/Units/UnitWeapons.slk`

## Counts
| table | rows |
|---|---:|
| abilities | 763 |
| ability_columns | 68 |
| gameplay_constants | 0 |
| item_columns | 33 |
| items | 273 |
| misc_constants | 79 |
| unit_balance_columns | 57 |
| unit_weapons_columns | 79 |
| units | 812 |
| weapons | 812 |

## Field units
| field | unit |
|---|---|
| `Cast1` | seconds |
| `Cool1` | seconds |
| `DPS` | damage per second |
| `Dur1` | seconds |
| `HP` | hit points / hit points per second |
| `HeroDur1` | seconds |
| `Level` | item level |
| `avgdmg1` | damage points |
| `bldtm` | seconds |
| `collision` | map units |
| `cool1` | seconds |
| `def` | armor points |
| `defUp` | armor points |
| `dice1` | count |
| `dmgUp1` | damage points |
| `dmgplus1` | damage points |
| `goldcost` | resources |
| `level` | level |
| `lumbercost` | resources |
| `mana0` | mana points / mana points per second |
| `manaN` | mana points / mana points per second |
| `maxSpd` | map units per second |
| `maxdmg1` | damage points |
| `minSpd` | map units per second |
| `mindmg1` | damage points |
| `nsight` | map units |
| `rangeN1` | map units |
| `realHP` | hit points / hit points per second |
| `realM` | mana points / mana points per second |
| `realdef` | armor points |
| `regenHP` | hit points / hit points per second |
| `regenMana` | mana points / mana points per second |
| `reptm` | seconds |
| `sides1` | count |
| `sight` | map units |
| `spd` | map units per second |
| `stockMax` | count / seconds |
| `stockRegen` | count / seconds |
| `stockStart` | count / seconds |

## Formulas
| formula | expression | source |
|---|---|---|
| weapon_average_damage | `avg = dmgplus + dice * (sides + 1) / 2` | UnitWeapons.slk dmgplus1/dice1/sides1 (slot 2: 2) |
| weapon_dps | `dps = avg / cool` | UnitWeapons.slk cool1/avgdmg1/DPS (slot 2: cool2/avgdmg2/DPS2) |
| hero_hp_estimate | `maxHp(level) = realHP + (level - 1) * HP, only when HP != realHP (hero rows)` | UnitBalance.slk HP/realHP/level; hero signal observed on Hamg (450 + 4*100) vs hfoo/hpea (HP == realHP) |
| hero_xp_curve | `f(x) = A*f(x-1) + B*x + C` | MiscGame.txt NeedHeroXPFormulaA/B/C, GrantHeroXPFormulaA/B/C |
| hero_revive_cost | `goldRevivalCost = originalCost * (ReviveBaseFactor + ReviveLevelFactor*(level-1)) capped by ReviveMaxFactor` | MiscGame.txt revive formula comments |
| hero_revive_time | `revivalTime = originalTime * level * ReviveTimeFactor capped by ReviveMaxTimeFactor` | MiscGame.txt revive formula comments |
| frost_slow | `slowedSpeed = speed * (1 - FrostMoveSpeedDecrease)` | MiscGame.txt FrostMoveSpeedDecrease |

## Validation
| check | result |
|---|---|
| ok | true |
| weapon_avg_matches | 716/717 shipped avgdmg values equal the formula (tol 0.01) |
| weapon_dps_matches | 570/570 shipped DPS values equal avg/cool (tol 0.05) |

## MiscGame / GameplayConstants

| section | key | value | unit | formula |
|---|---|---|---|---|
| Misc | `GlobalExperience` | `1` | experience points |  |
| Misc | `MaxLevelHeroesDrainExp` | `1` | experience points |  |
| Misc | `BuildingKillsGiveExp` | `0` | experience points |  |
| Misc | `HeroMaxReviveCostGold` | `700` | resources |  |
| Misc | `HeroMaxReviveCostLumber` | `0` | resources |  |
| Misc | `HeroMaxReviveTime` | `150` | seconds |  |
| Misc | `HeroMaxAwakenCostGold` | `1400` | resources |  |
| Misc | `HeroMaxAwakenCostLumber` | `350` | resources |  |
| Misc | `HeroExpRange` | `1200` | map units |  |
| Misc | `ReviveBaseFactor` | `.40` | multiplier | goldRevivalCost = originalCost * (ReviveBaseFactor + ReviveLevelFactor*(level-1)), capped at originalCost * ReviveMaxFactor |
| Misc | `ReviveLevelFactor` | `.10` | multiplier | see ReviveBaseFactor |
| Misc | `ReviveBaseLumberFactor` | `0` | multiplier | lumberRevivalCost = originalCost * (ReviveBaseLumberFactor + ReviveLumberLevelFactor*(level-1)), capped at originalCost * ReviveMaxFactor |
| Misc | `ReviveLumberLevelFactor` | `0` | multiplier | lumberRevivalCost = originalCost * (ReviveBaseLumberFactor + ReviveLumberLevelFactor*(level-1)), capped at originalCost * ReviveMaxFactor |
| Misc | `ReviveMaxFactor` | `4.0` | multiplier |  |
| Misc | `ReviveTimeFactor` | `0.65` | seconds | revivalTime = originalTime * level * ReviveTimeFactor, capped at originalTime * ReviveMaxTimeFactor |
| Misc | `ReviveMaxTimeFactor` | `2.0` | seconds |  |
| Misc | `AwakenBaseFactor` | `.80` | multiplier | awakenCost = originalCost * (AwakenBaseFactor + AwakenLevelFactor*(level-1)), capped at originalCost * AwakenMaxFactor |
| Misc | `AwakenLevelFactor` | `.20` | multiplier |  |
| Misc | `AwakenBaseLumberFactor` | `.80` | multiplier |  |
| Misc | `AwakenLumberLevelFactor` | `.20` | multiplier |  |
| Misc | `AwakenMaxFactor` | `8.0` | multiplier |  |
| Misc | `MinUnitSpeed` | `150` | map units per second | clamp unit movement speed |
| Misc | `MaxUnitSpeed` | `400` | map units per second | clamp unit movement speed |
| Misc | `MinBldgSpeed` | `25` | map units per second |  |
| Misc | `MaxBldgSpeed` | `400` | map units per second |  |
| Misc | `FrostMoveSpeedDecrease` | `0.5` | map units per second | slowedSpeed = speed * (1 - value) |
| Misc | `FrostAttackSpeedDecrease` | `0.25` | map units per second |  |
| Misc | `MaxHeroLevel` | `10` | level |  |
| Misc | `MaxUnitLevel` | `20` | level |  |
| Misc | `NeedHeroXP` | `200` | experience points |  |
| Misc | `GrantHeroXP` | `100,120,160,220,300` | experience points |  |
| Misc | `GrantNormalXP` | `25` | experience points |  |
| Misc | `HeroFactorXP` | `80,70,60,50,0` | experience points |  |
| Misc | `SummonedKillFactor` | `0.5` | multiplier |  |
| Misc | `StrAttackBonus` | `1.0` | raw | damage += STR * value |
| Misc | `StrHitPointBonus` | `25` | raw | maxHp += STR * value |
| Misc | `StrRegenBonus` | `0.05` | raw | hpRegen += STR * value |
| Misc | `IntManaBonus` | `15` | raw | maxMana += INT * value |
| Misc | `IntRegenBonus` | `0.05` | raw | manaRegen += INT * value |
| Misc | `AgiDefenseBonus` | `0.30` | raw | armor += AGI * value + AgiDefenseBase |
| Misc | `AgiDefenseBase` | `-2` | raw | armor = AGI * AgiDefenseBonus + value |
| Misc | `AgiMoveBonus` | `0` | raw |  |
| Misc | `NeedHeroXPFormulaA` | `1` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `NeedHeroXPFormulaB` | `100` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `NeedHeroXPFormulaC` | `0` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `GrantHeroXPFormulaA` | `1` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `GrantHeroXPFormulaB` | `0` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `GrantHeroXPFormulaC` | `100` | experience points | f(x) = A*f(x-1) + B*x + C |
| Misc | `GrantNormalXPFormulaA` | `1` | experience points |  |
| Misc | `GrantNormalXPFormulaB` | `5` | experience points |  |
| Misc | `GrantNormalXPFormulaC` | `5` | experience points |  |
| Misc | `HeroAbilityLevelSkip` | `2` | level |  |
| Misc | `DropItemRange` | `100` | map units |  |
| Misc | `GiveItemRange` | `150` | map units |  |
| Misc | `PickupItemRange` | `150` | map units |  |
| Misc | `PawnItemRange` | `300` | map units |  |
| Misc | `PawnItemRate` | `0.50` | raw |  |
| Misc | `CallForHelp` | `600` | raw |  |
| Misc | `CreepCallForHelp` | `600` | raw |  |
| Misc | `DefenseArmor` | `0.06` | raw |  |
| Misc | `DamageBonusNormal` | `1.00,1.50,1.00,0.70,1.00,1.00,0.05,1.00` | raw |  |
| Misc | `DamageBonusPierce` | `2.00,0.75,1.00,0.35,1.00,0.50,0.05,1.50` | raw |  |
| Misc | `DamageBonusSiege` | `1.00,0.50,1.00,1.50,1.00,0.50,0.05,1.50` | raw |  |
| Misc | `DamageBonusMagic` | `1.25,0.75,2.00,0.35,1.00,0.50,0.05,1.00` | raw |  |
| Misc | `DamageBonusChaos` | `1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00` | raw |  |
| Misc | `DamageBonusSpells` | `1.00,1.00,1.00,1.00,1.00,0.75,0.05,1.00` | raw |  |
| Misc | `DamageBonusHero` | `1.00,1.00,1.00,0.50,1.00,1.00,0.05,1.00` | raw |  |
| Misc | `EtherealDamageBonus` | `1.66,1.66,1.66,1.66,1.66,1.66,1.66` | raw |  |
| Misc | `EtherealHealBonus` | `1.66` | raw |  |
| Misc | `GuardDistance` | `600` | raw |  |
| Misc | `MaxGuardDistance` | `1000` | raw |  |
| Misc | `GuardReturnTime` | `5.0` | seconds |  |
| Misc | `ConstructionRefundRate` | `0.75	// for cancelled construction` | raw |  |
| Misc | `ResearchRefundRate` | `1.0		// for cancelled research of spells or unit improvements` | raw |  |
| Misc | `ReviveRefundRate` | `1.0		// for cancelled hero revival` | raw |  |
| Misc | `TrainRefundRate` | `1.0		// currently applied regardless of location in queue` | raw |  |
| Misc | `UpgradeRefundRate` | `0.75		// for a cancelled structure upgrade` | raw |  |
| Misc | `ConstructionLifeDrainRate` | `10.0	// hp per second drained when building construction is halted` | raw |  |
| Misc | `MissDamageReduction` | `0.5` | raw |  |

## UnitBalance (812 rows)

| unitBalanceID | level | goldcost | lumbercost | HP | realHP | regenHP | manaN | realM | mana0 | regenMana | def | defUp | realdef | spd | minSpd | maxSpd | bldtm | reptm | sight | nsight | collision | isbldg | STR | INT | AGI | STRplus | INTplus | AGIplus | Primary | tilesets | upgrades | hero_hp_formula |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Hamg | 5 | 425 | 100 | 100 | 450 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 3.1 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 14 | 19 | 17 | 1.8 | 3.2 | 1 | INT | * | _ | 850.0 |
| Hblm | 5 | 425 | 100 | 100 | 550 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 2.2 | 300 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 19 | 14 | 2 | 3 | 1 | INT | * | _ | 950.0 |
| Hmkg | 5 | 425 | 100 | 100 | 700 | 0.25 | 0 | 225 | 100 | 0.01 | 1 | 0 | 2.3 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 24 | 15 | 11 | 3 | 1.5 | 1.5 | STR | * | _ | 1100.0 |
| Hpal | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 100 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| hbot | 2 | 170 | 50 | 1500 | 1500 | - | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hbsh | 6 | 500 | 200 | 1000 | 1000 | - | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 35 | 35 | 1600 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hdes | 4 | 250 | 100 | 575 | 575 | - | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1500 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hdhw | 5 | 235 | 40 | 575 | 575 | 0.25 | 300 | 300 | 150 | 0.75 | 1 | 2 | 1 | 350 | 0 | 0 | 45 | 65 | 1600 | 900 | 8 | 0 | - | - | - | - | - | - | _ | * | Rhla,Rhme,Rguv,Rhan,Rhcd |  |
| hfoo | 2 | 135 | 0 | 420 | 420 | 0.25 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme,Rhde,Rhpm,Rguv |  |
| hgry | 5 | 280 | 70 | 825 | 825 | 0.25 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 45 | 45 | 1600 | 900 | 8 | 0 | - | - | - | - | - | - | _ | * | Rhan,Rhla,Rhhb,Rhme,Rguv |  |
| hgyr | 1 | 90 | 30 | 175 | 175 | - | - | - | - | - | 2 | 2 | 2 | 400 | 0 | 0 | 13 | 13 | 1800 | 1100 | 8 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhra,Rhgb,Rhfc,Rguv |  |
| hkni | 4 | 245 | 60 | 800 | 800 | 0.25 | - | - | - | - | 5 | 2 | 5 | 350 | 0 | 0 | 40 | 45 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme,Rhan,Rhpm,Rguv |  |
| hmil | 2 | 75 | 0 | 220 | 220 | 0.25 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 15 | 15 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme,Rguv |  |
| hmpr | 2 | 135 | 10 | 290 | 290 | 0.25 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhpt,Rhpm,Rguv |  |
| hmtm | 2 | 180 | 70 | 360 | 360 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 32 | 32 | 1400 | 1200 | 32 | 0 | - | - | - | - | - | - | _ | * | Rhla,Rhra,Rhfl,Rhfs,Rguv |  |
| hmtt | 2 | 195 | 60 | 700 | 700 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 55 | 55 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhra,Rhrt,Rguv |  |
| hpea | 1 | 75 | 0 | 220 | 220 | 0.25 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhlh,Rguv |  |
| hphx | 0 | 0 | 0 | 1250 | 1250 | -25 | - | - | - | - | 1 | 2 | 1 | 320 | 0 | 0 | 45 | 45 | 1600 | 900 | 8 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| hpxe | 6 | 0 | 0 | 200 | 200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 45 | 45 | 600 | 600 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hrif | 3 | 205 | 30 | 520 | 520 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rhla,Rhra,Rhri,Rhpm,Rguv |  |
| hrtt | 2 | 195 | 60 | 700 | 700 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 55 | 55 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhra,Rguv |  |
| hsor | 2 | 155 | 20 | 325 | 325 | 0.25 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhst,Rhpm,Rguv |  |
| hspt | 3 | 215 | 30 | 650 | 650 | 0.5 | 250 | 250 | 75 | 0.8 | 3 | 2 | 3 | 300 | 0 | 0 | 28 | 28 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme,Rhss,Rhpm,Rguv |  |
| hwat | 4 | 0 | 0 | 450 | 450 | 0.25 | - | - | - | - | 0 | 2 | 0 | 220 | 0 | 0 | 1 | 1 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| hwt2 | 5 | 0 | 0 | 675 | 675 | 0.25 | - | - | - | - | 1 | 2 | 1 | 220 | 0 | 0 | 1 | 1 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| hwt3 | 6 | 0 | 0 | 900 | 900 | 0.25 | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 1 | 1 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| halt | - | 180 | 50 | 900 | 900 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| harm | - | 140 | 140 | 1200 | 1200 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hars | - | 150 | 140 | 1050 | 1050 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hatw | 1 | 80 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 50 | 50 | 1600 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hbar | - | 160 | 60 | 1500 | 1500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hbla | - | 140 | 60 | 1200 | 1200 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hcas | - | 380 | 230 | 2500 | 2500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 140 | 120 | 900 | 600 | 172 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hctw | 1 | 170 | 100 | 600 | 600 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 75 | 75 | 1600 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hgra | - | 120 | 240 | 1200 | 1200 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 75 | 75 | 900 | 600 | 100 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hgtw | 1 | 80 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 50 | 50 | 1600 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hhou | - | 80 | 20 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 35 | 35 | 900 | 600 | 72 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hkee | - | 320 | 210 | 2000 | 2000 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 140 | 120 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hlum | - | 145 | 0 | 900 | 900 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hshy | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 900 | 900 | 50 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| htow | - | 385 | 205 | 1500 | 1500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 180 | 180 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hvlt | - | 130 | 30 | 485 | 485 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 1000 | 750 | 50 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| hwtw | - | 30 | 20 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 25 | 25 | 1600 | 1000 | 72 | 1 | - | - | - | - | - | - | _ | * | Rhac,Rgfo |  |
| Obla | 5 | 425 | 100 | 100 | 550 | 0.25 | 0 | 240 | 100 | 0.01 | 0 | 0 | 4.9 | 300 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 16 | 23 | 2 | 2.25 | 1.75 | AGI | * | _ | 950.0 |
| Ofar | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 3.4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 19 | 18 | 2 | 3 | 1 | INT | * | _ | 875.0 |
| Oshd | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 255 | 100 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 17 | 20 | 2 | 2.5 | 1.5 | AGI | * | _ | 875.0 |
| Otch | 5 | 425 | 100 | 100 | 725 | 0.25 | 0 | 210 | 100 | 0.01 | 1 | 0 | 2 | 250 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 14 | 10 | 3.2 | 1.3 | 1.5 | STR | * | _ | 1125.0 |
| nwad | - | 0 | 0 | 200 | 200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 1600 | 1600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| obot | 2 | 170 | 50 | 1500 | 1500 | - | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ocat | 2 | 220 | 50 | 425 | 425 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 36 | 36 | 1400 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | Roar,Rora,Robf,Rolf,Rguv |  |
| odes | 4 | 250 | 100 | 575 | 575 | - | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1500 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| odoc | 2 | 145 | 25 | 315 | 315 | 0.25 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rowd,Rotr,Ropm,Rguv |  |
| oeye | - | 0 | 0 | 200 | 200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 1600 | 1600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ogru | 3 | 200 | 0 | 700 | 700 | 0.25 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Robs,Ropg,Ropm,Rguv |  |
| ohun | 2 | 135 | 20 | 350 | 350 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 22 | 22 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rora,Rotr,Ropm,Robk,Rguv |  |
| ohwd | - | 0 | 0 | 5 | 5 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| okod | 4 | 255 | 60 | 1000 | 1000 | 0.25 | - | - | - | - | 1 | 2 | 1 | 220 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rwdm,Rguv |  |
| opeo | 1 | 75 | 0 | 250 | 250 | 0.25 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Ropg,Rguv |  |
| orai | 3 | 180 | 40 | 610 | 610 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Roen,Ropg,Ropm,Rguv |  |
| oshm | 2 | 130 | 20 | 335 | 335 | 0.25 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rost,Ropm,Rguv |  |
| osp1 | 1 | 0 | 0 | 75 | 75 | 0.25 | - | - | - | - | 0 | 2 | 0 | 0 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| osp2 | 2 | 0 | 0 | 135 | 135 | 0.25 | - | - | - | - | 0 | 2 | 0 | 0 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| osp3 | 3 | 0 | 0 | 135 | 135 | 0.25 | - | - | - | - | 0 | 2 | 0 | 0 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| osp4 | 3 | 0 | 0 | 135 | 135 | 0.25 | - | - | - | - | 0 | 2 | 0 | 0 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ospm | 3 | 195 | 35 | 500 | 500 | 0.25 | 300 | 300 | 100 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 42 | 42 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rowt,Ropm,Rguv |  |
| ospw | 3 | 195 | 35 | 500 | 500 | 0.25 | 300 | 300 | 100 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 42 | 42 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rowt,Ropm,Rguv |  |
| osw1 | 2 | 0 | 0 | 200 | 200 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| osw2 | 3 | 0 | 0 | 300 | 300 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| osw3 | 4 | 0 | 0 | 400 | 400 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| otau | 5 | 280 | 80 | 1300 | 1300 | 0.25 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 44 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Rows,Ropm,Rguv |  |
| otbk | 2 | 135 | 20 | 450 | 450 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 22 | 22 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rora,Rotr,Ropm,Rguv |  |
| otbr | 2 | 160 | 40 | 500 | 500 | 0.25 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 1400 | 8 | 0 | - | - | - | - | - | - | _ | * | Roar,Rora,Rguv,Rotr |  |
| otot | - | 0 | 0 | 100 | 100 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| owyv | 4 | 265 | 40 | 600 | 600 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 35 | 35 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Roar,Rora,Rovs,Rguv |  |
| oalt | - | 180 | 50 | 900 | 900 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| obar | - | 180 | 50 | 1200 | 1200 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| obea | - | 145 | 140 | 1100 | 1100 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| ofor | - | 205 | 0 | 1000 | 1000 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 70 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| ofrt | - | 345 | 210 | 2500 | 2500 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 140 | 120 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| ogre | - | 385 | 185 | 1500 | 1500 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 150 | 150 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| oshy | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 900 | 900 | 50 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| osld | - | 150 | 150 | 800 | 800 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| ostr | - | 315 | 190 | 2000 | 2000 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 140 | 120 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| otrb | 1 | 160 | 40 | 600 | 600 | - | - | - | - | - | 2 | 0 | 2 | - | 0 | 0 | 50 | 50 | 800 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rorb,Rgfo |  |
| otto | - | 90 | 200 | 1200 | 1200 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| ovln | - | 130 | 30 | 500 | 500 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 1000 | 750 | 50 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rgfo |  |
| owtw | 1 | 110 | 80 | 500 | 500 | - | - | - | - | - | 3 | 1 | 3 | - | 0 | 0 | 55 | 55 | 1600 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rorb,Rgfo |  |
| Edem | 5 | 425 | 100 | 100 | 575 | 0.5 | 0 | 240 | 100 | 0.01 | 0 | 0 | 4.6 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 19 | 16 | 22 | 2.4 | 2.1 | 1.5 | AGI | * | Reuv | 975.0 |
| Edmm | 5 | 425 | 100 | 100 | 750 | 1 | 0 | 240 | 100 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 26 | 16 | 20 | 2.4 | 2.1 | 1.5 | AGI | * | Reuv | 1150.0 |
| Ekee | 5 | 425 | 100 | 100 | 500 | 0.5 | 0 | 270 | 100 | 0.01 | 0 | 0 | 2.5 | 320 | 0 | 0 | 55 | 55 | 1800 | 900 | 32 | 0 | 16 | 18 | 15 | 1.8 | 2.7 | 1.5 | INT | * | Reuv | 900.0 |
| Emoo | 5 | 425 | 100 | 100 | 550 | 0.5 | 0 | 225 | 100 | 0.01 | 0 | 0 | 3.7 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 15 | 19 | 1.9 | 2.6 | 1.5 | AGI | * | Reuv | 950.0 |
| Ewar | 5 | 425 | 100 | 100 | 550 | 0.5 | 0 | 225 | 100 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 15 | 20 | 2.4 | 2 | 1.6 | AGI | * | Reuv | 950.0 |
| earc | 2 | 130 | 10 | 310 | 310 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Reib,Remk,Reuv,Repm |  |
| ebal | 2 | 210 | 65 | 300 | 300 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 48 | 48 | 1400 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | Resm,Reuv,Repb |  |
| ebsh | 6 | 500 | 200 | 1000 | 1000 | - | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 35 | 35 | 1600 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| echm | 5 | 330 | 70 | 1000 | 1000 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 60 | 65 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Recb,Reuv |  |
| edcm | 4 | 255 | 80 | 810 | 810 | 1 | 200 | 200 | 100 | 0.333333333333333 | 3 | 2 | 3 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Reuv,Redc,Repm |  |
| edes | 4 | 250 | 100 | 575 | 575 | - | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1500 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| edoc | 4 | 255 | 80 | 430 | 430 | 0.5 | 200 | 200 | 100 | 0.666666666666667 | 1 | 2 | 1 | 270 | 0 | 0 | 35 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Redc,Reuv,Repm,Reeb |  |
| edot | 2 | 135 | 20 | 300 | 300 | 0.5 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 22 | 22 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Redt,Reuv,Repm,Reec |  |
| edry | 2 | 145 | 60 | 435 | 435 | 0.5 | 200 | 200 | 75 | 0.75 | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Resi,Reuv,Repm |  |
| edtm | 2 | 135 | 20 | 300 | 300 | 1 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 350 | 0 | 0 | 22 | 22 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Reuv,Redt |  |
| efdr | 3 | 155 | 25 | 450 | 450 | 0.5 | 200 | 200 | 75 | 0.75 | 0 | 2 | 0 | 350 | 0 | 0 | 25 | 25 | 1600 | 1000 | 8 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh |  |
| efon | 2 | 0 | 0 | 300 | 300 | 0.5 | - | - | - | - | 0 | 5 | 0 | 220 | 0 | 0 | 30 | 30 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Reuv,Renb |  |
| ehip | 2 | 160 | 20 | 525 | 525 | 0.5 | - | - | - | - | 0 | 2 | 0 | 400 | 0 | 0 | 30 | 30 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Reht,Reuv |  |
| ehpr | 4 | 290 | 30 | 835 | 835 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 1 | 1 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Reib,Remk,Reht,Reuv |  |
| emtg | 6 | 425 | 100 | 1400 | 1400 | 1.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 50 | 50 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Resw,Rerh,Rers,Rehs,Repm,Reuv |  |
| esen | 3 | 195 | 20 | 575 | 575 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Resc,Reuv,Remg,Repm |  |
| espv | 7 | 0 | 0 | 1200 | 1200 | 1.6 | 400 | 400 | 400 | 2 | 2 | 2 | 2 | 320 | 0 | 0 | 20 | 20 | 1200 | 1000 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| even | 2 | 0 | 0 | 500 | 500 | 1.6 | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ewsp | 1 | 60 | 0 | 120 | 120 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 14 | 14 | 1000 | 750 | 16 | 0 | - | - | - | - | - | - | _ | * | Rewd,Reuv |  |
| eaoe | 1 | 155 | 145 | 900 | 900 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| eaom | 1 | 150 | 60 | 1000 | 1000 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| eaow | 1 | 150 | 140 | 900 | 900 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| eate | - | 180 | 50 | 900 | 900 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| eden | 1 | 90 | 30 | 450 | 450 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 60 | 60 | 1000 | 750 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| edob | - | 210 | 100 | 1100 | 1100 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| edos | - | 100 | 230 | 1200 | 1200 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 80 | 80 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| egol | - | 0 | 0 | 800 | 800 | - | - | - | - | - | 2 | 5 | 2 | - | 0 | 0 | 60 | 60 | 900 | 600 | 50 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| emow | - | 180 | 40 | 600 | 600 | - | 300 | 300 | 100 | 1.25 | 5 | 0 | 5 | - | 0 | 0 | 50 | 50 | 900 | 600 | 72 | 1 | - | - | - | - | - | - | _ | * | Rews,Rgfo |  |
| eshy | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 900 | 900 | 50 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| etoa | 2 | 320 | 180 | 1700 | 1700 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 140 | 120 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| etoe | 3 | 350 | 220 | 2000 | 2000 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 140 | 130 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| etol | 1 | 340 | 185 | 1300 | 1300 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 110 | 110 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| etrp | 3 | 135 | 80 | 600 | 600 | 0.5 | - | - | - | - | 1 | 2 | 1 | 40 | 0 | 0 | 60 | 60 | 1900 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb,Rgfo |  |
| Ucrl | 5 | 425 | 100 | 25 | 675 | 2 | 0 | 210 | 100 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 26 | 14 | 14 | 3.2 | 1.6 | 1.2 | STR | * | _ | 775.0 |
| Udea | 5 | 425 | 100 | 100 | 675 | 2 | 0 | 255 | 100 | 0.01 | 2 | 0 | 3.6 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 23 | 17 | 12 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1075.0 |
| Udre | 5 | 425 | 100 | 100 | 600 | 2 | 0 | 270 | 100 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 2.5 | 1 | STR | * | _ | 1000.0 |
| Ulic | 5 | 425 | 100 | 100 | 475 | 2 | 0 | 300 | 100 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 20 | 14 | 2 | 3.4 | 1 | INT | * | _ | 875.0 |
| uabo | 4 | 240 | 70 | 1080 | 1080 | 2 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 45 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Ruar,Rume,Rupc,Rupm,Rguv |  |
| uaco | 1 | 75 | 0 | 220 | 220 | 2 | - | - | - | - | 0 | 2 | 0 | 220 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| uban | 2 | 155 | 30 | 285 | 285 | 2 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 35 | 35 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Ruba,Rupm,Rguv |  |
| ubsp | 5 | 100 | 50 | 900 | 900 | 2 | 400 | 400 | 0 | -3 | 4 | 2 | 4 | 320 | 0 | 0 | 1 | 60 | 1400 | 1000 | 8 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Rusp,Rguv |  |
| ucrm | 3 | 215 | 40 | 550 | 550 | 10 | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Ruwb,Rubu,Rupm,Rguv |  |
| ucry | 3 | 215 | 40 | 550 | 550 | 2 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Ruwb,Rubu,Rupm,Rguv |  |
| ucs1 | 1 | 0 | 0 | 140 | 140 | 2 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ucs2 | 2 | 0 | 0 | 275 | 275 | 2 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ucs3 | 3 | 0 | 0 | 410 | 410 | 2 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ucsB | 2 | 0 | 0 | 275 | 275 | 2 | - | - | - | - | 2 | 2 | 2 | - | 0 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ucsC | 3 | 0 | 0 | 410 | 410 | 2 | - | - | - | - | 2 | 2 | 2 | - | 0 | 0 | 20 | 20 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ufro | 6 | 385 | 120 | 1350 | 1350 | 2 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 80 | 80 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Rufb,Rguv |  |
| ugar | 2 | 185 | 30 | 410 | 410 | 2 | - | - | - | - | 3 | 2 | 3 | 350 | 0 | 0 | 35 | 35 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Rusf,Rguv |  |
| ugho | 2 | 120 | 0 | 330 | 330 | 2 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 18 | 18 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Ruar,Rume,Rugf,Ruac,Rupm,Rguv |  |
| ugrm | 2 | 185 | 30 | 410 | 410 | 2 | - | - | - | - | 15 | 2 | 15 | - | 0 | 0 | 35 | 35 | 1600 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rura,Rucr,Rusf,Rguv |  |
| uloc | 0 | 0 | 0 | 65 | 65 | 0.25 | - | - | - | - | 0 | 2 | 0 | 400 | 0 | 0 | 20 | 20 | 0 | 0 | 8 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| umtw | 2 | 230 | 50 | 380 | 380 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 36 | 36 | 1400 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | Rupc,Rume,Rguv |  |
| unec | 2 | 145 | 20 | 305 | 305 | 2 | 200 | 200 | 75 | 0.666666666666667 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 16 | 0 | - | - | - | - | - | - | _ | * | Rusl,Rune,Rusm,Rupm,Rguv |  |
| uobs | 2 | 200 | 35 | 800 | 800 | 2 | 600 | 600 | 400 | 1.5 | 4 | 2 | 4 | 270 | 0 | 0 | 35 | 35 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rusp,Rguv |  |
| uplg | - | 0 | 0 | 100 | 100 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 0 | 0 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ushd | 1 | 0 | 0 | 125 | 125 | 2 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 15 | 15 | 1900 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| uske | 1 | 0 | 0 | 180 | 180 | 2 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | * | Ruar,Rume,Rusl,Rguv |  |
| uskm | 1 | 0 | 0 | 230 | 230 | 2 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 31 | 0 | - | - | - | - | - | - | _ | * | Ruar,Rume,Rusl,Rguv |  |
| uubs | 6 | 500 | 200 | 1000 | 1000 | - | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 35 | 35 | 1600 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| uaod | - | 180 | 50 | 900 | 900 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| ubon | - | 125 | 250 | 1500 | 1500 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 80 | 80 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| ugol | - | 255 | 220 | 950 | 950 | 0 | - | - | - | - | 5 | 2 | 5 |  | 0 | 0 | 100 | 100 | 900 | 600 | 50 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| ugrv | - | 215 | 0 | 900 | 900 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 80 | 80 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| unp1 | 2 | 320 | 210 | 1750 | 1750 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 140 | 120 | 1200 | 800 | 196 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| unp2 | 3 | 345 | 250 | 2000 | 2000 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 140 | 120 | 1200 | 800 | 196 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| unpl | - | 255 | 0 | 1500 | 1500 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 120 | 120 | 900 | 600 | 196 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| usap | - | 75 | 150 | 900 | 900 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 45 | 45 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| usep | - | 200 | 50 | 1300 | 1300 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| ushp | - | 50 | 0 | 750 | 750 | - | - | - | - | - | 2 | 2 | 2 | - | 0 | 0 | 60 | 60 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| uslh | - | 140 | 135 | 1200 | 1200 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| utod | - | 155 | 140 | 1100 | 1100 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| utom | - | 130 | 30 | 475 | 475 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 1000 | 750 | 50 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| uzg1 | 1 | 145 | 40 | 550 | 550 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 35 | 35 | 1600 | 800 | 120 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| uzg2 | 1 | 100 | 20 | 550 | 550 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 30 | 35 | 1600 | 800 | 120 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| uzig | - | 150 | 50 | 550 | 550 | 0 | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 50 | 50 | 1600 | 600 | 120 | 1 | - | - | - | - | - | - | _ | * | Rgfo |  |
| Nbrn | 5 | 425 | 100 | 100 | 550 | 2 | 0 | 225 | 100 | 0.01 | 0 | 0 | 3.7 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 15 | 19 | 1.9 | 2.6 | 1.5 | AGI | * | _ | 950.0 |
| Nbst | 5 | 425 | 100 | 100 | 725 | 0.5 | 0 | 225 | 100 | 0.01 | 1 | 0 | 3.2 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 15 | 14 | 2.9 | 1.8 | 1.3 | STR | * | _ | 1125.0 |
| Nngs | 5 | 425 | 100 | 100 | 475 | 0.5 | 0 | 330 | 100 | 0.01 | 0 | 0 | 2.8 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 22 | 16 | 2 | 3 | 1 | INT | * | _ | 875.0 |
| Npbm | 5 | 425 | 100 | 100 | 650 | 0.5 | 0 | 225 | 100 | 0.01 | 1 | 0 | 3.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 15 | 14 | 3 | 1.5 | 1.5 | STR | * | _ | 1050.0 |
| Nplh | 5 | 425 | 100 | 100 | 700 | 2 | 0 | 210 | 100 | 0.01 | 0 | 0 | 2.2 | 300 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 24 | 14 | 14 | 3.2 | 1.5 | 1.3 | STR | * | _ | 1100.0 |
| ndr1 | 2 | 0 | 0 | 200 | 200 | 2 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ndr2 | 3 | 0 | 0 | 260 | 260 | 2 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ndr3 | 4 | 0 | 0 | 320 | 320 | 2 | - | - | - | - | 3 | 2 | 3 | 330 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngz1 | 4 | 0 | 0 | 600 | 600 | 0.5 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngz2 | 5 | 0 | 0 | 900 | 900 | 1 | - | - | - | - | 1 | 2 | 1 | 320 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngz3 | 6 | 0 | 0 | 1200 | 1200 | 1.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngzc | 4 | 0 | 0 | 900 | 900 | 0.5 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngzd | 5 | 0 | 0 | 1350 | 1350 | 1 | - | - | - | - | 1 | 2 | 1 | 320 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngza | 6 | 0 | 0 | 1800 | 1800 | 1.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ngz4 | 7 | 0 | 0 | 2250 | 2250 | 2 | - | - | - | - | 3 | 2 | 3 | 370 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| npn1 | 6 | 0 | 0 | 900 | 900 | 2 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npn2 | 6 | 0 | 0 | 1000 | 1000 | 1 | 500 | 500 | 500 | 1.5 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npn3 | 6 | 0 | 0 | 1500 | 1500 | 1 | - | - | - | - | 5 | 2 | 5 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npn4 | 8 | 0 | 0 | 1350 | 1350 | 4 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npn5 | 8 | 0 | 0 | 1500 | 1500 | 2 | 750 | 750 | 750 | 1.5 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npn6 | 8 | 0 | 0 | 2250 | 2250 | 2 | - | - | - | - | 5 | 2 | 5 | 300 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nsw1 | 2 | 0 | 0 | 200 | 200 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nsw2 | 3 | 0 | 0 | 300 | 300 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nsw3 | 4 | 0 | 0 | 400 | 400 | 0.25 | 150 | 150 | 150 | 0.375 | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| ncat | 2 | 220 | 50 | 425 | 425 | - | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 36 | 36 | 1400 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | O,K | Roar,Rora,Robf,Rguv |  |
| nqb1 | 3 | 0 | 0 | 425 | 425 | 0.5 | - | - | - | - | 0 | 2 | 0 | 300 | 0 | 0 | 20 | 20 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nqb2 | 4 | 0 | 0 | 515 | 515 | 1 | - | - | - | - | 0 | 2 | 0 | 300 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nqb3 | 5 | 0 | 0 | 600 | 600 | 1.5 | - | - | - | - | 0 | 2 | 0 | 300 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nqb4 | 6 | 0 | 0 | 700 | 700 | 2 | - | - | - | - | 0 | 2 | 0 | 300 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nwe1 | 2 | 0 | 0 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 22 | 22 | 1600 | 1200 | 8 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nwe2 | 5 | 0 | 0 | 625 | 625 | 1 | - | - | - | - | 3 | 2 | 3 | 350 | 0 | 0 | 22 | 22 | 1600 | 1200 | 8 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nwe3 | 6 | 0 | 0 | 875 | 875 | 1.5 | - | - | - | - | 5 | 2 | 5 | 350 | 0 | 0 | 22 | 22 | 1600 | 1200 | 8 | 0 | - | - | - | - | - | - | _ | * | Rguv |  |
| nhyc | 5 | 320 | 65 | 750 | 750 | 1 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 45 | 45 | 1400 | 950 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat |  |
| nmyr | 4 | 225 | 55 | 1080 | 1080 | 1 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb,Rnen |  |
| nmys | 4 | 225 | 55 | 1080 | 1080 | 1 | - | - | - | - | 2 | 2 | 2 | - | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb,Rnen |  |
| nnmg | 2 | 120 | 0 | 400 | 400 | 1 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 18 | 18 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat |  |
| nnrg | 6 | 300 | 100 | 1350 | 1350 | 1 | 500 | 500 | 500 | 1.25 | 5 | 2 | 5 | 320 | 0 | 0 | 45 | 45 | 1600 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb |  |
| nnrs | 6 | 300 | 100 | 1350 | 1350 | 1 | 500 | 500 | 500 | 1.25 | 5 | 2 | 5 | - | 0 | 0 | 45 | 45 | 1600 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb |  |
| nnsu | 2 | 130 | 20 | 350 | 350 | 1 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rnsw,Rguv |  |
| nnsw | 2 | 130 | 20 | 350 | 350 | 1 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rnsw,Rguv |  |
| nwgs | 2 | 235 | 30 | 525 | 525 | 1 | 200 | 200 | 50 | 0.5 | 3 | 2 | 3 | 350 | 0 | 0 | 35 | 35 | 1600 | 800 | 8 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsi |  |
| nsbs | 3 | 200 | 25 | 500 | 500 | 1 | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb |  |
| nsnp | 3 | 200 | 25 | 500 | 500 | 1 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rguv,Rnam,Rnat,Rnsb |  |
| nadk | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N,W,I,N | _ |  |
| nadr | 10 | 595 | 200 | 2200 | 2200 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N,W,I,N | _ |  |
| nadw | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N,W,I,N | _ |  |
| nahy | 10 | 595 | 200 | 1600 | 1600 | 25 | 600 | 600 | 600 | 1.5 | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nanb | 1 | 75 | 5 | 200 | 200 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nanm | 1 | 75 | 5 | 200 | 200 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nanc | 1 | 85 | 0 | 200 | 200 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nane | 4 | 120 | 10 | 400 | 400 | 0.5 | 200 | 200 | 200 | 0.5 | 2 | 2 | 2 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nano | 5 | 205 | 30 | 750 | 750 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nanw | 3 | 170 | 20 | 375 | 375 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| narg | 3 | 155 | 10 | 500 | 500 | 0.5 | - | - | - | - | 0 | 2 | 0 | 240 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | D,G | _ |  |
| nass | 4 | 200 | 30 | 450 | 450 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nbal | 8 | 340 | 100 | 1350 | 1350 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 270 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | C,O,K | _ |  |
| nban | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nbda | 4 | 155 | 10 | 350 | 350 | 0.5 | 250 | 250 | 250 | 0.625 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbdk | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | F | _ |  |
| nbdm | 3 | 115 | 0 | 500 | 500 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbdo | 8 | 205 | 30 | 1000 | 1000 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbdr | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | F | _ |  |
| nbds | 6 | 290 | 80 | 675 | 675 | 0.5 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbdw | 5 | 205 | 30 | 775 | 775 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbld | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nbnb | 1 | 75 | 5 | 200 | 200 | 10 | - | - | - | - | 1 | 2 | 1 | - | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nbot | 2 | 170 | 50 | 1500 | 1500 | - | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nbrg | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nbwm | 10 | 595 | 200 | 2200 | 2200 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | F | _ |  |
| nbzd | 10 | 595 | 200 | 2200 | 2200 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nbzk | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nbzw | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncea | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncen | 4 | 155 | 0 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncer | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncfs | 2 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ncim | 4 | 205 | 30 | 350 | 350 | 0.5 | 200 | 200 | 200 | 0.5 | 1 | 2 | 1 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncks | 5 | 255 | 50 | 600 | 600 | 1 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 350 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ncnk | 8 | 385 | 120 | 900 | 900 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| ndqn | 3 | 180 | 40 | 400 | 400 | 0.5 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndqp | 8 | 180 | 40 | 1050 | 1050 | 0.5 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 320 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndqs | 10 | 180 | 40 | 1600 | 1600 | 0.5 | 500 | 500 | 500 | 1.25 | 1 | 2 | 1 | 320 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndqt | 6 | 180 | 40 | 750 | 750 | 0.5 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndqv | 5 | 180 | 40 | 510 | 510 | 0.5 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 320 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrv | 8 | 205 | 30 | 1000 | 1000 | 0.5 | 500 | 500 | 500 | 1.25 | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ndtb | 4 | 205 | 30 | 450 | 450 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| ndth | 4 | 205 | 30 | 450 | 450 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| ndtp | 2 | 155 | 10 | 240 | 240 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| ndtr | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| ndtt | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| ndtw | 6 | 290 | 80 | 750 | 750 | 0.5 | - | - | - | - | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| nehy | 7 | 205 | 30 | 850 | 850 | 15 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nelb | 8 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 270 | 0 | 0 | 80 | 80 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | X,J | _ |  |
| nele | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | X,J | _ |  |
| nenc | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nenf | 5 | 170 | 20 | 500 | 500 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nenp | 3 | 170 | 20 | 290 | 290 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nepl | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nerd | 6 | 85 | 0 | 630 | 630 | 1 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ners | 4 | 85 | 0 | 425 | 425 | 1 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nerw | 9 | 85 | 0 | 1350 | 1350 | 1 | 500 | 500 | 500 | 1.25 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nfel | 5 | 255 | 50 | 750 | 750 | 0.5 | 200 | 200 | 200 | 0.5 | 3 | 2 | 3 | 350 | 0 | 0 | 50 | 50 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | C,O,K | _ |  |
| nfgb | 4 | 135 | 0 | 450 | 450 | 0.25 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nfgo | 15 | 595 | 250 | 4000 | 4000 | 10 | 1000 | 1000 | 1000 | 5 | 4 | 2 | 4 | - | 0 | 0 | 80 | 80 | 1400 | 800 | 50 | 1 | - | - | - | - | - | - | _ | I | _ |  |
| nfgt | 1 | 0 | 0 | 200 | 200 | 0.5 | - | - | - | - | 0 | 2 | 0 | 0 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nfgu | 2 | 135 | 0 | 300 | 300 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nfod | 10 | 595 | 200 | 1900 | 1900 | 0.5 | 500 | 500 | 500 | 1.25 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nfor | 6 | 205 | 30 | 675 | 675 | 0.5 | 300 | 300 | 300 | 0.75 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nfot | 8 | 385 | 120 | 1150 | 1150 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nfov | 6 | 135 | 0 | 775 | 775 | 0.25 | 300 | 300 | 300 | 0.75 | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nfpc | 7 | 340 | 100 | 950 | 950 | 0.5 | - | - | - | - | 5 | 2 | 5 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfpe | 8 | 385 | 120 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfpl | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfps | 4 | 220 | 30 | 550 | 550 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfpt | 6 | 290 | 80 | 950 | 950 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfpu | 8 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nfra | 8 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfrb | 6 | 290 | 80 | 950 | 950 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfre | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfrg | 7 | 340 | 100 | 950 | 950 | 0.5 | - | - | - | - | 5 | 2 | 5 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfrl | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfrp | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nfrs | 4 | 205 | 30 | 550 | 550 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfsh | 4 | 245 | 40 | 450 | 450 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| nfsp | 2 | 155 | 10 | 240 | 240 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| nftb | 4 | 195 | 30 | 450 | 450 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| nftk | 6 | 290 | 80 | 750 | 750 | 0.5 | - | - | - | - | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| nftr | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| nftt | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,V,Q | _ |  |
| ngdk | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nggr | 9 | 435 | 150 | 1500 | 1500 | 1.5 | 600 | 600 | 600 | 1.5 | 5 | 2 | 5 | 270 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,D,J | _ |  |
| ngh1 | 3 | 170 | 20 | 300 | 300 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | N,D,G | _ |  |
| ngh2 | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | N,D,G | _ |  |
| ngir | 4 | 375 | 100 | 600 | 600 | - | - | - | - | - | 3 | 2 | 3 | 240 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nglm | - | 0 | 0 | 100 | 100 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 0 | 0 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ngna | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngnb | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngno | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngns | 3 | 170 | 20 | 320 | 320 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngnv | 5 | 245 | 35 | 750 | 750 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngnw | 3 | 170 | 20 | 330 | 330 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngrd | 10 | 595 | 200 | 2200 | 2200 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| ngrk | 2 | 115 | 10 | 240 | 240 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 240 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,D,J | _ |  |
| ngrw | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| ngsp | 4 | 215 | 100 | 100 | 100 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ngst | 6 | 290 | 80 | 675 | 675 | 0.5 | 400 | 400 | 400 | 1 | 4 | 2 | 4 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,D,J | _ |  |
| nhar | 1 | 120 | 0 | 210 | 210 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nhdc | 3 | 170 | 20 | 300 | 300 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y | _ |  |
| nhfp | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y | _ |  |
| nhhr | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y | _ |  |
| nhrh | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 350 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nhrq | 7 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nhrr | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nhrw | 3 | 190 | 30 | 280 | 280 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nhyd | 6 | 205 | 30 | 575 | 575 | 10 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nhyh | 3 | 205 | 30 | 350 | 350 | 4 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nina | 10 | 220 | 50 | 1500 | 1500 | - | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 180 | 0 | 0 | 36 | 36 | 1400 | 1000 | 64 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ninc | 5 | 220 | 50 | 600 | 600 | - | - | - | - | - | 2 | 2 | 2 | 180 | 0 | 0 | 36 | 36 | 1400 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ninf | 8 | 385 | 120 | 1500 | 1500 | 1 | - | - | - | - | 6 | 2 | 6 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | C,O,K | _ |  |
| ninm | 8 | 220 | 50 | 1200 | 1200 | - | 350 | 350 | 350 | 0.875 | 2 | 2 | 2 | 180 | 0 | 0 | 36 | 36 | 1400 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nith | 4 | 205 | 30 | 450 | 450 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| nitp | 2 | 155 | 10 | 240 | 240 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| nitr | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| nits | 4 | 195 | 30 | 450 | 450 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| nitt | 3 | 160 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| nitw | 6 | 290 | 80 | 750 | 750 | 0.5 | - | - | - | - | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| njgb | 9 | 435 | 150 | 1600 | 1600 | 0.5 | 600 | 600 | 600 | 1.5 | 2 | 2 | 2 | 320 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| njga | 6 | 300 | 0 | 900 | 900 | 0.5 | - | - | - | - | 1 | 2 | 1 | 320 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| njg1 | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nkob | 1 | 70 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D,Y,X,V,Q,G,J | _ |  |
| nkog | 3 | 205 | 30 | 300 | 300 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D,Y,X,V,Q,G,J | _ |  |
| nkol | 5 | 255 | 50 | 650 | 650 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D,Y,X,V,Q,G,J | _ |  |
| nkot | 3 | 170 | 20 | 325 | 325 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D,Y,X,V,Q,G,J | _ |  |
| nlds | 5 | 400 | 100 | 480 | 480 | 0.5 | 300 | 300 | 300 | 0.75 | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlkl | 7 | 85 | 0 | 800 | 800 | 0.5 | - | - | - | - | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlpd | 2 | 85 | 0 | 210 | 210 | 0.5 | - | - | - | - | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlpr | 1 | 85 | 0 | 170 | 170 | 0.5 | - | - | - | - | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlps | 1 | 85 | 0 | 170 | 170 | 0.5 | - | - | - | - | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlrv | 10 | 595 | 200 | 2100 | 2100 | 0.5 | 600 | 600 | 300 | 1.5 | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nlsn | 5 | 250 | 0 | 620 | 620 | 0.5 | - | - | - | - | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nltc | 2 | 85 | 0 | 240 | 240 | 0.5 | 300 | 300 | 300 | 0.75 | 6 | 2 | 6 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nltl | 2 | 155 | 10 | 280 | 280 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,A,C,G | _ |  |
| nlur | - | 0 | 0 | 200 | 200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 1600 | 1600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nmam | 3 | 205 | 30 | 450 | 450 | 0.5 | - | - | - | - | 0 | 2 | 0 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmbg | 2 | 85 | 0 | 300 | 300 | 0.5 | 200 | 200 | 200 | 0.5 | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nmcf | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nmdr | 8 | 385 | 120 | 1550 | 1550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmfs | 3 | 110 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nmgd | 10 | 595 | 200 | 2100 | 2100 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 64 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmgr | 8 | 205 | 30 | 1500 | 1500 | 0.5 | 350 | 350 | 350 | 0.875 | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmgw | 5 | 270 | 15 | 900 | 900 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmit | 5 | 205 | 30 | 925 | 925 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nmmu | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nmpg | 2 | 85 | 0 | 180 | 180 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| nmrl | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,B,N,A,J | _ |  |
| nmrm | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,B,N,A,J | _ |  |
| nmrr | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | L,F,W,V,Q,Y,X,B,N,A,J | _ |  |
| nmrv | 6 | 85 | 0 | 1000 | 1000 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nmsc | 7 | 85 | 0 | 1000 | 1000 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nmsn | 4 | 170 | 20 | 375 | 375 | 0.5 | 300 | 300 | 300 | 0.75 | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nmtw | 3 | 85 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nndk | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nndr | 10 | 595 | 200 | 2200 | 2200 | 2 | 500 | 500 | 500 | 1.25 | 6 | 2 | 6 | 350 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nnht | 3 | 170 | 20 | 340 | 340 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nnwa | 3 | 170 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 320 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nnwl | 3 | 140 | 25 | 350 | 350 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 320 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nnwq | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nnwr | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 320 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nnws | 5 | 255 | 50 | 750 | 750 | 0.5 | - | - | - | - | 3 | 2 | 3 | 320 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| noga | 11 | 600 | 150 | 3300 | 3300 | 0.5 | - | - | - | - | 4 | 2 | 4 | 320 | 0 | 0 | 40 | 40 | 1400 | 800 | 64 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nogl | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nogm | 5 | 240 | 0 | 850 | 850 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nogn | 7 | 360 | 40 | 1060 | 1060 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nogo | 6 | 300 | 0 | 1060 | 1060 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nogr | 3 | 120 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nomg | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nowb | 4 | 155 | 0 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,D,G | _ |  |
| nowe | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 3 | 2 | 3 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,D,G | _ |  |
| nowk | 8 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,D,G | _ |  |
| npfl | 3 | 125 | 0 | 390 | 390 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 50 | 50 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | C,O,K | _ |  |
| npfm | 7 | 255 | 50 | 950 | 950 | 0.5 | 300 | 300 | 300 | 0.75 | 4 | 2 | 4 | 350 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | C,O,K | _ |  |
| nplb | 4 | 155 | 0 | 475 | 475 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nplg | 6 | 290 | 80 | 900 | 900 | 0.5 | - | - | - | - | 3 | 2 | 3 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nqbh | 3 | 170 | 20 | 375 | 375 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nrdk | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,V,Q | _ |  |
| nrdr | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,V,Q | _ |  |
| nrel | 2 | 135 | 0 | 300 | 300 | 0.5 | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 1 | 1 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nrog | 3 | 120 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nrvd | 9 | 435 | 150 | 1500 | 1500 | 1.5 | 600 | 600 | 600 | 1.5 | 5 | 2 | 5 | 320 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nrvf | 3 | 170 | 20 | 340 | 340 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nrvi | 8 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 270 | 0 | 0 | 80 | 80 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | N,G | _ |  |
| nrvl | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nrvs | 4 | 205 | 30 | 450 | 450 | 0.5 | 300 | 300 | 300 | 0.75 | 1 | 2 | 1 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | N,G | _ |  |
| nrwm | 10 | 595 | 200 | 2200 | 2200 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | L,V,Q | _ |  |
| nrzb | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nrzg | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nrzm | 5 | 280 | 60 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nrzs | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nrzt | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nsat | 1 | 85 | 0 | 240 | 240 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nsbm | 6 | 290 | 80 | 750 | 750 | 0.5 | - | - | - | - | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nsc2 | 3 | 120 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsc3 | 5 | 240 | 0 | 850 | 850 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nscb | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsel | 5 | 135 | 0 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 220 | 0 | 0 | 1 | 1 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsgb | 8 | 210 | 85 | 1000 | 1000 | 0.5 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 200 | 0 | 0 | 45 | 45 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsgg | 9 | 435 | 150 | 1900 | 1900 | 0.5 | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D,G | _ |  |
| nsgh | 5 | 210 | 85 | 725 | 725 | 0.5 | - | - | - | - | 0 | 2 | 0 | 200 | 0 | 0 | 45 | 45 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsgn | 3 | 210 | 85 | 350 | 350 | 0.5 | - | - | - | - | 0 | 2 | 0 | 200 | 0 | 0 | 45 | 45 | 1200 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsgt | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nska | 1 | 85 | 0 | 180 | 180 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | D,N,G | _ |  |
| nske | 1 | 85 | 0 | 180 | 180 | 2 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | D,N,G | _ |  |
| nskf | 3 | 180 | 25 | 300 | 300 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nskg | 3 | 130 | 0 | 380 | 380 | 0.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 20 | 20 | 800 | 600 | 47 | 0 | - | - | - | - | - | - | _ | D,G | _ |  |
| nskm | 3 | 170 | 20 | 300 | 300 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | N,G | _ |  |
| nsko | 3 | 70 | 0 | 375 | 375 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nslf | 3 | 205 | 30 | 340 | 340 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | C,D,G | _ |  |
| nslh | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nsll | 10 | 595 | 200 | 1800 | 1800 | 2 | 700 | 700 | 700 | 1.75 | 6 | 2 | 6 | 270 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nslm | 1 | 85 | 0 | 240 | 240 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | C,D,G | _ |  |
| nsln | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | C,D,G | _ |  |
| nslr | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nslv | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D | _ |  |
| nsns | 3 | 255 | 50 | 375 | 375 | 0.5 | 300 | 300 | 300 | 0.75 | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsoc | 8 | 70 | 0 | 1100 | 1100 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsog | 5 | 240 | 0 | 850 | 850 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 18 | 18 | 800 | 600 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nspb | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nspd | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I,N | _ |  |
| nspg | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| nspp | 2 | 0 | 0 | 200 | 200 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 20 | 20 | 1200 | 800 | 16 | 0 | - | - | - | - | - | - | _ | B | _ |  |
| nspr | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nsqa | 9 | 435 | 150 | 1200 | 1200 | 1.5 | 600 | 600 | 600 | 1.5 | 5 | 2 | 5 | 320 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | V,Q | _ |  |
| nsqe | 6 | 290 | 80 | 950 | 950 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | V,Q | _ |  |
| nsqo | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | V,Q | _ |  |
| nsqt | 5 | 255 | 50 | 750 | 750 | 0.5 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | V,Q | _ |  |
| nsra | 1 | 85 | 5 | 240 | 240 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsrh | 3 | 145 | 20 | 340 | 340 | 0.5 | 200 | 200 | 200 | 0.5 | 2 | 2 | 2 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsrn | 6 | 145 | 20 | 675 | 675 | 0.5 | 350 | 350 | 350 | 0.875 | 2 | 2 | 2 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsrv | 5 | 205 | 30 | 900 | 900 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsrw | 9 | 145 | 20 | 1500 | 1500 | 0.5 | 500 | 500 | 500 | 1.25 | 2 | 2 | 2 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nssp | 3 | 170 | 20 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nsth | 9 | 385 | 120 | 1100 | 1100 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nstl | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nsts | 3 | 205 | 30 | 340 | 340 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nstw | 9 | 435 | 150 | 1500 | 1500 | 1.5 | 600 | 600 | 600 | 1.5 | 5 | 2 | 5 | 270 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,A,C,G | _ |  |
| nsty | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A,C | _ |  |
| nthl | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | B,A,C,G | _ |  |
| ntka | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntkc | 7 | 340 | 100 | 950 | 950 | 0.5 | - | - | - | - | 4 | 2 | 4 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntkf | 2 | 135 | 0 | 250 | 250 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntkh | 3 | 170 | 20 | 300 | 300 | 0.5 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntks | 5 | 155 | 10 | 475 | 475 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntkt | 4 | 225 | 40 | 475 | 475 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntkw | 4 | 205 | 30 | 475 | 475 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| ntrd | 10 | 595 | 200 | 2000 | 2000 | 0.5 | 600 | 600 | 600 | 1.5 | 4 | 2 | 4 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 64 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntrg | 7 | 205 | 30 | 1250 | 1250 | 0.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntrh | 1 | 85 | 0 | 220 | 220 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntrs | 2 | 205 | 30 | 250 | 250 | 0.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntrt | 4 | 235 | 35 | 375 | 375 | 0.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntrv | 3 | 205 | 30 | 375 | 375 | 0.5 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 40 | 40 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ntws | 2 | 85 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nubk | 2 | 120 | 10 | 250 | 250 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nubr | 4 | 205 | 30 | 475 | 475 | 0.5 | - | - | - | - | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nubw | 5 | 205 | 30 | 600 | 600 | 0.5 | 200 | 200 | 200 | 0.5 | 1 | 2 | 1 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nvde | 9 | 85 | 0 | 1500 | 1500 | 1 | 500 | 500 | 500 | 1.25 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nvdg | 6 | 240 | 45 | 750 | 750 | 1 | 400 | 400 | 400 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nvdl | 1 | 85 | 0 | 240 | 240 | 1 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nvdw | 3 | 125 | 15 | 365 | 365 | 1 | 200 | 200 | 200 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nwen | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 320 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwiz | 1 | 85 | 0 | 180 | 180 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nwld | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| nwlg | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 47 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| nwlt | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | A | _ |  |
| nwna | 9 | 435 | 150 | 1200 | 1200 | 1.5 | 600 | 600 | 600 | 1.5 | 5 | 2 | 5 | 320 | 0 | 0 | 90 | 90 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwnr | 6 | 290 | 80 | 950 | 950 | 0.5 | - | - | - | - | 3 | 2 | 3 | 320 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwns | 7 | 340 | 100 | 950 | 950 | 0.5 | 500 | 500 | 500 | 1.25 | 3 | 2 | 3 | 320 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwrg | 6 | 290 | 80 | 1000 | 1000 | 0.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | D,G | _ |  |
| nws1 | 5 | 280 | 70 | 700 | 700 | 0.25 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 45 | 45 | 1600 | 900 | 48 | 0 | - | - | - | - | - | - | _ | * | Rhla |  |
| nwwd | 6 | 290 | 80 | 750 | 750 | 0.5 | 400 | 400 | 400 | 1 | 2 | 2 | 2 | 350 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwwf | 2 | 120 | 10 | 300 | 300 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwwg | 4 | 205 | 30 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 47 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nwzd | 8 | 385 | 120 | 1200 | 1200 | 1 | 500 | 500 | 500 | 1.25 | 4 | 2 | 4 | 320 | 0 | 0 | 80 | 80 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nwzg | 5 | 255 | 50 | 600 | 600 | 0.5 | 400 | 400 | 400 | 1 | 1 | 2 | 1 | 270 | 0 | 0 | 50 | 50 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nwzr | 3 | 170 | 20 | 340 | 340 | 0.5 | 300 | 300 | 300 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| nzep | 1 | 240 | 60 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nzom | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nchp | 2 | 135 | 10 | 370 | 370 | 0.25 | 400 | 400 | 200 | 1.33333333333333 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nhym | 2 | 155 | 20 | 405 | 405 | 0.25 | 400 | 400 | 200 | 1.33333333333333 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ntor | 4 | 0 | 0 | 500 | 500 | 0.5 | - | - | - | - | 0 | 2 | 0 | 60 | 60 | 0 | 20 | 20 | 1200 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nalb | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| ncrb | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nder | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | A,V,Q | _ |  |
| ndog | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | Y,X,V,Q,J | _ |  |
| ndwm | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nfbr | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| nhmc | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| now2 | - | 0 | 0 | 40 | 40 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 1 | 1 | 1500 | 1500 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| now3 | - | 0 | 0 | 40 | 40 | 0.5 | - | - | - | - | 0 | 2 | 0 | 400 | 0 | 0 | 1 | 1 | 2200 | 2200 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nowl | - | 0 | 0 | 40 | 40 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 1 | 1 | 1000 | 1000 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| npig | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | L,F,W,B | _ |  |
| npng | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| npnw | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nrat | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | D,G | _ |  |
| nsea | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nsha | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | L,F,W,Y,X,V,Q,J | _ |  |
| nshe | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | L,F,W,Y,X,V,Q,J | _ |  |
| nshf | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | _ | _ |  |
| nshw | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | L,F,W,Y,X,V,Q,J | _ |  |
| nskk | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 200 | 200 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | Z | _ |  |
| nsno | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 100 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | I | _ |  |
| nvil | - | 0 | 0 | 60 | 60 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nvk2 | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nvl2 | - | 0 | 0 | 60 | 60 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nvlk | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nvlw | - | 0 | 0 | 60 | 60 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nvul | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 100 | 0 | 0 | 1 | 1 | 350 | 350 | 16 | 0 | - | - | - | - | - | - | _ | C | _ |  |
| ncb0 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb1 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb2 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb3 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb4 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb5 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb6 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb7 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb8 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncb9 | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncba | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncbb | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncbc | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncbd | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncbe | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncbf | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| ncnt | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| ncop | - | 0 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 200 | 200 | 16 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ncp2 | - | 0 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 300 | 300 | 16 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ncp3 | - | 0 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 400 | 400 | 16 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nct1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| nct2 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| ndch | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndh0 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndh1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndh2 | - | 400 | 195 | 1500 | 1500 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 180 | 3 | 1600 | 1200 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndh3 | - | 170 | 55 | 1250 | 1250 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 3 | 1600 | 1200 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndh4 | - | 100 | 140 | 1150 | 1150 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 70 | 3 | 1600 | 1200 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrg | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | A,C,Z | _ |  |
| ndrk | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | F | _ |  |
| ndro | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrr | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | L | _ |  |
| ndru | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | W,N,I | _ |  |
| ndrz | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| nfh0 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W,V,Q | _ |  |
| nfh1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W,V,Q | _ |  |
| nfoh | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfr1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | A,C | _ |  |
| nfr2 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | A,C | _ |  |
| ngad | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ngme | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ngnh | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngni | - | 0 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ngol | - | 0 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 60 | 60 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ngt2 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W | _ |  |
| ngwr | - | 0 | 0 | 400 | 400 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 600 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nhns | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| nmer | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | L | _ |  |
| nmg0 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | Z | _ |  |
| nmg1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | Z | _ |  |
| nmh0 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W,B,A,C,N | _ |  |
| nmh1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | L,F,W,B,A,C,N | _ |  |
| nmoo | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nmr0 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | V,Q | _ |  |
| nmr2 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | F | _ |  |
| nmr3 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | W | _ |  |
| nmr4 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| nmr5 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | A | _ |  |
| nmr6 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | C | _ |  |
| nmr7 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | N | _ |  |
| nmr8 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Y | _ |  |
| nmr9 | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | X,J | _ |  |
| nmra | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | D | _ |  |
| nmrb | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | G | _ |  |
| nmrc | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | Z | _ |  |
| nmrd | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | I | _ |  |
| nmre | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | O | _ |  |
| nmrf | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | K | _ |  |
| nmrk | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nnzg | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | I,N | _ |  |
| nshp | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ntav | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nten | - | 0 | 0 | 75 | 75 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nth0 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | N | _ |  |
| nth1 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | N | _ |  |
| ntn2 | - | 0 | 0 | 75 | 75 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ntnt | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| ntt2 | - | 0 | 0 | 125 | 125 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 0 | 0 | 50 | 1 | - | - | - | - | - | - | _ | B | _ |  |
| nwgt | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| Nman | 5 | 425 | 100 | 4250 | 5000 | 1 | 0 | 300 | 200 | 0.01 | 0 | 2 | 3.1 | 250 | 0 | 0 | 60 | 60 | 1800 | 800 | 48 | 0 | 30 | 20 | 17 | 3 | 2 | 1 | STR | * | _ | 22000.0 |
| Ecen | 5 | 425 | 100 | 2675 | 3100 | 3 | 55 | 325 | 200 | 2 | 0 | 0 | 2.5 | 400 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 17 | 18 | 15 | 4 | 5 | 0.6 | INT | * | _ | 13800.0 |
| Eevi | 5 | 425 | 100 | 100 | 625 | 0.5 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4.6 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 21 | 16 | 22 | 2.4 | 2.1 | 1.5 | AGI | * | _ | 1025.0 |
| Eevm | 5 | 425 | 100 | 100 | 750 | 1 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 26 | 16 | 20 | 2.4 | 2.1 | 1.5 | AGI | * | _ | 1150.0 |
| Efur | 5 | 425 | 0 | 100 | 525 | 0.5 | 0 | 270 | 200 | 0.01 | 2 | 2 | 4.5 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 17 | 18 | 15 | 1.8 | 2.7 | 1.5 | INT | * | _ | 925.0 |
| Eidm | 5 | 425 | 100 | 100 | 750 | 1 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 26 | 16 | 20 | 2.4 | 2.1 | 1.5 | AGI | * | _ | 1150.0 |
| Eill | 5 | 425 | 100 | 100 | 625 | 0.5 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4.6 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 21 | 16 | 22 | 2.4 | 2.1 | 1.5 | AGI | * | _ | 1025.0 |
| Eilm | 5 | 425 | 100 | 100 | 750 | 1 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 26 | 16 | 20 | 2.4 | 2.1 | 1.5 | AGI | * | _ | 1150.0 |
| Ekgg | 5 | 425 | 100 | 100 | 500 | 0.5 | 0 | 270 | 100 | 0.01 | 0 | 0 | 2.5 | 320 | 0 | 0 | 55 | 55 | 1800 | 900 | 32 | 0 | 16 | 18 | 15 | 1.8 | 2.7 | 1.5 | INT | * | _ | 900.0 |
| Emfr | 5 | 425 | 0 | 100 | 525 | 0.5 | 0 | 270 | 200 | 0.01 | 2 | 2 | 4.5 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 17 | 18 | 15 | 1.8 | 2.7 | 1.5 | INT | * | _ | 925.0 |
| Emns | 5 | 425 | 100 | 100 | 525 | 0.5 | 0 | 270 | 200 | 0.01 | 2 | 2 | 4.5 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 17 | 18 | 15 | 1.8 | 2.7 | 1.5 | INT | * | _ | 925.0 |
| Etyr | 5 | 425 | 100 | 100 | 550 | 0.5 | 0 | 225 | 200 | 0.01 | 0 | 0 | 3.7 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 18 | 15 | 19 | 1.9 | 2.6 | 1.5 | AGI | * | _ | 950.0 |
| Ewrd | 5 | 425 | 100 | 100 | 550 | 0.5 | 0 | 225 | 100 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 18 | 15 | 20 | 2.4 | 2 | 1.6 | AGI | * | _ | 950.0 |
| Hant | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 285 | 200 | 0.01 | 0 | 0 | 3.1 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 19 | 17 | 1.8 | 3.2 | 1 | INT | * | _ | 875.0 |
| Hapm | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Harf | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hart | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hdgo | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hhkl | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hjai | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 285 | 200 | 0.01 | 0 | 0 | 3.1 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 19 | 17 | 1.8 | 3.2 | 1 | INT | * | _ | 875.0 |
| Hkal | 5 | 425 | 100 | 100 | 500 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 2.5 | 270 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 16 | 19 | 15 | 2 | 3 | 1 | INT | * | _ | 900.0 |
| Hlgr | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 100 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hmbr | 5 | 425 | 100 | 100 | 700 | 0.25 | 0 | 225 | 200 | 0.01 | 1 | 0 | 2.3 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 24 | 15 | 11 | 3 | 1.5 | 1.5 | STR | * | _ | 1100.0 |
| Hmgd | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hpb1 | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hpb2 | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Huth | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Hvsh | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 300 | 100 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 15 | 20 | 14 | 2 | 3.4 | 1 | INT | * | _ | 875.0 |
| Hvwd | 5 | 425 | 100 | 100 | 550 | 0.25 | 0 | 225 | 200 | 0.01 | 0 | 0 | 3.7 | 350 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 15 | 19 | 1.9 | 2.6 | 1.5 | AGI | * | _ | 950.0 |
| Nbbc | 5 | 425 | 100 | 100 | 575 | 0.25 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4.9 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 19 | 16 | 23 | 2 | 2.25 | 1.75 | AGI | * | _ | 975.0 |
| Nklj | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Nkjx | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | O,K | _ | 1000.0 |
| Nmag | 5 | 425 | 100 | 100 | 725 | 0.5 | 0 | 240 | 200 | 0.01 | 0 | 0 | 1.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 16 | 13 | 3 | 1.8 | 1.2 | STR | * | _ | 1125.0 |
| Npld | 5 | 425 | 100 | 100 | 850 | 1 | 0 | 300 | 200 | 0.01 | 0 | 2 | 3.1 | 250 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 30 | 20 | 17 | 3 | 2 | 1 | STR | C | _ | 1250.0 |
| Nsjs | 5 | 425 | 100 | 100 | 675 | 0.25 | 0 | 225 | 100 | 0.01 | 1 | 0 | 2.6 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 23 | 15 | 12 | 3 | 1.5 | 1.5 | STR | * | _ | 1075.0 |
| Ocbh | 5 | 425 | 100 | 100 | 725 | 0.25 | 0 | 210 | 200 | 0.01 | 1 | 0 | 2 | 250 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 14 | 10 | 3.2 | 1.3 | 1.5 | STR | * | _ | 1125.0 |
| Ocb2 | 5 | 425 | 100 | 100 | 725 | 0.25 | 0 | 210 | 200 | 0.01 | 1 | 0 | 2 | 250 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 14 | 10 | 3.2 | 1.3 | 1.5 | STR | * | _ | 1125.0 |
| Odrt | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 3.4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 19 | 18 | 2 | 3 | 1 | INT | * | _ | 875.0 |
| Ogld | 5 | 425 | 100 | 100 | 650 | 0.25 | 0 | 255 | 100 | 0.01 | 2 | 0 | 3.9 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 22 | 17 | 13 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1050.0 |
| Ogrh | 5 | 425 | 100 | 100 | 575 | 0.25 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4.9 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 19 | 16 | 23 | 2 | 2.25 | 1.75 | AGI | * | _ | 975.0 |
| Opgh | 5 | 425 | 100 | 100 | 575 | 0.25 | 0 | 240 | 200 | 0.01 | 0 | 0 | 4.9 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 19 | 16 | 23 | 2 | 2.25 | 1.75 | AGI | * | _ | 975.0 |
| Orex | 5 | 425 | 100 | 100 | 725 | 0.5 | 0 | 270 | 100 | 0.01 | 1 | 0 | 3.2 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 48 | 0 | 25 | 18 | 14 | 2.9 | 2 | 1.3 | STR | * | _ | 1125.0 |
| Orkn | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 270 | 100 | 0.01 | 0 | 0 | 4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 15 | 18 | 20 | 2 | 2.5 | 1.5 | AGI | * | _ | 875.0 |
| Osam | 5 | 425 | 100 | 100 | 550 | 0.25 | 0 | 240 | 100 | 0.01 | 0 | 0 | 4.9 | 300 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 18 | 16 | 23 | 2 | 2.25 | 1.75 | AGI | * | _ | 950.0 |
| Otcc | 5 | 425 | 100 | 100 | 725 | 0.25 | 0 | 210 | 200 | 0.01 | 1 | 0 | 2 | 250 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 25 | 14 | 10 | 3.2 | 1.3 | 1.5 | STR | * | _ | 1125.0 |
| Othr | 5 | 425 | 100 | 100 | 500 | 0.25 | 0 | 285 | 200 | 0.01 | 0 | 0 | 3.4 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 16 | 19 | 18 | 2 | 3 | 1 | INT | * | _ | 900.0 |
| Uanb | 5 | 425 | 100 | 25 | 675 | 2 | 0 | 210 | 100 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 26 | 14 | 14 | 3.2 | 1.6 | 1.2 | STR | * | _ | 775.0 |
| Ubal | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Uclc | 5 | 425 | 100 | 100 | 450 | 1 | 0 | 300 | 200 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 14 | 20 | 14 | 1.6 | 3.4 | 1 | INT | * | _ | 850.0 |
| Udth | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Uear | 5 | 425 | 100 | 100 | 675 | 1 | 0 | 255 | 200 | 0.01 | 2 | 0 | 3.6 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 23 | 17 | 12 | 2.7 | 1.8 | 1.5 | STR | * | _ | 1075.0 |
| Uktl | 5 | 425 | 100 | 100 | 450 | 1 | 0 | 300 | 200 | 0.01 | 0 | 0 | 2.2 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 14 | 20 | 14 | 1.6 | 3.4 | 1 | INT | * | _ | 850.0 |
| Umal | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 2.5 | 1 | STR | * | _ | 1000.0 |
| Usyl | 5 | 425 | 100 | 100 | 550 | 0.5 | 0 | 225 | 100 | 0.01 | 0 | 0 | 3.7 | 320 | 0 | 0 | 40 | 40 | 1800 | 800 | 32 | 0 | 18 | 15 | 19 | 1.9 | 2.6 | 1.5 | AGI | * | _ | 950.0 |
| Utic | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Uvar | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Uvng | 5 | 425 | 100 | 100 | 600 | 1 | 0 | 270 | 200 | 0.01 | 0 | 0 | 2.8 | 270 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 20 | 18 | 16 | 2.5 | 4.5 | 1 | STR | * | _ | 1000.0 |
| Uwar | 5 | 425 | 100 | 2000 | 2525 | 3 | 0 | 300 | 200 | 2 | 0 | 2 | 3.1 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 48 | 0 | 21 | 20 | 17 | 3 | 4 | 3 | INT | * | _ | 10525.0 |
| Hgam | 5 | 425 | 100 | 100 | 450 | 0.25 | 0 | 285 | 100 | 0.01 | 0 | 0 | 3.1 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 32 | 0 | 14 | 19 | 17 | 1.8 | 3.2 | 1 | INT | * | _ | 850.0 |
| Naka | 5 | 425 | 100 | 100 | 475 | 0.25 | 0 | 285 | 100 | 0.01 | 1 | 2 | 5 | 320 | 0 | 0 | 55 | 55 | 1800 | 800 | 31 | 0 | 15 | 19 | 20 | 2 | 2.8 | 1.2 | INT | O,K | _ | 875.0 |
| eilw | 1 | 195 | 35 | 330 | 330 | - | - | - | - | - | 0 | 2 | 0 | 180 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| enec | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 10 | 10 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ensh | 3 | 195 | 20 | 700 | 700 | 1.5 | - | - | - | - | 3 | 2 | 3 | 350 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Resc,Reuv |  |
| eshd | 4 | 215 | 20 | 450 | 450 | 1 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Reib,Remk,Reuv |  |
| etrs | 2 | 170 | 50 | 1500 | 1500 | - | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hbew | 1 | 195 | 35 | 600 | 600 | - | - | - | - | - | 0 | 2 | 0 | 180 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hcth | 6 | 130 | 0 | 600 | 600 | 0.25 | - | - | - | - | 3 | 2 | 3 | 270 | 0 | 0 | 18 | 18 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme |  |
| hhdl | 1 | 240 | 40 | 100 | 100 | 0.25 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rhan |  |
| hhes | 2 | 135 | 0 | 420 | 420 | 0.25 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rhar,Rhme,Rhde,Rhpm,Rguv |  |
| hrdh | 1 | 240 | 40 | 100 | 100 | 0.25 | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nbee | 1 | 75 | 0 | 220 | 220 | 0.25 | - | - | - | - | 0 | 2 | 0 | 240 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhlh,Rguv |  |
| nbel | 2 | 155 | 20 | 750 | 750 | 0.25 | 250 | 250 | 100 | 0.625 | 5 | 2 | 5 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nbsp | 1 | 0 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | 40 | 0 | 0 | 60 | 60 | 900 | 800 | 48 | 0 | - | - | - | - | - | - | _ | N | _ |  |
| nchg | 4 | 200 | 0 | 780 | 780 | 1 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Robs,Ropg |  |
| nchr | 5 | 180 | 40 | 690 | 690 | 1 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Roen,Ropg |  |
| nchw | 2 | 220 | 0 | 360 | 360 | 1 | 400 | 400 | 75 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 22 | 22 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nckb | 4 | 255 | 60 | 900 | 900 | 1 | - | - | - | - | 1 | 2 | 1 | 220 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Rwdm |  |
| ncpn | 1 | 70 | 0 | 320 | 320 | 1 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Ropg |  |
| ndmu | 2 | 120 | 0 | 330 | 330 | 1 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 18 | 18 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | J | _ |  |
| ndrd | 5 | 260 | 40 | 525 | 525 | 0.5 | 300 | 300 | 150 | 0.75 | 0 | 2 | 0 | 320 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrf | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrh | 4 | 165 | 20 | 450 | 450 | 0.5 | 400 | 400 | 200 | 1 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrj | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | J | _ |  |
| ndrm | 2 | 155 | 15 | 280 | 280 | 0.5 | 200 | 200 | 75 | 0.8 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrp | 2 | 135 | 0 | 325 | 325 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrs | 6 | 210 | 30 | 775 | 775 | 1 | 500 | 500 | 250 | 1.5 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 28 | 1400 | 1000 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrw | 3 | 135 | 0 | 400 | 400 | 0.5 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrl | 1 | 75 | 0 | 280 | 280 | 1 | - | - | - | - | 0 | 2 | 0 | 200 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | O,K | Ropg |  |
| ndsa | 3 | 215 | 40 | 625 | 625 | 1 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 20 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrn | 5 | 225 | 15 | 900 | 900 | 1.5 | - | - | - | - | 4 | 2 | 4 | 270 | 0 | 0 | 25 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| ndrt | 5 | 270 | 30 | 465 | 465 | 1 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 25 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | O,K | _ |  |
| negz | 4 | 215 | 100 | 100 | 100 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nemi | 2 | 205 | 0 | 220 | 220 | 0.25 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 28 | 28 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rhpt |  |
| nfgl | 11 | 500 | 150 | 5600 | 5600 | 3 | 600 | 600 | 600 | 1.5 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | J | _ |  |
| ngbl | 6 | 375 | 100 | 600 | 600 | - | - | - | - | - | 3 | 2 | 3 | 240 | 0 | 0 | 50 | 50 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nhea | 2 | 130 | 10 | 310 | 310 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Resm,Rema,Reib,Remk,Reuv,Repm |  |
| nhef | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 10 | 10 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nhem | - | 0 | 0 | 15 | 15 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 10 | 10 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nhew | 1 | 75 | 0 | 220 | 220 | 0.25 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | Rhlh,Rguv |  |
| njks | 1 | 85 | 0 | 240 | 240 | 0.5 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nmdm | 2 | 185 | 0 | 280 | 280 | 0.5 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 240 | 0 | 0 | 22 | 22 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nmed | 2 | 185 | 0 | 280 | 280 | 0.5 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 240 | 0 | 0 | 22 | 22 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nmpe | 1 | 75 | 0 | 250 | 250 | 0.5 | - | - | - | - | 0 | 2 | 0 | 190 | 0 | 0 | 15 | 15 | 800 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nmsh | 4 | 155 | 0 | 475 | 475 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 40 | 40 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nser | 8 | 510 | 120 | 1250 | 1250 | 0.5 | - | - | - | - | 0 | 2 | 0 | 280 | 0 | 0 | 60 | 60 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nspc | - | 50 | 0 | 50 | 50 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nssn | 3 | 195 | 20 | 550 | 550 | 0.5 | - | - | - | - | 2 | 2 | 2 | 300 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nthr | 8 | 510 | 150 | 800 | 800 | 0.5 | - | - | - | - | 5 | 2 | 5 | 300 | 0 | 0 | 70 | 70 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nw2w | 2 | 215 | 0 | 250 | 250 | 0.25 | 300 | 300 | 75 | 0.75 | 0 | 2 | 0 | 270 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | Rost |  |
| nwat | 2 | 140 | 0 | 300 | 300 | 0.5 | 300 | 300 | 75 | 0.5 | 1 | 2 | 1 | 280 | 0 | 0 | 25 | 25 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| odkt | 6 | 145 | 20 | 675 | 675 | 0.5 | 350 | 350 | 350 | 0.875 | 2 | 2 | 2 | 270 | 0 | 0 | 30 | 30 | 1400 | 900 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ogrk | 3 | 200 | 0 | 700 | 700 | 0.25 | - | - | - | - | 1 | 2 | 1 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Robs,Ropg,Ropm,Rguv |  |
| ojgn | 6 | 500 | 200 | 1000 | 1000 | - | - | - | - | - | 5 | 2 | 5 | 270 | 0 | 0 | 35 | 35 | 1600 | 1200 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| omtg | 2 | 130 | 0 | 340 | 340 | 0.25 | - | - | - | - | 0 | 2 | 0 | 240 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome |  |
| onzg | 3 | 180 | 40 | 610 | 610 | 0.25 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 28 | 28 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome,Roen,Ropg,Ropm,Rguv |  |
| oosc | 1 | 195 | 35 | 330 | 330 | - | - | - | - | - | 0 | 2 | 0 | 180 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| oswy | 5 | 130 | 0 | 520 | 520 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ovlj | 2 | 145 | 25 | 315 | 315 | 0.25 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | Rowd,Rotr,Ropm,Rguv |  |
| owar | 2 | 130 | 0 | 340 | 340 | 0.25 | - | - | - | - | 0 | 2 | 0 | 240 | 0 | 0 | 20 | 20 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Roar,Rome |  |
| ownr | 5 | 130 | 0 | 520 | 520 | 0.5 | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 40 | 40 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| uabc | 4 | 240 | 70 | 1080 | 1080 | 2 | - | - | - | - | 2 | 2 | 2 | 270 | 0 | 0 | 45 | 45 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | Ruar,Rume,Rupc |  |
| uarb | 1 | 240 | 60 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 30 | 30 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ubdd | 6 | 385 | 120 | 1100 | 1100 | 2 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 80 | 80 | 1600 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ubdr | 10 | 595 | 200 | 1800 | 1800 | 2 | - | - | - | - | 6 | 2 | 6 | 300 | 0 | 0 | 100 | 100 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| ubot | 2 | 170 | 50 | 1500 | 1500 | - | - | - | - | - | 0 | 2 | 0 | 320 | 0 | 0 | 25 | 25 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| udes | 4 | 250 | 100 | 575 | 575 | - | - | - | - | - | 0 | 2 | 0 | 350 | 0 | 0 | 30 | 30 | 1500 | 1000 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| uktg | 2 | 195 | 0 | 280 | 280 | 0.5 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 240 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| uktn | 2 | 195 | 0 | 280 | 280 | 0.5 | 200 | 200 | 75 | 0.5 | 0 | 2 | 0 | 240 | 0 | 0 | 30 | 30 | 1400 | 800 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| uswb | 3 | 225 | 0 | 400 | 400 | 1 | 300 | 300 | 75 | 0.8 | 3 | 2 | 3 | 270 | 0 | 0 | 35 | 35 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| hprt | - | 0 | 0 | 100 | 100 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 600 | 600 | 16 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| nzlc | - | 0 | 0 | 100 | 100 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 1 | 1 | 600 | 600 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| haro | - | 240 | 100 | 1200 | 1200 | - | - | - | - | - | 5 | 0 | 5 | - | 0 | 0 | 80 | 600 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbfl | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbsm | - | 50 | 0 | 50 | 50 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbt1 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbt2 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbwd | - | 210 | 100 | 1100 | 1100 | - | - | - | - | - | 2 | 0 | 2 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ncap | 3 | 240 | 130 | 500 | 500 | 0.5 | - | - | - | - | 2 | 3 | 2 | 40 | 0 | 0 | 70 | 70 | 1900 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb |  |
| ncaw | 1 | 255 | 70 | 800 | 800 | 0.5 | - | - | - | - | 2 | 5 | 2 | 40 | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb |  |
| ncmw | - | 155 | 40 | 600 | 600 | - | 300 | 300 | 300 | 1.25 | 2 | 0 | 2 | - | 0 | 0 | 50 | 50 | 900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ncta | 1 | 270 | 80 | 2000 | 2000 | 0.5 | - | - | - | - | 2 | 3 | 2 | 40 | 0 | 0 | 60 | 60 | 900 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb |  |
| ncte | 1 | 300 | 120 | 2500 | 2500 | 0.5 | - | - | - | - | 2 | 3 | 2 | 40 | 0 | 0 | 80 | 80 | 900 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb |  |
| nctl | 1 | 385 | 0 | 1500 | 1500 | 0.5 | - | - | - | - | 2 | 3 | 2 | 40 | 0 | 0 | 110 | 110 | 900 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | Renb |  |
| ndfl | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ndgt | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ndke | - | 50 | 0 | 2000 | 2000 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 3 | 3 | 400 | 400 | 50 | 1 | - | - | - | - | - | - | _ | O | _ |  |
| ndkw | - | 50 | 0 | 2000 | 2000 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 3 | 3 | 400 | 400 | 50 | 1 | - | - | - | - | - | - | _ | O | _ |  |
| ndmg | - | 50 | 0 | 2000 | 2000 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 3 | 3 | 400 | 400 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ndrb | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 5 | 2 | 5 | - | 0 | 0 | 60 | 60 | 1900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | Rosp |  |
| ndt1 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ndt2 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef0 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef1 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef2 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef3 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef4 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef5 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef6 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nef7 | - | 70 | 20 | 500 | 500 | - | - | - | - | - | 1 | 1 | 1 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nefm | - | 70 | 20 | 400 | 400 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 40 | 40 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| negf | 1 | 120 | 60 | 550 | 550 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| negm | 1 | 120 | 60 | 450 | 450 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| negt | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| net1 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| net2 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfnp | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfrm | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfrt | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nft1 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nft2 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfv0 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfv1 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfv2 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfv3 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nfv4 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ngob | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nhcn | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nheb | - | 255 | 80 | 1200 | 1200 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nico | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | I | _ |  |
| nitb | - | 50 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nmgv | - | 50 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nnad | - | 255 | 100 | 900 | 900 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 1800 | 800 | 144 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nnfm | - | 115 | 40 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1200 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nnsa | - | 180 | 70 | 1050 | 1050 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 70 | 70 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nnsg | - | 205 | 60 | 1500 | 1500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 60 | 60 | 900 | 600 | 144 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nntg | 1 | 130 | 80 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 50 | 50 | 1800 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nntt | - | 385 | 150 | 1500 | 1500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 180 | 180 | 900 | 600 | 176 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| npgf | - | 115 | 40 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | Rosp |  |
| npgr | - | 50 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nshr | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ntt1 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ntx2 | 1 | 120 | 60 | 500 | 500 | - | - | - | - | - | 5 | 1 | 5 | - | 0 | 0 | 45 | 45 | 1900 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nvr0 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nvr1 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nvr2 | - | 50 | 0 | 1200 | 1200 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nwc1 | - | 50 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nwc2 | - | 50 | 0 | 500 | 500 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| ocbw | 1 | 160 | 70 | 600 | 600 | - | - | - | - | - | 2 | 0 | 2 | - | 0 | 0 | 60 | 60 | 800 | 800 | 72 | 1 | - | - | - | - | - | - | _ | * | Rosp,Rorb,Rgfo |  |
| nzin | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 500 | 500 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbse | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| nbsw | - | 50 | 0 | 150000 | 150000 | - | - | - | - | - | 0 | 2 | 0 | - | 0 | 0 | 3 | 3 | 900 | 800 | 50 | 1 | - | - | - | - | - | - | _ | * | _ |  |
| zcso | 6 | 215 | 10 | 540 | 540 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| zhyd | 7 | 280 | 30 | 700 | 700 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| zjug | 6 | 205 | 30 | 520 | 520 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| zmar | 6 | 215 | 0 | 520 | 520 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 32 | 0 | - | - | - | - | - | - | _ | * | Rhla,Rhra,Rhri |  |
| zshv | 10 | 680 | 200 | 850 | 850 | 2 | 600 | 600 | 600 | 0.8 | 0 | 2 | 0 | 270 | 0 | 0 | 20 | 20 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| zsmc | 6 | 425 | 150 | 500 | 500 | 1 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 48 | 0 | - | - | - | - | - | - | _ | * | _ |  |
| zzrg | 3 | 85 | 0 | 250 | 250 | 0.25 | - | - | - | - | 0 | 2 | 0 | 270 | 0 | 0 | 26 | 26 | 1400 | 800 | 31 | 0 | - | - | - | - | - | - | _ | * | _ |  |

## UnitWeapons (812 rows)

| unitWeapID | weapsOn | atkType1 | weapTp1 | cool1 | dice1 | sides1 | dmgplus1 | dmgUp1 | mindmg1 | avgdmg1 | maxdmg1 | rangeN1 | targCount1 | DPS | atkType2 | weapTp2 | cool2 | dice2 | sides2 | dmgplus2 | dmgUp2 | mindmg2 | avgdmg2 | maxdmg2 | rangeN2 | targCount2 | DPS2 | avg1_formula | dps1_formula | avg1_match | dps1_match | avg2_formula | dps2_formula | avg2_match | dps2_match |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Hamg | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| Hblm | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| Hmkg | 1 | hero | normal | 2.22 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.15315315315315 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1532 | true | true | 5.0000 |  | true |  |
| Hpal | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| hbot | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hbsh | 1 | siege | artillery | 2 | 3 | 10 | 75 | - | 78 | 91.5 | 105 | 900 | 1 | 45.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 91.5000 | 45.7500 | true | true | 0.0000 |  | false |  |
| hdes | 1 | normal | msplash | 1.5 | 1 | 15 | 54 | - | 55 | 62 | 69 | 600 | 1 | 41.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 62.0000 | 41.3333 | true | true | 0.0000 |  | false |  |
| hdhw | 1 | pierce | missile | 1.75 | 1 | 3 | 18 | - | 19 | 20 | 21 | 300 | 1 | 11.4285714285714 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.0000 | 11.4286 | true | true | 0.0000 |  | false |  |
| hfoo | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 90 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| hgry | 3 | magic | mline | 2.2 | 1 | 11 | 44 | - | 45 | 50 | 55 | 450 | 1 | 22.7272727272727 | magic | missile | 2.4 | 1 | 11 | 44 | - | 45 | 50 | 55 | 450 | 1 |  | 50.0000 | 22.7273 | true | true | 50.0000 |  | true |  |
| hgyr | 1 | pierce | instant | 2 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 500 | 1 | 3.75 | siege | missile | 2.5 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 100 | 1 |  | 7.5000 | 3.7500 | true | true | 7.5000 |  | true |  |
| hkni | 1 | normal | normal | 1.5 | 2 | 5 | 25 | - | 27 | 31 | 35 | 100 | 1 | 20.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 20.6667 | true | true | 0.0000 |  | false |  |
| hmil | 1 | normal | normal | 1.2 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 90 | 1 | 10.4166666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 10.4167 | true | true | 0.0000 |  | false |  |
| hmpr | 1 | magic | missile | 2 | 1 | 2 | 7 | - | 8 | 8.5 | 9 | 600 | 1 | 4.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.2500 | true | true | 0.0000 |  | false |  |
| hmtm | 3 | siege | artillery | 3.5 | 1 | 13 | 51 | - | 52 | 58 | 64 | 1150 | 1 | 16.5714285714286 | siege | missile | 3.5 | 1 | 13 | 51 | - | 52 | 58 | 64 | 1000 | 1 |  | 58.0000 | 16.5714 | true | true | 58.0000 |  | true |  |
| hmtt | 1 | siege | instant | 2.1 | 1 | 11 | 44 | - | 45 | 50 | 55 | 192 | 1 | 23.8095238095238 | pierce | missile | 2 | 1 | 2 | 12 | 11 | 13 | 13.5 | 14 | 500 | 1 |  | 50.0000 | 23.8095 | true | true | 13.5000 |  | true |  |
| hpea | 3 | normal | normal | 2 | 1 | 2 | 4 | - | 5 | 5.5 | 6 | 90 | 1 | 2.75 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 5.5000 | 2.7500 | true | true | 1.0000 |  | true |  |
| hphx | 1 | magic | msplash | 1.4 | 1 | 15 | 60 | - | 61 | 68 | 75 | 600 | 1 | 48.5714285714286 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 68.0000 | 48.5714 | true | true | 0.0000 |  | false |  |
| hpxe | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hrif | 1 | pierce | instant | 1.5 | 2 | 4 | 16 | - | 18 | 21 | 24 | 400 | 1 | 14 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 21.0000 | 14.0000 | true | true | 0.0000 |  | false |  |
| hrtt | 3 | siege | instant | 2.1 | 1 | 11 | 44 | - | 45 | 50 | 55 | 192 | 1 | 23.8095238095238 | pierce | missile | 2 | 1 | 2 | 12 | 11 | 13 | 13.5 | 14 | 500 | 1 |  | 50.0000 | 23.8095 | true | true | 13.5000 |  | true |  |
| hsor | 1 | magic | missile | 1.75 | 1 | 3 | 7 | - | 8 | 9 | 10 | 600 | 1 | 5.14285714285714 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.0000 | 5.1429 | true | true | 0.0000 |  | false |  |
| hspt | 1 | normal | missile | 1.9 | 1 | 3 | 12 | - | 13 | 14 | 15 | 250 | 1 | 7.36842105263158 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 7.3684 | true | true | 0.0000 |  | false |  |
| hwat | 1 | pierce | missile | 1.5 | 2 | 5 | 19 | - | 21 | 25 | 29 | 300 | 1 | 16.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 16.6667 | true | true | 0.0000 |  | false |  |
| hwt2 | 1 | pierce | missile | 1.5 | 2 | 5 | 29 | - | 31 | 35 | 39 | 300 | 1 | 23.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.0000 | 23.3333 | true | true | 0.0000 |  | false |  |
| hwt3 | 1 | pierce | missile | 1.5 | 2 | 5 | 39 | - | 41 | 45 | 49 | 300 | 1 | 30 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 45.0000 | 30.0000 | true | true | 0.0000 |  | false |  |
| halt | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| harm | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hars | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hatw | 1 | normal | missile | 1 | 1 | 3 | 7 | - | 8 | 9 | 10 | 700 | 1 | 9 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.0000 | 9.0000 | true | true | 0.0000 |  | false |  |
| hbar | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hbla | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hcas | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hctw | 3 | siege | artillery | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 | 40.2 | siege | missile | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 |  | 100.5000 | 40.2000 | true | true | 100.5000 |  | true |  |
| hgra | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hgtw | 1 | pierce | missile | 0.9 | 1 | 5 | 22 | - | 23 | 25 | 27 | 700 | 1 | 27.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 27.7778 | true | true | 0.0000 |  | false |  |
| hhou | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hkee | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hlum | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hshy | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| htow | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hvlt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hwtw | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| Obla | 1 | hero | normal | 1.77 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.34463276836158 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.3446 | true | true | 5.0000 |  | true |  |
| Ofar | 1 | hero | missile | 2.28 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.19298245614035 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.1930 | true | true | 5.0000 |  | true |  |
| Oshd | 1 | hero | missile | 2.28 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.19298245614035 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.1930 | true | true | 5.0000 |  | true |  |
| Otch | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 128 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| nwad | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| obot | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ocat | 3 | siege | artillery | 4.5 | 1 | 21 | 81 | - | 82 | 92 | 102 | 1150 | 1 | 20.4444444444444 | siege | missile | 4.5 | 1 | 21 | 81 | - | 82 | 92 | 102 | 1000 | 1 |  | 92.0000 | 20.4444 | true | true | 92.0000 |  | true |  |
| odes | 1 | normal | msplash | 1.5 | 1 | 15 | 54 | - | 55 | 62 | 69 | 600 | 1 | 41.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 62.0000 | 41.3333 | true | true | 0.0000 |  | false |  |
| odoc | 1 | magic | missile | 1.75 | 1 | 5 | 9 | - | 10 | 12 | 14 | 600 | 1 | 6.85714285714286 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.0000 | 6.8571 | true | true | 0.0000 |  | false |  |
| oeye | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ogru | 1 | normal | normal | 1.6 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 12.1875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 12.1875 | true | true | 0.0000 |  | false |  |
| ohun | 1 | pierce | missile | 2.31 | 1 | 5 | 22 | - | 23 | 25 | 27 | 450 | 1 | 10.8225108225108 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 10.8225 | true | true | 0.0000 |  | false |  |
| ohwd | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| okod | 1 | pierce | missile | 1.44 | 1 | 5 | 15 | - | 16 | 18 | 20 | 500 | 1 | 12.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.0000 | 12.5000 | true | true | 0.0000 |  | false |  |
| opeo | 3 | normal | normal | 3 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 90 | 1 | 2.5 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 7.5000 | 2.5000 | true | true | 1.0000 |  | true |  |
| orai | 1 | siege | normal | 1.85 | 1 | 5 | 22 | - | 23 | 25 | 27 | 100 | 1 | 13.5135135135135 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.5135 | true | true | 0.0000 |  | false |  |
| oshm | 1 | magic | missile | 2.1 | 1 | 2 | 7 | - | 8 | 8.5 | 9 | 600 | 1 | 4.04761904761905 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.0476 | true | true | 0.0000 |  | false |  |
| osp1 | 1 | pierce | missile | 1.5 | 1 | 3 | 10 | - | 11 | 12 | 13 | 600 | 1 | 8 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.0000 | 8.0000 | true | true | 0.0000 |  | false |  |
| osp2 | 1 | pierce | missile | 1.5 | 1 | 4 | 22 | - | 23 | 24.5 | 26 | 600 | 1 | 16.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 24.5000 | 16.3333 | true | true | 0.0000 |  | false |  |
| osp3 | 1 | pierce | missile | 1.5 | 1 | 5 | 40 | - | 41 | 43 | 45 | 600 | 1 | 28.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 43.0000 | 28.6667 | true | true | 0.0000 |  | false |  |
| osp4 | 1 | pierce | msplash | 1.5 | 1 | 5 | 40 | - | 41 | 43 | 45 | 600 | 1 | 28.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 43.0000 | 28.6667 | true | true | 0.0000 |  | false |  |
| ospm | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ospw | 1 | magic | missile | 1.75 | 1 | 6 | 16 | - | 17 | 19.5 | 22 | 400 | 1 | 11.1428571428571 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 11.1429 | true | true | 0.0000 |  | false |  |
| osw1 | 1 | normal | normal | 1 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 90 | 1 | 11.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 11.5000 | true | true | 0.0000 |  | false |  |
| osw2 | 1 | normal | normal | 1 | 1 | 2 | 15 | - | 16 | 16.5 | 17 | 90 | 1 | 16.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.5000 | 16.5000 | true | true | 0.0000 |  | false |  |
| osw3 | 1 | normal | normal | 1 | 1 | 2 | 20 | - | 21 | 21.5 | 22 | 90 | 1 | 21.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 21.5000 | 21.5000 | true | true | 0.0000 |  | false |  |
| otau | 1 | normal | normal | 1.9 | 1 | 7 | 29 | - | 30 | 33 | 36 | 100 | 1 | 17.3684210526316 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 33.0000 | 17.3684 | true | true | 0.0000 |  | false |  |
| otbk | 1 | pierce | missile | 2.31 | 1 | 5 | 22 | - | 23 | 25 | 27 | 450 | 1 | 10.8225108225108 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 10.8225 | true | true | 0.0000 |  | false |  |
| otbr | 1 | siege | missile | 1.8 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 300 | 1 | 6.38888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 6.3889 | true | true | 0.0000 |  | false |  |
| otot | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| owyv | 1 | pierce | missile | 2 | 2 | 5 | 37 | - | 39 | 43 | 47 | 450 | 1 | 21.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 43.0000 | 21.5000 | true | true | 0.0000 |  | false |  |
| oalt | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| obar | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| obea | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ofor | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ofrt | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ogre | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| oshy | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| osld | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ostr | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| otrb | 1 | pierce | missile | 4 | 1 | 8 | 33 | - | 34 | 37.5 | 41 | 700 | 1 | 9.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 37.5000 | 9.3750 | true | true | 0.0000 |  | false |  |
| otto | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ovln | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| owtw | 1 | pierce | missile | 0.6 | 1 | 3 | 15 | - | 16 | 17 | 18 | 800 | 1 | 28.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.0000 | 28.3333 | true | true | 0.0000 |  | false |  |
| Edem | 1 | hero | normal | 1.7 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.64705882352941 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.6471 | true | true | 5.0000 |  | true |  |
| Edmm | 1 | chaos | msplash | 1.6 | 2 | 12 | 0 | - | 2 | 13 | 24 | 600 | 1 | 8.125 | chaos | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 8.1250 | true | true | 5.0000 |  | true |  |
| Ekee | 1 | hero | missile | 2.18 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.29357798165138 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.2936 | true | true | 5.0000 |  | true |  |
| Emoo | 1 | hero | missile | 2.46 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 2.84552845528455 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 2.8455 | true | true | 5.0000 |  | true |  |
| Ewar | 1 | hero | normal | 2.05 | 2 | 11 | 0 | - | 2 | 12 | 22 | 100 | 1 | 5.85365853658537 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 12.0000 | 5.8537 | true | true | 5.0000 |  | true |  |
| earc | 1 | pierce | missile | 1.5 | 1 | 3 | 15 | - | 16 | 17 | 18 | 500 | 1 | 11.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.0000 | 11.3333 | true | true | 0.0000 |  | false |  |
| ebal | 1 | siege | aline | 3.5 | 1 | 12 | 38 | - | 39 | 44.5 | 50 | 1150 | 1 | 12.7142857142857 | siege | aline | 3.5 | 1 | 12 | 38 | - | 39 | 44.5 | 50 | 1150 | 1 |  | 44.5000 | 12.7143 | true | true | 44.5000 |  | true |  |
| ebsh | 1 | siege | artillery | 2 | 3 | 10 | 75 | - | 78 | 91.5 | 105 | 900 | 1 | 45.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 91.5000 | 45.7500 | true | true | 0.0000 |  | false |  |
| echm | 2 | siege | missile | 2.5 | 1 | 11 | 44 | - | 45 | 50 | 55 | 850 | 1 | 20 | magic | msplash | 2.5 | 1 | 17 | 66 | - | 67 | 75 | 83 | 450 | 1 |  | 50.0000 | 20.0000 | true | true | 75.0000 |  | true |  |
| edcm | 1 | normal | normal | 1.5 | 1 | 6 | 26 | - | 27 | 29.5 | 32 | 100 | 1 | 19.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.5000 | 19.6667 | true | true | 0.0000 |  | false |  |
| edes | 1 | normal | msplash | 1.5 | 1 | 15 | 54 | - | 55 | 62 | 69 | 600 | 1 | 41.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 62.0000 | 41.3333 | true | true | 0.0000 |  | false |  |
| edoc | 1 | normal | normal | 1.5 | 1 | 4 | 18 | - | 19 | 20.5 | 22 | 100 | 1 | 13.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 13.6667 | true | true | 0.0000 |  | false |  |
| edot | 1 | magic | missile | 1.6 | 1 | 3 | 10 | - | 11 | 12 | 13 | 600 | 1 | 7.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.0000 | 7.5000 | true | true | 0.0000 |  | false |  |
| edry | 1 | pierce | missile | 2 | 1 | 3 | 14 | - | 15 | 16 | 17 | 500 | 1 | 8 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.0000 | 8.0000 | true | true | 0.0000 |  | false |  |
| edtm | 1 | magic | missile | 1.75 | 1 | 7 | 34 | - | 35 | 38 | 41 | 600 | 1 | 21.7142857142857 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 38.0000 | 21.7143 | true | true | 0.0000 |  | false |  |
| efdr | 1 | pierce | missile | 1.75 | 1 | 2 | 12 | - | 13 | 13.5 | 14 | 300 | 1 | 7.71428571428571 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 13.5000 | 7.7143 | true | true | 0.0000 |  | false |  |
| efon | 1 | normal | normal | 1.75 | 1 | 3 | 14 | - | 15 | 16 | 17 | 100 | 1 | 9.14285714285714 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.0000 | 9.1429 | true | true | 0.0000 |  | false |  |
| ehip | 1 | normal | normal | 1.05 | 1 | 8 | 49 | - | 50 | 53.5 | 57 | 128 | 1 | 50.9523809523809 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 53.5000 | 50.9524 | true | true | 0.0000 |  | false |  |
| ehpr | 1 | pierce | missile | 1.1 | 1 | 3 | 15 | - | 16 | 17 | 18 | 400 | 1 | 15.4545454545455 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.0000 | 15.4545 | true | true | 0.0000 |  | false |  |
| emtg | 1 | normal | normal | 2.5 | 2 | 7 | 26 | - | 28 | 34 | 40 | 128 | 1 | 13.6 | siege | normal | 2.5 | 2 | 8 | 32 | - | 34 | 41 | 48 | 250 | 1 |  | 34.0000 | 13.6000 | true | true | 41.0000 |  | true |  |
| esen | 1 | normal | mbounce | 1.8 | 1 | 3 | 15 | - | 16 | 17 | 18 | 225 | 2 | 9.44444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.0000 | 9.4444 | true | true | 0.0000 |  | false |  |
| espv | 1 | normal | missile | 1.35 | 1 | 12 | 24 | - | 25 | 30.5 | 36 | 450 | 1 | 22.5925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 30.5000 | 22.5926 | true | true | 0.0000 |  | false |  |
| even | 1 | pierce | missile | 1.35 | 1 | 5 | 13 | - | 14 | 16 | 18 | 450 | 1 | 11.8518518518519 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.0000 | 11.8519 | true | true | 0.0000 |  | false |  |
| ewsp | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| eaoe | 1 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 | 18.2 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 |  | 45.5000 | 18.2000 | true | true | 45.5000 |  | true |  |
| eaom | 1 | normal | normal | 2.5 | 1 | 11 | 44 | - | 45 | 50 | 55 | 128 | 1 | 20 | normal | normal | 2.5 | 1 | 11 | 44 | - | 45 | 50 | 55 | 128 | 1 |  | 50.0000 | 20.0000 | true | true | 50.0000 |  | true |  |
| eaow | 1 | normal | normal | 2.5 | 1 | 9 | 37 | - | 38 | 42 | 46 | 128 | 1 | 16.8 | normal | normal | 2.5 | 1 | 9 | 37 | - | 38 | 42 | 46 | 128 | 1 |  | 42.0000 | 16.8000 | true | true | 42.0000 |  | true |  |
| eate | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| eden | 1 | normal | normal | 2.5 | 1 | 5 | 20 | - | 21 | 23 | 25 | 128 | 1 | 9.2 | normal | normal | 2.5 | 1 | 5 | 20 | - | 21 | 23 | 25 | 128 | 1 |  | 23.0000 | 9.2000 | true | true | 23.0000 |  | true |  |
| edob | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| edos | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| egol | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| emow | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| eshy | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| etoa | 1 | normal | normal | 2.5 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 128 | 1 | 21.8 | normal | normal | 2.5 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 128 | 1 |  | 54.5000 | 21.8000 | true | true | 54.5000 |  | true |  |
| etoe | 1 | normal | normal | 2.5 | 1 | 15 | 59 | - | 60 | 67 | 74 | 128 | 1 | 26.8 | normal | normal | 2.5 | 1 | 15 | 59 | - | 60 | 67 | 74 | 128 | 1 |  | 67.0000 | 26.8000 | true | true | 67.0000 |  | true |  |
| etol | 1 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 | 18.2 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 |  | 45.5000 | 18.2000 | true | true | 45.5000 |  | true |  |
| etrp | 3 | normal | normal | 1.5 | 1 | 8 | 25 | - | 26 | 29.5 | 33 | 128 | 1 | 19.6666666666667 | pierce | msplash | 2 | 1 | 10 | 44 | - | 45 | 49.5 | 54 | 700 | 1 |  | 29.5000 | 19.6667 | true | true | 49.5000 |  | true |  |
| Ucrl | 1 | hero | normal | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 128 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| Udea | 1 | hero | normal | 2.33 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.00429184549356 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.0043 | true | true | 5.0000 |  | true |  |
| Udre | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Ulic | 1 | hero | missile | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| uabo | 1 | normal | normal | 1.9 | 1 | 7 | 32 | - | 33 | 36 | 39 | 128 | 1 | 18.9473684210526 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.0000 | 18.9474 | true | true | 0.0000 |  | false |  |
| uaco | 1 | normal | normal | 2.5 | 1 | 2 | 8 | - | 9 | 9.5 | 10 | 90 | 1 | 3.8 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.5000 | 3.8000 | true | true | 0.0000 |  | false |  |
| uban | 1 | magic | missile | 1.5 | 1 | 5 | 8 | - | 9 | 11 | 13 | 500 | 1 | 7.33333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.0000 | 7.3333 | true | true | 0.0000 |  | false |  |
| ubsp | 1 | magic | msplash | 1.35 | 1 | 4 | 20 | - | 21 | 22.5 | 24 | 450 | 1 | 16.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 22.5000 | 16.6667 | true | true | 0.0000 |  | false |  |
| ucrm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ucry | 1 | pierce | missile | 2 | 1 | 6 | 27 | - | 28 | 30.5 | 33 | 550 | 1 | 15.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 30.5000 | 15.2500 | true | true | 0.0000 |  | false |  |
| ucs1 | 1 | normal | normal | 1.5 | 1 | 2 | 7 | - | 8 | 8.5 | 9 | 90 | 1 | 5.66666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 5.6667 | true | true | 0.0000 |  | false |  |
| ucs2 | 1 | normal | normal | 1.5 | 1 | 4 | 14 | - | 15 | 16.5 | 18 | 90 | 1 | 11 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.5000 | 11.0000 | true | true | 0.0000 |  | false |  |
| ucs3 | 1 | normal | normal | 1.5 | 1 | 6 | 21 | - | 22 | 24.5 | 27 | 90 | 1 | 16.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 24.5000 | 16.3333 | true | true | 0.0000 |  | false |  |
| ucsB | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ucsC | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ufro | 3 | magic | msplash | 3 | 2 | 11 | 83 | - | 85 | 95 | 105 | 300 | 1 | 31.6666666666667 | magic | msplash | 3 | 1 | 11 | 83 | - | 84 | 89 | 94 | 300 | 1 |  | 95.0000 | 31.6667 | true | true | 89.0000 |  | true |  |
| ugar | 3 | normal | normal | 1.4 | 1 | 10 | 60 | - | 61 | 65.5 | 70 | 128 | 1 | 46.7857142857143 | pierce | missile | 2.2 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 300 | 1 |  | 65.5000 | 46.7857 | true | true | 19.5000 |  | true |  |
| ugho | 3 | normal | normal | 1.3 | 2 | 2 | 10 | - | 12 | 13 | 14 | 90 | 1 | 10 | normal | normal | 1.35 | 2 | 1 | 0 | - | 2 | 2 | 2 | 66 | 1 |  | 13.0000 | 10.0000 | true | true | 2.0000 |  | true |  |
| ugrm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| uloc | 1 | spells | missile | 1 | 1 | 2 | 12 | - | 13 | 13.5 | 14 | 10 | 1 | 13.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 13.5000 | 13.5000 | true | true | 0.0000 |  | false |  |
| umtw | 3 | siege | artillery | 4 | 1 | 18 | 70 | - | 71 | 79.5 | 88 | 1150 | 1 | 19.875 | siege | missile | 4 | 1 | 18 | 70 | - | 71 | 79.5 | 88 | 1000 | 1 |  | 79.5000 | 19.8750 | true | true | 79.5000 |  | true |  |
| unec | 1 | magic | missile | 1.8 | 1 | 2 | 7 | - | 8 | 8.5 | 9 | 600 | 1 | 4.72222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.7222 | true | true | 0.0000 |  | false |  |
| uobs | 1 | magic | missile | 2.1 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 575 | 1 | 3.57142857142857 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 7.5000 | 3.5714 | true | true | 0.0000 |  | false |  |
| uplg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ushd | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| uske | 1 | normal | normal | 2 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 90 | 1 | 7.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 7.2500 | true | true | 0.0000 |  | false |  |
| uskm | 1 | pierce | missile | 1.5 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 500 | 1 | 7.66666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 7.6667 | true | true | 0.0000 |  | false |  |
| uubs | 1 | siege | artillery | 2 | 3 | 10 | 75 | - | 78 | 91.5 | 105 | 900 | 1 | 45.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 91.5000 | 45.7500 | true | true | 0.0000 |  | false |  |
| uaod | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ubon | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ugol | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ugrv | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| unp1 | 1 | pierce | missile | 1 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 800 | 1 | 45.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 45.5000 | 45.5000 | true | true | 0.0000 |  | false |  |
| unp2 | 1 | pierce | missile | 1 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 800 | 1 | 54.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 54.5000 | 54.5000 | true | true | 0.0000 |  | false |  |
| unpl | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| usap | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| usep | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ushp | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| uslh | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| utod | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| utom | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| uzg1 | 1 | pierce | missile | 1 | 1 | 6 | 26 | - | 27 | 29.5 | 32 | 700 | 1 | 29.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.5000 | 29.5000 | true | true | 0.0000 |  | false |  |
| uzg2 | 1 | normal | missile | 1 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 700 | 1 | 11.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 11.5000 | true | true | 0.0000 |  | false |  |
| uzig | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| Nbrn | 1 | hero | missile | 2.42 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 2.89256198347107 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 2.8926 | true | true | 5.0000 |  | true |  |
| Nbst | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 128 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| nnht | 3 | pierce | msplash | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | pierce | msplash | 1.8 | 1 | 4 | 24 | - | 25 | 26.5 | 28 | 300 | 1 |  | 28.5000 | 15.8333 | true | true | 26.5000 |  | true |  |
| npfl | 1 | chaos | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| ndr1 | 1 | normal | normal | 2 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 90 | 1 | 7.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 7.2500 | true | true | 0.0000 |  | false |  |
| ndr2 | 1 | normal | normal | 2 | 1 | 3 | 14 | - | 15 | 16 | 17 | 90 | 1 | 8 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.0000 | 8.0000 | true | true | 0.0000 |  | false |  |
| ndr3 | 1 | normal | normal | 2 | 1 | 4 | 15 | - | 16 | 17.5 | 19 | 90 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.5000 | 8.7500 | true | true | 0.0000 |  | false |  |
| ngz1 | 1 | normal | normal | 1.5 | 1 | 3 | 18 | - | 19 | 20 | 21 | 128 | 1 | 13.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.0000 | 13.3333 | true | true | 0.0000 |  | false |  |
| ngz2 | 1 | normal | normal | 1.5 | 1 | 3 | 27 | - | 28 | 29 | 30 | 128 | 1 | 19.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.0000 | 19.3333 | true | true | 0.0000 |  | false |  |
| ngz3 | 1 | normal | normal | 1.5 | 1 | 3 | 37 | - | 38 | 39 | 40 | 128 | 1 | 26 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.0000 | 26.0000 | true | true | 0.0000 |  | false |  |
| ngzc | 1 | normal | normal | 1.5 | 1 | 4 | 27 | - | 28 | 29.5 | 31 | 128 | 1 | 19.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.5000 | 19.6667 | true | true | 0.0000 |  | false |  |
| ngzd | 1 | normal | normal | 1.5 | 1 | 4 | 37 | - | 38 | 39.5 | 41 | 128 | 1 | 26.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 26.3333 | true | true | 0.0000 |  | false |  |
| ngza | 1 | normal | normal | 1.5 | 1 | 4 | 45 | - | 46 | 47.5 | 49 | 128 | 1 | 31.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 47.5000 | 31.6667 | true | true | 0.0000 |  | false |  |
| ngz4 | 1 | normal | normal | 1.5 | 2 | 6 | 57 | - | 59 | 64 | 69 | 128 | 1 | 42.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 64.0000 | 42.6667 | true | true | 0.0000 |  | false |  |
| npn1 | 1 | normal | normal | 1.35 | 2 | 6 | 50 | - | 52 | 57 | 62 | 128 | 1 | 42.2222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 57.0000 | 42.2222 | true | true | 0.0000 |  | false |  |
| npn2 | 1 | pierce | missile | 1.5 | 2 | 4 | 35 | - | 37 | 40 | 43 | 500 | 1 | 26.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 40.0000 | 26.6667 | true | true | 0.0000 |  | false |  |
| npn3 | 1 | normal | normal | 1.35 | 2 | 4 | 35 | - | 37 | 40 | 43 | 128 | 1 | 29.6296296296296 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 40.0000 | 29.6296 | true | true | 0.0000 |  | false |  |
| npn4 | 1 | normal | normal | 1.35 | 3 | 6 | 75 | - | 78 | 85.5 | 93 | 128 | 1 | 63.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 85.5000 | 63.3333 | true | true | 0.0000 |  | false |  |
| npn5 | 1 | pierce | missile | 1.5 | 3 | 4 | 52 | - | 55 | 59.5 | 64 | 500 | 1 | 39.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 59.5000 | 39.6667 | true | true | 0.0000 |  | false |  |
| npn6 | 1 | normal | normal | 1.35 | 3 | 4 | 52 | - | 55 | 59.5 | 64 | 128 | 1 | 44.0740740740741 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 59.5000 | 44.0741 | true | true | 0.0000 |  | false |  |
| npng | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nqb2 | 1 | pierce | missile | 1.5 | 2 | 3 | 24 | - | 26 | 28 | 30 | 550 | 1 | 18.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.0000 | 18.6667 | true | true | 0.0000 |  | false |  |
| nqb3 | 1 | pierce | msplash | 1.5 | 3 | 3 | 36 | - | 39 | 42 | 45 | 550 | 1 | 28 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 42.0000 | 28.0000 | true | true | 0.0000 |  | false |  |
| nqb4 | 1 | pierce | msplash | 1.5 | 4 | 3 | 48 | - | 52 | 56 | 60 | 550 | 1 | 37.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 56.0000 | 37.3333 | true | true | 0.0000 |  | false |  |
| nqbh | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| nwe1 | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nwe2 | 1 | magic | msplash | 1.5 | 1 | 5 | 20 | - | 21 | 23 | 25 | 300 | 1 | 15.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 15.3333 | true | true | 0.0000 |  | false |  |
| nwe3 | 1 | magic | msplash | 1.5 | 3 | 4 | 45 | - | 48 | 52.5 | 57 | 300 | 1 | 35 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.5000 | 35.0000 | true | true | 0.0000 |  | false |  |
| nadk | 3 | pierce | msplash | 1.8 | 1 | 12 | 30 | - | 31 | 36.5 | 42 | 500 | 1 | 20.2777777777778 | pierce | msplash | 1.8 | 1 | 8 | 40 | - | 41 | 44.5 | 48 | 300 | 1 |  | 36.5000 | 20.2778 | true | true | 44.5000 |  | true |  |
| nadr | 3 | chaos | msplash | 1.5 | 3 | 12 | 42 | - | 45 | 61.5 | 78 | 500 | 1 | 41 | chaos | msplash | 1.5 | 3 | 9 | 58 | - | 61 | 73 | 85 | 300 | 1 |  | 61.5000 | 41.0000 | true | true | 73.0000 |  | true |  |
| nadw | 3 | pierce | msplash | 1.8 | 1 | 8 | 22 | - | 23 | 26.5 | 30 | 600 | 1 | 14.7222222222222 | pierce | msplash | 1.8 | 1 | 4 | 20 | - | 21 | 22.5 | 24 | 300 | 1 |  | 26.5000 | 14.7222 | true | true | 22.5000 |  | true |  |
| nahy | 1 | chaos | missile | 1.5 | 2 | 11 | 75 | - | 77 | 87 | 97 | 500 | 1 | 58 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 87.0000 | 58.0000 | true | true | 0.0000 |  | false |  |
| nanb | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 600 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| nanm | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 600 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| nanc | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nane | 1 | pierce | missile | 1.6 | 1 | 6 | 23 | - | 24 | 26.5 | 29 | 500 | 1 | 16.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 26.5000 | 16.5625 | true | true | 0.0000 |  | false |  |
| nano | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nanw | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| narg | 1 | normal | normal | 1.35 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 100 | 1 | 8.51851851851852 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 8.5185 | true | true | 0.0000 |  | false |  |
| nass | 1 | pierce | missile | 1.6 | 1 | 6 | 24 | - | 25 | 27.5 | 30 | 500 | 1 | 17.1875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 27.5000 | 17.1875 | true | true | 0.0000 |  | false |  |
| nbal | 3 | chaos | msplash | 1.8 | 1 | 8 | 40 | - | 41 | 44.5 | 48 | 600 | 1 | 24.7222222222222 | chaos | normal | 1.35 | 1 | 8 | 34 | - | 35 | 35 | 38.5 | 128 | 1 |  | 44.5000 | 24.7222 | true | true | 38.5000 |  | false |  |
| nban | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nbda | 1 | normal | normal | 1.8 | 1 | 7 | 27 | - | 28 | 31 | 34 | 100 | 1 | 17.2222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 17.2222 | true | true | 0.0000 |  | false |  |
| nbdk | 3 | pierce | msplash | 1.8 | 1 | 12 | 33 | - | 34 | 39.5 | 45 | 500 | 1 | 21.9444444444444 | pierce | msplash | 1.8 | 1 | 8 | 48 | - | 49 | 52.5 | 56 | 300 | 1 |  | 39.5000 | 21.9444 | true | true | 52.5000 |  | true |  |
| nbdm | 1 | normal | normal | 1.35 | 1 | 5 | 20 | - | 21 | 23 | 25 | 100 | 1 | 17.037037037037 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 17.0370 | true | true | 0.0000 |  | false |  |
| nbdo | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nbdr | 3 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | pierce | missile | 1.8 | 1 | 4 | 24 | - | 25 | 26.5 | 28 | 300 | 1 |  | 28.5000 | 15.8333 | true | true | 26.5000 |  | true |  |
| nbds | 1 | normal | normal | 1.35 | 1 | 6 | 28 | - | 29 | 31.5 | 34 | 100 | 1 | 23.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.5000 | 23.3333 | true | true | 0.0000 |  | false |  |
| nbdw | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nbld | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nbnb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbot | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbrg | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nbwm | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | missile | 1.5 | 3 | 9 | 67 | - | 70 | 82 | 94 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 82.0000 |  | true |  |
| nbzd | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | msplash | 1.5 | 3 | 9 | 67 | - | 70 | 82 | 94 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 82.0000 |  | true |  |
| nbzk | 3 | pierce | msplash | 1.8 | 1 | 12 | 33 | - | 34 | 39.5 | 45 | 500 | 1 | 21.9444444444444 | pierce | msplash | 1.8 | 1 | 8 | 48 | - | 49 | 52.5 | 56 | 300 | 1 |  | 39.5000 | 21.9444 | true | true | 52.5000 |  | true |  |
| nbzw | 3 | pierce | msplash | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | pierce | msplash | 1.8 | 1 | 4 | 24 | - | 25 | 26.5 | 28 | 300 | 1 |  | 28.5000 | 15.8333 | true | true | 26.5000 |  | true |  |
| ncea | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| ncen | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - |  | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| ncer | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 9.25925925925926 | normal | _ | - | - | - |  | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| ncfs | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| ncim | 1 | pierce | missile | 1.6 | 1 | 6 | 24 | - | 25 | 27.5 | 30 | 500 | 1 | 17.1875 | normal | _ | - | - | - |  | - | - | - | - | - | 1 |  | 27.5000 | 17.1875 | true | true | 0.0000 |  | false |  |
| ncks | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 600 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| ncnk | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| ndqn | 1 | normal | normal | 1.35 | 1 | 3 | 14 | - | 15 | 16 | 17 | 100 | 1 | 11.8518518518519 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.0000 | 11.8519 | true | true | 0.0000 |  | false |  |
| ndqp | 1 | chaos | missile | 1.35 | 1 | 7 | 53 | - | 54 | 57 | 60 | 500 | 1 | 42.2222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 57.0000 | 42.2222 | true | true | 0.0000 |  | false |  |
| ndqs | 1 | chaos | normal | 1.35 | 1 | 9 | 68 | - | 69 | 73 | 77 | 100 | 1 | 54.0740740740741 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 73.0000 | 54.0741 | true | true | 0.0000 |  | false |  |
| ndqt | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| ndqv | 1 | pierce | missile | 1.6 | 1 | 7 | 30 | - | 31 | 34 | 37 | 500 | 1 | 21.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 34.0000 | 21.2500 | true | true | 0.0000 |  | false |  |
| ndrv | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| ndtb | 1 | pierce | missile | 1.6 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 20.3125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 20.3125 | true | true | 0.0000 |  | false |  |
| ndth | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| ndtp | 1 | pierce | missile | 1.8 | 1 | 8 | 16 | - | 17 | 20.5 | 24 | 600 | 1 | 11.3888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 11.3889 | true | true | 0.0000 |  | false |  |
| ndtr | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| ndtt | 1 | pierce | missile | 1.6 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| ndtw | 1 | pierce | missile | 1.6 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 500 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| nehy | 1 | chaos | msplash | 1.6 | 1 | 10 | 34 | - | 35 | 39.5 | 44 | 600 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| nelb | 1 | chaos | missile | 1.8 | 1 | 9 | 38 | - | 39 | 43 | 47 | 300 | 1 | 23.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 43.0000 | 23.8889 | true | true | 0.0000 |  | false |  |
| nele | 1 | pierce | missile | 1.8 | 1 | 9 | 16 | - | 17 | 21 | 25 | 300 | 1 | 11.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 21.0000 | 11.6667 | true | true | 0.0000 |  | false |  |
| nenc | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nenf | 1 | normal | normal | 1.35 | 1 | 6 | 28 | - | 29 | 31.5 | 34 | 100 | 1 | 23.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.5000 | 23.3333 | true | true | 0.0000 |  | false |  |
| nenp | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nepl | 1 | normal | normal | 1.35 | 1 | 5 | 31 | - | 32 | 34 | 36 | 100 | 1 | 25.1851851851852 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 34.0000 | 25.1852 | true | true | 0.0000 |  | false |  |
| nerd | 1 | chaos | msplash | 1.8 | 1 | 7 | 27 | - | 28 | 31 | 34 | 600 | 1 | 17.2222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 17.2222 | true | true | 0.0000 |  | false |  |
| ners | 1 | chaos | msplash | 1.8 | 1 | 4 | 20 | - | 21 | 22.5 | 24 | 600 | 1 | 12.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 22.5000 | 12.5000 | true | true | 0.0000 |  | false |  |
| nerw | 1 | chaos | msplash | 1.8 | 1 | 7 | 48 | - | 49 | 52 | 55 | 600 | 1 | 28.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 28.8889 | true | true | 0.0000 |  | false |  |
| nfel | 1 | chaos | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nfgb | 1 | chaos | normal | 1.35 | 1 | 4 | 22 | - | 23 | 24.5 | 26 | 100 | 1 | 18.1481481481481 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 24.5000 | 18.1481 | true | true | 0.0000 |  | false |  |
| nfgo | 1 | chaos | msplash | 2 | 2 | 15 | 40 | - | 170 | 56 | 70 | 600 | 1 | 28 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 56.0000 | 28.0000 | true | true | 0.0000 |  | false |  |
| nfgt | 1 | normal | normal | 1.5 | 1 | 5 | 25 | - | 26 | 28 | 30 | 180 | 1 | 18.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.0000 | 18.6667 | true | true | 0.0000 |  | false |  |
| nfgu | 1 | chaos | normal | 1.35 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 100 | 1 | 8.51851851851852 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 8.5185 | true | true | 0.0000 |  | false |  |
| nfod | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | normal | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 100 | 1 |  | 64.5000 | 43.0000 | true | true | 64.5000 |  | true |  |
| nfor | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nfot | 3 | chaos | msplash | 1.8 | 1 | 8 | 40 | - | 41 | 44.5 | 48 | 500 | 1 | 24.7222222222222 | chaos | normal | 1.8 | 1 | 8 | 40 | - | 41 | 44.5 | 48 | 100 | 1 |  | 44.5000 | 24.7222 | true | true | 44.5000 |  | true |  |
| nfov | 1 | chaos | normal | 1.35 | 1 | 4 | 25 | - | 26 | 27.5 | 29 | 100 | 1 | 20.3703703703704 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 27.5000 | 20.3704 | true | true | 0.0000 |  | false |  |
| nfpc | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nfpe | 1 | chaos | missile | 1.8 | 1 | 9 | 30 | - | 31 | 35 | 39 | 600 | 1 | 19.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.0000 | 19.4444 | true | true | 0.0000 |  | false |  |
| nfpl | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nfps | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| nfpt | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nfpu | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nfra | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nfrb | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nfre | 1 | chaos | missile | 1.8 | 1 | 9 | 30 | - | 31 | 35 | 39 | 600 | 1 | 19.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.0000 | 19.4444 | true | true | 0.0000 |  | false |  |
| nfrg | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nfrl | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nfrp | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nfrs | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| nfsh | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| nfsp | 1 | pierce | missile | 1.8 | 1 | 8 | 16 | - | 17 | 20.5 | 24 | 600 | 1 | 11.3888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 11.3889 | true | true | 0.0000 |  | false |  |
| nftb | 1 | pierce | missile | 1.6 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 20.3125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 20.3125 | true | true | 0.0000 |  | false |  |
| nftk | 1 | pierce | missile | 1.6 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 500 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| nftr | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nftt | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| ngdk | 3 | pierce | missile | 1.8 | 1 | 12 | 33 | - | 34 | 39.5 | 45 | 500 | 1 | 21.9444444444444 | pierce | missile | 1.8 | 1 | 8 | 48 | - | 49 | 52.5 | 56 | 300 | 1 |  | 39.5000 | 21.9444 | true | true | 52.5000 |  | true |  |
| nggr | 3 | chaos | normal | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 128 | 1 | 47.7777777777778 | chaos | missile | 1.35 | 1 | 6 | 45 | - | 46 | 48.5 | 51 | 500 | 1 |  | 64.5000 | 47.7778 | true | true | 48.5000 |  | true |  |
| ngh1 | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 400 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| ngh2 | 1 | pierce | missile | 1.8 | 1 | 11 | 30 | - | 31 | 36 | 41 | 500 | 1 | 20 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.0000 | 20.0000 | true | true | 0.0000 |  | false |  |
| ngir | 3 | normal | normal | 1.4 | 3 | 10 | 31 | - | 34 | 47.5 | 61 | 128 | 1 | 33.9285714285714 | normal | normal | 1.35 | 10 | 1 | 0 | - | 10 | 10 | 10 | 66 | 1 |  | 47.5000 | 33.9286 | true | true | 10.0000 |  | true |  |
| nglm | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngna | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 500 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| ngnb | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| ngno | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| ngns | 1 | pierce | missile | 1.6 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| ngnv | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| ngnw | 1 | pierce | missile | 1.6 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| ngrd | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | missile | 1.5 | 3 | 9 | 45 | - | 48 | 60 | 72 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 60.0000 |  | true |  |
| ngrk | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| ngrw | 3 | pierce | msplash | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | pierce | msplash | 1.8 | 1 | 4 | 24 | - | 25 | 26.5 | 28 | 300 | 1 |  | 28.5000 | 15.8333 | true | true | 26.5000 |  | true |  |
| ngsp | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngst | 3 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | pierce | missile | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 500 | 1 |  | 31.0000 | 22.9630 | true | true | 31.0000 |  | true |  |
| nhar | 1 | pierce | missile | 1.8 | 1 | 4 | 6 | - | 7 | 8.5 | 10 | 600 | 1 | 4.72222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.7222 | true | true | 0.0000 |  | false |  |
| nhdc | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nhfp | 1 | pierce | missile | 1.8 | 1 | 4 | 6 | - | 7 | 8.5 | 10 | 600 | 1 | 4.72222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.7222 | true | true | 0.0000 |  | false |  |
| nhhr | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 600 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nhrh | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nhrq | 1 | chaos | msplash | 1.8 | 1 | 9 | 23 | - | 24 | 28 | 32 | 500 | 1 | 15.5555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.0000 | 15.5556 | true | true | 0.0000 |  | false |  |
| nhrr | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| nhrw | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nhyc | 3 | pierce | missile | 1.75 | 1 | 4 | 22 | - | 23 | 24.5 | 26 | 480 | 1 | 14 | siege | missile | 1.8 | 1 | 18 | 62 | - | 63 | 71.5 | 80 | 450 | 1 |  | 24.5000 | 14.0000 | true | true | 71.5000 |  | true |  |
| nhyd | 1 | pierce | missile | 1.6 | 1 | 6 | 25 | - | 26 | 28.5 | 31 | 500 | 1 | 17.8125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 17.8125 | true | true | 0.0000 |  | false |  |
| nhyh | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| nina | 1 | chaos | msplash | 1.6 | 1 | 9 | 60 | - | 61 | 65 | 69 | 600 | 1 | 40.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 65.0000 | 40.6250 | true | true | 0.0000 |  | false |  |
| ninc | 1 | pierce | msplash | 1.6 | 1 | 6 | 27 | - | 28 | 30.5 | 33 | 600 | 1 | 19.0625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 30.5000 | 19.0625 | true | true | 0.0000 |  | false |  |
| ninf | 1 | chaos | normal | 1.35 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 100 | 1 | 40.3703703703704 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 54.5000 | 40.3704 | true | true | 0.0000 |  | false |  |
| ninm | 1 | chaos | msplash | 1.6 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 600 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| nith | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| nitp | 1 | pierce | missile | 1.8 | 1 | 8 | 16 | - | 17 | 20.5 | 24 | 600 | 1 | 11.3888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 11.3889 | true | true | 0.0000 |  | false |  |
| nitr | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nits | 1 | pierce | missile | 1.6 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 20.3125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 20.3125 | true | true | 0.0000 |  | false |  |
| nitt | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| nitw | 1 | pierce | missile | 1.6 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 500 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| njgb | 1 | normal | normal | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 128 | 1 | 47.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 64.5000 | 47.7778 | true | true | 0.0000 |  | false |  |
| njga | 1 | normal | normal | 1.35 | 1 | 5 | 35 | - | 36 | 38 | 40 | 128 | 1 | 28.1481481481481 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 38.0000 | 28.1481 | true | true | 0.0000 |  | false |  |
| njg1 | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 128 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nkob | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nkog | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nkol | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nkot | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nlds | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 600 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nlkl | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nlpd | 1 | normal | normal | 1.35 | 1 | 6 | 7 | - | 8 | 10.5 | 13 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nlpr | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nlps | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nlrv | 1 | chaos | normal | 1.35 | 1 | 9 | 68 | - | 69 | 73 | 77 | 128 | 1 | 54.0740740740741 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 73.0000 | 54.0741 | true | true | 0.0000 |  | false |  |
| nlsn | 1 | normal | normal | 1.35 | 1 | 5 | 25 | - | 26 | 28 | 30 | 100 | 1 | 20.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.0000 | 20.7407 | true | true | 0.0000 |  | false |  |
| nltc | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nltl | 1 | pierce | msplash | 1.5 | 1 | 8 | 14 | - | 15 | 18.5 | 22 | 600 | 1 | 12.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 12.3333 | true | true | 0.0000 |  | false |  |
| nlur | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmam | 1 | normal | normal | 1.35 | 1 | 3 | 13 | - | 14 | 15 | 16 | 100 | 1 | 11.1111111111111 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 15.0000 | 11.1111 | true | true | 0.0000 |  | false |  |
| nmbg | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nmcf | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nmdr | 1 | chaos | normal | 1.35 | 1 | 9 | 60 | - | 61 | 65 | 69 | 100 | 1 | 48.1481481481481 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 65.0000 | 48.1481 | true | true | 0.0000 |  | false |  |
| nmfs | 1 | normal | normal | 1.35 | 1 | 2 | 12 | - | 13 | 13.5 | 14 | 100 | 1 | 10 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 13.5000 | 10.0000 | true | true | 0.0000 |  | false |  |
| nmgd | 3 | chaos | normal | 1.5 | 1 | 10 | 75 | - | 76 | 80.5 | 85 | 100 | 1 | 53.6666666666667 | chaos | missile | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 500 | 1 |  | 80.5000 | 53.6667 | true | true | 64.5000 |  | true |  |
| nmgr | 3 | chaos | normal | 1.5 | 1 | 8 | 53 | - | 54 | 57.5 | 61 | 100 | 1 | 38.3333333333333 | chaos | missile | 1.35 | 1 | 5 | 40 | - | 41 | 43 | 45 | 500 | 1 |  | 57.5000 | 38.3333 | true | true | 43.0000 |  | true |  |
| nmgw | 1 | normal | normal | 1.5 | 1 | 4 | 26 | - | 27 | 28.5 | 30 | 100 | 1 | 19 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 19.0000 | true | true | 0.0000 |  | false |  |
| nmit | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nmmu | 1 | normal | normal | 1.35 | 1 | 5 | 24 | - | 25 | 27 | 29 | 100 | 1 | 20 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 27.0000 | 20.0000 | true | true | 0.0000 |  | false |  |
| nmpg | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nmrm | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nmrr | 1 | normal | normal | 1.35 | 1 | 5 | 5 | - | 6 | 8 | 10 | 100 | 1 | 5.92592592592593 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.0000 | 5.9259 | true | true | 0.0000 |  | false |  |
| nmrv | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nmsc | 1 | chaos | missile | 1.8 | 1 | 9 | 30 | - | 31 | 35 | 39 | 600 | 1 | 19.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.0000 | 19.4444 | true | true | 0.0000 |  | false |  |
| nmsh | 1 | normal | normal | 1.35 | 1 | 5 | 20 | - | 21 | 23 | 25 | 128 | 1 | 17.037037037037 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 17.0370 | true | true | 0.0000 |  | false |  |
| nmtw | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nmyr | 1 | normal | normal | 1.9 | 1 | 7 | 32 | - | 33 | 36 | 39 | 128 | 1 | 18.9473684210526 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.0000 | 18.9474 | true | true | 0.0000 |  | false |  |
| nmys | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnad | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nndr | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | msplash | 1.5 | 3 | 9 | 67 | - | 70 | 82 | 94 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 82.0000 |  | true |  |
| nnfm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnmg | 1 | normal | normal | 1.6 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 12.1875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 12.1875 | true | true | 0.0000 |  | false |  |
| nnrg | 1 | chaos | normal | 1.7 | 2 | 7 | 45 | - | 47 | 53 | 59 | 128 | 1 | 31.1764705882353 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 53.0000 | 31.1765 | true | true | 0.0000 |  | false |  |
| nnrs | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnsa | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnsw | 1 | magic | missile | 1.75 | 1 | 4 | 8 | - | 9 | 10.5 | 12 | 600 | 1 | 6 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 6.0000 | true | true | 0.0000 |  | false |  |
| nntg | 1 | pierce | missile | 1.5 | 3 | 5 | 30 | - | 33 | 39 | 45 | 700 | 1 | 26 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.0000 | 26.0000 | true | true | 0.0000 |  | false |  |
| nnwl | 1 | pierce | missile | 1.6 | 1 | 5 | 20 | - | 21 | 23 | 25 | 600 | 1 | 14.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 14.3750 | true | true | 0.0000 |  | false |  |
| nnwq | 1 | chaos | missile | 1.6 | 1 | 10 | 30 | - | 31 | 35.5 | 40 | 600 | 1 | 22.1875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.5000 | 22.1875 | true | true | 0.0000 |  | false |  |
| nnwr | 1 | pierce | missile | 1.6 | 1 | 7 | 29 | - | 30 | 33 | 36 | 500 | 1 | 20.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 33.0000 | 20.6250 | true | true | 0.0000 |  | false |  |
| nnws | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nnzg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nogl | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nogm | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nogn | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nogo | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nogr | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nomg | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| now2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nowe | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 128 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nowk | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 128 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nowl | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| npfm | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| npgf | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| Npld | 1 | chaos | normal | 2.05 | 2 | 6 | 10 | - | 12 | 17 | 22 | 128 | 1 | 8.29268292682927 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 17.0000 | 8.2927 | true | true | 5.0000 |  | true |  |
| Nplh | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| nrat | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nrdr | 3 | pierce | missile | 1.8 | 1 | 12 | 35 | - | 36 | 41.5 | 47 | 400 | 1 | 23.0555555555556 | pierce | missile | 1.8 | 1 | 8 | 50 | - | 51 | 54.5 | 58 | 300 | 1 |  | 41.5000 | 23.0556 | true | true | 54.5000 |  | true |  |
| nrel | 1 | pierce | missile | 1.6 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 11.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 11.5625 | true | true | 0.0000 |  | false |  |
| nrmf | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nrog | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nrvd | 1 | chaos | normal | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 128 | 1 | 47.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 64.5000 | 47.7778 | true | true | 0.0000 |  | false |  |
| nrvf | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nrvi | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 128 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nrvl | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 128 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nrvs | 1 | normal | normal | 1.35 | 1 | 3 | 17 | - | 18 | 19 | 20 | 128 | 1 | 14.0740740740741 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.0000 | 14.0741 | true | true | 0.0000 |  | false |  |
| nrwm | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | missile | 1.5 | 3 | 9 | 45 | - | 48 | 60 | 72 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 60.0000 |  | true |  |
| nrzb | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nrzg | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nrzm | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nrzs | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nrzt | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 500 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| nsat | 1 | pierce | missile | 1.8 | 1 | 4 | 6 | - | 7 | 8.5 | 10 | 600 | 1 | 4.72222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.7222 | true | true | 0.0000 |  | false |  |
| nsbm | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nsbs | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nsc2 | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nsc3 | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nscb | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nsel | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 600 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nsgb | 1 | chaos | normal | 1.35 | 1 | 7 | 42 | - | 43 | 46 | 49 | 100 | 1 | 34.0740740740741 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 46.0000 | 34.0741 | true | true | 0.0000 |  | false |  |
| nsgg | 1 | chaos | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nsgh | 1 | normal | normal | 1.5 | 1 | 4 | 19 | - | 20 | 21.5 | 23 | 100 | 1 | 14.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 21.5000 | 14.3333 | true | true | 0.0000 |  | false |  |
| nsgn | 1 | normal | normal | 1.5 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 8.33333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 8.3333 | true | true | 0.0000 |  | false |  |
| nsgt | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nska | 1 | pierce | missile | 1.8 | 1 | 3 | 12 | - | 13 | 14 | 15 | 500 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nske | 1 | normal | normal | 2 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 90 | 1 | 7.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 7.2500 | true | true | 0.0000 |  | false |  |
| nskf | 1 | pierce | missile | 1.6 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| nskg | 1 | normal | normal | 2 | 1 | 7 | 11 | - | 12 | 15 | 18 | 90 | 1 | 7.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 15.0000 | 7.5000 | true | true | 0.0000 |  | false |  |
| nskm | 1 | pierce | missile | 1.6 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| nsko | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nslf | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nslh | 1 | pierce | msplash | 1.8 | 1 | 8 | 20 | - | 21 | 24.5 | 28 | 400 | 1 | 13.6111111111111 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 24.5000 | 13.6111 | true | true | 0.0000 |  | false |  |
| nsll | 1 | chaos | msplash | 1.4 | 3 | 8 | 43 | - | 46 | 56.5 | 67 | 500 | 1 | 40.3571428571429 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 56.5000 | 40.3571 | true | true | 0.0000 |  | false |  |
| nslm | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nsln | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nslr | 1 | pierce | msplash | 1.8 | 1 | 10 | 29 | - | 30 | 34.5 | 39 | 500 | 1 | 19.1666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 34.5000 | 19.1667 | true | true | 0.0000 |  | false |  |
| nslv | 1 | chaos | msplash | 1.8 | 1 | 9 | 30 | - | 31 | 35 | 39 | 500 | 1 | 19.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 35.0000 | 19.4444 | true | true | 0.0000 |  | false |  |
| nsnp | 1 | pierce | missile | 2 | 1 | 5 | 24 | - | 25 | 27 | 29 | 550 | 1 | 13.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 27.0000 | 13.5000 | true | true | 0.0000 |  | false |  |
| nsns | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nsoc | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nsog | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nspb | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nspd | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nspg | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nspp | 1 | normal | normal | 1 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 90 | 1 | 11.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 11.5000 | true | true | 0.0000 |  | false |  |
| nspr | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nsqa | 1 | chaos | normal | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 128 | 1 | 47.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 64.5000 | 47.7778 | true | true | 0.0000 |  | false |  |
| nsqe | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 128 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nsqo | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 128 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nsqt | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 128 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nsra | 1 | pierce | missile | 1.8 | 1 | 4 | 16 | - | 17 | 18.5 | 20 | 500 | 1 | 10.2777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.5000 | 10.2778 | true | true | 0.0000 |  | false |  |
| nsrh | 1 | pierce | missile | 1.8 | 1 | 5 | 20 | - | 21 | 23 | 25 | 500 | 1 | 12.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 12.7778 | true | true | 0.0000 |  | false |  |
| nsrn | 1 | pierce | missile | 1.8 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 600 | 1 | 21.9444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 21.9444 | true | true | 0.0000 |  | false |  |
| nsrv | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nsrw | 1 | chaos | missile | 1.8 | 2 | 8 | 73 | - | 75 | 82 | 89 | 600 | 1 | 45.5555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 82.0000 | 45.5556 | true | true | 0.0000 |  | false |  |
| nssp | 1 | pierce | missile | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 28.5000 | 15.8333 | true | true | 0.0000 |  | false |  |
| nsth | 1 | chaos | normal | 1.35 | 1 | 7 | 48 | - | 49 | 52 | 55 | 100 | 1 | 38.5185185185185 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 52.0000 | 38.5185 | true | true | 0.0000 |  | false |  |
| nstl | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nsts | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nstw | 1 | chaos | msplash | 1.8 | 1 | 10 | 45 | - | 46 | 50.5 | 55 | 500 | 1 | 28.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 50.5000 | 28.0556 | true | true | 0.0000 |  | false |  |
| nsty | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nthl | 1 | pierce | msplash | 1.8 | 1 | 8 | 26 | - | 27 | 30.5 | 34 | 500 | 1 | 16.9444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 30.5000 | 16.9444 | true | true | 0.0000 |  | false |  |
| ntka | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 500 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| ntkc | 1 | chaos | normal | 1.35 | 1 | 6 | 43 | - | 44 | 46.5 | 49 | 100 | 1 | 34.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 46.5000 | 34.4444 | true | true | 0.0000 |  | false |  |
| ntkf | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| ntkh | 1 | normal | normal | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 125 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| ntks | 1 | pierce | missile | 1.8 | 1 | 7 | 33 | - | 34 | 37 | 40 | 600 | 1 | 20.5555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 37.0000 | 20.5556 | true | true | 0.0000 |  | false |  |
| ntkt | 1 | pierce | missile | 1.6 | 1 | 7 | 27 | - | 28 | 31 | 34 | 500 | 1 | 19.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 19.3750 | true | true | 0.0000 |  | false |  |
| ntkw | 1 | normal | normal | 1.35 | 1 | 5 | 20 | - | 21 | 23 | 25 | 100 | 1 | 17.037037037037 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 17.0370 | true | true | 0.0000 |  | false |  |
| ntor | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntrd | 1 | chaos | normal | 1.35 | 1 | 19 | 64 | - | 65 | 74 | 83 | 128 | 1 | 54.8148148148148 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 74.0000 | 54.8148 | true | true | 0.0000 |  | false |  |
| ntrg | 1 | chaos | normal | 1.35 | 1 | 8 | 38 | - | 39 | 42.5 | 46 | 100 | 1 | 31.4814814814815 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 42.5000 | 31.4815 | true | true | 0.0000 |  | false |  |
| ntrh | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 500 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| ntrs | 1 | normal | normal | 1.35 | 1 | 5 | 5 | - | 6 | 8 | 10 | 100 | 1 | 5.92592592592593 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.0000 | 5.9259 | true | true | 0.0000 |  | false |  |
| ntrt | 1 | pierce | missile | 1.6 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 20.3125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 20.3125 | true | true | 0.0000 |  | false |  |
| ntrv | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| ntws | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nubk | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| nubr | 1 | normal | normal | 1.35 | 1 | 5 | 20 | - | 21 | 23 | 25 | 100 | 1 | 17.037037037037 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 17.0370 | true | true | 0.0000 |  | false |  |
| nubw | 1 | normal | normal | 1.35 | 1 | 4 | 23 | - | 24 | 25.5 | 27 | 100 | 1 | 18.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.5000 | 18.8889 | true | true | 0.0000 |  | false |  |
| nvde | 1 | chaos | missile | 1.35 | 1 | 8 | 67 | - | 68 | 71.5 | 75 | 450 | 1 | 52.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 71.5000 | 52.9630 | true | true | 0.0000 |  | false |  |
| nvdg | 1 | pierce | missile | 1.6 | 1 | 8 | 35 | - | 36 | 39.5 | 43 | 450 | 1 | 24.6875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.5000 | 24.6875 | true | true | 0.0000 |  | false |  |
| nvdl | 1 | pierce | missile | 1.6 | 1 | 3 | 12 | - | 13 | 14 | 15 | 450 | 1 | 8.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 8.7500 | true | true | 0.0000 |  | false |  |
| nvdw | 1 | pierce | missile | 1.6 | 1 | 5 | 22 | - | 23 | 25 | 27 | 450 | 1 | 15.625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.6250 | true | true | 0.0000 |  | false |  |
| nwen | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 128 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nwgs | 1 | pierce | missile | 1.8 | 2 | 5 | 40 | - | 42 | 46 | 50 | 450 | 1 | 25.5555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 46.0000 | 25.5556 | true | true | 0.0000 |  | false |  |
| nwiz | 1 | pierce | missile | 1.8 | 1 | 4 | 6 | - | 7 | 8.5 | 10 | 600 | 1 | 4.72222222222222 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.7222 | true | true | 0.0000 |  | false |  |
| nwld | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 90 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nwlg | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 90 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nwlt | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| nwna | 1 | chaos | normal | 1.35 | 1 | 8 | 60 | - | 61 | 64.5 | 68 | 128 | 1 | 47.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 64.5000 | 47.7778 | true | true | 0.0000 |  | false |  |
| nwnr | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 128 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nwns | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 128 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nwrg | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nws1 | 1 | pierce | missile | 2 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 600 | 1 | 22.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 45.5000 | 22.7500 | true | true | 0.0000 |  | false |  |
| nwwd | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 90 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| nwwf | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| nwwg | 1 | normal | normal | 1.35 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 90 | 1 | 14.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 14.4444 | true | true | 0.0000 |  | false |  |
| nwzd | 1 | chaos | missile | 1.8 | 1 | 9 | 38 | - | 39 | 43 | 47 | 500 | 1 | 23.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 43.0000 | 23.8889 | true | true | 0.0000 |  | false |  |
| nwzg | 1 | pierce | missile | 1.8 | 1 | 10 | 29 | - | 30 | 34.5 | 39 | 600 | 1 | 19.1666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 34.5000 | 19.1667 | true | true | 0.0000 |  | false |  |
| nwzr | 1 | pierce | missile | 1.8 | 1 | 5 | 22 | - | 23 | 25 | 27 | 600 | 1 | 13.8888888888889 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.8889 | true | true | 0.0000 |  | false |  |
| nzep | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nzom | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| nhym | 1 | magic | missile | 1.75 | 1 | 3 | 7 | - | 8 | 9 | 10 | 600 | 1 | 5.14285714285714 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.0000 | 5.1429 | true | true | 0.0000 |  | false |  |
| nchp | 1 | magic | missile | 2 | 1 | 2 | 7 | - | 8 | 8.5 | 9 | 600 | 1 | 4.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 8.5000 | 4.2500 | true | true | 0.0000 |  | false |  |
| nalb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncrb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nder | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndog | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndwm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfbr | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nhmc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| now3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nowb | 1 | normal | normal | 1.35 | 1 | 4 | 18 | - | 19 | 20.5 | 22 | 128 | 1 | 15.1851851851852 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 15.1852 | true | true | 0.0000 |  | false |  |
| Npbm | 1 | hero | normal | 2.22 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.15315315315315 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1532 | true | true | 5.0000 |  | true |  |
| nplb | 1 | normal | normal | 1.35 | 1 | 5 | 20 | - | 21 | 23 | 25 | 128 | 1 | 17.037037037037 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.0000 | 17.0370 | true | true | 0.0000 |  | false |  |
| npnw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nqb1 | 1 | pierce | missile | 1.5 | 1 | 3 | 12 | - | 13 | 14 | 15 | 550 | 1 | 9.33333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.0000 | 9.3333 | true | true | 0.0000 |  | false |  |
| nrdk | 3 | pierce | msplash | 1.8 | 1 | 8 | 24 | - | 25 | 28.5 | 32 | 600 | 1 | 15.8333333333333 | pierce | msplash | 1.8 | 1 | 4 | 24 | - | 25 | 26.5 | 28 | 300 | 1 |  | 28.5000 | 15.8333 | true | true | 26.5000 |  | true |  |
| nsea | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nsha | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nshe | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nshf | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nshw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nskk | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nsno | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvil | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvk2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvl2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvlk | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvlw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvul | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb4 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb5 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb6 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb7 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb8 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncb9 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncba | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncbb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncbc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncbd | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncbe | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncbf | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncnt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncop | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncp2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncp3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nct1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nct2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndch | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndh0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndh1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndh2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndh3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndh4 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndrg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndrk | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndro | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndrr | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndru | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndrz | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfh0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfh1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfoh | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfr1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfr2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngad | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngme | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngnh | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngni | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngol | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngt2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngwr | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nhns | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmer | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmg0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmg1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmh0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmh1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmoo | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr4 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr5 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr6 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr7 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr8 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmr9 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmra | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmrb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmrc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmrd | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmre | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmrk | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmrl | 1 | normal | normal | 1.35 | 1 | 2 | 8 | - | 9 | 9.5 | 10 | 100 | 1 | 7.03703703703704 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.5000 | 7.0370 | true | true | 0.0000 |  | false |  |
| noga | 1 | chaos | normal | 1.35 | 2 | 10 | 94 | - | 96 | 105 | 114 | 100 | 1 | 77.7777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 105.0000 | 77.7778 | true | true | 0.0000 |  | false |  |
| nshp | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntav | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nten | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nth0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nth1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntn2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntnt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntt2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nwgt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| Ecen | 1 | chaos | missile | 2 | 8 | 8 | 40 | - | 48 | 76 | 104 | 1000 | 1 | 38 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 76.0000 | 38.0000 | true | true | 5.0000 |  | true |  |
| Eevi | 1 | hero | normal | 1.7 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.64705882352941 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.6471 | true | true | 5.0000 |  | true |  |
| Eevm | 1 | chaos | missile | 1.6 | 2 | 12 | 12 | - | 14 | 25 | 36 | 600 | 1 | 15.625 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 25.0000 | 15.6250 | true | true | 5.0000 |  | true |  |
| Efur | 1 | hero | missile | 2.18 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.29357798165138 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.2936 | true | true | 5.0000 |  | true |  |
| Eidm | 1 | chaos | missile | 1.6 | 2 | 12 | 12 | - | 14 | 25 | 36 | 600 | 1 | 15.625 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 25.0000 | 15.6250 | true | true | 5.0000 |  | true |  |
| Eill | 1 | hero | normal | 1.7 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.64705882352941 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.6471 | true | true | 5.0000 |  | true |  |
| Eilm | 1 | chaos | missile | 1.6 | 2 | 12 | 12 | - | 14 | 25 | 36 | 600 | 1 | 15.625 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 25.0000 | 15.6250 | true | true | 5.0000 |  | true |  |
| Ekgg | 1 | hero | missile | 2.18 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.29357798165138 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.2936 | true | true | 5.0000 |  | true |  |
| Emfr | 1 | hero | missile | 2.18 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.29357798165138 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.2936 | true | true | 5.0000 |  | true |  |
| Emns | 1 | hero | missile | 2.18 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.29357798165138 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.2936 | true | true | 5.0000 |  | true |  |
| Etyr | 1 | hero | missile | 2.46 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 2.84552845528455 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 2.8455 | true | true | 5.0000 |  | true |  |
| Ewrd | 1 | hero | normal | 2.05 | 2 | 11 | 0 | - | 2 | 12 | 22 | 100 | 1 | 5.85365853658537 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 12.0000 | 5.8537 | true | true | 5.0000 |  | true |  |
| Hant | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| Hapm | 1 | hero | normal | 2.2 | 2 | 6 | 22 | - | 24 | 29 | 34 | 100 | 1 | 13.1818181818182 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 29.0000 | 13.1818 | true | true | 5.0000 |  | true |  |
| Harf | 1 | chaos | normal | 2.2 | 2 | 6 | 22 | - | 24 | 29 | 34 | 100 | 1 | 13.1818181818182 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 29.0000 | 13.1818 | true | true | 5.0000 |  | true |  |
| Hart | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hdgo | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hhkl | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hjai | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| Hkal | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| Hlgr | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hmbr | 1 | hero | normal | 2.22 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.15315315315315 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1532 | true | true | 5.0000 |  | true |  |
| Hmgd | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hpb1 | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hpb2 | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Huth | 1 | hero | normal | 2.2 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.18181818181818 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1818 | true | true | 5.0000 |  | true |  |
| Hvsh | 1 | hero | missile | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| Hvwd | 1 | hero | missile | 2.46 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 2.84552845528455 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 2.8455 | true | true | 5.0000 |  | true |  |
| Nbbc | 1 | hero | normal | 1.77 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.34463276836158 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.3446 | true | true | 5.0000 |  | true |  |
| Nklj | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Nkjx | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Nmag | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 200 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| Nman | 1 | pierce | normal | 1.3 | 1 | 8 | 14 | - | 15 | 18.5 | 22 | 200 | 1 | 14.2307692307692 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 18.5000 | 14.2308 | true | true | 5.0000 |  | true |  |
| nplg | 1 | normal | normal | 1.35 | 1 | 5 | 33 | - | 34 | 36 | 38 | 128 | 1 | 26.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.0000 | 26.6667 | true | true | 0.0000 |  | false |  |
| Nsjs | 1 | hero | normal | 2.22 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.15315315315315 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.1532 | true | true | 5.0000 |  | true |  |
| Ocbh | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 128 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| Ocb2 | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 128 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| Odrt | 1 | hero | missile | 2.28 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.19298245614035 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.1930 | true | true | 5.0000 |  | true |  |
| Ogld | 1 | chaos | missile | 2.28 | 4 | 20 | 80 | - | 84 | 122 | 160 | 1500 | 1 | 53.5087719298246 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 122.0000 | 53.5088 | true | true | 5.0000 |  | true |  |
| Ogrh | 1 | hero | normal | 1.77 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.34463276836158 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.3446 | true | true | 5.0000 |  | true |  |
| Opgh | 1 | chaos | normal | 1.77 | 2 | 12 | 12 | - | 14 | 25 | 36 | 100 | 1 | 14.1242937853107 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 25.0000 | 14.1243 | true | true | 5.0000 |  | true |  |
| Orex | 3 | hero | normal | 2.05 | 2 | 6 | 10 | - | 12 | 17 | 22 | 128 | 1 | 8.29268292682927 | hero | missile | 2.13 | 2 | 6 | 0 | - | 2 | 7 | 12 | 500 | 1 |  | 17.0000 | 8.2927 | true | true | 7.0000 |  | true |  |
| Orkn | 1 | hero | missile | 2.28 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.19298245614035 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.1930 | true | true | 5.0000 |  | true |  |
| Osam | 1 | hero | normal | 1.77 | 2 | 12 | 0 | - | 2 | 13 | 24 | 100 | 1 | 7.34463276836158 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 13.0000 | 7.3446 | true | true | 5.0000 |  | true |  |
| Otcc | 1 | hero | normal | 2.05 | 2 | 6 | 0 | - | 2 | 7 | 12 | 128 | 1 | 3.41463414634146 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.4146 | true | true | 5.0000 |  | true |  |
| Othr | 1 | hero | missile | 2.28 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.19298245614035 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.1930 | true | true | 5.0000 |  | true |  |
| Uanb | 1 | hero | normal | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 128 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| Ubal | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Uclc | 1 | hero | missile | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| Udth | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Uear | 1 | hero | normal | 2.33 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.00429184549356 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.0043 | true | true | 5.0000 |  | true |  |
| Uktl | 1 | hero | missile | 1.9 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.63157894736842 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.6316 | true | true | 5.0000 |  | true |  |
| Umal | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Usyl | 1 | hero | missile | 2.46 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 2.84552845528455 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 2.8455 | true | true | 5.0000 |  | true |  |
| Utic | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Uvar | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Uvng | 1 | hero | normal | 1.8 | 2 | 6 | 0 | - | 2 | 7 | 12 | 100 | 1 | 3.88888888888889 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.8889 | true | true | 5.0000 |  | true |  |
| Uwar | 1 | chaos | msplash | 2 | 6 | 20 | 0 | - | 6 | 63 | 120 | 600 | 1 | 31.5 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 63.0000 | 31.5000 | true | true | 5.0000 |  | true |  |
| Hgam | 1 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 600 | 1 | 2.34741784037559 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 5.0000 | 2.3474 | true | true | 5.0000 |  | true |  |
| eilw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| enec | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ensh | 1 | normal | mbounce | 1.8 | 1 | 5 | 26 | - | 27 | 29 | 31 | 225 | 5 | 16.1111111111111 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.0000 | 16.1111 | true | true | 0.0000 |  | false |  |
| eshd | 1 | pierce | missile | 1.3 | 1 | 6 | 26 | - | 27 | 29.5 | 32 | 600 | 1 | 22.6923076923077 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 29.5000 | 22.6923 | true | true | 0.0000 |  | false |  |
| etrs | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hbew | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hcth | 1 | normal | normal | 1 | 1 | 2 | 19 | - | 20 | 20.5 | 21 | 100 | 1 | 20.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.5000 | 20.5000 | true | true | 0.0000 |  | false |  |
| hhdl | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| hhes | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 90 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| hrdh | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| Naka | 1 | hero | normal | 1.8 | 2 | 8 | 0 | - | 2 | 9 | 16 | 128 | 1 | 5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 9.0000 | 5.0000 | true | true | 0.0000 |  | false |  |
| nsw1 | 1 | normal | normal | 1 | 1 | 2 | 10 | - | 11 | 11.5 | 12 | 90 | 1 | 11.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.5000 | 11.5000 | true | true | 0.0000 |  | false |  |
| nsw2 | 1 | normal | normal | 1 | 1 | 2 | 15 | - | 16 | 16.5 | 17 | 90 | 1 | 16.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.5000 | 16.5000 | true | true | 0.0000 |  | false |  |
| nsw3 | 1 | normal | normal | 1 | 1 | 2 | 20 | - | 21 | 21.5 | 22 | 90 | 1 | 21.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 21.5000 | 21.5000 | true | true | 0.0000 |  | false |  |
| ncat | 3 | siege | artillery | 4.5 | 3 | 21 | 81 | - | 84 | 114 | 144 | 1150 | 1 | 25.3333333333333 | siege | missile | 4.5 | 1 | 21 | 81 | - | 82 | 92 | 102 | 1000 | 1 |  | 114.0000 | 25.3333 | true | true | 92.0000 |  | true |  |
| nbee | 3 | normal | normal | 3 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 90 | 1 | 2.5 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 7.5000 | 2.5000 | true | true | 1.0000 |  | true |  |
| nbel | 1 | normal | missile | 1.9 | 2 | 6 | 35 | - | 37 | 42 | 47 | 200 | 1 | 22.1052631578947 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 42.0000 | 22.1053 | true | true | 0.0000 |  | false |  |
| nbsp | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nchg | 1 | chaos | normal | 1.2 | 1 | 5 | 22 | - | 23 | 25 | 27 | 100 | 1 | 20.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 20.8333 | true | true | 0.0000 |  | false |  |
| nchr | 1 | chaos | normal | 1.65 | 1 | 5 | 22 | - | 23 | 25 | 27 | 100 | 1 | 15.1515151515152 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 15.1515 | true | true | 0.0000 |  | false |  |
| nchw | 1 | chaos | missile | 1.75 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 14.2857142857143 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 14.2857 | true | true | 0.0000 |  | false |  |
| nckb | 1 | chaos | missile | 1.2 | 1 | 5 | 15 | - | 16 | 18 | 20 | 500 | 1 | 15 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.0000 | 15.0000 | true | true | 0.0000 |  | false |  |
| ncpn | 3 | chaos | normal | 3 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 90 | 1 | 2.5 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 7.5000 | 2.5000 | true | true | 1.0000 |  | true |  |
| ndmu | 1 | normal | normal | 1.3 | 2 | 2 | 10 | - | 12 | 13 | 14 | 90 | 1 | 10 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 13.0000 | 10.0000 | true | true | 0.0000 |  | false |  |
| ndrd | 1 | normal | normal | 1.35 | 1 | 3 | 18 | - | 19 | 20 | 21 | 100 | 1 | 14.8148148148148 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 20.0000 | 14.8148 | true | true | 0.0000 |  | false |  |
| ndrn | 1 | normal | normal | 1.35 | 1 | 5 | 28 | - | 29 | 31 | 33 | 100 | 1 | 22.962962962963 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.0000 | 22.9630 | true | true | 0.0000 |  | false |  |
| ndrt | 1 | normal | normal | 1 | 5 | 3 | 27 | - | 32 | 37 | 42 | 100 | 1 | 37 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 37.0000 | 37.0000 | true | true | 0.0000 |  | false |  |
| ndrf | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| ndrh | 1 | pierce | missile | 1.8 | 1 | 6 | 28 | - | 29 | 31.5 | 34 | 600 | 1 | 17.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 31.5000 | 17.5000 | true | true | 0.0000 |  | false |  |
| ndrj | 1 | normal | normal | 1.35 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 7.77777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 7.7778 | true | true | 0.0000 |  | false |  |
| ndrm | 1 | pierce | missile | 1.8 | 1 | 5 | 21 | - | 22 | 24 | 26 | 600 | 1 | 13.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 24.0000 | 13.3333 | true | true | 0.0000 |  | false |  |
| ndrp | 1 | normal | normal | 1.35 | 1 | 2 | 11 | - | 12 | 12.5 | 13 | 100 | 1 | 9.25925925925926 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 9.2593 | true | true | 0.0000 |  | false |  |
| ndrs | 1 | pierce | missile | 1.8 | 1 | 7 | 35 | - | 36 | 39 | 42 | 600 | 1 | 21.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 39.0000 | 21.6667 | true | true | 0.0000 |  | false |  |
| ndrw | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| ndrl | 3 | normal | normal | 2 | 1 | 2 | 5 | - | 6 | 6.5 | 7 | 90 | 1 | 3.25 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 6.5000 | 3.2500 | true | true | 1.0000 |  | true |  |
| ndsa | 1 | pierce | msplash | 2 | 1 | 8 | 32 | - | 33 | 36.5 | 40 | 500 | 1 | 18.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.5000 | 18.2500 | true | true | 0.0000 |  | false |  |
| negz | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nemi | 1 | pierce | missile | 2 | 1 | 6 | 7 | - | 8 | 10.5 | 13 | 600 | 1 | 5.25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 5.2500 | true | true | 0.0000 |  | false |  |
| nfgl | 1 | normal | normal | 1.35 | 3 | 8 | 36 | - | 39 | 49.5 | 60 | 100 | 1 | 36.6666666666667 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 49.5000 | 36.6667 | true | true | 0.0000 |  | false |  |
| ngbl | 1 | pierce | missile | 1.4 | 3 | 10 | 31 | - | 34 | 47.5 | 61 | 200 | 1 | 33.9285714285714 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 47.5000 | 33.9286 | true | true | 0.0000 |  | false |  |
| nhea | 1 | pierce | missile | 1.5 | 1 | 3 | 15 | - | 16 | 17 | 18 | 500 | 1 | 11.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 17.0000 | 11.3333 | true | true | 0.0000 |  | false |  |
| nhef | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nhem | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nhew | 3 | normal | normal | 3 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 90 | 1 | 2.5 | normal | normal | 1.1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 7.5000 | 2.5000 | true | true | 1.0000 |  | true |  |
| njks | 1 | chaos | normal | 1.35 | 1 | 6 | 38 | - | 39 | 41.5 | 44 | 100 | 1 | 30.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 41.5000 | 30.7407 | true | true | 0.0000 |  | false |  |
| nmdm | 1 | pierce | missile | 1.1 | 1 | 16 | 8 | - | 9 | 16.5 | 24 | 600 | 1 | 15 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.5000 | 15.0000 | true | true | 0.0000 |  | false |  |
| nmed | 1 | pierce | missile | 1.1 | 1 | 16 | 8 | - | 9 | 16.5 | 24 | 600 | 1 | 15 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 16.5000 | 15.0000 | true | true | 0.0000 |  | false |  |
| nmpe | 3 | normal | normal | 3 | 1 | 2 | 6 | - | 7 | 7.5 | 8 | 90 | 1 | 2.5 | normal | normal | 1 | 1 | 1 | 0 | - | 1 | 1 | 1 | 66 | 1 |  | 7.5000 | 2.5000 | true | true | 1.0000 |  | true |  |
| nmsn | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 500 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| nser | 1 | pierce | missile | 1.1 | 5 | 16 | 20 | - | 25 | 62.5 | 100 | 500 | 1 | 56.8181818181818 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 62.5000 | 56.8182 | true | true | 0.0000 |  | false |  |
| nspc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nssn | 1 | pierce | missile | 1 | 1 | 8 | 8 | - | 9 | 12.5 | 16 | 650 | 1 | 12.5 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.5000 | 12.5000 | true | true | 0.0000 |  | false |  |
| nthr | 1 | pierce | missile | 1.5 | 3 | 14 | 51 | - | 54 | 73.5 | 93 | 500 | 1 | 49 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 73.5000 | 49.0000 | true | true | 0.0000 |  | false |  |
| nw2w | 1 | chaos | missile | 1.75 | 2 | 3 | 14 | - | 16 | 18 | 20 | 600 | 1 | 10.2857142857143 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 18.0000 | 10.2857 | true | true | 0.0000 |  | false |  |
| nwat | 1 | normal | normal | 1.6 | 1 | 2 | 9 | - | 10 | 10.5 | 11 | 100 | 1 | 6.5625 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 6.5625 | true | true | 0.0000 |  | false |  |
| odkt | 1 | pierce | missile | 1.8 | 1 | 10 | 27 | - | 28 | 32.5 | 37 | 600 | 1 | 18.0555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 32.5000 | 18.0556 | true | true | 0.0000 |  | false |  |
| ogrk | 1 | normal | normal | 1.6 | 1 | 4 | 17 | - | 18 | 19.5 | 21 | 100 | 1 | 12.1875 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 19.5000 | 12.1875 | true | true | 0.0000 |  | false |  |
| ojgn | 1 | siege | artillery | 2 | 3 | 10 | 75 | - | 78 | 91.5 | 105 | 900 | 1 | 45.75 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 91.5000 | 45.7500 | true | true | 0.0000 |  | false |  |
| omtg | 1 | normal | normal | 1.1 | 2 | 8 | 2 | - | 4 | 11 | 18 | 100 | 1 | 10 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.0000 | 10.0000 | true | true | 0.0000 |  | false |  |
| onzg | 1 | siege | normal | 1.85 | 1 | 5 | 22 | - | 23 | 25 | 27 | 100 | 1 | 13.5135135135135 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 13.5135 | true | true | 0.0000 |  | false |  |
| oosc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| oswy | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ovlj | 1 | magic | missile | 1.75 | 1 | 5 | 9 | - | 10 | 12 | 14 | 600 | 1 | 6.85714285714286 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 12.0000 | 6.8571 | true | true | 0.0000 |  | false |  |
| owar | 1 | normal | normal | 1.1 | 2 | 8 | 2 | - | 4 | 11 | 18 | 100 | 1 | 10 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 11.0000 | 10.0000 | true | true | 0.0000 |  | false |  |
| ownr | 1 | normal | normal | 1.2 | 2 | 3 | 36 | - | 38 | 40 | 42 | 128 | 1 | 33.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 40.0000 | 33.3333 | true | true | 0.0000 |  | false |  |
| uabc | 1 | normal | normal | 1.9 | 1 | 7 | 32 | - | 33 | 36 | 39 | 128 | 1 | 18.9473684210526 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 36.0000 | 18.9474 | true | true | 0.0000 |  | false |  |
| uarb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ubdd | 3 | pierce | msplash | 3 | 2 | 11 | 83 | - | 85 | 95 | 105 | 300 | 1 | 31.6666666666667 | pierce | msplash | 3 | 1 | 11 | 83 | - | 84 | 89 | 94 | 300 | 1 |  | 95.0000 | 31.6667 | true | true | 89.0000 |  | true |  |
| ubdr | 3 | chaos | msplash | 1.5 | 3 | 12 | 45 | - | 48 | 64.5 | 81 | 500 | 1 | 43 | chaos | msplash | 1.5 | 3 | 9 | 45 | - | 48 | 60 | 72 | 300 | 1 |  | 64.5000 | 43.0000 | true | true | 60.0000 |  | true |  |
| ubot | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| udes | 1 | normal | msplash | 1.5 | 1 | 15 | 54 | - | 55 | 62 | 69 | 600 | 1 | 41.3333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 62.0000 | 41.3333 | true | true | 0.0000 |  | false |  |
| uktg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| uktn | 1 | pierce | missile | 1.8 | 1 | 4 | 8 | - | 9 | 10.5 | 12 | 600 | 1 | 5.83333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 5.8333 | true | true | 0.0000 |  | false |  |
| uswb | 1 | normal | missile | 1.4 | 1 | 9 | 20 | - | 21 | 25 | 29 | 500 | 1 | 17.8571428571429 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 17.8571 | true | true | 0.0000 |  | false |  |
| hprt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| haro | 0 | normal | _ | - | - | - | - | - | - | - | - | - | - | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbfl | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbsm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbt1 | 3 | siege | artillery | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 | 40.2 | siege | missile | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 |  | 100.5000 | 40.2000 | true | true | 100.5000 |  | true |  |
| nbt2 | 3 | siege | artillery | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 | 40.2 | siege | missile | 2.5 | 1 | 22 | 89 | - | 90 | 100.5 | 111 | 800 | 1 |  | 100.5000 | 40.2000 | true | true | 100.5000 |  | true |  |
| nbwd | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncap | 3 | normal | normal | 1.5 | 1 | 8 | 33 | - | 34 | 37.5 | 41 | 128 | 1 | 25 | siege | msplash | 2 | 2 | 12 | 41 | - | 43 | 54 | 65 | 700 | 1 |  | 37.5000 | 25.0000 | true | true | 54.0000 |  | true |  |
| ncaw | 1 | normal | normal | 2.5 | 1 | 11 | 44 | - | 45 | 50 | 55 | 128 | 1 | 20 | normal | normal | 2.5 | 1 | 11 | 44 | - | 45 | 50 | 55 | 128 | 1 |  | 50.0000 | 20.0000 | true | true | 50.0000 |  | true |  |
| ncmw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ncta | 1 | normal | normal | 2.5 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 128 | 1 | 21.8 | normal | normal | 2.5 | 1 | 12 | 48 | - | 49 | 54.5 | 60 | 128 | 1 |  | 54.5000 | 21.8000 | true | true | 54.5000 |  | true |  |
| ncte | 1 | normal | normal | 2.5 | 1 | 15 | 59 | - | 60 | 67 | 74 | 128 | 1 | 26.8 | normal | normal | 2.5 | 1 | 15 | 59 | - | 60 | 67 | 74 | 128 | 1 |  | 67.0000 | 26.8000 | true | true | 67.0000 |  | true |  |
| nctl | 1 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 | 18.2 | normal | normal | 2.5 | 1 | 10 | 40 | - | 41 | 45.5 | 50 | 128 | 1 |  | 45.5000 | 18.2000 | true | true | 45.5000 |  | true |  |
| ndfl | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndgt | 1 | pierce | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| ndke | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndkw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndmg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndrb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ndt1 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| ndt2 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| nef0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef4 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef5 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef6 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nef7 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nefm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| negf | 1 | pierce | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| negm | 1 | pierce | missile | 1.2 | 2 | 5 | 19 | - | 21 | 25 | 29 | 700 | 1 | 20.8333333333333 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 20.8333 | true | true | 0.0000 |  | false |  |
| negt | 1 | pierce | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| net1 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| net2 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| nfnp | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfrm | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfrt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nft1 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| nft2 | 1 | magic | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| nfv0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfv1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfv2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfv3 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nfv4 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ngob | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nhcn | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nheb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nico | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nitb | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nmgv | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nndk | 3 | pierce | msplash | 1.8 | 1 | 12 | 33 | - | 34 | 39.5 | 45 | 500 | 1 | 21.9444444444444 | pierce | msplash | 1.8 | 1 | 8 | 48 | - | 49 | 52.5 | 56 | 300 | 1 |  | 39.5000 | 21.9444 | true | true | 52.5000 |  | true |  |
| Nngs | 1 | hero | missile | 1.9 | 2 | 6 | 0 | - | 2 | 7 | 12 | 600 | 1 | 3.68421052631579 | hero | missile | 2.13 | 2 | 4 | 0 | - | 2 | 5 | 8 | 500 | 1 |  | 7.0000 | 3.6842 | true | true | 5.0000 |  | true |  |
| nnsg | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnsu | 1 | magic | missile | 1.75 | 1 | 4 | 8 | - | 9 | 10.5 | 12 | 600 | 1 | 6 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 10.5000 | 6.0000 | true | true | 0.0000 |  | false |  |
| nntt | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nnwa | 1 | normal | normal | 1.35 | 1 | 2 | 13 | - | 14 | 14.5 | 15 | 100 | 1 | 10.7407407407407 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 14.5000 | 10.7407 | true | true | 0.0000 |  | false |  |
| npgr | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| npig | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nshr | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ntt1 | 1 | chaos | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| ntx2 | 1 | chaos | missile | 1 | 2 | 5 | 19 | - | 21 | 25 | 29 | 600 | 1 | 25 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 25.0000 | 25.0000 | true | true | 0.0000 |  | false |  |
| nvr0 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvr1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nvr2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nwc1 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nwc2 | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| ocbw | 1 | chaos | missile | 4 | 1 | 8 | 33 | - | 34 | 37.5 | 41 | 700 | 1 | 9.375 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 37.5000 | 9.3750 | true | true | 0.0000 |  | false |  |
| nzin | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbse | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| nbsw | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |
| zcso | 1 | pierce | missile | 0.9 | 2 | 5 | 80 | - | 82 | 86 | 90 | 300 | 1 | 95.5555555555556 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 86.0000 | 95.5556 | true | true | 0.0000 |  | false |  |
| zhyd | 1 | pierce | instant | 1 | 4 | 10 | 80 | - | 84 | 102 | 120 | 400 | 1 | 102 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 102.0000 | 102.0000 | true | true | 0.0000 |  | false |  |
| zjug | 1 | pierce | msplash | 1.6 | 2 | 4 | 80 | - | 82 | 85 | 88 | 400 | 1 | 53.125 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 85.0000 | 53.1250 | true | true | 0.0000 |  | false |  |
| zmar | 1 | pierce | instant | 0.9 | 2 | 4 | 80 | - | 82 | 85 | 88 | 400 | 1 | 94.4444444444444 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 85.0000 | 94.4444 | true | true | 0.0000 |  | false |  |
| zshv | 1 | normal | normal | 1.35 | 5 | 15 | 200 | - | 205 | 240 | 275 | 100 | 1 | 177.777777777778 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 240.0000 | 177.7778 | true | true | 0.0000 |  | false |  |
| zsmc | 1 | normal | missile | 2.13 | 2 | 4 | 80 | - | 82 | 85 | 88 | 600 | 1 | 39.906103286385 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 85.0000 | 39.9061 | true | true | 0.0000 |  | false |  |
| zzrg | 1 | normal | normal | 1.35 | 1 | 4 | 21 | - | 22 | 23.5 | 25 | 100 | 1 | 17.4074074074074 | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 23.5000 | 17.4074 | true | true | 0.0000 |  | false |  |
| nzlc | 0 | normal | _ | 0 | - | - | - | - | - | - | - | - | 1 | #VALUE! | normal | _ | - | - | - | - | - | - | - | - | - | 1 |  | 0.0000 |  | false |  | 0.0000 |  | false |  |

## AbilityData (763 rows)

| alias | alias | levels | reqLevel | levelSkip | priority | hero | item | useInEditor | Cast1 | Cool1 | Dur1 | HeroDur1 | Cost1 | targs1 | rangeN1 | Farea1 | Harea1 | Qarea1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| AHbz | AHbz | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 6 | 0 | 0 | 75 |  |  |  |  |  |
| AHab | AHab | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AHmt | AHmt | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 0 | 0 | 100 |  |  |  |  |  |
| AHwe | AHwe | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 60 | 60 | 125 |  |  |  |  |  |
| ANst | ANst | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 30 | 30 | 150 |  |  |  |  |  |
| ANsg | ANsg | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 40 | 70 | 70 | 125 |  |  |  |  |  |
| ANsq | ANsq | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 70 | 70 | 75 |  |  |  |  |  |
| ANsw | ANsw | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 70 | 70 | 70 | 35 |  |  |  |  |  |
| AOww | AOww | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 7 | 5 | 250 |  |  |  |  |  |
| AOcr | AOcr | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AOmi | AOmi | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 3 | 60 | 60 | 150 |  |  |  |  |  |
| AOwk | AOwk | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 5 | 20 | 20 | 75 |  |  |  |  |  |
| AHbn | AHbn | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 12 | 5 | 75 |  |  |  |  |  |
| AHfs | AHfs | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 1.33 | 10 | 9 | 2.67 | 135 |  |  |  |  |  |
| AHdr | AHdr | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 6 | 6 | 10 |  |  |  |  |  |
| AHpx | AHpx | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 0 | 0 | 175 |  |  |  |  |  |
| AUcb | AUcb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 0 | 0 | 30 |  |  |  |  |  |
| AUim | AUim | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 2 | 1 | 100 |  |  |  |  |  |
| AUls | AUls | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 30 | 30 | 150 |  |  |  |  |  |
| AUts | AUts | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| ANba | ANba | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 2 | 2 | 0 | 6 |  |  |  |  |  |
| ANch | ANch | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 45 | 0 | 0 | 150 |  |  |  |  |  |
| ANdr | ANdr | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 8 | 8 | 75 |  |  |  |  |  |
| ANsi | ANsi | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 16 | 8 | 75 |  |  |  |  |  |
| AUan | AUan | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 40 | 40 | 250 |  |  |  |  |  |
| AUdc | AUdc | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 5 | 0 | 0 | 75 |  |  |  |  |  |
| AUdp | AUdp | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 0 | 0 | 50 |  |  |  |  |  |
| AUau | AUau | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AEev | AEev | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AEim | AEim | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 1 | 1 | 25 |  |  |  |  |  |
| AEmb | AEmb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 7 | 0 | 0 | 50 |  |  |  |  |  |
| AEme | AEme | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 1.5 | 60 | 150 |  |  |  |  |  |
| AUsl | AUsl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 20 | 5 | 100 |  |  |  |  |  |
| AUav | AUav | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AUcs | AUcs | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 0 | 0 | 110 |  |  |  |  |  |
| AUin | AUin | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 4 | 2 | 175 |  |  |  |  |  |
| AOcl | AOcl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 0 | 0 | 120 |  |  |  |  |  |
| AOeq | AOeq | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 90 | 25 | 20 | 150 |  |  |  |  |  |
| AOfs | AOfs | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 8 | 8 | 75 |  |  |  |  |  |
| AOsf | AOsf | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 60 | 60 | 100 |  |  |  |  |  |
| AEer | AEer | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 12 | 3 | 75 |  |  |  |  |  |
| AEfn | AEfn | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 60 | 60 | 125 |  |  |  |  |  |
| AEah | AEah | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AEtq | AEtq | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 60 | 30 | 30 | 125 |  |  |  |  |  |
| AUdr | AUdr | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 0 | 0 | 25 |  |  |  |  |  |
| AUdd | AUdd | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 150 | 35 | 35 | 250 |  |  |  |  |  |
| AUfa | AUfa | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 2 | 5 | 5 | 40 |  |  |  |  |  |
| AUfu | AUfu | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 2 | 5 | 5 | 40 |  |  |  |  |  |
| AUfn | AUfn | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 4 | 4 | 125 |  |  |  |  |  |
| AHav | AHav | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 60 | 60 | 150 |  |  |  |  |  |
| AHbh | AHbh | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 2 | 1 | - |  |  |  |  |  |
| AHtb | AHtb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 5 | 3 | 75 |  |  |  |  |  |
| AHtc | AHtc | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 5 | 3 | 90 |  |  |  |  |  |
| ANfl | ANfl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 11 | 0.7 | 0 | 110 |  |  |  |  |  |
| ANfa | ANfa | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 5 | 1.5 | 7 |  |  |  |  |  |
| ANto | ANto | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 40 | 40 | 250 |  |  |  |  |  |
| ANms | ANms | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 0 | 0 | 25 |  |  |  |  |  |
| AHad | AHad | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AHds | AHds | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 35 | 15 | 15 | 25 |  |  |  |  |  |
| AHhb | AHhb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 5 | 0 | 0 | 75 |  |  |  |  |  |
| AHre | AHre | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 240 | 0 | 0 | 200 |  |  |  |  |  |
| ANbf | ANbf | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 5 | 5 | 75 |  |  |  |  |  |
| ANdb | ANdb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANdh | ANdh | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 12 | 12 | 5 | 75 |  |  |  |  |  |
| ANef | ANef | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 60 | 60 | 150 |  |  |  |  |  |
| ANdo | ANdo | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 0 | 0 | 150 |  |  |  |  |  |
| ANht | ANht | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 12 | 15 | 15 | 75 |  |  |  |  |  |
| ANca | ANca | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANrf | ANrf | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 8 | 3 | 3 | 85 |  |  |  |  |  |
| AHfa | AHfa | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 8 |  |  |  |  |  |
| AEst | AEst | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 60 | 60 | 100 |  |  |  |  |  |
| AEsf | AEsf | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 45 | 30 | 200 |  |  |  |  |  |
| AEar | AEar | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AOae | AOae | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AOre | AOre | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 3 | 240 | 0 | 0 | - |  |  |  |  |  |
| AOsh | AOsh | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| AOws | AOws | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 3 | 2 | 90 |  |  |  |  |  |
| AOhw | AOhw | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 0 | 0 | 90 |  |  |  |  |  |
| AOhx | AOhx | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 7 | 15 | 4 | 70 |  |  |  |  |  |
| AOsw | AOsw | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6.5 | 40 | 40 | 30 |  |  |  |  |  |
| AOvd | AOvd | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 30 | 30 | 200 |  |  |  |  |  |
| AEbl | AEbl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 0.33 | 0 | 50 |  |  |  |  |  |
| AEfk | AEfk | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 0 | 0 | 100 |  |  |  |  |  |
| AEsh | AEsh | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 3 | 8 | 15.1 | 15.1 | 65 |  |  |  |  |  |
| AEsv | AEsv | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 180 | 180 | 150 |  |  |  |  |  |
| ANin | ANin | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 4 | 2 | 175 |  |  |  |  |  |
| SNin | SNin | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 30 | 4 | 2 | 100 |  |  |  |  |  |
| ANfb | ANfb | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 2 | 2 | 75 |  |  |  |  |  |
| ANfd | ANfd | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| ACfd | ACfd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 0 | 0 | 50 |  |  |  |  |  |
| ACf3 | ACf3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 0 | 0 | 50 |  |  |  |  |  |
| ANdp | ANdp | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 100 | 1 | 0 | 0 |  |  |  |  |  |
| ANrc | ANrc | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 1 | 0 | 0 |  |  |  |  |  |
| ANr3 | ANr3 | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 1 | 0 | 0 |  |  |  |  |  |
| AEsb | AEsb | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 100 | 35 | 35 | 300 |  |  |  |  |  |
| ANrn | ANrn | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 3 | 240 | 0 | 0 | - |  |  |  |  |  |
| ANdc | ANdc | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 5 | 0.1 | 6 | 6 | 0 |  |  |  |  |  |
| SNdc | SNdc | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 0.1 | 6 | 6 | 0 |  |  |  |  |  |
| ANsl | ANsl | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0.5 | 0.1 | 3 | 3 | 0 |  |  |  |  |  |
| AEIl | AEIl | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 1.5 | 0 | 150 |  |  |  |  |  |
| AEvi | AEvi | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 120 | 1.5 | 60 | 150 |  |  |  |  |  |
| SNeq | SNeq | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 90 | 25 | 20 | 150 |  |  |  |  |  |
| SNdd | SNdd | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 150 | 35 | 35 | 250 |  |  |  |  |  |
| ANmo | ANmo | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 12 | 12 | 75 |  |  |  |  |  |
| AEpa | AEpa | 3 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| ANwm | ANwm | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 60 | 60 | 75 |  |  |  |  |  |
| AHca | AHca | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 6 | 1 | 8 |  |  |  |  |  |
| ANbr | ANbr | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 25 | 10 | 10 | 50 |  |  |  |  |  |
| Arsg | Arsg | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 30 | 0 | 0 | 150 |  |  |  |  |  |
| Aamk | Aamk | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arsq | Arsq | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 70 | 70 | 50 |  |  |  |  |  |
| Arsp | Arsp | 2 | 6 | 6 | 0 | 1 | 0 | 1 | 0 | 180 | 30 | 30 | 150 |  |  |  |  |  |
| ANsb | ANsb | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 5 | 3 | 75 |  |  |  |  |  |
| ANhw | ANhw | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 9 | 0 | 0 | 90 |  |  |  |  |  |
| Arsw | Arsw | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6.5 | 40 | 40 | 30 |  |  |  |  |  |
| ANhx | ANhx | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 7 | 15 | 4 | 70 |  |  |  |  |  |
| AOls | AOls | 2 | 6 | 6 | 0 | 1 | 0 | 1 | 0 | 180 | 30 | 30 | 150 |  |  |  |  |  |
| ANcf | ANcf | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 10 | 5 | 5 | 75 |  |  |  |  |  |
| Acdb | Acdb | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Acdh | Acdh | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 12 | 12 | 5 | 75 |  |  |  |  |  |
| Acef | Acef | 2 | 6 | 6 | 0 | 1 | 0 | 1 | 0 | 180 | 60 | 60 | 150 |  |  |  |  |  |
| AOr2 | AOr2 | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| AOr3 | AOr3 | 2 | 6 | 6 | 0 | 1 | 0 | 1 | 3 | 240 | 0 | 0 | - |  |  |  |  |  |
| AOs2 | AOs2 | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| AOw2 | AOw2 | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 6 | 3 | 2 | 90 |  |  |  |  |  |
| ANcl | ANcl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aadm | Aadm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| Andm | Andm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| ACdm | ACdm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| ACd2 | ACd2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| Aabs | Aabs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aaha | Aaha | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANav | ANav | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 0 | 180 | 60 | 60 | 150 |  |  |  |  |  |
| Aalr | Aalr | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 3 | 0 |  |  |  |  |  |
| Aall | Aall | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aast | Aast | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0 | 0 | 250 |  |  |  |  |  |
| ACad | ACad | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 240 | 120 | 120 | 250 |  |  |  |  |  |
| Aams | Aams | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 90 | 90 | 50 |  |  |  |  |  |
| ACam | ACam | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 90 | 90 | 50 |  |  |  |  |  |
| Aatk | Aatk | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACba | ACba | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACac | ACac | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACav | ACav | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| SCae | SCae | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aap1 | Aap1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aap2 | Aap2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aap3 | Aap3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aap4 | Aap4 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aoar | Aoar | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aabr | Aabr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aasl | Aasl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACat | ACat | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aakb | Aakb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aave | Aave | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 1.1 | 0 | 0 |  |  |  |  |  |
| Aawa | Aawa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Abof | Abof | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 2.51 | 1.01 | 125 |  |  |  |  |  |
| ACbn | ACbn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 24 | 6 | 50 |  |  |  |  |  |
| ACbh | ACbh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| ANbh | ANbh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| ANb2 | ANb2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| Abtl | Abtl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sbtl | Sbtl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Abrf | Abrf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.45 | 0 | 25 |  |  |  |  |  |
| Absk | Absk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 12 | 12 | 0 |  |  |  |  |  |
| Sbsk | Sbsk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACbk | ACbk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 2 | 2 | 6 |  |  |  |  |  |
| Abds | Abds | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.08 | 0.08 | 0 |  |  |  |  |  |
| Abdl | Abdl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.08 | 0.08 | 0 |  |  |  |  |  |
| Abgs | Abgs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.08 | 0.08 | 0 |  |  |  |  |  |
| Abgl | Abgl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.08 | 0.08 | 0 |  |  |  |  |  |
| Abgm | Abgm | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANbl | ANbl | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 0.33 | 0 | 0 |  |  |  |  |  |
| ACbz | ACbz | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 12 | 0 | 0 | 125 |  |  |  |  |  |
| Ablo | Ablo | 1 | 0 | 0 | 11 | 0 | 0 | 1 | 0 | 0 | 60 | 60 | 40 |  |  |  |  |  |
| ACbl | ACbl | 1 | 0 | 0 | 11 | 0 | 0 | 1 | 0 | 8 | 40 | 40 | 50 |  |  |  |  |  |
| ACbb | ACbb | 1 | 0 | 0 | 11 | 0 | 0 | 1 | 0 | 8 | 40 | 40 | 50 |  |  |  |  |  |
| ACbc | ACbc | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 5 | 5 | 125 |  |  |  |  |  |
| ACbf | ACbf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 5 | 5 | 125 |  |  |  |  |  |
| ANbu | ANbu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AHbu | AHbu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AObu | AObu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AEbu | AEbu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AUbu | AUbu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AGbu | AGbu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Abur | Abur | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.45 | 0 | 0 |  |  |  |  |  |
| Abu2 | Abu2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.45 | 0 | 0 |  |  |  |  |  |
| Abu3 | Abu3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.45 | 0 | 0 |  |  |  |  |  |
| Abu5 | Abu5 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.45 | 0 | 0 |  |  |  |  |  |
| Abdt | Abdt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Acan | Acan | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 33 | 33 | 0 |  |  |  |  |  |
| Acn2 | Acn2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 33 | 33 | 0 |  |  |  |  |  |
| ACcn | ACcn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 33 | 33 | 0 |  |  |  |  |  |
| Abun | Abun | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 |  |  |  |  |  |
| Advc | Advc | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.5 | 0.5 | 0 |  |  |  |  |  |
| Sch2 | Sch2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.5 | 0.5 | 0 |  |  |  |  |  |
| Sch5 | Sch5 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sch4 | Sch4 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.5 | 0.5 | 0 |  |  |  |  |  |
| Sch3 | Sch3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.5 | 0.5 | 0 |  |  |  |  |  |
| Aenc | Aenc | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Achd | Achd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 60 | 30 | 50 |  |  |  |  |  |
| ACca | ACca | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| ACcv | ACcv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| ACc2 | ACc2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| ACc3 | ACc3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 0 | 0 | 125 |  |  |  |  |  |
| ACcl | ACcl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 0 | 0 | 120 |  |  |  |  |  |
| Ache | Ache | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 0 | 0 | 75 |  |  |  |  |  |
| Sca1 | Sca1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sca2 | Sca2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sca3 | Sca3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sca4 | Sca4 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sca5 | Sca5 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sca6 | Sca6 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Achl | Achl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACch | ACch | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 20 | 0 | 0 | 125 |  |  |  |  |  |
| ACce | ACce | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aclf | Aclf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 20 | 30 | 30 | 100 |  |  |  |  |  |
| ACcw | ACcw | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 3 | 0.75 | 5 |  |  |  |  |  |
| Acmg | Acmg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 60 | 60 | 25 |  |  |  |  |  |
| Acpf | Acpf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0.7 | 0 | 0 |  |  |  |  |  |
| Acor | Acor | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 5 | 5 | 0 |  |  |  |  |  |
| Acoa | Acoa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Acoh | Acoh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aco2 | Aco2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0 | 0 | 0 |  |  |  |  |  |
| Aco3 | Aco3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0 | 0 | 0 |  |  |  |  |  |
| ACsp | ACsp | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Acri | Acri | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 60 | 10 | 175 |  |  |  |  |  |
| Scri | Scri | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 60 | 10 | 175 |  |  |  |  |  |
| ACcr | ACcr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 60 | 10 | 175 |  |  |  |  |  |
| ACct | ACct | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Acrs | Acrs | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 1 | 120 | 60 | 40 |  |  |  |  |  |
| ACcs | ACcs | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 5 | 60 | 30 | 40 |  |  |  |  |  |
| Acyc | Acyc | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 20 | 6 | 150 |  |  |  |  |  |
| Acny | Acny | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 20 | 6 | 150 |  |  |  |  |  |
| ACcy | ACcy | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 20 | 6 | 150 |  |  |  |  |  |
| SCc1 | SCc1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 30 | 6 | 150 |  |  |  |  |  |
| ACdc | ACdc | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 75 |  |  |  |  |  |
| Adda | Adda | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.3 | 0.3 | 0 |  |  |  |  |  |
| Amnx | Amnx | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.3 | 0.3 | 0 |  |  |  |  |  |
| Amnz | Amnz | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.3 | 0.3 | 0 |  |  |  |  |  |
| Adec | Adec | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0 | 0 | 0 |  |  |  |  |  |
| Adef | Adef | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adt1 | Adt1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Atru | Atru | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adtg | Adtg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANtr | ANtr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Agyv | Agyv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adts | Adts | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adtn | Adtn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adev | Adev | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACdv | ACdv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Advm | Advm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 0 |  |  |  |  |  |
| ACde | ACde | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 0 | 0 | 0 |  |  |  |  |  |
| Adch | Adch | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| Adcn | Adcn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 125 |  |  |  |  |  |
| Adis | Adis | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 75 |  |  |  |  |  |
| Adsm | Adsm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 75 |  |  |  |  |  |
| ACds | ACds | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 60 | 10 | 10 | 125 |  |  |  |  |  |
| ACdr | ACdr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 8 | 8 | 75 |  |  |  |  |  |
| Adri | Adri | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Adro | Adro | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sdro | Sdro | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Atdp | Atdp | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aeat | Aeat | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| ANen | ANen | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 12 | 12 | 12 | 0 |  |  |  |  |  |
| Aens | Aens | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 12 | 12 | 3 | 0 |  |  |  |  |  |
| ACen | ACen | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 10 | 5 | 0 |  |  |  |  |  |
| Aent | Aent | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aegm | Aegm | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aenr | Aenr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 10 | 10 | 75 |  |  |  |  |  |
| Aenw | Aenw | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 10 | 10 | 75 |  |  |  |  |  |
| Aetl | Aetl | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aetf | Aetf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 0.7 | 0 | 0 |  |  |  |  |  |
| ACev | ACev | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| ACes | ACes | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| Aexh | Aexh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 15 | 0 | 0 |  |  |  |  |  |
| Afae | Afae | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0.5 | 120 | 120 | 35 |  |  |  |  |  |
| Afa2 | Afa2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0.5 | 120 | 120 | 35 |  |  |  |  |  |
| ACff | ACff | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 5 | 90 | 90 | 35 |  |  |  |  |  |
| Afbk | Afbk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afbt | Afbt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afbb | Afbb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACsf | ACsf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 60 | 60 | 100 |  |  |  |  |  |
| ACs9 | ACs9 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 60 | 60 | 100 |  |  |  |  |  |
| ACs8 | ACs8 | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 20 | 60 | 60 | 125 |  |  |  |  |  |
| ACs7 | ACs7 | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 15 | 60 | 60 | 100 |  |  |  |  |  |
| Afod | Afod | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| Awfb | Awfb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 2 | 2 | 75 |  |  |  |  |  |
| ACfb | ACfb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 2 | 2 | 75 |  |  |  |  |  |
| Aflk | Aflk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afla | Afla | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 15 | 15 | 0 |  |  |  |  |  |
| ACfs | ACfs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.33 | 10 | 6 | 1.67 | 100 |  |  |  |  |  |
| ANfs | ANfs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.33 | 10 | 8 | 2.67 | 100 |  |  |  |  |  |
| ACfr | ACfr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 20 | 60 | 60 | 175 |  |  |  |  |  |
| ACfl | ACfl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 11 | 0.7 | 0 | 110 |  |  |  |  |  |
| Afsh | Afsh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afrz | Afrz | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 5 | 5 | 0 |  |  |  |  |  |
| Afzy | Afzy | 1 | 0 | 0 | 11 | 0 | 0 | 1 | 0 | 30 | 20 | 20 | 0 |  |  |  |  |  |
| ACfa | ACfa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 5 | 5 | 40 |  |  |  |  |  |
| ACf2 | ACf2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 5 | 5 | 40 |  |  |  |  |  |
| ACfu | ACfu | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 5 | 5 | 40 |  |  |  |  |  |
| Afra | Afra | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 5 | 5 | 0 |  |  |  |  |  |
| Afrb | Afrb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| ACfn | ACfn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 4 | 4 | 125 |  |  |  |  |  |
| ACcb | ACcb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 5 | 3 | 75 |  |  |  |  |  |
| Agho | Agho | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aeth | Aeth | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Agld | Agld | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Agra | Agra | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 0 | 0 | 0 |  |  |  |  |  |
| Agyd | Agyd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| Agyb | Agyb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Assk | Assk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ansk | Ansk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ahar | Ahar | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.1 | 0 | 0 |  |  |  |  |  |
| ANha | ANha | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 |  |  |  |  |  |
| Ahrl | Ahrl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.35 | 0 | 0 |  |  |  |  |  |
| Ahr3 | Ahr3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.35 | 0 | 0 |  |  |  |  |  |
| Ahr2 | Ahr2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1.35 | 0 | 0 |  |  |  |  |  |
| Ahea | Ahea | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 4 |  |  |  |  |  |
| Anhe | Anhe | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 5 |  |  |  |  |  |
| Anh1 | Anh1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 5 |  |  |  |  |  |
| Anh2 | Anh2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 5 |  |  |  |  |  |
| Ahwd | Ahwd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 30 | 30 | 200 |  |  |  |  |  |
| AChw | AChw | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 30 | 30 | 200 |  |  |  |  |  |
| AChv | AChv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 0 | 0 | 90 |  |  |  |  |  |
| Ahnl | Ahnl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 50 |  |  |  |  |  |
| AHer | AHer | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AChx | AChx | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 17 | 15 | 5 | 100 |  |  |  |  |  |
| Acht | Acht | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 12 | 10 | 10 | 100 |  |  |  |  |  |
| ACim | ACim | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 25 |  |  |  |  |  |
| ACmp | ACmp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 11 | 2 | 1 | 100 |  |  |  |  |  |
| Aimp | Aimp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ainf | Ainf | 1 | 0 | 0 | 10 | 0 | 0 | 1 | 0 | 1 | 60 | 60 | 35 |  |  |  |  |  |
| ACif | ACif | 1 | 0 | 0 | 10 | 0 | 0 | 1 | 0 | 5 | 60 | 60 | 35 |  |  |  |  |  |
| Aivs | Aivs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 120 | 120 | 50 |  |  |  |  |  |
| AInv | AInv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Apak | Apak | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aion | Aion | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aihn | Aihn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aien | Aien | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aiun | Aiun | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Avul | Avul | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Alit | Alit | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Alsh | Alsh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 20 | 20 | 100 |  |  |  |  |  |
| ACls | ACls | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 20 | 20 | 100 |  |  |  |  |  |
| Aliq | Aliq | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 3 | 3 | 0 |  |  |  |  |  |
| Aloa | Aloa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sloa | Sloa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Slo2 | Slo2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Slo3 | Slo3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Atlp | Atlp | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aloc | Aloc | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amdf | Amdf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amim | Amim | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACmi | ACmi | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACm2 | ACm2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACm3 | ACm3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amls | Amls | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 30 | 40 | 10 | 75 |  |  |  |  |  |
| Ambt | Ambt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amb2 | Amb2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amnb | Amnb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 0 | 0 | 50 |  |  |  |  |  |
| Ambd | Ambd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 0 | 0 | 50 |  |  |  |  |  |
| Ambb | Ambb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 9 | 0 | 0 | 50 |  |  |  |  |  |
| Amfl | Amfl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0.75 | 20 | 30 | 30 | 50 |  |  |  |  |  |
| ACmf | ACmf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 0 | 0 | 25 |  |  |  |  |  |
| Amed | Amed | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amel | Amel | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amil | Amil | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| Amic | Amic | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANmr | ANmr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 30 | 10 | 0 |  |  |  |  |  |
| Amin | Amin | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACmo | ACmo | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 15 | 15 | 75 |  |  |  |  |  |
| Amgl | Amgl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amgr | Amgr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Amov | Amov | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aneu | Aneu | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ane2 | Ane2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Andt | Andt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 6 | 6 | 0 |  |  |  |  |  |
| ANre | ANre | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACnr | ACnr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ansp | Ansp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| Afak | Afak | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 25 |  |  |  |  |  |
| ANak | ANak | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 15 |  |  |  |  |  |
| Afir | Afir | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afih | Afih | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afio | Afio | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afin | Afin | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Afiu | Afiu | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANpa | ANpa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 90 | 5 | 30 | 0 | 50 |  |  |  |  |  |
| ACpa | ACpa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 90 | 5 | 30 | 0 | 50 |  |  |  |  |  |
| ANpi | ANpi | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 0 |  |  |  |  |  |
| Apmf | Apmf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 0 |  |  |  |  |  |
| Apig | Apig | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 0 |  |  |  |  |  |
| Apiv | Apiv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| Apsh | Apsh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6.5 | 1.5 | 1.5 | 20 |  |  |  |  |  |
| Aphx | Aphx | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0.7 | 0 | 0.5 | 10 | 0 |  |  |  |  |  |
| Apxf | Apxf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0.5 | 10 | 7 | 0 |  |  |  |  |  |
| Apts | Apts | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| Apoi | Apoi | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| Aply | Aply | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 60 | 60 | 220 |  |  |  |  |  |
| ACpy | ACpy | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 10 | 25 | 25 | 220 |  |  |  |  |  |
| Apos | Apos | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0.6 | 0 | 0 | 0 | 250 |  |  |  |  |  |
| ACps | ACps | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 250 |  |  |  |  |  |
| Awar | Awar | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACpv | ACpv | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Apit | Apit | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aprg | Aprg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 15 | 5 | 75 |  |  |  |  |  |
| ACpu | ACpu | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 5 | 5 | 75 |  |  |  |  |  |
| ACrf | ACrf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 12 | 0 | 0 | 125 |  |  |  |  |  |
| ACrg | ACrg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 12 | 3 | 3 | 125 |  |  |  |  |  |
| Arai | Arai | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 40 | 40 | 75 |  |  |  |  |  |
| ACrd | ACrd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 40 | 40 | 75 |  |  |  |  |  |
| ARal | ARal | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arav | Arav | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.05 | 0 | 0.6 | 0 | 50 |  |  |  |  |  |
| Amrf | Amrf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.05 | 0 | 0.6 | 0 | 0 |  |  |  |  |  |
| ACrn | ACrn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 | 240 | 0 | 0 | - |  |  |  |  |  |
| ANr2 | ANr2 | 1 | 6 | 0 | 0 | 1 | 0 | 1 | 3 | 240 | 0 | 0 | - |  |  |  |  |  |
| Arbr | Arbr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arej | Arej | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 12 | 12 | 125 |  |  |  |  |  |
| ACrj | ACrj | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 12 | 12 | 125 |  |  |  |  |  |
| ACr2 | ACr2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 12 | 12 | 150 |  |  |  |  |  |
| Aren | Aren | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ahrp | Ahrp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arep | Arep | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arpb | Arpb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 1 | 1 | 6 |  |  |  |  |  |
| Arpl | Arpl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 1 | 1 | 2 |  |  |  |  |  |
| Arpm | Arpm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 1 | 1 | 2 |  |  |  |  |  |
| Arsk | Arsk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACrk | ACrk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACsk | ACsk | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arst | Arst | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Argd | Argd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Argl | Argl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arlm | Arlm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AHta | AHta | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 180 | 15 | 15 | 0 |  |  |  |  |  |
| Arng | Arng | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arev | Arev | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aroa | Aroa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 45 | 45 | 100 |  |  |  |  |  |
| Ara2 | Ara2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 45 | 45 | 100 |  |  |  |  |  |
| ACr1 | ACr1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 45 | 45 | 100 |  |  |  |  |  |
| ACro | ACro | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 45 | 45 | 100 |  |  |  |  |  |
| Aroc | Aroc | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 1 | 1 | 0 |  |  |  |  |  |
| Aro1 | Aro1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2.5 | 2.5 | 0 |  |  |  |  |  |
| Aro2 | Aro2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2.5 | 2.5 | 0 |  |  |  |  |  |
| Asac | Asac | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asal | Asal | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Alam | Alam | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACsa | ACsa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 8 |  |  |  |  |  |
| Asds | Asds | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.1 | 0.1 | 0 |  |  |  |  |  |
| Asid | Asid | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asud | Asud | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aesn | Aesn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 120 | 0 | 0 | 0 |  |  |  |  |  |
| Aesr | Aesr | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 60 | 0 | 0 | 0 |  |  |  |  |  |
| Aeye | Aeye | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 600 | 600 | 50 |  |  |  |  |  |
| ACtn | ACtn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 180 | 180 | 5 |  |  |  |  |  |
| Ashm | Ashm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Sshm | Sshm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ahid | Ahid | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACss | ACss | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 | 8 | 15.1 | 15.1 | 65 |  |  |  |  |  |
| ACsh | ACsh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| ACst | ACst | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 0 | 0 | 0 |  |  |  |  |  |
| ANsh | ANsh | 3 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 8 | 0 | 0 | 100 |  |  |  |  |  |
| ACsi | ACsi | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 20 | 40 | 8.5 | 75 |  |  |  |  |  |
| ACsm | ACsm | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 8 | 8 | 25 |  |  |  |  |  |
| ACsl | ACsl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 10 | 5 | 125 |  |  |  |  |  |
| Asla | Asla | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aslo | Aslo | 1 | 0 | 0 | 3 | 0 | 0 | 1 | 0 | 0 | 60 | 10 | 40 |  |  |  |  |  |
| ACsw | ACsw | 1 | 0 | 0 | 3 | 0 | 0 | 1 | 0 | 8 | 20 | 10 | 40 |  |  |  |  |  |
| Aspo | Aspo | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 5 | 1 | 0 |  |  |  |  |  |
| Asod | Asod | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Assp | Assp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aspd | Aspd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aspy | Aspy | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aspt | Aspt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asps | Asps | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 0 | 0 | 60 |  |  |  |  |  |
| Asph | Asph | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp1 | Asp1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp2 | Asp2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp3 | Asp3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp4 | Asp4 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp5 | Asp5 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Asp6 | Asp6 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.01 | 0.01 | 0 |  |  |  |  |  |
| Aspa | Aspa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 0 |  |  |  |  |  |
| Aspi | Aspi | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aspl | Aspl | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 75 | 75 | 100 |  |  |  |  |  |
| Astd | Astd | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asta | Asta | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 150 | 2.5 | 100 |  |  |  |  |  |
| Astn | Astn | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0.7 | 30 | 0.5 | 0 | 0 |  |  |  |  |  |
| Asth | Asth | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asb1 | Asb1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.67 | 0 | 0 |  |  |  |  |  |
| Asb2 | Asb2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.7 | 0 | 0 |  |  |  |  |  |
| Asb3 | Asb3 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.6 | 0 | 0 |  |  |  |  |  |
| Aslp | Aslp | 1 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 30 | 60 | 60 | 75 |  |  |  |  |  |
| ACwe | ACwe | 1 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 30 | 65 | 65 | 125 |  |  |  |  |  |
| Attu | Attu | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| Srtt | Srtt | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Atau | Atau | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| ANta | ANta | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| ANth | ANth | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| ANt2 | ANt2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| ACah | ACah | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| ACtb | ACtb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 8 | 2 | 2 | 75 |  |  |  |  |  |
| ACtc | ACtc | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 4 | 2 | 90 |  |  |  |  |  |
| ACt2 | ACt2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 4 | 2 | 90 |  |  |  |  |  |
| Atdg | Atdg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Atsp | Atsp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 12 | 6 | 0 |  |  |  |  |  |
| Atwa | Atwa | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| Atol | Atol | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ault | Ault | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACua | ACua | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| Auhf | Auhf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 45 | 45 | 50 |  |  |  |  |  |
| Suhf | Suhf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 3 | 45 | 45 | 50 |  |  |  |  |  |
| ACuf | ACuf | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 5 | 45 | 45 | 50 |  |  |  |  |  |
| Auco | Auco | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Auns | Auns | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| SCva | SCva | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ACvp | ACvp | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | - |  |  |  |  |  |
| Avng | Avng | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 50 | 50 | 25 |  |  |  |  |  |
| Awan | Awan | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Awrs | Awrs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 3 | 2 | 90 |  |  |  |  |  |
| Awrg | Awrg | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 3 | 2 | 90 |  |  |  |  |  |
| Awrh | Awrh | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 6 | 3 | 2 | 90 |  |  |  |  |  |
| ANwk | ANwk | 1 | 1 | 0 | 0 | 1 | 0 | 1 | 0 | 7 | 20 | 20 | 75 |  |  |  |  |  |
| Awha | Awha | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 8 | 8 | 0 |  |  |  |  |  |
| Awh2 | Awh2 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 8 | 8 | 0 |  |  |  |  |  |
| Aven | Aven | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 25 | 1 | 0 |  |  |  |  |  |
| ACvs | ACvs | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 20 | 10 | 0 |  |  |  |  |  |
| Awrp | Awrp | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aweb | Aweb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 12 | 12 | 7 | 0 |  |  |  |  |  |
| ACwb | ACwb | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 12 | 12 | 7 | 0 |  |  |  |  |  |
| AIa1 | AIa1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIa3 | AIa3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIa4 | AIa4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIa6 | AIa6 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIx5 | AIx5 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIx1 | AIx1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIx2 | AIx2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIs1 | AIs1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIs3 | AIs3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIs4 | AIs4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIs6 | AIs6 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIi1 | AIi1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIi3 | AIi3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIi4 | AIi4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIi6 | AIi6 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIxm | AIxm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIam | AIam | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIim | AIim | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIsm | AIsm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIgm | AIgm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItm | AItm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AInm | AInm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIaa | AIaa | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIat | AIat | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIt6 | AIt6 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIt9 | AIt9 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItc | AItc | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItf | AItf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItg | AItg | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIth | AIth | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIti | AIti | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItj | AItj | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItk | AItk | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItl | AItl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AItn | AItn | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIva | AIva | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIbk | AIbk | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 30 | 0.33 | 0 | 0 |  |  |  |  |  |
| AIbl | AIbl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 0 | 0 |  |  |  |  |  |
| AIbg | AIbg | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 20 | 0 | 0 |  |  |  |  |  |
| AIbt | AIbt | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 4 | 0 | 0 |  |  |  |  |  |
| AIbb | AIbb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 15 | 0 | 0 |  |  |  |  |  |
| AIbf | AIbf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 5 | 0 | 0 |  |  |  |  |  |
| AIbr | AIbr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 10 | 0 | 0 |  |  |  |  |  |
| AIbs | AIbs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 15 | 0 | 0 |  |  |  |  |  |
| AIbh | AIbh | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 15 | 0 | 0 |  |  |  |  |  |
| AIcy | AIcy | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 20 | 5.6 | 0 |  |  |  |  |  |
| AId1 | AId1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId2 | AId2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId3 | AId3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId4 | AId4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId5 | AId5 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIgf | AIgf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIgu | AIgu | 2 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIem | AIem | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIe2 | AIe2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfd | AIfd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 0 | 0 |  |  |  |  |  |
| AIff | AIff | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 0 | 0 |  |  |  |  |  |
| AIfr | AIfr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 0 | 0 |  |  |  |  |  |
| AIfu | AIfu | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 0 | 0 |  |  |  |  |  |
| AIfh | AIfh | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 0 | 0 |  |  |  |  |  |
| AIfs | AIfs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 120 | 60 | 0 |  |  |  |  |  |
| AIir | AIir | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 180 | 0 |  |  |  |  |  |
| AIuw | AIuw | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 180 | 0 |  |  |  |  |  |
| AIfl | AIfl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfm | AIfm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfo | AIfo | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfn | AIfn | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfe | AIfe | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfx | AIfx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfa | AIfa | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIin | AIin | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 4 | 2 | 0 |  |  |  |  |  |
| AIlm | AIlm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIlp | AIlp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 3 | 0.3 | 0 |  |  |  |  |  |
| AIlf | AIlf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIl1 | AIl1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIl2 | AIl2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIms | AIms | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANbs | ANbs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 3 | 0 | 0 |  |  |  |  |  |
| AIdf | AIdf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcb | AIcb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 5 | 5 | 0 |  |  |  |  |  |
| AIdn | AIdn | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfb | AIfb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIgd | AIgd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIzb | AIzb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIob | AIob | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 3 | 1 | 0 |  |  |  |  |  |
| AIll | AIll | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIlb | AIlb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIsb | AIsb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIpb | AIpb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Apo2 | Apo2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 10 | 10 | 0 |  |  |  |  |  |
| AInd | AInd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 90 | 60 | 60 | 0 |  |  |  |  |  |
| Arel | Arel | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Arll | Arll | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIsi | AIsi | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIos | AIos | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 60 | 20 | 0 |  |  |  |  |  |
| AIso | AIso | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Asou | Asou | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcf | AIcf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 1 | 1 | 0 |  |  |  |  |  |
| AIco | AIco | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIdm | AIdm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIda | AIda | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIdb | AIdb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIta | AIta | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 10 | 10 | 0 |  |  |  |  |  |
| AIdi | AIdi | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIds | AIds | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 7.5 | 0 | 0 | 0 |  |  |  |  |  |
| APdi | APdi | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIh1 | AIh1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 0 | 0 | 0 |  |  |  |  |  |
| AIh2 | AIh2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| AIh3 | AIh3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 0 | 0 | 0 |  |  |  |  |  |
| AIha | AIha | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 25 | 0 | 0 | 0 |  |  |  |  |  |
| AIhb | AIhb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APh1 | APh1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APh2 | APh2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APh3 | APh3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIhw | AIhw | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIsw | AIsw | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 600 | 600 | 0 |  |  |  |  |  |
| AIil | AIil | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 60 | 60 | 0 |  |  |  |  |  |
| AIv1 | AIv1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 120 | 120 | 0 |  |  |  |  |  |
| AIv2 | AIv2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 180 | 180 | 0 |  |  |  |  |  |
| AIvu | AIvu | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 45 | 15 | 15 | 0 |  |  |  |  |  |
| AIvl | AIvl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 45 | 7 | 7 | 0 |  |  |  |  |  |
| AIm1 | AIm1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 0 | 0 | 0 |  |  |  |  |  |
| AIm2 | AIm2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| AImr | AImr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APmr | APmr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APmg | APmg | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIpm | AIpm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrt | AIrt | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrm | AIrm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrn | AIrn | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrc | AIrc | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIre | AIre | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| AIra | AIra | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| APra | APra | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| AIsp | AIsp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIsa | AIsa | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 10 | 10 | 0 |  |  |  |  |  |
| APsa | APsa | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 60 | 60 | 0 |  |  |  |  |  |
| AItp | AItp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIad | AIad | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcd | AIcd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIba | AIba | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIav | AIav | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIar | AIar | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIae | AIae | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIau | AIau | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIuv | AIuv | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIls | AIls | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 20 | 20 | 0 |  |  |  |  |  |
| AIxs | AIxs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 30 | 10 | 10 | 0 |  |  |  |  |  |
| AIan | AIan | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 40 | 40 | 0 |  |  |  |  |  |
| AIrs | AIrs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrr | AIrr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIev | AIev | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AImx | AImx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AImh | AImh | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AImb | AImb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIbm | AIbm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIsx | AIsx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIs2 | AIs2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrl | AIrl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIpr | AIpr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIsl | AIsl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIpl | AIpl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIp1 | AIp1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIp2 | AIp2 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIp3 | AIp3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIp4 | AIp4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 30 | 30 | 0 |  |  |  |  |  |
| AIp5 | AIp5 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIp6 | AIp6 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 45 | 45 | 0 |  |  |  |  |  |
| AIgo | AIgo | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIlu | AIlu | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrv | AIrv | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 5 | 5 | 0 |  |  |  |  |  |
| AIdc | AIdc | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIwb | AIwb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 20 | 7 | 0 |  |  |  |  |  |
| AImo | AImo | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 60 | 60 | 75 |  |  |  |  |  |
| AIct | AIct | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 70 | 30 | 0 | 0 |  |  |  |  |  |
| AIri | AIri | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIsr | AIsr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ablp | Ablp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.08 | 0.08 | 0 |  |  |  |  |  |
| AIpv | AIpv | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 20 | 20 | 0 |  |  |  |  |  |
| Aste | Aste | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 10 | 0 | 0 | 0 |  |  |  |  |  |
| Amec | Amec | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Ashs | Ashs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| ANpr | ANpr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 15 | 0 | 0 | 0 |  |  |  |  |  |
| ANsa | ANsa | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 45 | 0 | 0 | 0 |  |  |  |  |  |
| ANss | ANss | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 40 | 0 | 0 | 0 |  |  |  |  |  |
| ANse | ANse | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aret | Aret | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AImt | AImt | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 90 | 0 | 0 | 0 |  |  |  |  |  |
| Aspb | Aspb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrd | AIrd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 15 | 65 | 65 | 0 |  |  |  |  |  |
| AItb | AItb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 20 | 20 | 0 |  |  |  |  |  |
| AIdv | AIdv | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 25 | 25 | 0 |  |  |  |  |  |
| AIse | AIse | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 12 | 6 | 0 |  |  |  |  |  |
| AIpg | AIpg | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 15 | 5 | 0 |  |  |  |  |  |
| AIps | AIps | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 8 | 15 | 5 | 0 |  |  |  |  |  |
| AIfg | AIfg | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 15 | 30 | 30 | 0 |  |  |  |  |  |
| APrl | APrl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| APrr | APrr | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrb | AIrb | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| Aspp | Aspp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 75 | 75 | 0 |  |  |  |  |  |
| AUds | AUds | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 90 | 0 | 0 | 0 |  |  |  |  |  |
| APwt | APwt | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIuf | AIuf | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 1 | 45 | 45 | 50 |  |  |  |  |  |
| AId0 | AId0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcm | AIcm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 5 | 60 | 60 | 25 |  |  |  |  |  |
| AImz | AImz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfz | AIfz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 30 | 0 | 0 | 0 |  |  |  |  |  |
| AIdp | AIdp | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 15 | 0 | 0 | 50 |  |  |  |  |  |
| AImv | AImv | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIpx | AIpx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIdd | AIdd | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId7 | AId7 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AId8 | AId8 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIlz | AIlz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIhx | AIhx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIaz | AIaz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIrx | AIrx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 240 | 0 | 0 | 0 |  |  |  |  |  |
| AIbx | AIbx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 2 | 2 | 0 |  |  |  |  |  |
| AItx | AItx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIwm | AIwm | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 20 | 60 | 60 | 0 |  |  |  |  |  |
| AIsh | AIsh | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 60 | 60 | 60 | 0 |  |  |  |  |  |
| AI2m | AI2m | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIgx | AIgx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIhl | AIhl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 5 | 0 | 0 | 0 |  |  |  |  |  |
| AIsz | AIsz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 5 | 1 | 0 |  |  |  |  |  |
| AIpz | AIpz | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIfw | AIfw | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIft | AIft | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 3 | 1 | 0 |  |  |  |  |  |
| AIlx | AIlx | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcs | AIcs | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIcl | AIcl | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 9 | 0 | 0 | 0 |  |  |  |  |  |
| AIx3 | AIx3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIx4 | AIx4 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 |  |  |  |  |  |
| AIxk | AIxk | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 30 | 12 | 12 | 0 |  |  |  |  |  |

## ItemData (273 rows)

| itemID | itemID | Level | goldcost | lumbercost | stockMax | stockRegen | stockStart | usable | droppable | pawnable | sellable | powerup | perishable | drop | HP | morph |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ckng | ckng | 8 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| modt | modt | 8 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tkno | tkno | 8 | 1250 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| ratf | ratf | 7 | 800 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rde4 | rde4 | 7 | 800 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ofro | ofro | 7 | 800 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| fgdg | fgdg | 6 | 750 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| infs | infs | 6 | 750 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| shar | shar | 6 | 750 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sand | sand | 6 | 750 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sres | sres | 6 | 750 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wild | wild | 6 | 750 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| srrc | srrc | 6 | 750 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej6 | rej6 | 6 | 500 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| odef | odef | 6 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rde3 | rde3 | 6 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| pmna | pmna | 6 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rhth | rhth | 6 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ssil | ssil | 6 | 500 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| spsh | spsh | 6 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| desc | desc | 7 | 400 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ankh | ankh | 5 | 800 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pdiv | pdiv | 5 | 600 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pres | pres | 5 | 600 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| whwd | whwd | 5 | 600 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej5 | rej5 | 5 | 400 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| hcun | hcun | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| hval | hval | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| mcou | mcou | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| belv | belv | 4 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| bgst | bgst | 4 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ciri | ciri | 4 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ajen | ajen | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| clfm | clfm | 5 | 600 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ratc | ratc | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ward | ward | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| kpin | kpin | 5 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| crys | crys | 5 | 500 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| totw | totw | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| fgfh | fgfh | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| fgrd | fgrd | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| fgrg | fgrg | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| fgsk | fgsk | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wcyc | wcyc | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| hlst | hlst | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| mnst | mnst | 4 | 450 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej4 | rej4 | 4 | 450 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| ram4 | ram4 | 4 | 750 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| lhst | lhst | 4 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| afac | afac | 4 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| lgdh | lgdh | 4 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sbch | sbch | 4 | 450 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| brac | brac | 4 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rwiz | rwiz | 4 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| dsum | dsum | 4 | 400 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pghe | pghe | 3 | 400 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pgma | pgma | 3 | 400 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pnvu | pnvu | 3 | 400 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sror | sror | 3 | 400 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| woms | woms | 3 | 400 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej3 | rej3 | 2 | 400 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| ofir | ofir | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ocor | ocor | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| oli2 | oli2 | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| oven | oven | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ram3 | ram3 | 3 | 550 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| evtl | evtl | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| penr | penr | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| prvt | prvt | 3 | 350 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rat9 | rat9 | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rde2 | rde2 | 3 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rlif | rlif | 3 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tret | tret | 3 | 300 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tgrh | tgrh | 3 | 600 | 185 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| will | will | 2 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wlsd | wlsd | 2 | 150 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wswd | wswd | 2 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej2 | rej2 | 2 | 150 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| gemt | gemt | 2 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| ram2 | ram2 | 2 | 300 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| stel | stel | 2 | 100 | 0 | 1 | 120 | 220 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| cnob | cnob | 2 | 175 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| gcel | gcel | 2 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rat6 | rat6 | 2 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rde1 | rde1 | 2 | 150 | 0 | 1 | 90 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| bspd | bspd | 2 | 150 | 0 | 1 | 60 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tdx2 | tdx2 | 2 | 300 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| texp | texp | 2 | 500 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tin2 | tin2 | 2 | 300 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tpow | tpow | 2 | 300 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tst2 | tst2 | 2 | 300 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| pnvl | pnvl | 2 | 150 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| stwp | stwp | 2 | 350 | 0 | 2 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wneg | wneg | 2 | 200 | 0 | 2 | 90 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sneg | sneg | 2 | 200 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| wneu | wneu | 2 | 150 | 0 | 2 | 90 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| shea | shea | 2 | 250 | 0 | 2 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sman | sman | 2 | 150 | 0 | 5 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rej1 | rej1 | 1 | 100 | 0 | 0 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 1 |
| pspd | pspd | 1 | 75 | 0 | 1 | 45 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| dust | dust | 1 | 75 | 0 | 1 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| ram1 | ram1 | 1 | 125 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| clsd | clsd | 1 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rag1 | rag1 | 1 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rin1 | rin1 | 1 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rst1 | rst1 | 1 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| manh | manh | 1 | 200 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tdex | tdex | 1 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tint | tint | 1 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| tstr | tstr | 1 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| pinv | pinv | 1 | 100 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| phea | phea | 1 | 150 | 0 | 3 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pman | pman | 1 | 200 | 0 | 2 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| spro | spro | 1 | 150 | 0 | 2 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| hslv | hslv | 1 | 100 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| moon | moon | 1 | 50 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| shas | shas | 1 | 50 | 0 | 1 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| skul | skul | 1 | 50 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| mcri | mcri | 1 | 50 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rnec | rnec | 1 | 150 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tsct | tsct | 1 | 30 | 20 | 3 | 30 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| azhr | azhr | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| bzbe | bzbe | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| bzbf | bzbf | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| ches | ches | 0 | 250 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| cnhn | cnhn | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| glsk | glsk | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| gopr | gopr | 0 | 250 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| k3m1 | k3m1 | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| k3m2 | k3m2 | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| k3m3 | k3m3 | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| ktrm | ktrm | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| kybl | kybl | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| kygh | kygh | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| kymn | kymn | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| kysn | kysn | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| ledg | ledg | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| phlt | phlt | 0 | 500 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| sehr | sehr | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| engs | engs | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| sorf | sorf | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |
| gmfr | gmfr | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| jpnt | jpnt | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| shwd | shwd | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |
| skrt | skrt | 0 | 250 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| thle | thle | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |
| pomn | pomn | 0 | 400 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| wshs | wshs | 0 | 150 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| fgun | fgun | 0 | 125 | 0 | 2 | 45 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| lure | lure | 0 | 200 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| olig | olig | 0 | 450 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| amrc | amrc | 0 | 250 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| ccmd | ccmd | 0 | 1000 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| flag | flag | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 1 | 75 | 0 |
| gobm | gobm | 0 | 225 | 0 | 3 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| gsou | gsou | 0 | 1000 | 0 | 3 | 120 | 0 | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 75 | 0 |
| nflg | nflg | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 1 | 75 | 0 |
| nspi | nspi | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| oflg | oflg | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 1 | 75 | 0 |
| pams | pams | 0 | 100 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| pgin | pgin | 0 | 200 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rat3 | rat3 | 0 | 50 | 0 | 3 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rde0 | rde0 | 0 | 50 | 0 | 3 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rnsp | rnsp | 0 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| soul | soul | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| tels | tels | 0 | 200 | 0 | 3 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tgxp | tgxp | 0 | 1000 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| uflg | uflg | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 1 | 75 | 0 |
| anfg | anfg | 0 | 150 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| brag | brag | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| drph | drph | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| iwbr | iwbr | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| jdrn | jdrn | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| lnrn | lnrn | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| mlst | mlst | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| oslo | oslo | 0 | 550 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sbok | sbok | 0 | 325 | 0 | 1 | 120 | 440 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sksh | sksh | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sprn | sprn | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tmmt | tmmt | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| vddl | vddl | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| spre | spre | 0 | 200 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sfog | sfog | 0 | 200 | 0 | 2 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor1 | sor1 | 0 | 50 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor2 | sor2 | 0 | 100 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor3 | sor3 | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor4 | sor4 | 0 | 300 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor5 | sor5 | 0 | 350 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor6 | sor6 | 0 | 400 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor7 | sor7 | 0 | 550 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor8 | sor8 | 0 | 700 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sor9 | sor9 | 0 | 900 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| sora | sora | 0 | 1250 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| fwss | fwss | 0 | 750 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shtm | shtm | 0 | 600 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| esaz | esaz | 0 | 600 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| btst | btst | 0 | 1000 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 75 | 0 |
| gold | gold | 0 | 150 | 0 | 3 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| lmbr | lmbr | 0 | 150 | 0 | 3 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| gfor | gfor | 0 | 200 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| guvi | guvi | 0 | 125 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rspl | rspl | 0 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rre1 | rre1 | 0 | 100 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rre2 | rre2 | 0 | 300 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| gomn | gomn | 0 | 300 | 0 | 1 | 240 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rsps | rsps | 0 | 300 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rspd | rspd | 0 | 200 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rman | rman | 0 | 100 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rma2 | rma2 | 0 | 300 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rres | rres | 0 | 250 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rreb | rreb | 0 | 250 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rhe1 | rhe1 | 0 | 100 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rhe2 | rhe2 | 0 | 200 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rhe3 | rhe3 | 0 | 300 | 0 | 1 | 90 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rdis | rdis | 0 | 75 | 0 | 1 | 60 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| rwat | rwat | 0 | 75 | 0 | 1 | 60 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 75 | 0 |
| sclp | sclp | 0 | 75 | 0 | 1 | 60 | 0 | 0 | 1 | 0 | 0 | 1 | 1 | 0 | 75 | 0 |
| pclr | pclr | 0 | 160 | 0 | 2 | 45 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| plcl | plcl | 0 | 70 | 0 | 2 | 30 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| silk | silk | 0 | 50 | 0 | 2 | 60 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| vamp | vamp | 0 | 75 | 0 | 2 | 45 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| sreg | sreg | 0 | 100 | 0 | 2 | 90 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| ssan | ssan | 0 | 250 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| tcas | tcas | 0 | 800 | 300 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tbsm | tbsm | 0 | 200 | 50 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tfar | tfar | 0 | 75 | 20 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tlum | tlum | 0 | 150 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tbar | tbar | 0 | 160 | 50 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tbak | tbak | 0 | 180 | 50 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| gldo | gldo | 0 | 450 | 0 | 1 | 120 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| wtlg | wtlg | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| wolg | wolg | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| mgtk | mgtk | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| stre | stre | 0 | 200 | 0 | 1 | 120 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| horl | horl | 0 | 950 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| hbth | hbth | 0 | 4200 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| blba | blba | 0 | 3500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rugt | rugt | 0 | 725 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| frhg | frhg | 0 | 3500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| gvsm | gvsm | 0 | 1400 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| crdt | crdt | 0 | 6400 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| arsc | arsc | 0 | 1000 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| scul | scul | 0 | 950 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| tmsc | tmsc | 0 | 1250 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| dtsb | dtsb | 0 | 3350 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| grsl | grsl | 0 | 1350 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| arsh | arsh | 0 | 3500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shdt | shdt | 0 | 9000 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shhn | shhn | 0 | 3350 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shen | shen | 0 | 650 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| thdm | thdm | 0 | 1190 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| stpg | stpg | 0 | 450 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shrs | shrs | 0 | 150 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| bfhr | bfhr | 0 | 2500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| cosl | cosl | 0 | 10000 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| shcw | shcw | 0 | 950 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| srbd | srbd | 0 | 1650 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| frgd | frgd | 0 | 1400 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| envl | envl | 0 | 450 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| rump | rump | 0 | 100 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| mort | mort | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 75 | 0 |
| srtl | srtl | 0 | 5500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| stwa | stwa | 0 | 600 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| klmm | klmm | 0 | 7500 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| rots | rots | 0 | 1000 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 1 | 0 | 75 | 0 |
| axas | axas | 0 | 3000 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| mnsf | mnsf | 0 | 1800 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| schl | schl | 0 | 4200 | 0 | 1 | 180 | 0 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| asbl | asbl | 0 | 2000 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| kgal | kgal | 0 | 850 | 0 | 1 | 180 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 75 | 0 |
| dphe | dphe | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |
| dkfw | dkfw | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |
| dthb | dthb | 0 | 200 | 0 | 1 | 120 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 75 | 0 |

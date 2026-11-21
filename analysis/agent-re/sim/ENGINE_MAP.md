# ENGINE_MAP — WC3 simulation core (Game.dll TFT 1.27) vs `crates/wc3-sim`

Ledger 404 · generated 2026-11-15 · report-only (no `src/` edits)

| item | value |
|---|---|
| binary | `analysis/tft_files_Files100_Game.dll` (8 671 295 B) |
| sha256 | `d1a80c89597e08e00715dac19a5e906c58b2a351285b7e2ae94bbc5c73f4ad2e` |
| format | PE32 i386, image base `0x6f000000` |
| tools | radare2 6.2.6 + r2ghidra (`pdg`), `rabin2 -z`, `$PREFIX/tmp/opencode/pe_scan.py` (PE VA scanner written for this pass) |
| method | targeted `af`/`pdf`/`pdg` per function (no `aaa`), string→xref anchors, float/byte pattern scans |
| evidence labels | **observed** = read from bytes/disassembly; **inferred** = derived from observed shape; **reported-unverified** = value not in repo extracts; **unknown** = not located |

Machine-readable numbers: [`constants.json`](constants.json).

---

## 1. Summary table

| subsystem | engine address | r2 command | decompiled snippet (trimmed) | extracted constants / algorithm | our file:line | verdict | proposed test |
|---|---|---|---|---|---|---|---|
| DamageBonus 7×8 table | loader `0x6f19c130`, table `0x6f7d88f8` | `pdg @ 0x6f19c130` | `mov dword [esi*4 + 0x6f7d8918], ecx` (case Normal); jump table `0x6f19c2a0` | rows 7 × cols 8, stride `0x20`; cols SMALL,MEDIUM,LARGE,FORT,NORMAL,HERO,DIVINE,NONE; values = `MiscGame.txt:106-112` | `crates/wc3-sim/src/data.rs:150-168` | **MATCH** (all 56 values) | `damage_table_matches_miscgame`: parse MiscGame, compare every cell to `DAMAGE_MULT_PERMILLE` |
| attack-type id order | `0x6f19c2a0` | `pxw 32 @ 0x6f19c2a0` | jump table → case blocks | engine ids: **0=spells,1=normal,2=pierce,3=siege,4=magic,5=chaos,6=hero** | `data.rs:100-113` (Normal=0…) | **DIFF** (ids), values MATCH | `attack_type_engine_id_map` constant table + round-trip test |
| Armor rating formula (a ≥ 0) | helper `0x6f19c0a0` | `pd 40 @ 0x6f19c0a0` | `push "DefenseArmor"` → `call 0x6f37ce60`(mul) → `call 0x6f37d0a0`(add 1.0f) → `call 0x6f37cf40`(div) | `f = armor·0.06 / (1 + armor·0.06)`; caller converts with `1 − f` ⇒ multiplier `1/(1+0.06a)` | `data.rs:181-195` (`damage*100/(100+6a)`) | **MATCH** | `armor_positive_formula_matches_engine` (a=0..30, compare to 1/(1+0.06a)) |
| Armor negative table (a < 0) | table `0x6f7d8c98`, init `0x6f19bcf0..0x6f19c009` | `pd 240 @ 0x6f19bd1a` | `mov edx, 0xffffe890 … mov ecx, 0x6f7d8c9c` (per entry) | 21 multipliers `1 + |c|/1e5`: 1.0, 1.06, 1.1164, 1.16941, 1.21925, 1.26609, 1.31013, 1.35152, 1.39043, 1.42699, 1.46138, 1.4937, 1.52399, 1.55263, 1.57947, 1.6047, 1.62842, 1.65072, 1.67295, 1.69137, 1.70989; index `|armor|` clamped to 20 | `data.rs:172-176` (permille table) | **MATCH** (21/21 to 4 dp) | `neg_armor_table_matches_engine_constants` from `constants.json` |
| Armor application gate | `0x6f19c330` (virtual) | `pdg @ 0x6f19c330` | `if (*(damage + 0x14) == 0x10) { armor step }` | armor applies **only** when damage-type field `+0x14 == 0x10`; table row from `+0x20` | `data.rs:197-204` (always applies) | **DIFF** | `spell_damage_bypasses_armor` + `physical_damage_uses_armor` |
| Ethereal damage bonus | `0x6f7d8c30` | `pd @ 0x6f19c330` | `fld dword [ebx*4 + 0x6f7d8c30]` | 7 floats, `Misc/EtherealDamageBonus = 1.66 ×7` (TFT) | — | **MISSING** | `ethereal_units_take_166pct` |
| Upkeep brackets (defaults) | `0x6f6932b8`, accessor `0x6f008b30` | `pxw 160 @ 0x6f6932b8` | `mov dword [esi*4 + 0x6f7536c0], eax` | rec0 `{40,70}`, rec1 `{50,80}`, rec2 `{90,100}`, rest `10000` sentinel; 10 slots; read from `Misc/UpkeepUsage` | `crates/wc3-sim/src/lib.rs:201-204, 5628` | **MATCH** (TFT 50/80, RoC 40/70) | `upkeep_thresholds_match_engine_defaults` |
| Upkeep taxes | `0x6f008bd0` / `0x6f008c90` | `pd 60 @ 0x6f008b74` | `push "UpkeepGoldTax"` → `mov [0x6f7b4384], eax` | read from `Misc/UpkeepGoldTax` ×10, fallback 0.0f | `lib.rs:203-204` (300/600) | **reported-unverified** | `upkeep_tax_matches_data` once the TFT MiscGame extract lands |
| Harvest/gather orders | natives `0x6f261330/60/b0`, `0x6f260a60` | `pd 26 @ 0x6f260260` | `push "(II)V"; mov edx,"HarvestGold"; mov ecx,0x6f261360; call 0x6f62f140` | harvest is issued through ability-slot dispatcher `0x6f25c2c0(unit, slot 0..3, target, 1)` | `crates/wc3-sim/src/ai.rs:249,259` (`Order::Harvest`) | **DIFF** (ours is a direct order, engine goes via ability slot) | `harvest_issued_through_ability_slot` |
| Guard-post / harvest AI | `0x6f260a90`, `0x6f260b10`, `0x6f261330` | `pd 26 @ 0x6f260260` | `AddGuardPost (IRR)V`, `FillGuardPosts ()V`, `ClearHarvestAI ()V` | native surface located; internal wave logic not decompiled | `ai.rs` (no guard posts) | **MISSING** | `ai_guard_posts_*` (after handler decompile) |
| Pathing registration | `0x6f110d60` ("RegisterPathingFootprint") | `pd 30 @ 0x6f110d50` | `mov edx,"RegisterPathingFootprint"; call 0x6f1103c0` | footprint registry; kinds `A/C/D-pathing`; unit columns `pathTex`, `movetp` | `crates/wc3-path/` | **MISSING** (grid/cell/cap unknown) | `path_grid_cell_size` after locating the A* |
| Missile columns | `0x6f76aa0c/1c` | `rabin2 -z` | `Missilespeed`, `Missilehoming` | UnitWeapons data columns (projectile contract lives in data) | — | **MISSING** (engine handler not located) | `missile_homing_uses_column` |
| Armour upgrade amount | class `CUpgradeArmor` `0x6f771a34`; RTTI `0x6f771d64`; class-info `0x6f1942a0` | `pd 22 @ 0x6f194294` | `push 1; push 0x38; mov ecx,0x6f7d8854; call alloc; mov [0x6f7d8850],0x6f69fe9c` | data has **no** amount (`base1/mod1 = '-'`), tooltip has no number; hardcoded constant **not located** | `crates/wc3-sim` upgrade handler | **MISSING** | see §7 next probe |

---

## 2. Combat — damage table (observed)

Loader/getter `0x6f19c130` (367 B) loops `edi = 0..6` (row) and `esi = 0..7` (column):

```
0x6f19c152  cmp edi, 6
0x6f19c15b  jmp dword [edi*4 + 0x6f19c2a0]      ; row dispatch
0x6f19c185  push "DamageBonusNormal"
0x6f19c192  call 0x6f008360                     ; Misc getter (edx="Misc")
0x6f19c19a  mov dword [esi*4 + 0x6f7d8918], ecx ; Normal row
...
0x6f19c23f  inc esi / cmp esi,8
0x6f19c249  inc edi / cmp edi,7
```

Jump table `0x6f19c2a0`:

```
[0] 0x6f19c162  DamageBonusSpells -> 0x6f7d88f8
[1] 0x6f19c184  DamageBonusNormal -> 0x6f7d8918
[2] 0x6f19c1a6  DamageBonusPierce -> 0x6f7d8938
[3] 0x6f19c1c5  DamageBonusSiege -> 0x6f7d8958
[4] 0x6f19c1e4  DamageBonusMagic -> 0x6f7d8978
[5] 0x6f19c203  DamageBonusChaos -> 0x6f7d8998
[6] 0x6f19c222  DamageBonusHero  -> 0x6f7d89b8
```

Table is `.bss` (all zero on disk) — filled at load from `Units\MiscGame.txt` `[Misc]`:

```
// combat related entries
CreepCallForHelp=600
DefenseArmor=0.06

// Damage bonus lists: SMALL, MEDIUM, LARGE, FORT, NORMAL, HERO, DIVINE, NONE
DamageBonusNormal=1.00,1.50,1.00,0.70,1.00,1.00,0.05,1.00
DamageBonusPierce=2.00,0.75,1.00,0.35,1.00,0.50,0.05,1.50
DamageBonusSiege=1.00,0.50,1.00,1.50,1.00,0.50,0.05,1.50
DamageBonusMagic=1.25,0.75,2.00,0.35,1.00,0.50,0.05,1.00
DamageBonusChaos=1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00
DamageBonusSpells=1.00,1.00,1.00,1.00,1.00,0.75,0.05,1.00
DamageBonusHero=1.00,1.00,1.00,0.50,1.00,1.00,0.05,1.00
```
(`analysis/war3x_local/Units/MiscGame.txt:101-112`; same in `analysis/agent-tft/local/Melee_V0/Units/MiscGame.txt:106-112`)

**Verdict: MATCH** — `data.rs:150-168` carries all 56 cells verbatim (0.05 divine ⇒ 50 permille included). The only divergence is the **numeric attack-type id** (engine `0=spells`, ours `0=Normal`); table content per attack type is identical.

## 3. Combat — armour formula (observed)

Helper `0x6f19c0a0` (`uint *f(uint *out, float *armor)`):

```
0x6f19c0a9  fld dword [esi]              ; armor
0x6f19c0ac  fcomp dword [0x6f7b4460]      ; 0.0f
0x6f19c0b9  jne 0x6f19c101               ; armor < 0 -> table path
0x6f19c0bd  push "DefenseArmor"           ; Misc section
0x6f19c0c2  mov edx,"Misc"
0x6f19c0d8  call 0x6f37ce60              ; v = armor * 0.06
0x6f19c0e1  mov edx,0x6f7b4464           ; 1.0f
0x6f19c0e9  call 0x6f37d0a0              ; v2 = 1 + v
0x6f19c0f4  call 0x6f37cf40              ; out = v / v2   (equal-operand => 1.0 special case)
```

Caller `0x6f19c5f8..0x6f19c628` (inside `0x6f19c330`):

```
0x6f19c5f8  call 0x6f19c0a0              ; factor
0x6f19c5fd  fld dword [ebp-8]
0x6f19c600  fcomp dword [0x6f7b4464]      ; 1.0f
0x6f19c60b  jne (factor > 1.0) -> push 0.0f
0x6f19c620  call 0x6f37cf80              ; multiplier = 1 - factor
0x6f19c628  call 0x6f1978b0              ; fold into damage accumulator
```

So: `fraction = 0.06a/(1+0.06a)`, `multiplier = 1 − fraction = 1/(1+0.06a)`.

**Verdict: MATCH** — `data.rs:181-188` computes `damage*100/(100+6a)` (exact integer form of `1/(1+0.06a)`). Integer truncation differs from float rounding by ≤1 damage; see proposed test.

Negative side: `0x6f19c114 mov edx, dword [eax*4 + 0x6f7d8c98]` with `eax = clamp(-armor, 0..20)`:

```
0x6f19c0f9  cmp iVar1, -0x14
0x6f19c100  iVar2 = min(-iVar1, 0x14)     ; clamp 20
0x6f19c114  *out = table[iVar2]
```

Table `0x6f7d8c98` is filled by `0x6f19bcf0..0x6f19c009`, one hardcoded constant per entry:

```
0x6f19bd1a  mov edx,0x186a0              ; 100000 (scale)
0x6f19bd28  mov edx,0xffffe890           ; -6000   -> 1.06
0x6f19bd37  mov ecx,0x6f7d8c9c
0x6f19bd4f  mov edx,0xffffd288           ; -11640  -> 1.1164
...
0x6f19c004  mov edx,0xfffeeab3           ; -70989  -> 1.70989
0x6f19c009  mov ecx,0x6f7d8ce8
```

All 21 values agree with `data.rs:172-176` (`NEG_ARMOR_MULT_PERMILLE`) — 1060, 1116, 1169, 1219, 1266, 1310, 1352, 1390, 1427, 1461, 1494, 1524, 1553, 1580, 1605, 1628, 1651, 1672, 1691, 1710. **Verdict: MATCH.**

Note on sign convention (observed): the init stores **negative fractions** (`-6000/100000 = -0.06`), and the caller's `1 - fraction` yields the multiplier `1.06`. This also explains the caller clamp at `0x6f19c60b`: `fraction <= 1.0` is true for both positive fractions (0…1) and negative ones, so the `0.0f` fallback only triggers for impossible fractions > 1. The armour field is `[unit + 0xe0]`, read at `0x6f19c5e9` (`mov ecx,[ebx+0xe0]`) and passed by address to the helper — field identity is established **by use**, not by a symbol.

## 4. Combat — application gates and the two addenda

`0x6f19c330` (1373 B) is the damage-application routine (virtual — `r2 /r` finds no direct `call`). Observed structure:

```
if (unit->flags5c & 0x100) return 0;
if (damage->flags_c & 0x4) return 0;                  ; immune/absorb
if (*(damage + 0x14) == 0x10) {                       ; <-- ARMOUR GATE
    var = *(target + 0xe0);                            ; armour rating (field identity by use)
    fcn.6f19c0a0(&factor, &var);
    fcn.6f1978b0(1 - factor);
}
attack = *(damage + 0x20);                             ; 0..6 (0=spells)
fcn.6f19c130(*(target + 0xe4));                        ; defence type
fcn.6f1978b0(table[attack][defence]);
if (target->ethereal158 > 0) fcn.6f1978b0(Ethereal[attack]);
```

### 4.1 "Magic damage bypasses armour" (addendum 2)

Observed facts:
* armour step is gated **only** on `*(damage+0x14) == 0x10` (`0x6f19c62d` region), not on the attack-type index;
* attack-type index `*(damage+0x20)` selects the row (0 = **spells**, per jump table `0x6f19c2a0`);
* row 0 (`DamageBonusSpells`) is `1.00 ×8 except hero 0.75`, i.e. no armour-class interplay.

Rule to implement (inferred, high confidence): **armour rating applies only to physical damage (`damageType == 0x10`); spell damage must skip `armor_reduction` but still use the row-0 multiplier table.**
Our `data.rs:197-204` applies `armor_reduction` unconditionally → **DIFF**, and `abilities.rs:528+` (Magic casts) therefore over-reduces.

### 4.2 Armour upgrades (addendum 1)

Observed:
* `Units/UpgradeData.slk` `Rhar`: `class="armor"`, `effect1="rarm"`, `base1="-"`, `mod1="-"`, `maxlevel=3` (lines 299-325 of `analysis/war3_full/Units/UpgradeData.slk`);
* tooltip `HumanUpgradeStrings.txt:13-16` — no numeric amount;
* engine side: class symbol `CUpgradeArmor` `0x6f771a34`, RTTI `.?AVCUpgradeArmor@@` `0x6f771d64`, class-info singleton init `0x6f1942a0` (`push 1; push 0x38; mov ecx,0x6f7d8854; call 0x6f00ab56; mov dword [0x6f7d8850],0x6f69fe9c`).

The per-level constant was **not located** in this pass → `upgrades.json` is emitted with `per_level: null` and the exact next probe (see §7). No number is invented.

## 5. Economy (observed)

Accessor `0x6f008b30(idx 0..9)` — upkeep usage:

```
0x6f008b3c  cmp dword [0x6f7536c0], -1
0x6f008b61  push "UpkeepUsage" / "Misc"
0x6f008b7b  push "UpkeepUsage"
0x6f008b83  call 0x6f002220              ; list element
0x6f008b88  mov [esi*4 + 0x6f7536c0], eax
...
0x6f008ba0  mov eax,[0x6f753668]         ; game-type index
0x6f008ba8  lea esi,[edx*8 + 0x6f6932b8] ; default record (40 B)
0x6f008bb9  rep movsd (10 dwords)
```

Default table `0x6f6932b8`:

```
record 0: 40, 70, 10000 ×8
record 1: 50, 80, 10000 ×8
record 2: 90, 100, 10000 ×8
```

`10000` is the "no further bracket" sentinel. TFT = record 1 (`50/80`), RoC = record 0 (`40/70`).

**Verdict: MATCH** with `lib.rs:201-204` (`UPKEEP_LOW_FOOD=50`, `UPKEEP_HIGH_FOOD=80`) and the RoC branch at `lib.rs:5628`.

Taxes: `0x6f008bd0` (gold) / `0x6f008c90` (lumber) read `Misc/UpkeepGoldTax` and `Misc/UpkeepLumberTax` (10 slots each, fallback `0.0f` at `0x6f7b4384` / `0x6f7b43b0`). The TFT `MiscGame.txt` extract in this repo has **no** `UpkeepGoldTax` key, so our `300/600` permille is **reported-unverified** (matches published values, not the local data).

Carry amounts are data (`UnitBalance.slk` columns `goldRep`, `lumberRep`, `fused` — observed present); per-unit values not extracted here. Our sim hardcodes lumber 10 at `lib.rs:120`.

## 6. Orders, AI, pathing, projectiles

Orders/state machine (observed): the native registry at `0x6f260260` (`mov edx,str.X; mov ecx,handler; call 0x6f62f140`) exposes

| native | handler | signature |
|---|---|---|
| ClearHarvestAI | `0x6f261330` | `()V` |
| HarvestGold | `0x6f261360` | `(II)V` |
| HarvestWood | `0x6f2613b0` | `(II)V` |
| StopGathering | `0x6f260a60` | `()V` |
| AddGuardPost | `0x6f260a90` | `(IRR)V` |
| FillGuardPosts | `0x6f260b10` | `()V` |

`HarvestGold` → `0x6f25c2c0(unit, abilitySlot 0..3, target, 1)`; harvest is an **ability-slot** order in the engine, while `crates/wc3-sim/src/ai.rs:249,259` models it as a direct `Order::Harvest` → **DIFF**.

Pathing (observed anchors only): `RegisterPathingFootprint` (`0x6f76c578`) → registrar `0x6f110d60`; kind names `A-pathing`/`C-pathing`/`D-pathing` (`0x6f7551a0/94/88`); unit columns `pathTex`, `movetp`. Grid size, cell size, node cap and smoothing were **not located** → `crates/wc3-path` comparison **MISSING**.

AI: guard-post natives located; creep aggro/leash constant **unknown** (`CAbilityLeash` is the magic leash, not creep leash).

Projectiles/heroes/items: `Missilespeed`/`Missilehoming` data columns and `CMissile*` class names observed; handlers not decompiled → **MISSING**.

## 7. Gaps → exact next probes (owner: ledger 393/401 implementation, 404 RE)

1. **Armour upgrade per-level constant** (blocking `upgrades.json`): find the effect dispatcher that maps `class="armor"` to the `CUpgradeArmor` behaviour. Probe: `r2 -q -e bin.relocs.apply=true -c 's 0x6f1980e0; af; pdg'` chain for the vtable at `0x6f69fe9c`, then follow the `apply(CUnit*, level)` slot and look for the float added to the unit armour field. Also try `pd 900 @ 0x6f197000 | grep 0x3f800000` style constant sweeps in the class region.
2. **Damage-type field `+0x14` producer**: find the routine that builds the damage struct (writes `+0x10`, `+0x14`, `+0x20`). Probe: cross-reference the float constant `0x10` writes near `mov [x+0x20]` in `CUnit` attack code; then confirm spells set `+0x14 != 0x10`.
3. **Pathfinding core**: locate the A* by xref to `0x6f110d60`'s grid table, or search for the 128-unit cell constant near `RegisterPathingFootprint`.
4. **Creep aggro/leash**: `pdg` on the unit-guard routines referenced by `AddGuardPost 0x6f260a90`.

## 8. Reproduction commands

```
cd ~/src/herofolends-rs
B=analysis/tft_files_Files100_Game.dll
r2 -q -e bin.relocs.apply=true -c 'izz~DamageBonus' $B
r2 -q -e bin.relocs.apply=true -c 'pdg @ 0x6f19c130' $B
r2 -q -e bin.relocs.apply=true -c 'pd 40 @ 0x6f19c0a0' $B
r2 -q -e bin.relocs.apply=true -c 'pd 240 @ 0x6f19bd1a' $B
r2 -q -e bin.relocs.apply=true -c 'pdg @ 0x6f19c330' $B
r2 -q -e bin.relocs.apply=true -c 'pxw 160 @ 0x6f6932b8' $B
r2 -q -e bin.relocs.apply=true -c 'pd 26 @ 0x6f260260' $B
python3 $PREFIX/tmp/opencode/pe_scan.py $B e024776f 988c7d6f 6037756f
```

## 9. Bottom line for `crates/wc3-sim`

| item | state |
|---|---|
| 7×8 attack/armour table | **exact match** (values + column semantics) |
| positive armour formula | **match** (`1/(1+0.06a)`, integer form) |
| negative armour table | **exact match** (21/21 constants) |
| armour gate for spells | **diff** — armour must be skipped for non-physical damage |
| ethereal bonus | **missing** in sim |
| upkeep brackets | **match** (RO: 40/70, TFT: 50/80) |
| upkeep taxes | engine reads data; our 30/60 % still unverified against local data |
| harvest order model | **diff** (ability-slot dispatch vs direct order) |
| upgrade armour constant | **not extracted** — do not guess |

---

## 10. Ledger 412 follow-up probes — results

Method and full command log: [`PROBES.md`](PROBES.md).

| probe | outcome | key evidence |
|---|---|---|
| 1. armour-upgrade per-level | **unreachable** (6 probes) | class registry `0x6f69fe9c` = RTTI thunks; class enum `0x6f773ddc` only used by the unit-type loader; tech core `0x6f2e2990`; `+0xe0` has a single writer (`0x6f18f830`) with a single caller (`0x6f1a2869`); no `Rhar`/`rarm` literals |
| 2. `damage+0x14` producer | **unknown**, but the rule is settled | no immediate store in 12 addressing forms; no vtable ctor; no attack-type branch in `0x6f19c330` |
| 2b. spells rule | **observed**: table **yes**, armour **no**, ethereal **yes** | armour block gated at `0x6f19c5f8..628`; table at `0x6f19c64a`; ethereal at `0x6f19c677` are outside the gate |
| 3. ethereal | **observed** | trigger `*(target+0x158) > 0`; `Ether[attack]` = 1.66 (TFT `MiscGame.txt:107`); heals use `EtherealHealBonus` (`:108`) |
| 4. pathfinding | **partial**: anchors + creep-camp distance reader; node cap/cost **unknown** | `RegisterPathingFootprint` `0x6f110d60`; `CPathingFootprint.cpp` asserts `0x6f2ddc19/84`; `CreepCampPathingCellDistance` read `0x6f2672d9`; `CreepCallForHelp=600` |
| 5. harvest routing | **observed** | `0x6f25c2c0(unit, slot 0..3, target, flag)`; ability ids at `unit+0x1b8+slot*4`; two issue paths `0x6f25c3b0` / `0x6f25c6e0` |

**Implementation deltas for `crates/wc3-sim`:** (1) `AttackType::Spells` → keep the type table, skip `armor_reduction`; (2) add an ethereal
multiplier (`1.66` TFT) applied to damage and to heals; (3) model harvest as an ability-slot order with `StopGathering`/`ClearHarvestAI`
cancel semantics; (4) armour-upgrade amount stays unimplemented until probe 1's next step lands.


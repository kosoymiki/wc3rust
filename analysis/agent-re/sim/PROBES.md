# PROBES — ledger 412 (WC3 TFT Game.dll RE follow-ups)

Binary: `analysis/tft_files_Files100_Game.dll`, sha256 `d1a80c89…f4ad2e`, PE32 i386, image base `0x6f000000`.
Tooling: radare2 6.2.6 + r2ghidra, `rabin2 -z`, `$PREFIX/tmp/opencode/pe_scan.py` (PE VA scanner: maps file offsets to VAs, flags CODE/DATA).
All commands below are literal; every claim is labelled **observed** / **inferred** / **unknown**.

---

## Probe 1 — armour upgrade per-level constant → **unreachable** (six probes, all negative)

Goal: the amount added to a unit's armour per researched armour upgrade (e.g. `Rhar`), which is absent from the shipped data
(`UpgradeData.slk` `Rhar`: `class="armor"`, `effect1="rarm"`, `base1="-"`, `mod1="-"`, `maxlevel=3`; tooltip `HumanUpgradeStrings.txt:13-16` has no number).

### P1.1 CClass singleton `CUpgradeArmor`
```
r2 -q -e bin.relocs.apply=true -c 'pd 22 @ 0x6f194294' $B
0x6f1942a0  push 1
0x6f1942a2  push 0x38                    ; 56-byte class-info object
0x6f1942a4  mov ecx, 0x6f7d8854
0x6f1942a9  call 0x6f00ab56              ; War3.exe ordinal (allocator/registrar)
0x6f1942ae  mov dword [0x6f7d8850], 0x6f69fe9c   ; <- "vtable"
```
`pxw 96 @ 0x6f69fe9c` → 25 entries; sampled entries (`0x6f1980e0`, `0x6f198190`, `0x6f198350`, `0x6f198420`, …) each begin
`push -2; push ".?AVCUpgradeX@@"; call 0x6f194300; call 0x6f00ab7a/92/98` and reference **different** classes
(`CUpgradeArmor`, `CUpgradeAttackEx`, `CUpgradeAttackDamageLoss`). **Observed:** this table is the RTTI/class-descriptor registry, not a functional vtable; no `apply` slot.

### P1.2 RTTI/COL chain
`.?AVCUpgradeArmor@@` @ `0x6f771d64` ⇒ TypeDescriptor `0x6f771d5c` (dump shows `0x6f7454cc` vftable, 0 spare, then the name).
`/x 5c1d776f` (COL → TD) and `/x 34166a6f|30166a6f|2c166a6f` (vtable stores in code) → **no hits**. The damage-object vtable
`0x6f6a1634` area likewise has no constructor storing its address. **Observed:** no static C++ vtable→constructor chain is recoverable this way.

### P1.3 Upgrade *class* name enum
```
pxw 128 @ 0x6f773db0   ; table base 0x6f773ddc, stride 8: {char* name; int id}
PS @ pointers: "_"=0x6f76c370, "-"=0x6f76c36c, "armor"=0x6f771204, "artillery"=0x6f771970,
              "melee"=0x6f773e24, "ranged"=0x6f773e1c, "caster"=0x6f773e14
ids: "_"→0, "-"→0, "armor"→1, "artillery"→2, "melee"→3, "ranged"→4, "caster"→5
```
Lookup fn `0x6f1dcaa0` (loops i=6..0, `strcmp`). **Only caller** `0x6f1a2ee9` — inside the unit-type loader (`CUnitDatabase.cpp`
module, asserts at `0x6f18eafc/0x6f18eb4e`). **Observed:** the class→id map exists but the upgrade path does not consult it from a distinct site.

### P1.4 Player tech level core
```
r2 … -c 's 0x6f2e2990; af; pdg'
iVar1 = func_0x6f2e2590(techId,0);     ; tech record
level = *(iVar1+0x1c) | +0x20 | +0x24 | +0x28   (per flag bits 1/2/4/0x10)
```
`/r 0x6f306900` (wrapper used by `GetPlayerTechCount`) → call sites `0x6f260bd5/0x6f260c88/0x6f260d02/0x6f260fc5/0x6f260fe2/0x6f260ffb`,
all **requirement checks** (`cmp eax,1`, `add eax,edi; cmp` against a cost), plus JASS natives `0x6f2bc490/0x6f2bc6a9` and UI.
**Observed:** no armour multiply anywhere on the tech-level read path.

### P1.5 The armour field `+0xe0` has exactly one writer
```
pdf @ 0x6f18f830
0x6f18f833  mov edx,[ebp+8]
0x6f18f836  lea eax,[ecx+0xe0]     ; <- only lea to +0xe0 in the image
0x6f18f83e  call 0x6f18f5c0        ; generic data-source/column getter
```
`/r 0x6f18f830` → **one** caller `0x6f1a2869` (unit-type loader; sets several columns then validates `targType`, `0x6f1a2882 "Invalid targType flag"`).
Scans that found nothing: `fstp dword [r+0xe0]` (6 modrm forms), `movss [r+0xe0]` (6 forms), `mov [r+0xe0], imm`,
`lea r,[r+0xe0]` (6 forms), `add r,0xe0` (6 forms + `add eax,imm32`). **Observed:** nothing else writes the field the damage routine reads as armour.

### P1.6 No literal upgrade ids / effect codes
`/x 52686172` (`"Rhar"`), `/x 52686d65` (`"Rhme"`), `/x 7261726d` (`"rarm"`), `/x 72617464` (`"ratd"`) → **no hits**, and no
`rarm/ratd/rhpx/rmnx/renw` strings in the `rabin2 -z` index. Effect codes are not compared literally.

**Verdict: unreachable with these probes.** The engine stores the class id (`armor`=1) and the tech levels, but the amount is applied
from a path that is not reachable from the class registry, the tech-level getter, or the `+0xe0` field setter. Named next probes:
1. `..\Game/CPlayerWar3.cpp` module (`0x6f7876e8` → assert sites `0x6f303290/0x6f3033f8/0x6f306f82`, module `0x6f303xxx–0x6f31xxxx`):
   find the research-completion function and follow its unit-update loop.
2. Trace `SetPlayerTechResearched` (native handler `0x6f2bc3c0`) with r2 debug-free dataflow: the handler's tail calls
   `0x6f314200`/`0x6f15f210` (handle helpers) — the real mutation is via an indirect call not resolved by `pdg`.
3. Search for the unit-update loop by xref to the unit list globals `0x6f7da070/0x6f7da088` (players' unit arrays; used by the creep-camp
   scan at `0x6f2672b0`), then filter functions that call a float multiply/addition on a unit.

---

## Probe 2 — producer of `damage+0x14` and the exact spells rule → **rule delivered, producer unknown**

### Observed structure (`pdg @ 0x6f19c330`, 1373 B virtual method; vtable slot @ `0x6f6a1640`)
```
if (*(target + 0x5c) & 0x100) return 0;                       ; immune
if (*(damage + 0x0c) & 0x04) return 0;                        ; absorbed
if (*(damage + 0x14) == 0x10) {                               ; <<< ARMOUR GATE
    v = *(target + 0xe0);                                     ; armour rating
    factor = 0x6f19c0a0(&v); fcn.6f1978b0(1 - factor);
}
attack = *(damage + 0x20);                                    ; 0..6, 0=spells
def    = *(target + 0xe4);                                    ; defence type
fcn.6f19c130(def);                                            ; 7x8 table, edx=attack
if (table != 1.0) fcn.6f1978b0(table);                        ; <<< UNCONDITIONAL
if (*(target + 0x158) > 0 && Ether[attack] != 1.0)
    fcn.6f1978b0(Ether[attack]);                              ; <<< UNCONDITIONAL
```
**No comparison of `damage+0x20` against 0 exists in the function.** `+0x20` is used only as an index (table row, ethereal row).

### Producer probes (negative)
* `mov dword [reg+0x14], 0x10` — 8 modrm forms (`c740/41/42/43/45/46/47/…`) → no hits.
* stack forms `c744241410000000`, `c745??10000000`, `c746??10000000`, `c747??10000000` → no hits.
* `cmp dword [reg+0x14], 0x10` (7 forms) → 4 sites (`0x6f4b6361`, `0x6f5bad5e`, `0x6f4e9c6f`, `0x6f586f58`, `0x6f470cb8`), none in the combat module (`0x6f19xxxx`/`0x6f1cxxxx`).
* damage-object vtable stores → none (see P1.2).

### Rule for the sim (evidence-backed, producer-independent)
1. **Armour is applied only when the damage is marked physical** (`+0x14 == 0x10`); attack type does not gate armour.
2. **The attack/armour type table is always applied**, including for spells (row 0 = `DamageBonusSpells`, `1.00×8 / hero 0.75`).
3. **The ethereal multiplier is always applied** when the target is ethereal (1.66 in TFT), independent of damage type.
⇒ Implement `AttackType::Spells` as *"table yes, armour no"*. `AttackType::Magic` keeps armour (it is reduced in WC3).
Producer identity is **unknown**; next probe: resolve the virtual call to slot 3 of `0x6f6a1634` (call sites `call dword [r+0xc]` near stores of
`+0x10`/`+0x20`) and read the `+0x14` value it passes.

---

## Probe 3 — ethereal bonus conditions → **observed**

Call site (inside `0x6f19c330`):
```
0x6f19c677  fld dword [ebx*4 + 0x6f7d8c30]     ; Ether[attackType]
0x6f19c685  fcomp dword [0x6f7b4464]           ; 1.0f
0x6f19c695  call 0x6f1978b0                    ; fold into multiplier chain
```
Guard: `0 < *(target + 0x158)` (integer counter on the **target**; ethereal-form state — order names `etherealform`/`unetherealform`
exist at `0x6f798c64`/`0x6f798c54`, buff string `Ethereal` `0x6f69fb78`, unit-flag text `Notethereal` `0x6f7694fc`).

Data (TFT): `MiscGame.txt:107` `EtherealDamageBonus=1.66,1.66,1.66,1.66,1.66,1.66,1.66` (7 attack types);
`MiscGame.txt:108` `EtherealHealBonus=1.66` (heals use a separate multiplier).

**Contract for the sim:** when `target.ethereal_stacks > 0`, multiply incoming damage by `EtherealDamageBonus[attack_type]` (1.66 TFT,
×1.0 in RoC-style data if the value is 1.0) after the type table and after armour; multiply healing by `EtherealHealBonus`.

---

## Probe 4 — pathfinding → **partial** (anchors yes; node cap / cost function unknown)

Observed anchors:
| item | address | evidence |
|---|---|---|
| footprint registration | `0x6f110d60` (registrar), `0x6f1103c0` (name lookup) | `mov edx,"RegisterPathingFootprint"; call 0x6f1103c0`; hash-map lookup `0x6f2ddb40` |
| footprint module | `0x6f2ddxxx` | assert file `..\Game/CPathingFootprint.cpp` `0x6f7869e4` → `0x6f2ddc19`, `0x6f2ddc84` |
| pathing kinds | `A-pathing` `0x6f7551a0`, `C-pathing` `0x6f755194`, `D-pathing` `0x6f755188` | string table |
| unit columns | `UnitData.slk pathTex`, `movetp`; `pPathingFootprint`, `pPathingFootprintAlt` | data + strings |
| unit pathing toggle | JASS `SetUnitPathing` `0x6f783488`, `DisablePathing` `0x6f77cc1c` | native name strings |
| creep-camp cell distance | read `0x6f2672d9`/`0x6f2672f8`, squared via `imul edx,esi`, compared to squared world distance | `pd 45 @ 0x6f2672a8` |
| creep call-for-help | `CreepCallForHelp=600` | `MiscGame.txt:102` (RoC), `:93` (TFT) |

Not located: grid origin/cell size, cost function, node cap, smoothing. **unknown**.
Next probes: (a) xref callers of `0x6f2ddb40` (footprint lookup) to reach the movement code that consumes footprint cells;
(b) follow `CAbilityMove` module (`CAbilityMove.cpp` `0x6f7a5144` → assert `0x6f50eea2`, module `0x6f50exxx`) through `0x6f6132f0`/`0x6f08d9e0`
into the path request; (c) scan for the A* open/closed arrays (large zero-initialised globals near `0x6f7dxxxx` resized per map).

---

## Probe 5 — harvest ability-slot routing (optional) → **observed**

`HarvestGold` native (`0x6f261360`) → `fcn.6f25c2c0(unit, slot, target, flag)`:
```
0x6f25c2d7  cmp ebx,4                      ; slot must be 0..3
0x6f25c2e0  mov ecx,[edi+ebx*4+0x1b8]      ; ability id in the unit's ability slots
0x6f25c2ea  cmp ecx,-1 ; je fail
0x6f25c2f0  call 0x6f264bf0                ; resolve ability object
0x6f25c306..  target checks: reject if target->[0x5c] & 0x2000000, and unless target->[0x30] == 0x75676f6c ("logu")
0x6f25c383  call 0x6f25c3b0                ; issue path A
0x6f25c39a  call 0x6f25c6e0                ; issue path B (queued/alternate)
```
**Observable differences vs a direct `Order::Harvest`:** the order is bound to an *ability slot* (max 4), so it inherits the ability's
target validation, range rules and cancel semantics; `StopGathering` (`0x6f260a60`) and `ClearHarvestAI` (`0x6f261330`) act on that
ability state, and two issue paths exist (immediate vs queued). For parity the sim needs harvest modelled as an ability-slot order
(unit → 4 slots → ability → order), not a standalone order enum; the difference is observable in cancel/queue behaviour, not in the
resource amounts.

---

## Files

* `constants.json` — machine-readable numbers (combat table, armour, gate, ethereal, upkeep, creep, harvest routing)
* `upgrades.json` — `per_level: null` + the six negative probes above
* `ENGINE_MAP.md` — row-per-subsystem map with verdicts

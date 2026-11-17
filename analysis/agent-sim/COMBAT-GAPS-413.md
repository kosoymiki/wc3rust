# COMBAT GAP CLOSURE — ledger 413

Date 2026-11-15 · owner lane COMBAT GAP (413) · repo `/data/data/com.termux/files/home/src/herofolends-rs`
Engine evidence: `analysis/agent-re/sim/ENGINE_MAP.md` §10, `PROBES.md` §2-3, `constants.json`
(r2 lane 404/412, binary `analysis/tft_files_Files100_Game.dll`, sha256 `d1a80c89…4ad2e`).

## 1. Goal

Close three combat gaps the binary proves and the sim did not model:

1. `combat.spells-bypass-armor` — a spells hit keeps the 7x8 attack/armor table but skips the
   armor-rating multiplier.
2. `combat.ethereal-damage` — an ethereal target takes `EtherealDamageBonus[attack]` (`1.66` in TFT)
   on top of the table and armor steps, for every attack class.
3. `combat.ethereal-heal` — healing on an ethereal target uses `EtherealHealBonus` (`1.66`).

## 2. Engine rule this implements (observed)

| address | instruction / rule | consequence |
|---|---|---|
| `0x6f19c5f8..0x6f19c628` | armor block gated on `*(damage + 0x14) == 0x10` | armor is a physical-damage step, not an attack-class step |
| `0x6f19c64a` | 7x8 table lookup, `edx = *(damage + 0x20)` | table applies **always**, spells included |
| `0x6f19c677` | `fld [ebx*4 + 0x6f7d8c30]` guarded by `*(target + 0x158) > 0` | ethereal applies **always**, after table and armor |
| — | no branch compares `*(damage + 0x20)` with 0; engine id 0 = `spells` | spells = table yes, armor no, ethereal yes |

Data: `Units/MiscGame.txt:107 EtherealDamageBonus=1.66` (7 rows), `:108 EtherealHealBonus=1.66`.
Local corroboration: `Units/CommandStrings.txt:306 Notethereal` — "units in the spirit world are
vulnerable only to spells and magic attacks".

## 3. Changes (file:line = post-edit)

| file:line | change |
|---|---|
| `crates/wc3-sim/src/data.rs:198-215` | `ETHEREAL_DAMAGE_MULT_PERMILLE: [i32; 7] = [1660; 7]`, `ETHEREAL_HEAL_PERMILLE = 1660` |
| `crates/wc3-sim/src/data.rs:219-221` | `skips_armor(attack)` — `true` only for `AttackType::Spells` (the `+0x14 == 0x10` gate) |
| `crates/wc3-sim/src/data.rs:237-259` | `typed_damage_vs(attack, armor_type, armor, base, ethereal)` — table → armor (non-spells only) → ethereal |
| `crates/wc3-sim/src/data.rs:226-228` | `typed_damage` kept as the non-ethereal wrapper (existing callers and pins unchanged) |
| `crates/wc3-sim/src/lib.rs:370` | `Unit::ethereal: bool` — engine `unit + 0x158` counter |
| `crates/wc3-sim/src/lib.rs:1154` | spawn default `ethereal: false` |
| `crates/wc3-sim/src/lib.rs:3051-3056` | `apply_damage` reads the target flag and calls `typed_damage_vs` |
| `crates/wc3-sim/src/lib.rs:3214-3243` | `heal_unit` applies `ETHEREAL_HEAL_PERMILLE`; `set_ethereal(unit, bool)` setter (returns `false` on unknown id) |
| `crates/wc3-sim/src/lib.rs:6124` | `state_hash` feeds `unit.ethereal` |
| `crates/wc3-sim/src/save.rs:1328` | load literal `ethereal: false` (see §7 boundary) |
| `crates/wc3-sim/tests/combat_depth.rs:162-170` | the 7x8 matrix test now models the armor gate instead of assuming armour for spells |
| `crates/wc3-sim/tests/combat_ethereal.rs` | new: `combat_spells_bypass_armor`, `combat_ethereal_damage`, `combat_ethereal_heal` |

Untouched: dice roll (`roll_weapon_damage`), miss policy, splash (`resolve_weapon_hit` splash block),
bounty/XP, `jass*`, `passive_income`, gather gates, creep camps.

`jass*` was not modified by this lane. `save.rs` got one added struct-literal field; it does not
conflict with ledger 410's hook work.

## 4. Tests (observed)

`cargo test -p wc3-sim --offline --test combat_ethereal` → **3 passed**, rc=0.

| test | assertion |
|---|---|
| `combat_spells_bypass_armor` | 10-armor Medium target, base 100: normal **93**, spells **100**, magic **46**; negative armor −5: spells still **100**, normal **189** |
| `combat_ethereal_damage` | non-ethereal normal **93** / spells **100**; ethereal normal **154**, spells **166**, chaos **102**; flipping the flag off restores 100/93; `set_ethereal` on unknown id returns `false` |
| `combat_ethereal_heal` | `heal_unit(100)` non-ethereal **100**, ethereal **166**; same through `Command::Cast{Ability::Heal(100)}` (200 vs 266 hp); overflow clamps at `max_hp` and reports the real gain |

## 5. Before / after on the same staged fight

Reproduction: `python3 analysis/agent-sim/measure_combat_gaps.py` (rc=0). The script replays the
**old** integer pipeline verbatim and validates it against three pins the tree already carries and
this lane did not change (`normal/medium a=2 → 133`, `normal/medium a=-5 → 189`,
`pierce/heavy a=2 → 89`), so the "before" column is formula-exact, not hand-waved.

Staged fight: attacker base 100, target `ogru` (Medium armour, rating 10).

| attack | before | after | after + ethereal |
|---|---|---|---|
| normal | 93 | 93 | 154 |
| pierce | 46 | 46 | 76 |
| siege | 31 | 31 | 51 |
| magic | 46 | 46 | 76 |
| chaos | 62 | 62 | 102 |
| **spells** | **62** | **100** | **166** |
| hero | 62 | 62 | 102 |

Heal `Ability::Heal(100)` on a 100-hp grunt: before 100 (hp 200) → after 100 non-ethereal (hp 200),
166 ethereal (hp 266).

Only the spells row (armor skipped) and the ethereal column change; every physical row is unchanged,
which is why no existing pin moved.

## 6. Suite result (observed)

`cargo test -p wc3-sim --offline` → **rc=101**, caused by one test outside this lane:
`jass::natives::tests::every_handler_name_is_a_registry_native`
(`crates/wc3-sim/src/jass/natives.rs:641`, file mtime `2026-11-15 05:30:10`, edited by the
parallel `jass*` lane while this lane ran; it lists 33 JASS handler names that the registry does not
declare). Nothing in the failure touches damage, armor, healing or the hash.

With that one foreign test skipped: `cargo test -p wc3-sim --offline --lib -- --skip
every_handler_name_is_a_registry_native` → **39 passed, 0 failed**, rc=0.
`cargo test -p wc3-sim --offline --tests --no-fail-fast` → **43 integration suites, 257 passed,
0 failed** (only the lib target's foreign test red). Relevant suites: `combat` 10, `combat_depth` 6,
`combat_ethereal` 3, `combat_parity` 1, `determinism` 3, `save_load` 10 — all ok.
`cargo check -p wc3-sim --offline` → 0 errors.

Log: `$PREFIX/tmp/opencode/wc3-sim-it-413.log`, `$PREFIX/tmp/opencode/wc3-sim-tests-413.log`.

## 7. Boundaries and next steps

1. **Save stream (owned by the save lane, not 413).** `ethereal` is not part of the version-2 unit
   record: `tests/save_load.rs:409` asserts that version 3 is an *unknown* version, so adding the
   byte requires a coordinated change — bump `SAVE_FORMAT_VERSION` to 3, accept `1..=3` in
   `open_envelope`, read the byte only for `v >= 3`, and move `3` out of the rejected list in
   `save_load.rs:409`. Until then a reloaded match restores `ethereal = false`, documented inline at
   `save.rs:1328`. Next owner: save/tech-lead lane; the state hash already includes the flag.
2. **Ability wiring (ability lane).** `abilities_gen.rs` carries `Aetf Ethereal Form` (duration 21
   ticks) but generic-ability execution does not set the flag. `Sim::set_ethereal` is the hook:
   an ability handler that toggles it for the duration is a one-line call in the ability path.
3. **Gap map / runner (tech-lead).** `analysis/tech-lead/MECHANICS_GAP_MAP.md` needs rows for
   `combat.spells-bypass-armor`, `combat.ethereal-damage`, `combat.ethereal-heal` pointing at
   `crates/wc3-sim/tests/combat_ethereal.rs`, and `run_mechanic_tests.sh` needs the
   `combat_ethereal` target; both files are outside this lane's owner paths.

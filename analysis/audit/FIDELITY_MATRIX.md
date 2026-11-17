# FIDELITY MATRIX — herofolends-rs vs the reverse-engineered WC3 engine

Ledger **421** · task `herofolends/fidelity-audit` · date **2026-11-15** · root `su -c 'id'` → `uid=0(root) gid=0(root) context=u:r:ksu:s0`.
Repo `/data/data/com.termux/files/home/src/herofolends-rs`. Authority binary
`analysis/tft_files_Files100_Game.dll` (TFT 1.27, PE32 i386, image base `0x6f000000`,
sha256 `d1a80c89…`). Every engine address below was read in this session with
`radare2 6.2.6` from that file, or is a constant already extracted into
`analysis/agent-re/*/constants.json` with its own address; data values come from the
local TFT extracts (`analysis/war3x_local`, `analysis/war3_full`, `analysis/war3_full3`)
and are labelled observed.

Scope of this lane: **audit + P0 fixes in `crates/wc3-audio/**` and `analysis/audit/**`**.
All other lanes are report-only with exact file:line and a proposed patch (§5).

Verdict vocabulary — `MATCH` (same mechanism and same constants), `DIFF` (mechanism or
constant differs; consequence named), `MISSING` (no implementation), `UNPROVEN`
(implementation exists, engine contract not established in the RE evidence; marked
honestly, never asserted as match).

---

## 0. Verdict counts

| verdict | rows |
|---|---:|
| MATCH | 54 |
| DIFF | 10 |
| MISSING | 1 |
| UNPROVEN | 8 |
| **total** | **73** |

P0 rows: **1** (audio per-channel voice limits — **fixed in this lane**, §3).
P1 rows: **4** in the matrix (invented miss chance, harvest routing, camp membership
metric, creep call-for-help) plus two short-list P2s that carry the same root cause
(W3E v3..v7 reader, declared-native JASS debt). P2 rows: 11, P3 rows: 3.

---

## 1. The matrix

### 1.1 `wc3-formats` — map/model/archive formats (8 592 LOC)

| # | module item | engine contract (address / constant) | our implementation (file:line) | verdict | sev | evidence | fix / ticket |
|---|---|---|---|---|---|---|---|
| 1 | MDX chunk dispatch | `fcn.6f32bcf0`, 20 handlers VERS/MODL/SEQS/GLBS/MTLS/TEXS/TXAN/GEOS/GEOA/BONE/LITE/HELP/ATCH/PIVT/PREM/PRE2/RIBB/EVTS/CAMS/CLID (`constants.json:mdx_chunk_dispatch`) | `crates/wc3-formats/src/mdx.rs:665-1098` handles every one of them, plus `SNEM` (engine handler `0x6f3511c0`) skipped as unknown-length | MATCH | — | chunk tags grepped 1:1 against the engine handler table | — |
| 2 | MDX geoset record | inclusiveSize counts its own 4 bytes; fixed section order VRTX,NRMS,PTYP,PCNT,PVTX,GNDX,MTGC,MATS; tail materialId/selectionGroup/unknown/radius/min/max; then UVAS→UVBS | `mdx.rs:1394-1542` (`parse_geoset`), `mdx.rs:1500-1523` (UVAS/UVBS) | MATCH | — | layout read at `fcn.6f3484e0/6f348640/6f348ac0`; round-trip writer `mdx.rs:2135` | — |
| 3 | MDX materials/layers | material 164 B, layer 72 B (`runtime_struct_sizes_bytes`); filter mode 0..5; two-sided | `mdx.rs:1313` `parse_materials` | MATCH | — | struct sizes observed; consumer is render §1.7 | — |
| 4 | MDX sequences / global sequences | SEQS 132 B on disk / 140 B in memory, interval [start,end], non-looping flag | `mdx.rs:671-690` | MATCH | — | `sequence_in_memory 140` vs `sequence_in_file 132` | — |
| 5 | MDX attachments / event objects / lights / PRE2 / RIBB / CAMS | handlers listed in row 1 | `mdx.rs:720-980` | MATCH | — | every tag parsed; PRE2 emitter size 528 matches `particle_emitter2` | — |
| 6 | W3E version gate | `cmp eax,3; jl fail` `0x6f3eb1fe`, `cmp eax,0xb; jg fail` `0x6f3eb207` | `w3e.rs:12-19` `VERSION_MIN=3`, `VERSION_MAX_ENGINE=11`, `parse_classic_engine` `w3e.rs:122` | MATCH | — | r2 `pd 45 @ 0x6f3eb1f0` this session | — |
| 7 | W3E tile record, v8..v11 | v8..v10 read via `fcn.6f3eb760(version)`; v11 via `fcn.6f3eb920`; heights are u16 fixed-point in both | `w3e.rs:186-215` fixed 7-byte record | MATCH (v11 corpus) / UNPROVEN (v8..v10) | P2 | 97/97 corpus maps are v11 (corpus audit); no v8..v10 file to test | corpus gate `w3e.version-bounds` |
| 8 | W3E tile record, v3..v7 | `0x6f3eb7d5 cmp ebx,8; jl 0x6f3eb7fc` → **three f32 reads** (ground, water, extra) with `(x+32768)*1/128` and `(x+2048)*4.0` conversions; v5..v7 read a fourth f32 `0x6f3eb8e6-0x6f3eb913` | `w3e.rs:186-215` always reads u16 heights | **DIFF** | P2 | r2 `pd 78 @ 0x6f3eb760` + `pd 24 @ 0x6f3eb8e6`; constants `0x47000000=32768`, `0x45000000=2048`, `0x3c000000=1/128`, `0x40800000=4.0` | §5.4 patch (version-branched reader) |
| 9 | W3E custom-tileset field | read only for `version >= 6` (`0x6f3eb2c9 cmp [ebp+0xc],6; jl 0x6f3eb2f5` — below 6 the engine writes defaults **without consuming bytes**) | `w3e.rs:151` reads `u32` unconditionally | **DIFF** | P2 | r2 `pd 30 @ 0x6f3eb2c0` | §5.4 |
| 10 | W3I | v18+ saves/editor version, v27+ 4-part game version; strings READ-ONLY | `w3i.rs:18-72` | MATCH | — | corpus 99/99 open | — |
| 11 | WPM pathing | `version == 0` only; bits are *disable* flags (walkable = `!(flag & 0x02)`) | `wpm.rs:20-40` | MATCH | — | Justice 512×384 grid used by sim tests | — |
| 12 | DOO units | `W3do` v6..v13, subversion 11 item table + hero stats, random payload 0/1/2 | `map.rs:102-238`; exact byte consume on Justice (11 389/11 389 B, 99 records) | MATCH | — | this session: `wc3-mpq dump` + independent Python parse, trailing=0 | — |
| 13 | DOO doodads | `W3do` doodad file, v6..v13 | `map.rs:283-330` | MATCH | — | Justice 1997 destructables / 905 decorations | — |
| 14 | WTS | `TRIGSTR_n` table, UTF-8 with BOM tolerance | `localization.rs:105` (`set_wts`), `fdf.rs:839` substitution | MATCH | — | map strings resolve in the FDF layer | — |
| 15 | Storm crypt/hash | 0x500-entry table, `hash_string(name, kind)` kinds 0/1/2/3 | `crates/wc3-mpq/src/lib.rs:194-232` | MATCH | — | byte-level test fixtures from an independent implementation | — |
| 16 | Storm locale/platform filter | skip hash entry when `wLocale`/`wPlatform` non-zero and ≠ current; both globals zero-init (`0x15045784/86`), no `SFileSetLocale` call in either Game.dll | `mpq/lib.rs:663` `find_block` | MATCH | — | `analysis/agent-formats/FORMATS_PARITY_2026-11-15.md` + real-archive audit (0 tagged entries in all three shipped archives) | — |
| 17 | Storm codecs | five masks only: `0x08` PKWARE, `0x02` zlib, `0x01` Huffman, `0x40`/`0x80` ADPCM; `0x300` = implode+compress copies raw | `mpq/lib.rs` `MPQ_CODEC_MASK_WC3` + decode table; raw-copy branch | MATCH | — | codec census: 17 289 files, 0 failures, no foreign codec | — |
| 18 | Storm sectors | sector table exactly `(n+1)*4` (no CRC dword), key `fileKey-1`/`+index`, stored iff `raw >= expected` | `mpq/lib.rs:430` | MATCH | — | ENGINE_MAP §1.5 | — |
| 19 | BLP1/BLP2 | paletted / JPEG / DXT1·3·5 / BGRA | `blp.rs:24-283` | MATCH | — | decodes all shipped unit icons in the asset tests | — |
| 20 | FDF | frame docs, `StringList`, `TRIGSTR_n`, skins | `fdf.rs` (62 KB), `wc3-game/src/fdf_ui.rs` | UNPROVEN | P2 | no engine-side FDF semantic extract exists (only layout constants: tile 0.038 / gap 0.0075) | ticket: FDF geometry parity vs `CCommandButton` ctor `0x6f12ae80` |

### 1.2 `wc3-data` — shipped tables (12 554 LOC)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 21 | Unit/Item/Ability/Buff/Upgrade tables | `UnitData` 466, `UnitBalance` 466, `UnitWeapons` 316, `UnitAbilities` 324, `UnitUI` 466, `AbilityData` 397, `ItemData` 132, `UpgradeData` 58 (RoC install) | `crates/wc3-data/src/{units,items,abilities,buffs,upgrades}.rs`; `table.rs:167` generic SLK row model | MATCH (row counts) | — | `analysis/agent-data/DATA_COVERAGE.md` measured counts | — |
| 22 | Menus (command card) | button art + hotkey + cost badge; 4×3 grid, tile 0.038, gap 0.0075, bar 0.1745×0.129 (`constants.json:command_card`) | `menu.rs:88,97,110` (`train_menu`/`build_menu`/`research_menu`), geometry in `wc3-game/src/mobile_hud.rs:1367-1385` | MATCH | — | engine globals `0x6f699c40=0.038`, `0x6f69ac20=0.0075` | — |
| 23 | Requirements | `Requirescount` + `Requires%s` tooltip path (`0x6f797d88/d98`), `reqLevel` for hero skills | `menu.rs:175` `requirement_name`, `items.rs`/`abilities.rs:341` `required_level` | MATCH | — | tooltip resource keys in `ui_audio/constants.json` | — |
| 24 | Localisation | profile sections + WTS + `war3skins.txt` | `localization.rs:58-134` | MATCH | — | every UI string in the game resolves through it | — |
| 25 | Object data (W3U/W3T/W3A/W3B/W3D/W3Q/W3H) | custom-object table patches | `object.rs`, `objectdata.rs` | MATCH | — | Justice custom ids (`o000`…) resolve | — |
| 26 | Drop tables | `ItemData.droppable` + `Level` 1..8 → 8 drop tables (272/273 items) | `drops.rs`, `creeps_gen.rs` | MATCH | — | corpus principle `dream.full-port-typed-tables-wc3-creep-drops-data-boun-4464` | — |
| 27 | Shops/sellunits | shop rows + sell list | `shop.rs` | MATCH | — | Justice 5 shops resolved from data | — |
| 28 | Table validation gate | n/a (ours) | `validate.rs` | MATCH | — | coverage report is machine-checked | — |

### 1.3 `wc3-sim` — combat, economy, orders, AI (83 855 LOC)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 29 | 7×8 damage table | loader `0x6f19c130`, table `0x6f7d88f8`, stride 32 B, rows spells,normal,pierce,siege,magic,chaos,hero; cols small,medium,large,fort,normal,hero,divine,none | `data.rs:150-168` `DAMAGE_MULT_PERMILLE` | MATCH | — | all 56 cells equal `MiscGame.txt:106-112` | — |
| 30 | Attack-type id order | engine ids 0=spells…6=hero (`0x6f19c2a0`) | `data.rs:100-113` enum order Normal=0… | DIFF (ids) | P2 | values per attack type identical; only the numeric id mapping differs | encode engine ids in the save/script boundary |
| 31 | Armour formula | `0x6f19c0a0`: `f = .06a/(1+.06a)`, multiplier `1−f = 1/(1+.06a)` | `data.rs:181-193` integer `100/(100+6a)` | MATCH | — | ±1 damage from integer truncation | — |
| 32 | Negative armour | table `0x6f7d8c98`, 21 entries, `1 + abs(c)/1e5`, index abs(armor) clamped 20 | `data.rs:172-178` | MATCH | — | 21/21 constants | — |
| 33 | Armour gate (spells) | armour block only when `*(damage+0x14)==0x10` (`0x6f19c5f8..628`) | `data.rs:219-221` `skips_armor`, `data.rs:242-255` | MATCH | — | ENGINE_MAP row 26 said DIFF; the gate has since been implemented — this row supersedes it | — |
| 34 | Ethereal bonus | `Ethereal[attack]` 7 floats `0x6f7d8c30`, trigger `unit+0x158>0`, applied after table+armour (`0x6f19c677`) | `data.rs:206-217`, `data.rs:235-260` | MATCH | — | TFT `MiscGame.txt:107` = 1.66 ×7 | — |
| 35 | Dice roll | `base + dice × U(1..sides)` (`dmgplus1`/`dice1`/`sides1`) | `combat.rs:108-122`, `data.rs:360-372` | MATCH | — | deterministic RNG injected | — |
| 36 | **Miss chance** | engine has **no** inherent cross-class miss; the only miss constant is `Misc/MissDamageReduction=0.5` (`analysis/war3x_local/Units/MiscGame.txt:139`) for ability-driven misses | `combat.rs:77-87` invents 5 % ground→air / 10 % air→ground, applied at `lib.rs:2949` | **DIFF** | **P1** | `grep Miss` in the DLL: only `Missilespeed/Missilehoming/…` and `MissDamageReduction`; no cross-class table; data has no such key | §5.1 remove the policy constant, route misses through buffs (`Curse`) |
| 37 | Splash | `Farea1/Harea1/Qarea1` + `Hfact1/Qfact1` (UnitWeapons columns) | `combat.rs:135-146`, `combat_gen.rs:8-40` | MATCH | — | data-driven falloff, no invented radii | — |
| 38 | Upkeep brackets | `0x6f6932b8` rec1 `{50,80}`, rec0 `{40,70}`, 10000 sentinel | `lib.rs:201-204` | MATCH | — | TFT record 1 = 50/80, RoC branch `lib.rs:5628` | — |
| 39 | Upkeep taxes | accessors `0x6f008bd0/8c90` read `UpkeepGoldTax/LumberTax` ×10; **no such key in the local TFT extract** | `lib.rs:205-206` 30 %/60 % | UNPROVEN | P2 | `grep -i upkeep analysis/war3x_local/Units/MiscGame.txt` → absent | ticket: extract from the 1.27 archive data (`war3xlocal`/`war3x` MiscGame) |
| 40 | Carry amounts | `UnitBalance.goldRep/lumberRep` per unit | `lib.rs:120` lumber 10 hardcoded | DIFF | P2 | columns observed in `UnitBalance.slk`; per-unit values not extracted | §5.5 |
| 41 | Harvest phases | ability-slot dispatch `0x6f25c2c0(unit, slot 0..3, target, flag)`, ability ids `unit+0x1b8+slot*4`, cancel `StopGathering 0x6f260a60` / `ClearHarvestAI 0x6f261330` | `lib.rs:208-213` `HarvestPhase`, `Order::Harvest/HarvestLumber`, cancel = order replaced | **DIFF** | **P1** | ENGINE_MAP §6 + `constants.json:orders.harvest_dispatch` | §5.2 ability-slot model + `GetUnitCurrentOrder` id |
| 42 | Pathfinding | only anchors known: `RegisterPathingFootprint 0x6f110d60`, kinds A/C/D-pathing `0x6f7551a0/94/88`, columns `pathTex`/`movetp`; **grid cell, cost, node cap unlocated** | `crates/wc3-path/src/lib.rs:66-144` A*, 8-neighbour, 10/14 octile, consistent heuristic, **no node cap** | UNPROVEN | P2 | ENGINE_MAP §6/§7 probe 3 (6 probes, unreachable) | ticket: node cap + engine cost when the A* is located; add a bounded open-set now (§5.6) |
| 43 | Orders | native registry `0x6f62f140`; order ids not mapped | `lib.rs:216-247` `Order`/`OrderSpec` | UNPROVEN | P2 | order ids absent from the RE extract | ticket |
| 44 | Creep camps | grouping by `Misc/CreepCampPathingCellDistance` (read `0x6f2672d9/f8`, squared compare in `0x6f2672b0`); **value not in repo extracts** | `lib.rs:143` `CAMP_CLUSTER_RADIUS=600` greedy anchor clustering | **DIFF** | **P1** | measured on Justice: 97 neutral-aggressive placements → 28 clusters at 600 u (47 at 300, 27 at 900); all camp anchors >600 apart, so the *count* is stable but the *membership metric* is straight-line, not pathing cells | §5.3 |
| 45 | Creep call-for-help | `Misc/CreepCallForHelp=600` (`MiscGame.txt:102` RoC / `:93` TFT), applied on damage to pull nearby creeps | `lib.rs:3251-3300` retaliation restricted to **the same camp** (`camp` id), aggro ring 500 | **DIFF** | **P1** | Justice: 185 creep pairs within 600 u, **19 of them in different camps** — engine would pull them, ours never does | §5.3 |
| 46 | Creep leash | `MaxGuardDistance` semantics; engine constant unlocated | `lib.rs:151` `CAMP_HOME_RADIUS=80`, `CAMP_LEASH_RADIUS=900` | UNPROVEN | P3 | 900/80 are our numbers | ticket |
| 47 | Buffs/auras | `CBuff*` classes; Warsmash donor semantics (2.00 s aura period, non-stacking keys) | `buffs.rs:1-40`, `effects.rs:1-20` | MATCH (donor) | — | donor-cited, engine-side classes not decompiled | — |
| 48 | JASS tick hook | VM advances with the sim | `lib.rs:1054` `advance_script`, `tests/jass_tick_hook.rs` | MATCH | — | 2 tests: timer damages a real unit; inert without bundle | — |
| 49 | Save format | n/a (our format) | `save.rs` v3 (`SAVE_FORMAT_VERSION=3`), v1/v2 migrate | MATCH | — | `tests/save_load.rs` 10 passed | — |
| 50 | Building footprint | units keep their own footprint (engine buildings occupy cells) | `lib.rs:2150` `unstick_units` skips buildings | MATCH | — | `GATHER_MULTICREW_414.md` §1b evidence | — |
| 51 | Guard-post AI | `AddGuardPost (IRR)V` `0x6f260a90`, `FillGuardPosts ()V` `0x6f260b10`, `ClearHarvestAI ()V` `0x6f261330` | no implementation (`grep GuardPost crates/wc3-sim` → 0) | **MISSING** | P2 | native registry located (`0x6f260260` block); handler internals not decompiled | ticket: port after the handler decompile (`ENGINE_MAP` §7 probe 4) |
| 52 | Missiles in flight | `Missilespeed`/`Missilehoming`/`Missilearc` columns; `CMissile` classes | `lib.rs:533-560` `Missile`, spawn at `lib.rs:2959-2970` | MATCH (data-driven) | — | homing/arc speed from UnitWeapons | — |

### 1.4 `wc3-game` — scene, assets, render, HUD, loop (40 887 LOC)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 51 | sim_bridge scene | map placement → sim/units/doodads/mines | `sim_bridge.rs` (84 KB), `spawn_map_scene` | MATCH | — | Justice: 30 units, 1997 destructables, 7 mines, 11 camps (melee verify) | — |
| 52 | Assets | MPQ → MDX/BLP/SLK with cache | `assets.rs:1-114 KB`; skin weights from `wc3_anim::GeosetSkin` `assets.rs:1720-1731` | MATCH | — | the stale `assets.rs.rej` (2026-11-12) hunk **is** applied; file is a leftover | delete `*.rej`/`*.bak-*` (P3 hygiene, game lane) |
| 53 | Model/anim runtime | sequences, geoset anim, attachments | `model_render.rs` (109 KB), `wc3-anim` | MATCH | — | sequence selection by index/name; attachment points parsed | — |
| 54 | Terrain/water | tiles, cliffs, water pass with `Water.slk` colours | `terrain_render.rs:1-120` (202 KB) | MATCH | — | Water biome colours from data | — |
| 55 | Doodads | destructables + decorations, per-type art | `doodads.rs` (35 KB) | MATCH | — | 1997/905 placements rendered | — |
| 56 | HUD (desktop) | FDF console + command card | `hud.rs` (95 KB), `fdf_ui.rs` | MATCH | — | FDF documents loaded from the archive | — |
| 57 | Mobile HUD | tile 0.038·H, gap 0.0075·H, pitch 0.0455·H, bar 0.1745×0.129·H | `mobile_hud.rs:1365-1385` | MATCH (with documented touch floor 48 px) | — | retail fractions are the contract; only the touch minimum deviates | — |
| 58 | Game loop | fixed sim tick, input gate | `game.rs:26` `TICK_SECONDS=1/30`, accumulator `game.rs:2180-2183` (≤5 steps), `issue()` sets `input_seen` `game.rs:2026-2028` | MATCH | — | the 2400-update/tick=0 defect from `GATHER_MULTICREW_414.md` §1a is fixed | engine tick rate itself unverified (P3) |

### 1.5 `wc3-audio` — sound (7 287 LOC; this lane's owner path)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 59 | Sound table parse | schema order SoundName…EAXFlags, defaults Volume 127/Pitch 1.0/Priority 8/DistanceCutoff 3000, `F;`-started rows valid | `soundinfo.rs:137-203` (747 ack rows), `soundinfo.rs:557` `parse_slk` | MATCH | — | row count 747 asserted in unit test | — |
| 60 | Distance/cutoff gate | 3D + cutoff≠0 + listener → reject when `dist² > cutoff²` (`0x6f102ac9..0x6f102b37`); cutoffs ack 100000 / combat 2100 / death 3000 | `soundinfo.rs:87-90`, `game_audio.rs:353-381` (`GATED` log + counter) | MATCH | — | equality audible (`>` not `>=`) | — |
| 61 | **Voice limits / channel priorities** | channel handed to the mixer (`play_instance+0x70`, `param[0x13]`); `Priority` (`+0x38`, `param[0x12]`); `CHANNELFULLPREEMPT` evicts a same-channel voice; `NODUPLICATES`/`NODUPEUSERNAMES` suppress duplicates; the numeric cap lives in Mss32 (not in Game.dll) | **BEFORE**: one global 64-voice pool, priority steal only (`engine.rs` pre-fix); `channel`/`flags` parsed but unused. **AFTER**: `PlayParams.channel/preempt/dedup` `engine.rs:108-135`, `admit_channel` `engine.rs:634-688`, config `engine.rs:193`, counters `engine.rs:288-296`, web `game_audio.rs:400-410,452-460` | **DIFF → MATCH (fixed here)** | P0 | retail rows: What/Pissed/Warcry channel 1, YesAttack 2, Yes 3, Ready 4, all priority 1000 + `CHANNELFULLPREEMPT` (parsed from `analysis/war3_full3/UI/SoundInfo/UnitAckSounds.slk`, 739 rows) | §3 |
| 62 | Ambient/music | tileset day/night ambience chain `WorldEditData`→`MIDISounds`→MIDI+DLS; race music | `ambient.rs` (1086 LOC), `events.rs:545` `music_for_race` | MATCH | — | renders Lordaeron-summer loop in tests | — |
| 63 | Occlusion/ducking | attenuation fn `0x6f10c880` (3 zones, sqrt distance), gated by flags `0x4000` | `engine.rs:816-817` low-pass + `game_audio` ducking | UNPROVEN | P3 | our curve is a low-pass approximation, not the engine's mixer attenuation | ticket: port the three-zone curve |

### 1.6 `wc3-render` — draw state and skinning (12 244 LOC)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 64 | Blend/alpha table | mode 1 `SRC_ALPHA/1-SRC_ALPHA` + alpha test **192/255**; mode 2 same pair + test **4/255**; mode 3 `SRC_ALPHA/ONE`; mode 4 `DST/0`; mode 5 `DST/SRC` (`0x6f767254/26c/84c/864/6f766648`) | `mesh.rs:128-176` `blend_state`, `mesh.rs:511-571` `PipelineKind` + `alpha_ref` | MATCH | — | table test `mesh.rs:2270-2286` | — |
| 65 | Skinning | matrix 48 B column-major 3×4, position `m0*x+m3*y+m6*z+m9`, normal drops translation; per-vertex index stride; matrix groups resolved whole | `wc3-anim/src/lib.rs:655` `MAX_INFLUENCES=5`, `GeosetSkin::from_geoset`; palette `mesh.rs:1750-1770`; shader `mesh.rs:400-440` | MATCH | — | 5-matrix group test `mesh.rs:2291-2334`; wide groups fall back to CPU (`MAX_BONES_PER_VERTEX=4`) | — |
| 66 | Two UV sets / TEXCOORDINDEX | two texture units; layer picks `TEXCOORD0/1` by layer index | `mesh.rs:94-110` (`uvs`/`uvs2`), `mesh.rs:379-382` shader `layer_uv`, `MeshLayer.uv_set` `mesh.rs:308-310` | MATCH | — | `render_two_uv_sets` test `mesh.rs:2189-2210` | — |
| 67 | Palette / node hierarchy | `CMatrixGroup` append `{first,count,groupIndex,-1,-1}` `0x6f032c50` | `wc3-anim` node stack + `push_palette` | MATCH | — | group dedup test | — |
| 68 | Particles/ribbons | PRE2 528 B, RIBB | `emitters.rs` (particle blend from layer mode `emitters.rs:494`) | UNPROVEN | P3 | no engine-side emitter step extract | ticket |

### 1.7 `wc3-re` / `wc3-jass` — script layer (19 752 + 1 501 LOC)

| # | module item | engine contract | our implementation | verdict | sev | evidence | fix/ticket |
|---|---|---|---|---|---|---|---|
| 69 | JASS lexer/parser | JASS2 grammar (`Scripts\common.j` + map script) | `crates/wc3-re/src/jass/{lexer,parser,ast}.rs:23` `parse`; `wc3-jass/src/lib.rs` interpreter | MATCH | — | 96-map corpus compiles and runs | — |
| 70 | Native registry | 1096 engine registrations at `0x6f62f140` | 1665 rows (`registry_gen.rs`), 1096 `engine_registered`, **516 handler rows** (`natives.rs` `handler_table!`), `Convert*` identity rule `natives.rs:615-620` | MATCH (corpus) / DIFF (engine surface) | P2 | corpus: **167/167 distinct natives handled, 67 085 calls, 0 unresolved** (`coverage_report.json`, `native_census.json`); engine: 428/1096 = 39 % handled (`registry::stats` assertion in `tests/jass_native_coverage.rs:200-204`) | §5.7 |
| 71 | Native coverage gate | n/a (ours) | `tests/jass_native_coverage.rs` (Justice + 96-map corpus, header-absent ≤20) | MATCH | — | test asserts every corpus native has a handler | — |

---

## 2. Punch list

### P0 — fixed in this lane
1. **Audio per-channel voice limits, priorities and duplicate suppression** — was: one global 64-voice pool with priority steal; the parsed `Channel` and `CHANNELFULLPREEMPT`/`NODUPLICATES` flags were dead data. Now implemented and tested (§3).

### P1 — open, other lanes (report-only)
1. `crates/wc3-sim/src/combat.rs:77-87` — invented 5 %/10 % cross-class miss. Engine has no such table; only `MissDamageReduction=0.5` (`MiscGame.txt:139`). Effect: ~5 % of ground→air auto-attacks and 10 % of air→ground attacks are silently dropped every game. **Patch in §5.1.**
2. `crates/wc3-sim/src/lib.rs:3251-3300` — creeps answer only inside their own camp; engine calls for help within **600 u** (`CreepCallForHelp=600`). Measured on Justice: **19 cross-camp pairs** within 600 u would respond in retail and do not here. **Patch in §5.3.**
3. `crates/wc3-sim/src/lib.rs:208-213` + `ai.rs` harvest issuance — direct `Order::Harvest` instead of the engine's ability-slot dispatch `0x6f25c2c0`, so `GetUnitCurrentOrder`, `StopGathering` and re-issue semantics differ. **Patch in §5.2.**
4. `crates/wc3-formats/src/w3e.rs:186-215` — one 7-byte tile layout for v3..v11; the engine branches (v<8 f32 heights, v5..7 fourth float, v<6 no custom-tileset field). No corpus map is affected (97 v11 + 2 v12), but any RoC-era v3..v7 map misparses. **Patch in §5.4.**
5. `crates/wc3-sim/src/lib.rs:143` + `crates/wc3-re/src/creeps.rs:29` — camp membership by straight-line distance with two different radii (600 sim / 300 RE helper); engine uses pathing-cell distance (`CreepCampPathingCellDistance`, value not in repo). **Patch in §5.3.**
6. `crates/wc3-sim/src/jass/natives` — 430 engine-registered natives declared in `common.j` have no handler (never called by the 96-map corpus, so no corpus failure, but campaign/melee-AI scripts can hit them). Ranked list computed in §5.7.

### P2/P3 — tickets
- `wc3-sim/src/combat.rs` attack-type enum order ≠ engine ids (§5.8).
- `wc3-sim/src/lib.rs:120` lumber capacity hardcoded 10.
- `wc3-sim/src/lib.rs:205-206` upkeep tax 30/60 % not in the local extract.
- `wc3-path/src/lib.rs:66` A* has no node cap (worst case = full grid; 512×384 cells × 2 u32 vectors = 1.5 MB per query).
- `wc3-formats/src/w3e.rs:151` unconditional custom-tileset read.
- `crates/wc3-game/src/*.bak-*`, `*.orig`, `assets.rs.rej` — stale patch leftovers in `src/` (the `.rej` hunk is applied; files are dead weight).
- `wc3-render` particle emitters and `wc3-audio` occlusion curve are donor/procedural, not engine-derived.
- FDF semantics, order ids, engine tick rate: not extracted yet.

---

## 3. What was actually changed (audio) + test evidence

### 3.1 Mechanism
`SoundInfo` rows already carried `Channel`, `Flags` and `Priority`; nothing consumed them. The engine hands channel and priority to the mixer (`play_instance+0x38`/`+0x70` → `param[0x12]`/`param[0x13]`, `0x6f10c880`) and marks rows that may evict a same-channel voice with `CHANNELFULLPREEMPT`. Implemented exactly that shape:

* `crates/wc3-audio/src/engine.rs`
  * `PlayParams { channel, preempt, dedup }` (`:108-135`);
  * `AudioConfig.channel_voices` + `DEFAULT_CHANNEL_VOICES = 1` (`:180-211`) — the numeric cap is **not** in Game.dll (Mss32 owns it, `constants.json:voice_limits.status = MISSING`), so it is a documented, configurable constant: one voice per channel number reproduces retail's one-line-per-channel behaviour with the shipped channels 1..4;
  * `Core::admit_channel` (`:634-688`): dedup first, then capacity, then `CHANNELFULLPREEMPT` eviction of the lowest-priority (tie → oldest) voice; lower priority loses;
  * `AudioStats { voices_channel_full, voices_channel_preempted, voices_deduped }` (`:288-296`);
* `crates/wc3-audio/src/soundinfo.rs`: `SoundDef::channel_number`, `has_flag`, `dedup_key` (FNV-1a) — `:93-127`;
* `crates/wc3-audio/src/game_audio.rs`: the def play path and the ambient path pass channel/flags through (`:400-410`, `:452-460`); `stats_line()` now reports `chan_full=/chan_preempt=/deduped=` (`:209-240`).

### 3.2 Tests added
| test | asserts |
|---|---|
| `engine::tests::channel_cap_drops_without_preempt` | 2nd voice on a channel without the flag is dropped; other channels and channel 0 still play |
| `engine::tests::channel_full_preempt_stops_the_lowest_priority_voice` | equal priority replaces (retail ack behaviour), higher priority replaces, lower is refused |
| `engine::tests::noduplicates_suppresses_a_second_copy` | same dedup key on a channel plays once |
| `soundinfo::tests::channel_number_parses_the_slk_cell` | numeric/garbage/empty cells → 3, 12, 0 |
| `soundinfo::tests::flags_drive_preempt_and_dedup` | flag parsing + dedup key presence |
| `soundinfo::tests::real_ack_rows_carry_channel_and_priority` | shipped table: What/Pissed/Warcry→1, YesAttack→2, Yes→3, Ready→4, priority 1000, `CHANNELFULLPREEMPT` |

### 3.3 Command and result (via the device-wide wrapper)
```
scripts/h2-cargo test -p wc3-audio --offline
  test result: ok. 45 passed; 0 failed          (lib: engine + soundinfo + ambient + game_audio)
  test result: ok. 20 passed; 0 failed          (tests/audio.rs)
  test result: ok.  1 passed; 0 failed          (tests/no_alloc.rs)
  test result: ok.  2 passed; 0 failed          (tests/shutdown.rs)
  test result: ok.  0 passed; 1 ignored         (doc tests)
```
rc = 0 (all suites green after the change).

### 3.4 Memory note (measured, not guessed)
The heaviest wc3-audio test binary (`target/debug/deps/audio-*`, 20 integration tests,
`--test-threads=2`) was sampled through `/proc/<pid>/status:VmHWM` every 100 ms:

```
PEAK_RSS_KB=72888 rc=0   (≈71 MiB)
```

Every cargo command in this lane ran through `scripts/h2-cargo` (device-wide flock,
`MemAvailable ≥ 3.2 GB` gate, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`RUST_TEST_THREADS=2`); no direct `cargo` invocation, no workspace-wide run, only
`-p wc3-audio`.

---

## 4. Known holes — confirmed / refuted with numbers

| hole (from the dispatch) | verdict | number / evidence |
|---|---|---|
| audio has no engine-style simultaneous voice cap / channel priorities | **CONFIRMED before the fix** (global pool only, channel field dead), **implemented now** | channels 1..4 × 739 rows, priority 1000, `CHANNELFULLPREEMPT` on every ack row; per-channel cap is in Mss32 → configurable constant, documented |
| creeps: proximity clusters vs engine camp data; "22 camps by cluster vs map data" | **CONFIRMED as a mechanism difference, count is stable** | Justice: 97 neutral-aggressive placements; clusters at 300/400/**600**/900 = 47/32/**28**/27 (all placements) and the sim's guard-only view = **22 camps / 76 armed guards** (test-asserted). All camp anchors are >600 u apart → the count does not depend on the radius; the **membership metric** (straight line vs pathing cells) does. Aggro-share is the measurable gameplay gap: **185 pairs within 600 u, 19 cross-camp**. |
| pathfinding: our A* cost/node cap vs the engine's | **UNPROVEN, honestly** | engine: only `RegisterPathingFootprint 0x6f110d60`, kind strings `0x6f7551a0/94/88`, columns `pathTex`/`movetp`; 6 probes failed to locate the A* (ENGINE_MAP §7 probe 3). Ours: octile 10/14, consistent heuristic, unbounded open set (`wc3-path/src/lib.rs:66-144`). |
| harvest: direct order vs ability-slot routing `0x6f25c2c0` | **CONFIRMED DIFF** | engine: `HarvestGold 0x6f261360` → `0x6f25c2c0(unit, slot 0..3, target, flag)`, ability ids at `unit+0x1b8+slot*4`, cancel via `StopGathering 0x6f260a60` / `ClearHarvestAI 0x6f261330`. Ours: `Order::Harvest{mine, phase}` + 3-phase machine (`lib.rs:208-213, 5490-5700`). Observable difference: no order id for `GetUnitCurrentOrder`, `StopGathering` is not distinguishable from `Stop`, and a unit with a harvest ability in a non-zero slot cannot be modelled. |
| W3E v3..v5 parsed with the v6+ layout | **CONFIRMED DIFF (and broader than v5)** | engine: `0x6f3eb2c9 cmp [ebp+0xc],6` (no custom-tileset field below v6), tile reader selection `0x6f3eb540` → `if (version < 11) fcn.6f3eb760(version) else fcn.6f3eb920`; inside, `cmp ebx,8; jl` selects **f32** ground/water/extra reads, `cmp ebx,4` adds a u16, `cmp ebx,5..8` adds a fourth f32. Ours: fixed 7 bytes. Corpus exposure: 0/99 maps (97 v11, 2 v12). |
| JASS 428/1096 (39 %), list the top-20 missing by corpus call frequency | **REFUTED for the corpus** | `native_census.json`: 96 maps, 167 distinct engine natives, 67 085 calls, **0 missing**; top calls createunit 11 620, player 6 584, setunitacquirerange 5 269, triggerregisterunitevent 3 772, triggeraddaction 3 215 … all handled. The 39 % figure is engine *registrations*; the unhandled ones (430 declared in `common.j`) are never called by any corpus map — ranked work list in §5.7. |

---

## 5. Proposed patches for the owning lanes (not applied here)

### 5.1 Remove the invented cross-class miss (`wc3-sim`)
```rust
// crates/wc3-sim/src/combat.rs:66-90  DELETE the policy constants and function.
// Engine evidence: no cross-class miss table exists in Game.dll; the only miss
// constant is Misc/MissDamageReduction=0.5 (MiscGame.txt:139), consumed by
// ability-driven misses (Curse/Drunken Brawler), not by auto-attacks.
```
`lib.rs:2949` then rolls only the buff-driven miss chance; `tests/combat_depth.rs`
expectations that assume 5 %/10 % must be re-pinned to 0.

### 5.2 Harvest as an ability-slot order (`wc3-sim`)
```rust
// Order::Harvest gains `slot: u8` (0..3) and an `order_id() -> i32` that mirrors
// the engine's harvest ability order id; `issue_harvest` routes through
// `slot_ability(slot)` exactly like `0x6f25c2c0`; `Stop` clears the slot order,
// `StopGathering` clears only harvest. GetUnitCurrentOrder returns order_id().
```

### 5.3 Creep call-for-help 600 u + pathing-cell camps (`wc3-sim`, `wc3-re`)
```rust
// lib.rs: after damage lands on a neutral-aggressive creep, iterate every
// alive owner==2 camp guard within MISC_CREEP_CALL_FOR_HELP (600.0) and set
// Order::Attack(attacker) — independent of camp id.
pub const CREEPS_CALL_FOR_HELP: f32 = 600.0; // MiscGame.txt:93 (TFT) / :102 (RoC)
// creeps.rs: unify DEFAULT_CLUSTER_RADIUS with CAMP_CLUSTER_RADIUS (one number),
// and replace the Euclidean test with a pathing-cell distance when the WPM grid
// is available; `CreepCampPathingCellDistance` value still unknown → keep the
// world-unit fallback but document it as such.
```

### 5.4 Version-branched W3E tile reader (`wc3-formats`)
```rust
// w3e.rs: parse custom_tileset only for version >= 6 (0x6f3eb2c9);
// tile record:
//   v3..v4 : u8 texture, u8 variation, f32 h, f32 w, f32 extra, u8 cliff_lo,
//            u8 cliff_hi, u8 cliff_var                     (17 B)
//   v5..v7 : same + f32 water_level                       (21 B)
//   v8..v10: u8 texture, u8 variation, u16 h, u16 w, 3 B cliff, u16 extra (11 B)
//   v11    : u8/u16 texture, u16 h, u16 w(+0x4000 water flag), u8 cliff… (7 B)
// Height conversion for the float forms: h16 = ((value + 32768.0) * 0.0078125) as i32,
// w16 = ((value + 2048.0) * 4.0) as i32   (0x6f3eb81d-0x6f3eb863).
```
Gate: only `parse` is affected; `parse_classic_engine` stays the runtime entry.

### 5.5 Per-unit carry capacity (`wc3-sim`)
```rust
// UnitBalance.slk goldRep/lumberRep columns → UnitStats { gold_capacity, lumber_capacity }
// instead of lib.rs:120 lumber = 10.
```

### 5.6 Bound the A* (`wc3-path`)
```rust
// lib.rs:66: add `max_expansions: usize` (e.g. 4 * width * height / 8) and return
// None when exceeded; also reuse per-grid scratch buffers instead of allocating
// two u32 vectors per query (1.5 MB at 512x384).
```

### 5.7 Ranked unhandled natives (`wc3-sim/src/jass`)
Computed from `registry_gen.rs` (engine_registered = true) minus `natives.rs` handler
rows: **707 names**, of which **430 are declared in `Scripts/common.j`** (the rest are
engine-internal). Corpus call frequency for all of them is **0** (they never appear in
67 085 corpus call sites), so this is debt, not a corpus blocker. Highest-value groups
that a melee/campaign script can reach: `ChooseRandomCreep`, `ChooseRandomNPBuilding`,
`CreateCorpse`, `CreateBlightedGoldmine`, `AddPlayerTechResearched`,
`AddResourceAmount`, `AddIndicator`, `AddSpecialEffect*` (8 variants), `ChangeLevel`,
`CommandAI`, `CachePlayerHeroData`, `CameraSetupGet*`, `CameraSetSource/TargetNoise`,
`AdjustCameraField` — implement the `AddSpecialEffect*`/`AddIndicator`/`AddResourceAmount`/
`ChooseRandomCreep` group first (cheap, side-effect-only, and they are the ones a
campaign map actually calls).

### 5.8 Attack-type ids (`wc3-sim`)
Add `AttackType::engine_id()` (0=spells … 6=hero) and use it at every serialisation
boundary (save, `GetUnitState`, item/ability data) so the on-disk/script-visible id
matches the engine; keep the internal enum order for the table index.

---

## 6. Reproduction

```
# engine anchors (this session)
B=analysis/tft_files_Files100_Game.dll
r2 -q -e scr.color=0 -e bin.relocs.apply=true -c 's 0x6f3eb1f0; pd 45' $B   # W3E version gate
r2 -q -e scr.color=0 -e bin.relocs.apply=true -c 's 0x6f3eb2c0; pd 30' $B   # custom tileset only v>=6
r2 -q -e scr.color=0 -e bin.relocs.apply=true -c 's 0x6f3eb540; af; pdg' $B # tile loop, reader by version
r2 -q -e scr.color=0 -e bin.relocs.apply=true -c 's 0x6f3eb760; pd 78' $B   # per-version tile layout
r2 -q -e scr.color=0 -e bin.relocs.apply=true -c 's 0x6f3eb8e6; pd 24' $B   # v5..v7 fourth float

# Justice camp measurement (this session)
scripts/h2-cargo run -q -p wc3-mpq --offline --example dump -- \
  "donors/War3Net/.../multi-(2)Justice.w3x" war3mapUnits.doo $PREFIX/tmp/opencode/justice_units.doo
python3 $PREFIX/tmp/opencode/justice_camps2.py      # 97 neutrals, 28 clusters @600, 19 cross-camp pairs

# audio gate
scripts/h2-cargo test -p wc3-audio --offline        # rc=0, 68 tests
```

## 7. Limits of this audit

* Engine addresses are static-analysis observations; nothing was executed under the
  retail engine. Where a constant is not in the local extracts it is marked
  `reported-unverified`/`UNPROVEN` instead of being approximated.
* `wc3-data` row-level column parity was not re-verified column by column; the rows
  above rest on the measured coverage report and the schema tests.
* The 39 % engine-native figure is a *registration* count; the corpus (what maps call)
  is 100 % handled. Do not read the two numbers as a contradiction.

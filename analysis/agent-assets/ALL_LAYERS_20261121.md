# ASSETS / FORMATS / MPQ — all-layer push, 2026-11-21

Agent C · packet `analysis/tech-lead/all-layers-20261121/PACKET_C_assets-formats.md` · ledger 599
Owners written: `crates/wc3-formats/**`, `crates/wc3-assetdb/**`, `crates/wc3-mpq/**` only.
Oracle: the operator's extracted retail tree `analysis/war3_full3` (10 523 files),
`analysis/war3_mpq_listing.txt` (10 525 `FILE` entries + 1 `ARCHIVE` header) and the
308-file `analysis/war3_data`. Every number below is reproduced by a command or a named test.

## 1. Listfile 1:1 against the shipped tree

| check | observed |
|---|---|
| archive listing | 1 `ARCHIVE` line, **10 525** `FILE` lines (10 527 lines total incl. `SUMMARY`) |
| extracted tree | **10 523** files |
| listed but not extracted | exactly **`(listfile)`** and **`(attributes)`** (StormLib metadata pseudo-entries) |
| extracted but not listed | **0** |
| per-file size drift | **0** (every on-disk size equals the listing size) |
| total bytes | tree total == matched listing total |

Proven by `crates/wc3-assetdb/tests/listfile_parity.rs` →
`assets_mpq_listfile_1to1_shipped_tree` (skips with an explicit line when the tree is absent).
Method: `DirSource::open(analysis/war3_full3)` (the same provider the runtime uses) vs a
standalone parse of the listing (`FILE <path> <size>`), case-insensitive archive spelling.

## 2. BLP inventory — byte oracle and decoder

Byte census (`python3` over the tree, independent of the Rust decoder):

* **3 935** `.blp`, all magic **BLP1** — 2 920 JPEG (compression 0), 1 015 paletted (compression 1);
  39 distinct dimensions; 1–10 stored mips per file.

Full decode (`asset_catalog --extracted analysis/war3_full3`, rc=0,
report `analysis/agent-assets/catalog-20261121/`):

```
textures: files=3935 ok=3935 header_fail=0 decode_fail=0 mip_fail=0 jpeg=2920 paletted=1015 alpha=1563 opaque=2372
```

Report JSON totals: `decoded_pixels=98 722 207`, `mip_levels_declared=28 748`,
`mip_levels_decoded=28 748`, `mips_incomplete_files=0`, `mip_layout_bad=0`,
`source_bytes=148 430 928`, `min_side=4`, `max_side=512`, `all_transparent=5`,
`constant_color=56`.

Named test: `assets_models_blp_inventory_shipped_tree` (headers always; full mip decode with
`H2_ASSET_CORPUS=1`) asserts the counts, the 39 dimensions, the 2 920/1 015 split, the
98 722 207 pixel total, the 28 748 decoded/declared mips and the 4..512 size range.

## 3. MDX inventory — byte oracle and parser

Byte census: **2 048** `.mdx` (16 of them uppercase `.MDX`), every one `MDLX` version **800**,
0 foreign magics. Top-level chunks present: VERS 2048, MODL 2048, SEQS 2046, MTLS 1920,
TEXS 2022, GEOS 1914, BONE 1914, PIVT 2035, GLBS 508, GEOA 804, HELP 743, ATCH 369, PRE2 630,
RIBB 65, EVTS 577, CLID 279, CAMS 369, TXAN 35, PREM 24, LITE 152.

Full parse (same catalog run):

```
models: files=2048 parsed=2048 parse_fail=0 refs=5216 replaceable=1138 unresolved=0 models_with_unresolved=0 all_resolved=2048
```

Report JSON totals: `geosets=5 127`, `sequences=6 512`, `nodes/pivots=36 023`,
`source_bytes=67 376 096`.

Named test: `assets_models_mdx_inventory_shipped_tree` (v800 header inventory always;
full parse + the three totals with `H2_ASSET_CORPUS=1`).

## 4. SLK cell values

Independent-reader audit over **all 33** shipped `.slk` (stateful `C;X;Y;K` reader, `""`
unescaping, header row at Y1) against `wc3-formats`' `slk_dump`:

```
FILES=33 bad=0 rows=6785 nonempty_cells=171614 quoted_values_with_semicolon=0
```

`analysis/war3_data` carries the same 33 tables (md5-identical), so the data tree and the
extracted tree agree.

Named tests assert the sheet shape and real cells:

* `abilities_data_slk_cells_shipped` — `Units/AbilityData.slk` 56 columns × 399 rows;
  `AHhb` (Paladin – Holy Light): `Cool1..3=5`, `Cost1..3=75`, `Data11=200`, `Data21=400`,
  `Data31=600`, `Rng1=800`, `levels=3`, `reqLevel=1`, `hero=1`,
  `targs=air,ground,organic,notself,invu,vuln,nonancient`.
* `items_data_slk_cells_shipped` — `Units/ItemData.slk` 21 × 132; `ratc` (Claws of Attack +12):
  `Level=7`, `goldcost=500`, `HP=100`, `abilList=AItc`, `armor=Wood`, `prio=63`,
  `stockRegen=120`, `file=Objects\InventoryItems\TreasureChest\treasurechest.mdl`.
* `combat_damage_table_stats_slk_cells_shipped` — `Units/UnitBalance.slk` 44 × 466 and
  `Units/UnitData.slk` 34 × 466. Footman `hfoo` balance cells: `HP=420`, `realHP=420`,
  `def=2`, `defUp=2`, `defType=medium`, `level=2`, `goldcost=160`, `bldtm=20`, `sight=1400`,
  `nsight=800`, `spd=270`, `regenHP=0.25`, `regenType=always`, `upgrades=Rhar,Rhme,Rhde`,
  `sortBalance=a2`. Footman data cells: `movetp=foot`, `collision=31`, `points=100`,
  `turnRate=0.6`, `castpt=0.3`, `castbsw=0.51`, `death=3.04`, `impactZ=60`, `launchZ=60`,
  `prio=6`, `propWin=60`, `targType=ground`, `race=human`, `valid=1`.

## 5. MPQ codec census (shipped archive bytes)

`codec_census` over the 16 shipped `.mpq` and the 44 campaign `.w3m` containers, rc=0:

| set | archives | files decoded | failures | bytes | codec histogram |
|---|---|---|---|---|---|
| `.mpq` | 16 | **685** | **0** | 45 082 510 | stored 513, zlib 165, stored+zlib 7 |
| `.w3m` | 44 | **708** (each map ships a 15–17-entry listfile) | **0** | 52 575 196 | pkware 44, zlib 617, stored+zlib 36, stored 11 |
| total | 60 | 1 393 | 0 | 97 657 706 | inside the five-codec mask 0x0CB |

All 44 campaign maps open at base 512 and every inner format parses (`map_corpus_audit`:
`SUMMARY maps=44 opened=44 failures=0`; W3E v11, W3I v18, WPM, doo/units, w3u in each).

Every declared tag is inside the engine's mask (`0x01` Huffman, `0x02` zlib, `0x08` PKWARE,
`0x40`/`0x80` ADPCM); no archive needs bzip2/sparse/LZMA.

### Drift fixed: `.w3m` containers could not be opened by the census

`MpqArchive::open` reads the MPQ header at offset 0; a classic RoC `.w3m` starts with a
512-byte `HM3W` header (all 44 shipped maps: signature at **512**), so the census failed with
`BadMagic([72,77,51,87])` = `HM3W`. Several call sites hand-rolled their own 4 KiB scan
(`wc3-formats::map::map_mpq_base`, every `wc3-sim` test helper).

Fix (single rule, `crates/wc3-mpq`):

* `MpqArchive::mpq_base(head)` — `0` for `MPQ\x1a`, first-signature offset for `HM3W`,
  `None` otherwise;
* `MpqArchive::open_any(path)` — bare archive or map container in one call, typed
  `BadMagic` for anything else;
* `wc3_formats::map::map_mpq_base` now delegates to it (no duplicated scan);
* `codec_census` opens through `open_any`; for a listfile-less archive it probes the known map/data names.

Named tests: `assets_mpq_shipped_archives_census` (16 archives, 685 entries, 0 failures,
14 stored-only archives, 172 zlib-declared entries, tags inside 0x0CB),
`assets_mpq_map_containers_open` (44 maps at base 512, 44 terrain, 44 scripts, 0 failures),
`assets_mpq_open_any_rejects_foreign_files` (plain file and `HM3W` without a signature are
typed `BadMagic`).

## 6. Drift fixed: MDX keyframe times are `int32` (WC3-I-009)

The catalog pass surfaced `GAME_DATA_SKIP reason=mdx_track_unsorted` 8 times on 3 models
(Hydralisk, HeroFarSeer, MainMenu3d). Root cause:

* the file stores keyframe times as **signed `int32`** — mdx-m3-viewer reads
  `stream.readInt32()` (`src/parsers/mdlx/animations.ts:40`, master, accessed 2026-11-21);
* Blizzard models use the negative range as **pre-start keys**: Hydralisk
  `BlizParticlePuff` emission rate starts at frame **-100** (`0xFFFF_FF9C`) before 3900,
  HeroFarSeer `BlizParticle01` visibility at **-167** and **-33**, MainMenu3d
  `BlizParticle04dandilion01` emission rate at **-133**;
* the parser compared and sorted as `u32`, so each pre-start key looked inverted and a
  `u32` sort moved it **past the sequence end**; `Track::sample` then interpolated toward
  that key at the tail, and `b.time - a.time` could underflow (debug panic) — the
  `WC3-I-009` defect;
* the engine's own sampling filters keys by `frame >= start`
  (`src/viewer/handlers/mdx/sd.ts:47`) and treats a negative span by wrapping the sequence
  (`:106-109`), i.e. pre-start keys never contribute to a sequence.

Fix (`crates/wc3-formats/src/mdx.rs`, full rewrite of the affected paths):

* `parse_track` orders keys by **signed** time (stable) and only reports a genuine signed
  inversion in `mdx_track_unsorted` (the shipped corpus has none — the 9 diagnostic hits
  were pre-start keys, not inversions);
* `Track::sample` walks keys in signed order, skips keys before frame 0, clamps to the
  first/last in-sequence key, and computes the span in `i64 → f32` so no `u32` subtraction
  can underflow; a track with only pre-start keys samples to `None` (caller's static value,
  matching the engine's per-sequence "no keys in range → default").

Named tests:

* `assets_models_mdx_signed_key_times` (in `mdx.rs`) — synthetic pre-start track:
  `sample(0)=1.0`, midpoint, tail holds the last value, pre-start-only track → `None`,
  a genuine `[500,100]` inversion parses as `[100,500]`;
* `assets_models_mdx_particle_prestart_keys_shipped` — the three real models: the pre-start
  key stays first in signed order and sampling past the last key holds its value; with
  `H2_ASSET_CORPUS=1` the whole corpus is scanned: **3 models / 9 tracks** carry pre-start
  keys (exact).

Census tool: `crates/wc3-formats/examples/mdx_track_time_scan.rs` prints every such track
(owner, key count, signed times, interpolation). Whole-corpus run over all 2 048 models:

```
$ find analysis/war3_full3 -iname '*.mdx' -print0 | xargs -0 ./target/debug/examples/mdx_track_time_scan
... owner="pre2 BlizParticlePuff" track=emission_rate keys=9 negative=[-100] sorted_signed=false interp=bezier times=[3900, 9067, ...]
... owner="pre2 BlizParticle03" track=visibility keys=11 negative=[-167] ...
... owner="pre2 BlizParticle04dandilion01" track=emission_rate keys=17 negative=[-133] ...
models_scanned=2048 files_with_negative_keys=3
       4 Units/Critters/Hydralisk/Hydralisk.mdx emission_rate
       4 Units/Orc/HeroFarseer/HeroFarSeer.mdx visibility
       1 UI/Glues/MainMenu/MainMenu3d/MainMenu3d.mdx emission_rate
```

## 6.1 Canonical catalog artifacts restored (red tests fixed)

`crates/wc3-assetdb/tests/art_c_icons.rs` includes
`analysis/agent-assets/texture-catalog.tsv` at compile time. That canonical file had been left as a
**100-byte header-only stub** (and `asset-catalog.json` as a 1.0 KB stub), so `artC_icon_keys` and
`artC_ghost_alpha` failed with "not in decoded-retail catalog" before any of their assertions ran.
The rc=0 catalog pass above regenerated the real artifacts; they are now restored at the canonical
path from `analysis/agent-assets/catalog-20261121/`:

```
texture-catalog.tsv 408.7K   model-catalog.tsv 143.0K   biome-catalog.tsv 881B
asset-catalog.json 2.7K      asset-catalog.md 1.8K
```

Both tests then resolve their retail rows (`BTNFootman.blp` alpha_bits 0, `TownHallCastleKeep.blp`
alpha_bits 8) and pass in the final run. No test body was changed.

## 7. Tests and commands (rc pasted)

```
scripts/h2-cargo test --offline -p wc3-formats -p wc3-assetdb -p wc3-mpq
```

Result: **rc=0**. Log `/data/data/com.termux/files/usr/tmp/opencode/agentC_tests5.log`
(209 tests, 0 failed):

| crate | targets | tests |
|---|---|---|
| `wc3-assetdb` | lib 21 · art_c_icons 2 · audio_hooks 4 · audio_parity 3 · blp_mips 3 · golden_blp 4 · hook_resolution 2 · **listfile_parity 1** · sound_paths 2 · source_resolution 5 | 47 |
| `wc3-formats` | lib 78 · **assets_shipped 6** · malformed_inputs 27 · map_corpus 1 · mdx_lod 4 | 116 |
| `wc3-mpq` | lib 34 · archive_parity 3 · **assets_mpq_census 3** · human04_map 1 · listfile_less 2 · real_archives_509 3 | 46 |

Full-corpus decode pass (headers + every BLP/MDX decoded, single-threaded):

```
H2_ASSET_CORPUS=1 scripts/h2-cargo test --offline -p wc3-formats --test assets_shipped -- --test-threads=1
```

Result: **rc=0**, 6 passed / 0 failed in 145.16 s (log
`/data/data/com.termux/files/usr/tmp/opencode/agentC_corpus_direct.log`), with the census printed:

```
test assets_models_blp_inventory_shipped_tree ... blp=3935 jpeg=2920 paletted=1015 distinct_dimensions=39
decoded_pixels=98722207 mips=28748 declared=28748 side=4..512
test assets_models_mdx_inventory_shipped_tree ... mdx=2048 geosets=5127 sequences=6512 nodes=36023
test assets_models_mdx_particle_prestart_keys_shipped ... models_with_prestart_keys=3 tracks=9
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 145.16s
```

## 8. Boundaries (honest)

* **No single `War3.mpq` on this device.** `/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq`
  is absent (only the 16 `A..Y` chunks plus `War3_low/med.mpq` in the tree). The
  byte-for-byte `audit_extract` example (archive payload vs extracted file, SHA-256) therefore
  cannot run; the replacement oracle is the archive **listing + per-file sizes** (10 523/10 523
  matched, 0 drift) plus the 60-archive decode census above. This is a device-input boundary,
  not a parser boundary: point `WC3_MPQ` at a single archive on a desktop install and the
  example runs unchanged.
* Sequence-bound **wrap** sampling (mdx-m3-viewer `sd.ts:106-109`) is implemented in
  `wc3_anim::sample_in_sequence` (owner: animation layer), not in `Track::sample`, which has no
  sequence bounds. `Track::sample` now clamps to the in-sequence keys; the sequence-aware
  sampler keeps the engine wrap. No shipped track outside the 9 pre-start ones exercises the
  difference (catalog: 0 parse failures, 0 unresolved texture refs).
* `analysis/war3_full3` and `analysis/war3_data` are untracked oracle trees; every test that
  needs them skips with an explicit `skip:` line on a checkout without them.

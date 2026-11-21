# original-parity — byte-level audit against the shipped game

Unit `original-parity` in the rust-anything loop. Engine: `scripts/original_parity.py`
(one process, `--json` for machines). The recording lives in
`analysis/port-parity/original-parity/cases.jsonl`, so any drift in these numbers fails
`scripts/port-parity-all.sh` like any other layer.

## What it proves

`analysis/port-parity/*` proves our output did not change. This unit proves a different
thing: that the output matches the shipped game where an oracle exists. Each check either

* parses the original with an **independent** reader (small Python parsers for SYLK, PE,
  MPQ via `mpyq`, SHA-256 of extracted bytes), or
* reads a shipped data file directly and compares it with a **runtime** Rust path that is
  used by the game (never a re-implementation of the check inside Rust).

Rust tools are built once per crate (`scripts/h2-cargo build`) and executed directly;
a crate that fails to build makes its checks FAIL — a stale binary is never accepted.

## Checks (19 applicable)

| Layer | Check | Oracle | Observed 2026-11-21 |
|---|---|---|---|
| mpq/inventory | listfile-vs-tree | `analysis/war3_mpq_listing.txt` vs `war3_full3` files | 10523 = 10523, missing 0, extra 0 |
| formats/slk | terrain-tables | independent SYLK row count vs `slk_info` | 4 tables, 0 mismatches |
| anim/mdx | geoset-count | `VRTX` blocks in `GEOS` / SEQS·132 / TEXS·268 vs `geoset_alpha`,`mdx_probe` | 3 models, 0 mismatches |
| render/blp | header | BLP1/BLP2 header vs `blp_info` | 5 files, 0 mismatches |
| campaign/maps | chapter-count | campaign map files vs `campaign_report chapters` | 44 = 44 |
| jass/natives | declared-vs-registry | `Scripts/common.j` `native … takes` vs registry | 791 original, missing 0, extra 874 documented |
| formats/maps | corpus-count | map files vs `map_corpus_audit maps=` | 44 = 44 |
| jass/signatures | signatures-vs-common.j | independent decoder vs `native_sig_dump` | 0 mismatches |
| render/blp | pixels-vs-independent-decoder | independent BLP1 RGBA decoder vs `blp_hash` SHA-256 | 8/8 identical |
| formats/slk | cell-values-vs-independent-parser | independent SYLK cells vs `slk_dump` | 137 lines, 0 mismatches |
| audio/soundinfo | event-files-vs-SoundInfo | `UI/SoundInfo/*.slk` (7 tables, 2530 defs) vs `audio_event_table` | 33 events, 1 documented gap, 0 mismatches |
| audio/voices | unitVoice-vs-UnitAckSounds | `Units/unitUI.slk` + `UnitAckSounds.slk` vs `unit_sound_sets` | 8 units, 0 mismatches, all voice files present |
| sim/units | stats-vs-UnitBalance | `UnitBalance.slk`+`UnitData.slk` vs `dump_gamedata sqlite` | 466 units × 10 fields, 0 mismatches |
| sim/weapons | damage-vs-UnitWeapons | `UnitWeapons.slk` vs `dump_gamedata unit` | 316 rows, 30-unit stride sample, 0 mismatches |
| formats/map-contents | inner-files-vs-independent-mpq | `mpyq` container listing + SHA-256 vs `map_files` | 44 maps, 620 presence, 548 byte-identical |
| formats/map-bytes | data-tree-vs-shipped-tree | `war3_data/Maps` vs `war3_full3/Maps` SHA-256 | 44 = 44, 0 mismatches |
| guest/pe | headers-vs-independent-parser | independent PE32 reader vs `pe_inspect` | 15 PE files, sections+exports+imports 0 mismatches |
| **mpq/real-archive** | retail-vs-stormlib-tree | `compare_oracle` retail `War3.mpq` vs the StormLib tree | 10 523/10 523 matched, 0 read_fail, 710 907 954 bytes |
| **mpq/codec-census** | decode-all-file-codecs | `codec_census` on the retail archive | 10 523 files, 0 failures, 7 codec combinations |

## NOT_APPLICABLE (honest boundary, not a skipped check)

| Layer | Reason | Next step |
|---|---|---|
| input/router | the original game has no touch router | device evidence (`operator-bugs` scenarios) |
| audio/layered | needs the layered War3/War3x install (TTS/proxy) | device audio stats |
| render/terrain | needs a GPU adapter | device frames (`GAME_TERRAIN_*`) |

`guest/pe` becomes NOT_APPLICABLE only when no `.exe`/`.dll` exists under `analysis/`;
the current samples are the operator's own licensed files.

`mpq/real-archive` and `mpq/codec-census` are NOT_APPLICABLE when no retail archive is
found at `$WC3_MPQ` or `~/.local/share/wc3-install/War3.mpq` — a fresh clone stays green
and the reason names the exact path to drop the extracted archive at.

## Retail archive (RoC, 2026-11-22)

* Installer: `.../Warcraft_install/{Warcraft III.mdf,THEFROZENTHRONE.mdf}` (Alcohol MDF),
  sector stride 2448 B (2352+96), data at +16, `War3.mpq` at LBA 110071, 438 529 036 B.
* `~/.local/share/wc3-install/War3.mpq`: `MPQ\x1a`, listfile 10 523, blocks 10 560.
* Cost on this device: `compare_oracle` ~3m54s, `codec_census` ~1m35s (SHA-256/decode of 710 MB).

## Rules

* Never edit `cases.jsonl` to hide a mismatch (`gate.port.no-fixture-edit`); an intended
  difference goes to `known-diffs.jsonl` with a reason.
* Re-record after a legitimate output change: `retire` the active case with a reason,
  then `record` the new one.
* A check is never weakened to pass: a missing oracle is NOT_APPLICABLE with the reason,
  and a divergence is a FAIL with the unit/field/expected/actual rows.

## Run

```sh
python3 scripts/original_parity.py          # human summary + ORIG_PARITY_SUMMARY
python3 scripts/original_parity.py --json   # machine form (checks/applicable/failures)
bash scripts/port-parity.sh gate original-parity --json
```

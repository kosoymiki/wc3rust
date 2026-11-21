# r2 artifact provenance

Generated 2026-11-12 with `r2 6.2.6` / `rabin2` on the operator's local
binaries; `su -c id` verified `uid=0` (KernelSU).

| artifact family | source | command |
|---|---|---|
| `*.info.txt`, `*.headers.txt`, `*.sections.txt` | `analysis/game_files`, `analysis/tft_files_*` | `rabin2 -I/-H/-S` |
| `*.exports.txt`, `*.imports.txt` | same binaries | `rabin2 -E/-i` |
| `*.afl.txt` | r2 function lists | `r2 -q -c 'aa; afl'`; RoC/TFT Game.dll and Storm lists copied byte-for-byte from the project r2 run (`analysis/r2/artifacts`, `analysis/r2/tft`) |
| `*.pdc.txt` | key functions | `r2 -q -c 's <addr>; af; pdf; pdc'` |
| `STORM_ORDINALS.md` | Storm.dll PE exports | 370 ordinals; 19 names from `donors/StormLib/storm.dll/storm.def`; RVA = vaddr − 0x15000000 |
| `STORM_IMPORTS.md` | Game.dll imports | 92 unique `Storm.dll` ordinals |
| `SUMMARY.md` | all of the above + `INVENTORY.tsv` SHA-256 | generated table |

Key addresses: `GameMain` RoC `0x6f004180`, TFT `0x6f005e70`; boot
`0x6f004200`; shutdown `0x6f004b80`; War3 entry `0x00467fdc`; Storm RoC entry
`0x15032950`.

No names were invented: a Storm function without a name in `storm.def` is
reported as `Ordinal_N`.

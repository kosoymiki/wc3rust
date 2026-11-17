### `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x` — Justice

- author: **Blizzard Entertainment** · recommended: `1v1` · `w3i` v26 · melee flag: `true`
- players: 2 playable / 0 neutral · playable area 122x92 · terrain 129x97 · pathing 512x384
- runtime: `2` start markers, `7` gold mines, `30` units, `1997` destructables, `11` camps (sim: mines=7 destructables=1997 units=30)
- camps: **2/26** fully spawnable by the runtime tables

| category | map placements | defined by shipped data | runtime used | runtime gaps | unknown ids | note |
|---|---:|---:|---:|---:|---:|---|
| start_locations | 2 | 2 | 2 | - | - | `sloc` markers from war3mapUnits.doo |
| gold_mines | 7 | 7 | 7 | - | - | `ngol`/`ugol` placements |
| shops | 5 | 5 | 0 | ngad, ngad, nmr8, nmr8, ntav | - | Sellunits= shop buildings |
| neutral_creeps | 85 | 83 | 30 | nfoh, nfsh, nfsp, ngad, nggr, nhdc, nhfp, nhhr, nkot, nmoo, nmr8, nmrl, nomg, nrdr, nrog, nshe, nsqt, ntav, nwzg | - | owner > 1 placements; camps cluster at 600 units |
| player_units | 0 | 0 | 0 | - | nrac | owner <= 1 placements in the map file |
| destructable_doodads | 2902 | 1997 | 1997 | - | - | doodad ids the shipped DestructableData defines |
| decoration_doodads | 905 | 0 | 0 | - | - | non-destructable doodads (no runtime entity expected) |

| start | x | y | inside | walkable | mines ≤1200 |
|---|---:|---:|---|---|---:|
| 1 | 14272 | 1984 | true | true | 1 |
| 2 | 2176 | 1984 | true | true | 1 |

| mine | x | y | nearest start | walkable | trees ≤500 |
|---|---:|---:|---:|---|---:|
| 1 | 14592 | 1280 | 773 | true | 3 |
| 2 | 1792 | 1280 | 802 | true | 4 |
| 3 | 14976 | 10816 | 8860 | true | 1 |
| 4 | 8448 | 704 | 5963 | true | 0 |
| 5 | 8448 | 11072 | 10794 | true | 4 |
| 6 | 1216 | 10624 | 8693 | true | 2 |
| 7 | 7360 | 11072 | 10463 | true | 3 |

| camp | x | y | units | runtime covered | ids |
|---|---:|---:|---:|---|---|
| 1 | 3968 | 10944 | 4 | false | nfoh, nrdr, nggr, nhfp |
| 2 | 1429 | 10205 | 4 | false | ngst, nhdc, nwzg, nhdc |
| 3 | 11776 | 11136 | 4 | false | nfoh, nggr, nrdr, nhfp |
| 4 | 1291 | 4347 | 2 | false | nhdc, nomg |
| 5 | 1925 | 1537 | 1 | false | nmrl |
| 6 | 5376 | 5568 | 4 | false | ngme, nhfp, nhfp, nsqt |
| 7 | 11136 | 5632 | 4 | false | ngme, nhfp, nsqt, nhfp |
| 8 | 8192 | 8576 | 7 | false | nmoo, nkob, nkog, nkob, nkob, nkog, nhhr |
| 9 | 4632 | 6418 | 4 | false | nhdc, nhdc, nkot, nkot |
| 10 | 14401 | 1499 | 1 | false | nmrl |
| 11 | 11067 | 6756 | 4 | false | nkot, nkot, nhdc, nhdc |
| 12 | 12176 | 3495 | 1 | false | nrac |
| 13 | 12430 | 692 | 2 | false | nrog, nhdc |
| 14 | 3935 | 540 | 2 | false | nhdc, nrog |
| 15 | 4173 | 3484 | 1 | false | nrac |
| 16 | 15232 | 6341 | 5 | true | nkog, ngrk, nkog, ngrk, nkol |
| 17 | 13890 | 6930 | 1 | false | nshe |
| 18 | 5817 | 10016 | 4 | false | ngnb, ngnb, nfsh, nhdc |
| 19 | 14951 | 10455 | 4 | false | nhdc, nhdc, nwzg, ngst |
| 20 | 8778 | 659 | 3 | false | nhfp, ngst, nfsp |
| 21 | 1257 | 6935 | 5 | true | nkol, ngrk, nkog, ngrk, nkog |
| 22 | 7948 | 10937 | 9 | false | nfsh, nban, nban, nbrg, nhhr, nhhr, nban, nban, nbrg |
| 23 | 3456 | 7569 | 1 | false | nshe |
| 24 | 15289 | 4098 | 2 | false | nomg, nhdc |
| 25 | 8060 | 661 | 2 | false | nhfp, nfsp |
| 26 | 9979 | 10119 | 4 | false | ngnb, ngnb, nfsh, nhdc |

| shop | x | y | runtime covered | sells |
|---|---:|---:|---|---|
| `ngad` | 1088 | 6720 | false | ngsp, nzep, ngir |
| `ngad` | 15232 | 6080 | false | ngsp, nzep, ngir |
| `nmr8` | 5504 | 9984 | false | nrog, nass, nkog, nfsh |
| `nmr8` | 10240 | 10112 | false | nrog, nass, nkog, nfsh |
| `ntav` | 8192 | 3904 | false | Nngs, Nbrn, Npbm, Nplh, Nbst |

**Map gate: PASS** — container/`w3e`/`wpm`/`w3i`/`doo` parsed, two walkable starts with adjacent mines, camps and trees placed, and the runtime takes its starts, mines and destructables from the map (no synthetic ones).

**Runtime parity: 58 placement(s) dropped** (owner: sim data tables)
- runtime tables miss 58 map unit placements defined by the shipped data: ["nfoh", "nfsh", "nfsp", "ngad", "nggr", "nhdc", "nhfp", "nhhr", "nkot", "nmoo", "nmr8", "nmrl", "nomg", "nrdr", "nrog", "nshe", "nsqt", "ntav", "nwzg"]

# Round 2 — map and biome acceptance

Status: **PASS**

| Gate | Status | Detail |
|---|---|---|
| `r2.maps.all` | **PASS** | 44/44 clean; core 44/44; units errors 0 |
| `r2.biomes.all` | **PASS** | 111/111 decoded; tilesets=15; missing=0 |
| `r2.box.boundary` | **PASS** | box classification=harness_boundary selected=1->0 |

## Maps

| # | map | core | units error | terrain | objects | script |
|---:|---|---|---|---:|---:|---:|
| 1 | Campaign/BonusCredits.w3m | PASS | PASS | 36225 | 21 | 253889 |
| 2 | Campaign/Human01.w3m | PASS | PASS | 9409 | 39 | 385683 |
| 3 | Campaign/Human02.w3m | PASS | PASS | 16641 | 19 | 295737 |
| 4 | Campaign/Human02Interlude.w3m | PASS | PASS | 6305 | 3 | 86065 |
| 5 | Campaign/Human03.w3m | PASS | PASS | 12513 | 18 | 340988 |
| 6 | Campaign/Human04.w3m | PASS | PASS | 16641 | 8 | 221222 |
| 7 | Campaign/Human05.w3m | PASS | PASS | 20769 | 23 | 595867 |
| 8 | Campaign/Human05Interlude.w3m | PASS | PASS | 9409 | 4 | 53202 |
| 9 | Campaign/Human06.w3m | PASS | PASS | 25921 | 17 | 325910 |
| 10 | Campaign/Human06Interlude.w3m | PASS | PASS | 4225 | 144 | 58273 |
| 11 | Campaign/Human07.w3m | PASS | PASS | 12513 | 2 | 222733 |
| 12 | Campaign/Human08.w3m | PASS | PASS | 16641 | 20 | 250398 |
| 13 | Campaign/Human09.w3m | PASS | PASS | 20769 | 13 | 317961 |
| 14 | Campaign/NightElf01.w3m | PASS | PASS | 12513 | 5 | 388043 |
| 15 | Campaign/NightElf02.w3m | PASS | PASS | 12513 | 13 | 402337 |
| 16 | Campaign/NightElf03.w3m | PASS | PASS | 12513 | 17 | 232055 |
| 17 | Campaign/NightElf04.w3m | PASS | PASS | 20769 | 50 | 514407 |
| 18 | Campaign/NightElf05.w3m | PASS | PASS | 25921 | 48 | 636922 |
| 19 | Campaign/NightElf06.w3m | PASS | PASS | 16641 | 49 | 247569 |
| 20 | Campaign/NightElf06Interlude.w3m | PASS | PASS | 9409 | 3 | 99760 |
| 21 | Campaign/NightElf07.w3m | PASS | PASS | 15617 | 73 | 432307 |
| 22 | Campaign/Orc01.w3m | PASS | PASS | 29025 | 40 | 305286 |
| 23 | Campaign/Orc02.w3m | PASS | PASS | 15617 | 13 | 348505 |
| 24 | Campaign/Orc02Interlude.w3m | PASS | PASS | 6305 | 14 | 53343 |
| 25 | Campaign/Orc03.w3m | PASS | PASS | 20769 | 5 | 246256 |
| 26 | Campaign/Orc04.w3m | PASS | PASS | 20769 | 37 | 241474 |
| 27 | Campaign/Orc04Interlude.w3m | PASS | PASS | 4225 | 9 | 77548 |
| 28 | Campaign/Orc05.w3m | PASS | PASS | 15617 | 31 | 496573 |
| 29 | Campaign/Orc06.w3m | PASS | PASS | 15617 | 33 | 262631 |
| 30 | Campaign/Orc07.w3m | PASS | PASS | 15617 | 13 | 361049 |
| 31 | Campaign/Orc08.w3m | PASS | PASS | 20769 | 31 | 280456 |
| 32 | Campaign/Prologue01.w3m | PASS | PASS | 12513 | 6 | 364898 |
| 33 | Campaign/Prologue02.w3m | PASS | PASS | 12513 | 3 | 451787 |
| 34 | Campaign/Undead01.w3m | PASS | PASS | 12513 | 10 | 505356 |
| 35 | Campaign/Undead02.w3m | PASS | PASS | 16641 | 18 | 394580 |
| 36 | Campaign/Undead02Interlude.w3m | PASS | PASS | 4225 | 4 | 64038 |
| 37 | Campaign/Undead03.w3m | PASS | PASS | 20769 | 15 | 308276 |
| 38 | Campaign/Undead04.w3m | PASS | PASS | 12513 | 18 | 281112 |
| 39 | Campaign/Undead05.w3m | PASS | PASS | 16641 | 18 | 303299 |
| 40 | Campaign/Undead05interlude.w3m | PASS | PASS | 6305 | 40 | 89296 |
| 41 | Campaign/Undead06.w3m | PASS | PASS | 16641 | 26 | 235374 |
| 42 | Campaign/Undead07.w3m | PASS | PASS | 18721 | 11 | 241053 |
| 43 | Campaign/Undead08.w3m | PASS | PASS | 16641 | 20 | 213964 |
| 44 | Campaign/WarCraftIIICredits.w3m | PASS | PASS | 4225 | 5 | 98992 |

## Biomes

Decoded: **111/111**; tilesets: **15**; missing thumbs: 0.

- Ashenvale: 8
- Barrens: 8
- Blight: 11
- Cityscape: 9
- Dalaran: 9
- Dungeon: 8
- Dungeon2: 8
- Felwood: 7
- LordaeronFall: 6
- LordaeronSummer: 6
- LordaeronWinter: 6
- Northrend: 7
- Nothing: 1
- Village: 8
- VillageFall: 9

## Box boundary

`analysis/agent-qa/final/focus-gates.json`: classification **harness_boundary**, selected 1 -> 0; retained as harness boundary, not product failure.

## Artifacts

- `analysis/agent-qa/r2/report.json`
- `analysis/agent-qa/r2/REPORT.md`
- `analysis/agent-qa/r2/maps/*.log` (44 one-shot map runs)
- `analysis/agent-qa/gallery/terrain/{manifest.jsonl,summary.json,*.ppm}`


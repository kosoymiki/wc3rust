# wc3-re scan report

- root: `analysis/war3_full3`
- maps: `analysis/war3_full3/Maps`
- elapsed: 183.06s

| subsystem | total | ok | failures | detail |
|---|---:|---:|---:|---|
| BLP decode | 3935 | 3935 | 0 | 2 encodings, 0 mips-zero, 98722207 pixels |
| MDX parse | 2048 | 2048 | 0 | 20502 chunks, 5127 geosets, 4209 materials |
| SLK tables | 33 | 33 | 0 | 6785 rows, 815 columns |
| TXT tables | 67 | 67 | 0 | 2516 sections, 19678 entries |
| FDF documents | 71 | 71 | 0 | 1956 nodes, 6514 props |
| JASS files | 3 | 3 | 0 | 641 functions, 791 natives |
| Map scripts | 44 | 44 | 0 | 44 main ok, 1311867 steps |
| AI scripts | 91 | 91 | 0 | 296 functions, 91 InitAI ok |
| Maps | 44 | 44 | 0 | 708 inner files, 4428 wts strings |
| Object database | 11 | 1665 | 0 | 117 metadata fields, 1182 icon refs |
| TriggerData | 676 | 676 | 0 | 25 events, 24 conditions, 326 actions, 301 calls, 676 strings |
| TerrainArt | 98 | 98 | 0 | 98 textures |
| Skins | 4 | 5 | 22 | 5 sections, 319 texture entries |
| Overall | | true | | elapsed 183.06s |

## Unimplemented native calls (map main)

- `triggerregisterplayerstateevent` × 138
- `unitsharevision` × 136
- `questcreateitem` × 129
- `setplayeronscorescreen` × 124
- `attachsoundtounit` × 112
- `setresourceamount` × 109
- `triggerregisterunitinrange` × 100
- `createmidisound` × 89
- `getfloatgamestate` × 89
- `getcameraboundmaxx` × 88
- `getcameraboundmaxy` × 88
- `getcameraboundminx` × 88
- `getcameraboundminy` × 88
- `isfogenabled` × 88
- `isfogmaskenabled` × 88
- `fogenable` × 79
- `fogmaskenable` × 75
- `setmapflag` × 73
- `setunitmovespeed` × 70
- `suspendheroxp` × 68
- `playthematicmusic` × 67
- `startcampaignai` × 66
- `unitadditemtoslotbyid` × 63
- `enableweathereffect` × 54
- `addindicator` × 49
- `setfloatgamestate` × 47
- `camerasetsmoothingfactor` × 43
- `clearmapmusic` × 43
- `suspendtimeofday` × 43
- `setdoodadanimationrect` × 42

## Archives

- `tft_files_Files100_War3x.mpq`: opened=true, listfile=5610, blocks=5634, error=None
- `tft_files_Files100_War3xlocal.mpq`: opened=true, listfile=1156, blocks=1167, error=None

# Sound path resolution

- source: `War3.mpq` (mpq)
- layers (priority order): ["War3.mpq"]
- definitions: 1410 (playable: 1359)
- referenced paths: 3013
- exact: 2883
- listfile-resolved: 6
- ambiguous: 0
- absent: 124
- resolved total: 2889/3013

## Resolved per layer

| layer | paths |
|---|---:|
| War3.mpq | 2889 |

## Absent per top-level directory

| dir | paths |
|---|---:|
| Abilities | 11 |
| Buildings | 5 |
| Units | 108 |

Absent paths are listed in `sound-paths-absent.tsv` with the layers that were searched. They are The Frozen Throne assets: mount `War3x.mpq` / `War3xLocal.mpq` (or a TFT extract) as higher-priority layers and they resolve without code changes.

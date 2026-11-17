# Sound path resolution

- source: `wc3-install` (layered)
- layers (priority order): ["War3xlocal", "War3x", "War3"]
- definitions: 2025 (playable: 1993)
- referenced paths: 4232
- exact: 3591
- listfile-resolved: 586
- ambiguous: 0
- absent: 55
- resolved total: 4177/4232

## Resolved per layer

| layer | paths |
|---|---:|
| War3 | 2992 |
| War3x | 168 |
| War3xlocal | 1017 |

## Absent per top-level directory

| dir | paths |
|---|---:|
| Abilities | 11 |
| Buildings | 2 |
| Sound | 1 |
| Units | 41 |

Absent paths are listed in `sound-paths-absent.tsv` with the layers that were searched. They are The Frozen Throne assets: mount `War3x.mpq` / `War3xLocal.mpq` (or a TFT extract) as higher-priority layers and they resolve without code changes.

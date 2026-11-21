# HeroFollends asset coverage — 100% target (2026-11-12)

Source: `War3.mpq` listfile (10,523 entries) + extracted `analysis/war3_full3` (10,523/10,523, 710,907,954 B, 0 failures).
"Done" means every item in a category is parsed/loaded/rendered or explicitly reported with a reason — never a sample.

## Models (2,048 MDX)

| Category | Count | Owner | Gate |
|---|---:|---|---|
| Doodads | 1,021 | world + QA gallery | loads, textures resolve, thumbnails, no missing/black/pink |
| Units | 411 | world + anim + QA | parse, Stand/Walk/Attack/Death, skin bounds, no corpse/effect geosets in Stand, facing |
| Abilities/effects | 230 | world + QA gallery | material layers/alpha, no black/white quads |
| Buildings | 131 | world + QA gallery | Stand visible (TownHall fallback), no helper planes, correct scale |
| Objects | 99 | world + QA gallery | parse/render |
| UI | 97 | UI framework + QA | frames/fonts/textures |
| Environment | 45 | world + QA gallery | parse/render |
| SharedModels | 14 | world + QA gallery | parse/render |

Systemic model fixes: GEOA single in-range key is authoritative; zero keys => 1.0; all-hidden sequence => model fallback 1.0; hide gutz/corpse/death textures in Stand/Walk/Attack; per-model yaw_offset (Archer PI); material layer stack; skin weights; mip/sRGB; no missing textures.

## Textures / icons / biomes

| Category | Count | Owner | Gate |
|---|---:|---|---|
| All BLP | 3,935 | assets + QA | header/decode/alpha/color, no missing |
| CommandButtons icons | 691 | HUD + QA | 64px source, crisp 64-72px render, alpha edges, no blur/pink |
| TerrainArt biome BLPs | 111 | world + assets + QA | correct CMYK/BGR/invert, all biomes in gallery, seamless terrain |
| UI/FDF textures | all referenced | UI framework/assets | resolves through war3skins |

## Data / gameplay

- RoC tables: 33 SLK / 6,785 rows (466 units, 399 abilities, 132 items, 128 destructables, 58 upgrades, 278 doodads).
- TFT tables: 57 SLK / 21,331 rows (812 units, 767 abilities, 273 items, 246 destructables, 89 upgrades, 469 doodads).
- Scripts: common.j 2,077 lines (TFT), Blizzard.j, 125 AI files; maps 44 RoC + TFT.
- Roster: data-driven definitions for all four races, real stats/costs/icons/model paths; sandbox all buildings/units selectable.

## Visual gates (QA)

- all models contact sheets by category; all 111 biome tiles; all 691 icons.
- no corpse/effect geoset visible in Stand; Archer faces velocity; no floating weapon.
- terrain seam/checkerboard score; no missing textures; no black/white/pink alpha.
- HUD: no system-bar overlap, 48dp+ targets, no button overlap/clipping, readable labels, high-contrast joystick.

# HUD asset pack — provenance and build

Delivered pack: `assets/hud/` + `assets/hud/manifest.json` (format 2). Access date **2026-11-18**. Every asset names its source archive path and the sha256 of the source bytes, plus the transform and the output sha256.

Labels: `extracted` = a direct crop/decode of the shipped art; `procedural reference v1` = a composite assembled from several shipped crops (corner/edge atlas, stone frame, arch stitch), never presented as hand-drawn final art.

## Build chain

1. `scripts/h2-cargo run -p wc3-game --example ui_assets --offline` decodes the shipped BLPs to RGBA PNGs in `analysis/design/src_rgba/` (and prints `UI_ASSET` / `UI_PIXEL` lines for the palette).
2. `python3 analysis/design/tools/build_assets.py` slices the eight-segment border atlases and writes the candidate 9-slices to `analysis/design/assets/` + `assets.json`.
3. `python3 analysis/design/tools/build_pack.py` copies the set into `assets/hud/<category>/`, trims and scales the resource icons, and writes `manifest.json` with provenance and output hashes.
4. `python3 analysis/design/tools/render_direction.py` composes the direction shots in `analysis/design/shots/` from the same files.

The pipeline/validator/atlas integration is line 555; this pack is the input. The validator must reject a pack when `left + right >= w`, `top + bottom >= h`, a corner is scaled, the centre carries border art, or a listed file is missing.

## Manifest check (this pack)

```
assets 21 ids 21 errors 0
MANIFEST_OK
```

Checked: unique ids, every file exists, every output sha256 matches the manifest, every 9-slice satisfies `l + r < w` and `t + b < h` (where insets are non-zero), every size is positive.

## Assets

| id | file | size | insets L/T/R/B | states | source (War3.mpq) | source sha256 (16) | output sha256 (16) | label |
|---|---|---|---|---|---|---|---|---|
| `panel.menu` | `panels/panel_9s.png` | 192×192 | 64/64/64/64 | normal | `UI/Widgets/EscMenu/Human/human-options-menu-border.blp` | `3380e7d8f4008212` | `3e19659282a21ff8` | extracted |
| `button.normal` | `buttons/button_normal_9s.png` | 96×96 | 32/32/32/32 | normal | `UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp` | `e3e3fc30fcc8badf` | `486914f206db9c94` | extracted |
| `button.hover` | `buttons/button_hover_9s.png` | 96×96 | 32/32/32/32 | hover | `UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp` | `e3e3fc30fcc8badf` | `09f7eae239ff9968` | extracted+derived |
| `button.pressed` | `buttons/button_pressed_9s.png` | 96×96 | 32/32/32/32 | pressed, selected | `UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder-Down.blp` | `69414402aac62b91` | `3e0fec2f3c95eb5e` | extracted |
| `button.disabled` | `buttons/button_disabled_9s.png` | 96×96 | 32/32/32/32 | disabled, locked | `UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder-Disabled.blp` | `e668f81eddb27319` | `14aed6aa22164644` | extracted+desaturated |
| `tab.normal` | `tabs/tab_9s.png` | 48×48 | 16/16/16/16 | normal | `UI/Widgets/Console/Human/human-console-button-up.blp` | `17cedeaa01165571` | `0d8801e5f9013ac8` | extracted |
| `tab.active` | `tabs/tab_active_9s.png` | 48×48 | 16/16/16/16 | selected, pressed | `UI/Widgets/Console/Human/human-console-button-down.blp` | `0281e50db3fce751` | `bf2f4998d3a87a13` | extracted |
| `tooltip.panel` | `tooltip/tooltip_9s.png` | 48×48 | 16/16/16/16 | normal | `UI/Widgets/ToolTips/Human/human-tooltip-border.blp` | `ed9b3e1a4b73eb10` | `b1db039b6e6f1c4f` | extracted |
| `command.normal` | `command/command_normal_9s.png` | 48×48 | 16/16/16/16 | normal | `UI/Widgets/Console/Human/human-console-button-up.blp` | `17cedeaa01165571` | `29367a63ff3064ce` | extracted |
| `command.pressed` | `command/command_pressed_9s.png` | 48×48 | 16/16/16/16 | pressed | `UI/Widgets/Console/Human/human-console-button-down.blp` | `0281e50db3fce751` | `7235d2c12121835a` | extracted |
| `command.disabled` | `command/command_disabled_9s.png` | 48×48 | 16/16/16/16 | disabled, locked | `UI/Widgets/Console/Human/human-console-button-up.blp` | `17cedeaa01165571` | `035c2b520eb675c8` | extracted+desaturated |
| `minimap.frame` | `minimap/minimap_frame_9s.png` | 96×96 | 32/32/32/32 | normal | `UI/Console/Human/HumanUITile02.blp` | `9be09472ff218316` | `09589fb7c9041b7f` | procedural reference v1 |
| `minimap.arch` | `minimap/minimap_arch.png` | 232×312 | 0/0/0/0 | normal | `UI/Console/Human/HumanUITile01.blp` | `fd62c4880fc6f15f` | `ea2776803094580a` | procedural reference v1 |
| `portrait.frame` | `portraits/portrait_frame.png` | 300×284 | 0/0/0/0 | normal | `UI/Console/Human/HumanUITile01.blp` | `fd62c4880fc6f15f` | `964644a81c15e5ea` | extracted |
| `resource.gold` | `resources/resource_gold_96.png` | 96×96 | - | normal | `UI/Feedback/Resources/ResourceGold.blp` | `a8e0f12d6a51f2be` | `a1e551ad6676d13c` | extracted |
| `resource.lumber` | `resources/resource_lumber_96.png` | 96×96 | - | normal | `UI/Feedback/Resources/ResourceLumber.blp` | `ffa8529266dd9906` | `1e45c27bad677c08` | extracted |
| `resource.supply` | `resources/resource_supply_96.png` | 96×96 | - | normal | `UI/Feedback/Resources/ResourceSupply.blp` | `fff25c2e635f52fa` | `decd9fdffcc34f9e` | extracted |
| `resource.upkeep` | `resources/resource_upkeep_96.png` | 96×96 | - | normal | `UI/Feedback/Resources/ResourceUpkeep.blp` | `f02615e2387b8d1c` | `ac1d33311829d5e7` | extracted |
| `resource.mana` | `resources/resource_mana_96.png` | 96×96 | - | normal | `UI/Feedback/Resources/ResourceManaStone.blp` | `45839292f27aab2c` | `2075e74c6e349274` | extracted |
| `infocard.gold` | `resources/infocard_gold_96.png` | 96×96 | - | normal | `UI/Widgets/Console/Human/infocard-gold.blp` | `7670218087827781` | `10b5f4f2c3508e8b` | extracted |
| `infocard.supply` | `resources/infocard_supply_96.png` | 96×96 | - | normal | `UI/Widgets/Console/Human/infocard-supply.blp` | `fe75fce0a286deb7` | `d51d9adeddfaff52` | extracted |

## Nine-slice checks (performed on this pack)

| id | l+r < w | t+b < h | corners opaque | centre clean | min render ≥ 48 dp |
|---|---|---|---|---|---|
| `panel.menu` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `button.normal` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `button.hover` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `button.pressed` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `button.disabled` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `tab.normal` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `tab.active` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `tooltip.panel` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `command.normal` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `command.pressed` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `command.disabled` | True | True | yes (baked fill) | flat/stretch (no border art) | yes |
| `minimap.frame` | True | True | yes (baked fill) | transparent centre (overlay on the map) | yes |

## Resource icons

| id | file | source | source sha256 (16) | transform |
|---|---|---|---|---|
| `resource.gold` | `resources/resource_gold_96.png` | `UI/Feedback/Resources/ResourceGold.blp` | `a8e0f12d6a51f2be` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `resource.lumber` | `resources/resource_lumber_96.png` | `UI/Feedback/Resources/ResourceLumber.blp` | `ffa8529266dd9906` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `resource.supply` | `resources/resource_supply_96.png` | `UI/Feedback/Resources/ResourceSupply.blp` | `fff25c2e635f52fa` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `resource.upkeep` | `resources/resource_upkeep_96.png` | `UI/Feedback/Resources/ResourceUpkeep.blp` | `f02615e2387b8d1c` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `resource.mana` | `resources/resource_mana_96.png` | `UI/Feedback/Resources/ResourceManaStone.blp` | `45839292f27aab2c` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `infocard.gold` | `resources/infocard_gold_96.png` | `UI/Widgets/Console/Human/infocard-gold.blp` | `7670218087827781` | alpha bbox trim, square pad, LANCZOS to 96x96 |
| `infocard.supply` | `resources/infocard_supply_96.png` | `UI/Widgets/Console/Human/infocard-supply.blp` | `fe75fce0a286deb7` | alpha bbox trim, square pad, LANCZOS to 96x96 |

The 32 × 32 resource BLPs are the retail icon atlas: gold coin, lumber, food/supply, upkeep, mana stone; the 64 × 64 infocard icons are the unit-card cost icons. Icons are alpha-trimmed, square-padded and LANCZOS-scaled to 96 × 96 for a 24 dp render at density 3.

## Fonts

| id | file | source | sha256 | coverage |
|---|---|---|---|---|
| `nimrod` | `Fonts/NIM_____.ttf` | `local:War3.mpq` | `8301b818d2999bb7c71a5d7a61352690ee247e9a47c77d16bc8ba5e0cbda9521` | latin, cyrillic, digits, punctuation |

Nimrod is not bundled: the app reads `Fonts/NIM_____.ttf` from the operator's `War3.mpq`. The pack records the hash so a swapped archive is detectable. The six type roles and the px@3× scale are in `HUD-DIRECTION.md` §4.

## Notes on the two procedural composites

- `minimap.frame`: a rectangular stone frame assembled from the `HumanUITile02` beam/column crops and a 2 px gold inner line. The retail minimap is an arch; the square frame is the mobile adaptation for a 112 × 112 map. Labelled `procedural reference v1`.
- `minimap.arch`: the retail arch itself, stitched from `HumanUITile01` (x420..512) and `HumanUITile02` (x0..140) along the seam; this one is a direct stitch of shipped pixels, labelled `procedural reference v1` because it is a composite.
- `panel.menu`: extracted segments + a flat `#00091A` fill; the fill is a sampled colour, not new art. The frame is a direct 8-segment atlas.

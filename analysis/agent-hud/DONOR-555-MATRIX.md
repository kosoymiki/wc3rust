# Donor matrix — HUD pack v2 (`hud-pack-v2`, ledger 555)

## Source

| item | value |
|---|---|
| zip | `/data/data/com.termux/files/usr/tmp/hud-pack-v2.zip` |
| zip sha256 | `b50039ea4f43eed4e37993f3f57d5e29ca61d9b5b4d144d4a869aa8c5a68c010` |
| extracted tree | `/data/data/com.termux/files/usr/tmp/hudpack-v2/` |
| files / bytes | 98 / 120416 |
| manifest sha256 | `5035db43b4fafb1b041b702d313ab7deab212d883e7ba7ac7d8f6c3eb01ae452` |
| atlas index sha256 | `76115a8e1a2fd0cff8b3115eaf2ef021278f3b74315ae329f88b75a661bbd29b` |
| assets | 91 (`hud.*` 16, `widgets.*` 13, `frames.*` 4, `icons.*` 51, `menus.*` 7) |

Read byte-level: every SVG, both JSON files and both docs. No donor code or SVG was executed as code; the rasterizer runs `rsvg-convert` on the source files in the temp tree only.

## Verdict summary

| donor artifact | verdict | adopted into | evidence |
|---|---|---|---|
| `manifest.json` schema (`id/path/width/height/format/states/nine_slice`) | **ACCEPTED / ADAPTED** | `crates/wc3-game/src/hudskin/manifest.rs` parser aliases; canonical keys emitted as `id/file/w/h/format/states/nine_slice` | 91/91 assets parse; `validate` 0 errors |
| `atlases/ui.atlas.json` (`packing`, `padding`, `gutter`, index by id/path/w/h) | **ACCEPTED / ADAPTED** | `AtlasDecl::padding` + new `AtlasDecl::gutter`; `pack_build` shelf pack writes both | donor `padding=4`, `gutter=2`; our pack writes `padding` and `gutter` fields |
| `docs/INTEGRATION.md` contract (deterministic raster at density, alpha/sRGB, nine-slice panels only, state by semantics, one geometry for draw+hit) | **ACCEPTED** | `pack_build` deterministic SVG stage; `HudDebug` collects draw/hit from one `MobileHudLayout`; report format | contract text mapped to code paths |
| `docs/VISUAL_GUIDELINES.md` (soot/ink, warm stone, oak, bronze, muted gold; gold = focus only) | **ACCEPTED as reference** | design tokens already in `analysis/design/HUD-DIRECTION.md`; no art copied | palette/token mapping by name |
| `README.md`, `ASSET_LICENSES.md` (CC0 metadata) | **ACCEPTED as provenance** | provenance recorded (sheet/zip sha256); license terms ignored per promth2 | no file copied |
| 91 SVGs (`assets/hud`, `assets/menus`, `assets/icons`, `assets/frames`, `assets/widgets`) | **REJECTED as art** | nothing copied into `assets/hud/**` | flat generator evidence below |
| `previews/panels_contact_sheet.svg` | **REJECTED** | diagnostic only | shows one flat rounded plate with different labels |

## Why the art is rejected (not a taste claim)

- 37/40 panel/menu/frame/widget SVGs use the same `<rect rx=...>` silhouette; 40/40 start with the identical `metal/gold/wood` gradient defs. The 16 `hud.*` panels differ only in label/child rects.
- 51/51 `icons.*` SVGs are a `<circle>` ring plus one primitive polygon/path (shield triangle, diamond, three-bar “attack”). No authored texture, highlights or material separation.
- Side-by-side with the pipeline art that is already in the repo (`assets/hud/panels/panel_9s.png`, `analysis/hudskin/final/wc3rust-hud-final/console/*`), the donor plates read as a UI wireframe, not shipping art. The operator’s 2026-11-18 orc sheet is the opposite case: authored stone/gold/moss/foliage, accepted through the segmentation pipeline (`analysis/agent-hud/ORC-SHEET-555.md`).

## Canonical manifest v2 (our format + donor fields)

```json
{
  "format_version": 2,
  "theme": "classic_fantasy | orc",
  "name": "optional donor metadata",
  "version": "optional donor metadata",
  "style": "optional donor metadata",
  "license": "recorded provenance only",
  "notes": "optional",
  "density": 3.0,
  "reference": {
    "landscape_px": [
      2414,
      1080
    ],
    "landscape_dp": [
      805,
      360
    ],
    "portrait_px": [
      1080,
      2414
    ],
    "portrait_dp": [
      360,
      805
    ]
  },
  "safe_area_dp": {
    "landscape": [
      70,
      40,
      30,
      32
    ],
    "portrait": [
      8,
      8,
      32,
      21
    ]
  },
  "fonts": {
    "nimrod": {
      "file": "Fonts/NIM_____.ttf",
      "bundled": false,
      "sha256": "8301...",
      "roles": [
        "body"
      ],
      "glyphs": [
        "latin",
        "cyrillic",
        "digits"
      ]
    }
  },
  "assets": [
    {
      "id": "panel.command_grid",
      "purpose": "...",
      "file": "panel.command_grid.png",
      "format": "png",
      "w": 268,
      "h": 310,
      "alpha": true,
      "color_space": "sRGB",
      "scale": "3x",
      "kind": "frame",
      "states": [
        "normal"
      ],
      "nine_slice": {
        "left": 50,
        "top": 50,
        "right": 50,
        "bottom": 48,
        "center": "stretch"
      },
      "three_slice": null,
      "fixed": false,
      "atlas": "ui",
      "atlas_rect": [
        2,
        2,
        268,
        310
      ],
      "mips": false,
      "critical": true,
      "sha256": "...",
      "provenance": {
        "sheet": "...",
        "sheet_sha256": "...",
        "crop": [
          1184,
          383,
          338,
          235
        ],
        "transform": "..."
      }
    }
  ],
  "atlases": {
    "ui": {
      "file": "atlas/ui_atlas.png",
      "w": 1024,
      "h": 1024,
      "padding": 2,
      "gutter": 2,
      "mips": false,
      "sha256": "...",
      "entries": [
        {
          "id": "...",
          "x": 2,
          "y": 2,
          "w": 268,
          "h": 310
        }
      ]
    }
  },
  "nine_slice": {
    "bleed_px": 1,
    "padding_px": 2,
    "min_center_px": 1
  },
  "states": {
    "normal": {
      "brightness": 1.0
    },
    "hover": {
      "brightness": 1.18
    },
    "disabled": {
      "brightness": 0.7,
      "saturation": 0.6
    }
  }
}
```

Accepted donor aliases: `path` → `file`, `width`/`height` → `w`/`h`, `format` (source `svg` or `png`), `gutter` (inter-entry gap; `padding` is the page border), `name/version/style/license/notes` metadata. A manifest without `format_version` but with the v2 asset shape is parsed as v2 and reported as `WARN format-version-missing`; a declared unsupported version is a hard `HUD_PACK manifest_version=...` error.

## File-to-file matrix (91 assets)

| id | donor path | w×h | nine_slice | verdict | reason | sha256 (donor) |
|---|---|---|---|---|---|---|
| `hud.topbar` | `assets/hud/topbar.svg` | 960×94 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `78235ab040de8641` |
| `hud.selection_single` | `assets/hud/selection_single.svg` | 560×218 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `3ad6671e1b5957f8` |
| `hud.selection_multi` | `assets/hud/selection_multi.svg` | 560×218 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `3ad6671e1b5957f8` |
| `hud.selection_building` | `assets/hud/selection_building.svg` | 560×218 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `3ad6671e1b5957f8` |
| `hud.command_card` | `assets/hud/command_card.svg` | 420×350 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `8af420d0c4d47f47` |
| `hud.minimap_frame` | `assets/hud/minimap_frame.svg` | 270×270 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `824b05a23924533a` |
| `hud.inventory_strip` | `assets/hud/inventory_strip.svg` | 410×86 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `bb8df2ee9b212cd6` |
| `hud.hero_stats` | `assets/hud/hero_stats.svg` | 300×130 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `e41c9c716134f69e` |
| `hud.unit_stats` | `assets/hud/unit_stats.svg` | 300×112 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `9b12404c684222e5` |
| `hud.production_queue` | `assets/hud/production_queue.svg` | 340×92 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `e9d91de715a59407` |
| `hud.tooltip` | `assets/hud/tooltip.svg` | 330×128 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `817f148be83dc2c9` |
| `hud.toast` | `assets/hud/toast.svg` | 420×70 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `ab16f90c69fb2b18` |
| `hud.status_strip` | `assets/hud/status_strip.svg` | 420×46 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `c47d67ce4d4cf396` |
| `hud.pause_overlay` | `assets/hud/pause_overlay.svg` | 700×480 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `cb7ec997f09370d4` |
| `hud.modal` | `assets/hud/modal.svg` | 620×420 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `4a26c226cf65dcfe` |
| `hud.loading_panel` | `assets/hud/loading_panel.svg` | 700×180 | L18 T18 R18 B18 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `93e3b031058ca550` |
| `widgets.button_normal` | `assets/widgets/button_normal.svg` | 200×54 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `3a9e177695d105e6` |
| `widgets.button_hover` | `assets/widgets/button_hover.svg` | 200×54 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `1fefa58b02b83cf0` |
| `widgets.button_pressed` | `assets/widgets/button_pressed.svg` | 200×54 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `afb0b01e81eee4a3` |
| `widgets.button_selected` | `assets/widgets/button_selected.svg` | 200×54 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `e609ac4d00e35a7c` |
| `widgets.button_disabled` | `assets/widgets/button_disabled.svg` | 200×54 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `7a17703a7ac053c7` |
| `widgets.slot_empty` | `assets/widgets/slot_empty.svg` | 64×64 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `56100b626f2cad64` |
| `widgets.slot_hover` | `assets/widgets/slot_hover.svg` | 64×64 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `975ed7815d63b0e7` |
| `widgets.slot_selected` | `assets/widgets/slot_selected.svg` | 64×64 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `7fbf0097f460da5f` |
| `widgets.slot_locked` | `assets/widgets/slot_locked.svg` | 64×64 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `de8a41ea785719d1` |
| `widgets.slot_cooldown` | `assets/widgets/slot_cooldown.svg` | 64×64 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `5501d017ab7f3f8e` |
| `frames.portrait_hero` | `assets/frames/portrait_hero.svg` | 160×160 | null | **REJECT (art)** | same plate generator; portrait frame is a flat rounded rect | `04151590456e0802` |
| `frames.portrait_unit` | `assets/frames/portrait_unit.svg` | 128×128 | null | **REJECT (art)** | same plate generator; portrait frame is a flat rounded rect | `8286203222223c49` |
| `frames.portrait_building` | `assets/frames/portrait_building.svg` | 160×128 | null | **REJECT (art)** | same plate generator; portrait frame is a flat rounded rect | `198743c01a452c23` |
| `frames.portrait_small` | `assets/frames/portrait_small.svg` | 64×64 | null | **REJECT (art)** | same plate generator; portrait frame is a flat rounded rect | `bcfc2585c87f2c6b` |
| `icons.gold` | `assets/icons/gold.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `f0528872648fb4cf` |
| `icons.wood` | `assets/icons/wood.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `9af71a165adfce5b` |
| `icons.food` | `assets/icons/food.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `c14b7686b8d01fec` |
| `icons.attack` | `assets/icons/attack.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `e16d5acf0107cd43` |
| `icons.move` | `assets/icons/move.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `557893645d48069f` |
| `icons.build` | `assets/icons/build.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `5846b8cb6f4d0932` |
| `icons.repair` | `assets/icons/repair.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `e02a5188806b5842` |
| `icons.cancel` | `assets/icons/cancel.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `0747ba385f48bc9b` |
| `icons.hero` | `assets/icons/hero.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `721e8f67351b5a92` |
| `icons.armor` | `assets/icons/armor.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `f9fb8b928eb40a9a` |
| `icons.health` | `assets/icons/health.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `ac517ce5fb2f8497` |
| `icons.mana` | `assets/icons/mana.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `31084d07ef5b7fa3` |
| `icons.inventory` | `assets/icons/inventory.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `25bc31fef48b3c45` |
| `icons.settings` | `assets/icons/settings.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `12ed4b1dead246e7` |
| `icons.back` | `assets/icons/back.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `6134a4f421359a87` |
| `icons.pause` | `assets/icons/pause.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `4653e42d345984ce` |
| `icons.sound` | `assets/icons/sound.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `2346fc22318517ca` |
| `icons.close` | `assets/icons/close.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `a07ffb8933f13a3b` |
| `icons.research` | `assets/icons/research.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `9f1c441abd53fb28` |
| `icons.train` | `assets/icons/train.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `f16ebb485e8d0f30` |
| `icons.upgrade` | `assets/icons/upgrade.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `2865b83584662ce6` |
| `icons.rally` | `assets/icons/rally.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `e936947227a8c48b` |
| `icons.patrol` | `assets/icons/patrol.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `2d97ee99d4ebc073` |
| `icons.stop` | `assets/icons/stop.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `994047e80cb81140` |
| `icons.heal` | `assets/icons/heal.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `0a2f74d2935983f6` |
| `icons.fire` | `assets/icons/fire.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `a1668b8c1c000168` |
| `icons.frost` | `assets/icons/frost.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `f6b91a68aaccea79` |
| `icons.lightning` | `assets/icons/lightning.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `117142a7987866ab` |
| `icons.summon` | `assets/icons/summon.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `50d0df0cc7371aa5` |
| `icons.stealth` | `assets/icons/stealth.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `14739a113356b735` |
| `icons.teleport` | `assets/icons/teleport.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `4bcebc0c0d5f4418` |
| `icons.potion_red` | `assets/icons/potion_red.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `9ef0fcd461ed3462` |
| `icons.potion_blue` | `assets/icons/potion_blue.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `acb9b0cc10d97233` |
| `icons.scroll` | `assets/icons/scroll.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `d55b04508654576e` |
| `icons.boots` | `assets/icons/boots.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `1a32d24ecec0da6c` |
| `icons.ring` | `assets/icons/ring.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `3d02a98fa5e6b5b3` |
| `icons.orb` | `assets/icons/orb.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `5cc53d12bb5e0461` |
| `icons.shield` | `assets/icons/shield.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `d1d2f69c06b12ce3` |
| `icons.sword` | `assets/icons/sword.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `68d1ebbc6141cd5f` |
| `icons.bow` | `assets/icons/bow.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `ba32dda0b9b291a0` |
| `icons.axe` | `assets/icons/axe.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `b4f8e4c0b66ca6bb` |
| `icons.staff` | `assets/icons/staff.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `37ca3083bf1139e9` |
| `icons.tower` | `assets/icons/tower.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `3471fd3cf2ebc7cc` |
| `icons.castle` | `assets/icons/castle.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `9609332b061e0c33` |
| `icons.worker` | `assets/icons/worker.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `a431a5dfe0190358` |
| `icons.infantry` | `assets/icons/infantry.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `659c86a279dbfaa0` |
| `icons.cavalry` | `assets/icons/cavalry.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `d5726ff78dd311ea` |
| `icons.mage` | `assets/icons/mage.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `ab98990272b9bf64` |
| `icons.demon` | `assets/icons/demon.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `5918b41efe71e49f` |
| `icons.undead` | `assets/icons/undead.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `3c16a7744e88889e` |
| `icons.nature` | `assets/icons/nature.svg` | 64×64 | null | **REJECT (art)** | circle ring + one geometric primitive; no authored material | `beafe42961864976` |
| `menus.main_menu` | `assets/menus/main_menu.svg` | 900×620 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `81bc58cb114e54ac` |
| `menus.submenu` | `assets/menus/submenu.svg` | 720×520 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `1f817cb39db39a6d` |
| `menus.settings` | `assets/menus/settings.svg` | 760×600 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `bc5524776276e42f` |
| `menus.map_select` | `assets/menus/map_select.svg` | 900×620 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `81bc58cb114e54ac` |
| `menus.save_load` | `assets/menus/save_load.svg` | 760×540 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `706bc525772fbf47` |
| `menus.confirmation` | `assets/menus/confirmation.svg` | 560×300 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `1e9a6fea48fcb2a0` |
| `menus.campaign_select` | `assets/menus/campaign_select.svg` | 900×620 | L24 T24 R24 B24 | **REJECT (art)** | flat rounded-rect plate; same silhouette as every other panel | `81bc58cb114e54ac` |
| `widgets.tab_normal` | `assets/widgets/tab_normal.svg` | 180×48 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `854dd91c07052290` |
| `widgets.tab_active` | `assets/widgets/tab_active.svg` | 180×48 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `3e61dc85d0fde67a` |
| `widgets.tab_disabled` | `assets/widgets/tab_disabled.svg` | 180×48 | null | **REJECT (art)** | same plate generator; flat bevel, no material separation | `febdd509560fbeae` |

Every listed SVG exists and hashes as shown; the verdict is about art quality, not a missing file. No SVG from the donor tree is copied into the repo or the pack.

## Rasterization determinism (pipeline capability, not donor art acceptance)

- Tool: `rsvg-convert` 2.63.4 (`/data/data/com.termux/files/usr/bin/rsvg-convert`), invoked by `wc3-hud-pack rasterize`.
- Single file: `assets/hud/topbar.svg` at 960×94, rendered twice: `sha256=13107ff5a96aba58ac3ada061612627a4b12251f380f35bb5541fb7f32d6fa53`, second identical (`deterministic=true`).
- Whole donor pack: 91 SVGs rendered twice at declared width/height; `HUD_RASTER_SUMMARY assets=91 deterministic=91 mismatched=0`; full log `analysis/agent-hud/DONOR-555-RASTER.txt`.
- The rasterized donor was then packed by the same v2 builder: `assets=91 atlases=1`, then `validate`: `assets=91 atlases=1 atlas_assets=91 errors=0 warnings=0`, 1 texture. This proves the 91-asset capacity and the atlas path; the donor art is still not adopted.

## 9-slice validator on the donor manifest

- Valid pass: `HUD_PACK_VALIDATE format=2 ... assets=91 atlases=0 atlas_assets=0 fonts=0 states=0 sliced=23 standalone=91 errors=0 warnings=1`; warning is the missing declared format version. 23 sliced rows all satisfy `l+r<w` and `t+b<h`. Log: `analysis/agent-hud/DONOR-555-VALIDATE.txt`.
- Negative case: `hud.topbar` insets changed to `600/18/600/18` on 960×94 → `ERROR element=hud.topbar rule=insets-cross-centre detail=insets [600,18,600,18] leave no centre cell on 960x94`, exit 1. Log: `analysis/agent-hud/DONOR-555-VALIDATE-BAD.txt`.


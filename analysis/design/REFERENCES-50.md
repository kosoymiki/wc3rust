# 50 GitHub references — WC3 mobile HUD direction (ledger 544)

Access date: **2026-11-18**. GitHub only. Each entry is a real file at a real commit; the sha256 is of the bytes downloaded from `raw.githubusercontent.com/<repo>/<commit>/<path>`. No donor code was executed.

The full machine-readable provenance (repo, path, full commit, sha256, bytes) is in `PROVENANCE.json` next to this file. Numbers are quoted from the files, not inferred.

## How to read the table

* **Pattern** — what the file proves for this HUD.
* **Taken** — the exact numbers/shape used in `HUD-DIRECTION.md` and the asset build. WC3 FDF numbers are in screen units where `0.6 = 1080p screen height` (so `0.176 = 317 px`, `0.032 = 57.6 px`).

| # | Repo / file | Pattern |
|---|---|---|
| 01 | `Frotty/wurst-table-layout` — `wurst/TableLayout.wurst` | spacing ladder + default content padding for WC3 frame tables |
| 02 | `Frotty/wurst-table-layout` — `wurst/components/TableUiPanels.wurst` | panel/card tiers, border inset, divider thickness, section header |
| 03 | `Frotty/wurst-table-layout` — `wurst/components/TableUiButtons.wurst` | text/icon/checkbox button primitives and focus release |
| 04 | `Frotty/wurst-table-layout` — `wurst/components/TableUiSafeArea.wurst` | measured WC3 melee safe area |
| 05 | `Frotty/wurst-table-layout` — `wurst/components/TableUiCards.wurst` | unit/info card composition (icon card, stat card) |
| 06 | `Frotty/wurst-table-layout` — `wurst/components/TableUiDialog.wurst` | confirm dialog geometry and button row |
| 07 | `Frotty/wurst-table-layout` — `imports/TableLayout.fdf` | FDF backdrops, tooltip frame, text area metrics |
| 08 | `cljnilsson/wc3-wurst-UI-system` — `wurst/Core/Ui.wurst` | command-card grid sorting and widths |
| 09 | `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/SmashConsoleUI.fdf` | retail console composition in FDF units |
| 10 | `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/UnitPortrait.fdf` | retail portrait frame and HP/mana text placement |
| 11 | `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/ToolTip.fdf` | retail tooltip 9-slice and resource icon row |
| 12 | `Retera/WarsmashModEngine` — `core/src/com/etheller/warsmash/parsers/fdf/frames/BackdropFrame.java` | the retail 8-segment border atlas decode |
| 13 | `nightknighto/Warcraft-3-Reforged-UI-Designer` — `src/ts/ClassesAndFunctions/Tooltips.ts` | WC3 tooltip z-order and max width |
| 14 | `TheLucifurry/warcraft-3-ui` — `src/components/base/Button.vue` | WC3-styled web button sizes and states |
| 15 | `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/NineSlice.lua` | the 3x3 nine-slice runtime (layout pieces, offsets, atlas regions) |
| 16 | `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/Mainline/NineSliceLayouts.lua` | named nine-slice layouts with per-piece atlas offsets |
| 17 | `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/Backdrop.lua` | backdrop presets: tile size, edge size, content insets |
| 18 | `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_ActionBar/Shared/ActionButton.lua` | action-button overlays and icon insets |
| 19 | `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_ActionBar/Mainline/ActionButtonTemplate.xml` | button, popup and icon-frame dimensions |
| 20 | `shagu/pfUI` — `modules/minimap.lua` | addon minimap size and button cluster |
| 21 | `ElvUI-WotLK/ElvUI` — `ElvUI/Core/Install.lua` | action-bar and minimap defaults |
| 22 | `ElvUI-WotLK/ElvUI` — `ElvUI/Modules/ActionBars/ActionBars.lua` | bar spacing/scale derivation |
| 23 | `OpenRA/OpenRA` — `mods/cnc/chrome/ingame.yaml` | RTS ingame HUD geometry (minimap, top strip, buttons) |
| 24 | `OpenRA/OpenRA` — `mods/common/chrome/ingame.yaml` | shared ingame containers and tooltip container |
| 25 | `0ad/0ad` — `binaries/data/mods/public/gui/session/session.xml` | bottom command deck composition |
| 26 | `0ad/0ad` — `binaries/data/mods/public/gui/common/tooltips.js` | tooltip/type roles in an RTS |
| 27 | `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/window.lua` | RTS window default size and 9-slice draw |
| 28 | `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/button.lua` | RTS button default box |
| 29 | `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/tabbar.lua` | tab row item box |
| 30 | `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/panel.lua` | plain panel default box |
| 31 | `warzone2100/warzone2100` — `src/hci.cpp` | RTS ingame option button / reticule dimensions |
| 32 | `Wargus/Wargus` — `scripts/human/ui_pandora.lua` | WC2-style RTS sidebar and selection grid |
| 33 | `Jerga99/hvo-mobile-rts` — `Assets/hvo/Prefabs/UI/ActionButton.prefab` | mobile RTS action button prefab |
| 34 | `Jerga99/hvo-mobile-rts` — `Assets/hvo/Images/Tiny Swords/UI/Banners/Carved_9Slides.png.meta` | fantasy 9-slice source sheet dimensions |
| 35 | `Anuken/Mindustry` — `core/src/mindustry/ui/Styles.java` | four-state button styles |
| 36 | `libgdx/libgdx` — `gdx/src/com/badlogic/gdx/graphics/g2d/NinePatch.java` | 9-patch split contract |
| 37 | `raysan5/raylib` — `examples/textures/textures_npatch_drawing.c` | nine-patch insets at three densities |
| 38 | `pixijs/pixijs` — `src/scene/sprite-nine-slice/NineSliceSprite.ts` | default slice widths and geometry |
| 39 | `phaserjs/phaser` — `src/gameobjects/nineslice/NineSlice.js` | default slice widths and 3-slice mode |
| 40 | `callum-gander/nine-slice-frame` — `packages/react/src/NineSliceFrame.tsx` | CSS border-image nine-slice wrapper |
| 41 | `Lommix/bevy_nine_slice_ui` — `src/lib.rs` | GPU nine-slice UI material |
| 42 | `vleue/bevy_ninepatch` — `src/ninepatch.rs` | ECS nine-patch component |
| 43 | `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/button.tsx` | WC3-styled button 9-slice and metrics |
| 44 | `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/card.tsx` | WC3-styled panel 9-slice and metrics |
| 45 | `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/tabs.tsx` | responsive tab row metrics |
| 46 | `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/tooltip.tsx` | fantasy tooltip box |
| 47 | `threejsassets/game-hud` — `dist/skins/fantasy.css` | fantasy stone/wood panel recipe with explicit values |
| 48 | `2lab-ai/game-ui` — `src/skin.rs` | measured FFT fantasy skin (colours, units, 1u ink line) |
| 49 | `Videogamers0/MGUI` — `MGUI.Core/UI/MGToolTip.cs` | tooltip border and padding contract |
| 50 | `jsfehler/phaser-ui-tools` — `src/containers/frame.js` | modal frame layering and padding layout |

## Entries

### 01 · `Frotty/wurst-table-layout` — `wurst/TableLayout.wurst`

commit `b723cb3beed5` · sha256 `d95d4f8182b78daef544d03e3669ecb21495c29ffa236f1cd02cb696b591949d` · 44571 B

**Pattern:** spacing ladder + default content padding for WC3 frame tables

**Taken:** SPACE_2XS 0.002, SPACE_XS 0.004, SPACE_S 0.008, SPACE_M 0.014, SPACE_L 0.022, SPACE_XL 0.034 screen units; DEFAULT_PADDING 0.0175. WC3 frame units: 0.6 = screen height, so 0.004 = 7.2 px at 1080p.

### 02 · `Frotty/wurst-table-layout` — `wurst/components/TableUiPanels.wurst`

commit `b723cb3beed5` · sha256 `2086ea4a2c186720d3f6b1139f845cd086154b4a2b9a3f9da2cf41994331e6e5` · 6057 B

**Pattern:** panel/card tiers, border inset, divider thickness, section header

**Taken:** PANEL_BORDER_INSET 0.014 (25.2 px @1080p = 8.4 dp), SEPARATOR_THICKNESS 0.0015 (2.7 px), section header 0.022 (39.6 px = 13.2 dp); rule: panel > card > container, never nest backdrops deeper than one level.

### 03 · `Frotty/wurst-table-layout` — `wurst/components/TableUiButtons.wurst`

commit `b723cb3beed5` · sha256 `4e2cfe7d1a0ca188c80725cbe3e2b1f663b29cc0fa2c1d1c398b43ef9d51d2bc` · 11122 B

**Pattern:** text/icon/checkbox button primitives and focus release

**Taken:** icon button default 0.025 x 0.025 (45 x 45 px @1080p = 15 dp); checkbox clamped to MIN_ICON_SIZE; native templates PickIconButton, QuestCheckBox2.

### 04 · `Frotty/wurst-table-layout` — `wurst/components/TableUiSafeArea.wurst`

commit `b723cb3beed5` · sha256 `cd33bc9bfd5b2a213996d3a14c9d6eaab4818cb2d4ac0338a1f3e487e62b800c` · 7531 B

**Pattern:** measured WC3 melee safe area

**Taken:** VISUAL_SAFE_AREA_MIN (0.0, 0.17), MAX (0.78, 0.56); strict SAFE_AREA_MIN (0.05, 0.17); debug border 0.002. The bottom 17% of the 4:3 box is console-occupied; the top-right 0.78/0.56 clears the resource bar and day/night clock.

### 05 · `Frotty/wurst-table-layout` — `wurst/components/TableUiCards.wurst`

commit `b723cb3beed5` · sha256 `c3b64ce664686a7cd1b11e7b049848e3cd4df9607a39a3d601ee6e709df848c4` · 2756 B

**Pattern:** unit/info card composition (icon card, stat card)

**Taken:** card height 0.05 and icon-card 0.046; inner inset 0.006; icon side = height - 2*inset = 0.034; title row 0.014, subtitle 0.012, value 0.022 (screen units).

### 06 · `Frotty/wurst-table-layout` — `wurst/components/TableUiDialog.wurst`

commit `b723cb3beed5` · sha256 `abda8807daea5dbcd1c090ce6b1d16487ed93d68e3abab4e94cabc158dd1dae3` · 13240 B

**Pattern:** confirm dialog geometry and button row

**Taken:** default dialog 0.24 x 0.13; accept/cancel 0.07 x 0.024; title row 0.018; message row 0.040; example custom dialog 0.30 x 0.20.

### 07 · `Frotty/wurst-table-layout` — `imports/TableLayout.fdf`

commit `b723cb3beed5` · sha256 `b4b08e8356a5d45a6801243e868b3066cedf1414e150ef73a7531af78d3f9952` · 10903 B

**Pattern:** FDF backdrops, tooltip frame, text area metrics

**Taken:** BackdropCornerSize 0.0125 with 0.005 insets; tooltip corner 0.008 / insets 0.0022; text area inset 0.008, line height 0.012, font 0.012, shadow offset 0.001; BoxedText 0.22 x 0.12.

### 08 · `cljnilsson/wc3-wurst-UI-system` — `wurst/Core/Ui.wurst`

commit `adf9146c40b3` · sha256 `c7ec46dffeed08a0be046c2a8ec50d1489a26f9c68fed916e3d7c19f65c81b92` · 2313 B

**Pattern:** command-card grid sorting and widths

**Taken:** UI height 350; hero row max width 4, ability row 6, UI row 1; three button classes sorted before layout (hero/ability/ui).

### 09 · `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/SmashConsoleUI.fdf`

commit `f9e0aeed4be3` · sha256 `bbefde1ffe6e22842f531844e8f16d99d7350a45dbfb02a4ede30db066d05330` · 2190 B

**Pattern:** retail console composition in FDF units

**Taken:** bottom console 0.176 of the 0.6-unit screen (317 px @1080), top strip 0.032 (57.6 px), tiles 0.256 wide; tile 02 bottom height 0.15; 3-slice top: 0.256 + 0.087 + 0.053 + 0.256 + 0.032.

### 10 · `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/UnitPortrait.fdf`

commit `f9e0aeed4be3` · sha256 `31d9ac2f50f00847c648f5ecf070555e449d006aa036a06982363bfade567cfc` · 1106 B

**Pattern:** retail portrait frame and HP/mana text placement

**Taken:** portrait 0.0835 x 0.114 (150 x 205 px @1080p = 50 x 68 dp), model 0.0835 x 0.085, model bottom offset 0.0285, HP text 0.014 above bottom, font 0.011, text height 0.0164.

### 11 · `Retera/WarsmashModEngine` — `resources/UI/FrameDef/SmashUI/ToolTip.fdf`

commit `f9e0aeed4be3` · sha256 `3aab6b71cd75c8c0de29a17e85e34396093f429d94f8375ebb9d2027fcf29730` · 2908 B

**Pattern:** retail tooltip 9-slice and resource icon row

**Taken:** corner 0.008 (14.4 px = 4.8 dp), background tile 0.036, background insets 0.0025, font 0.010; icon 0.008 square, label font 0.0085, gold #FCD312 (1.0, 0.827, 0.0705).

### 12 · `Retera/WarsmashModEngine` — `core/src/com/etheller/warsmash/parsers/fdf/frames/BackdropFrame.java`

commit `f9e0aeed4be3` · sha256 `c00f843c862d7cc48ed112cf22110b81a2170da0942ba7e732bc40a7c04833df` · 12016 B

**Pattern:** the retail 8-segment border atlas decode

**Taken:** edgeUVWidth = 1/8; segment mapping L=0, R=1, T=2, B=3, UL=4, UR=5, BL=6, BR=7; corners are cornerSize square; edges repeat cornerSize steps with a remainder crop. This is the exact 9-slice contract for every WC3 Backdrop.

### 13 · `nightknighto/Warcraft-3-Reforged-UI-Designer` — `src/ts/ClassesAndFunctions/Tooltips.ts`

commit `d20a0edb45e0` · sha256 `b66e7712fd9d0f671ff10dfa6c73928511f78484f8f112303582921e8584cbcb` · 952 B

**Pattern:** WC3 tooltip z-order and max width

**Taken:** zIndex 9999; maxWidth 500 (design px); tooltip layer above every frame.

### 14 · `TheLucifurry/warcraft-3-ui` — `src/components/base/Button.vue`

commit `7d5cbc908050` · sha256 `06500b427adb37bfea8e3c3de8aa4a0d980a40a843a1448368b8e25fed0bbf79` · 2141 B

**Pattern:** WC3-styled web button sizes and states

**Taken:** sizes: 32 px (line-height 32, min-width 100), 48 px (line-height 48, min-width 200), default font 14 px, letter-spacing 0.5 px, line-height 180%; four textures default/hover/pressed/disabled.

### 15 · `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/NineSlice.lua`

commit `297d7aaaa736` · sha256 `83ec4afca8f54d250a4fac1e76c398c6210580f3e09ae24d989ccebd14d7f146` · 12647 B

**Pattern:** the 3x3 nine-slice runtime (layout pieces, offsets, atlas regions)

**Taken:** 8 art pieces per layout (4 corners + 4 edges) plus a solid center colour; every piece carries an x/y offset (+ optional x1/y1); each piece names its own atlas region.

### 16 · `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/Mainline/NineSliceLayouts.lua`

commit `297d7aaaa736` · sha256 `679f46d39cde6cd77df3899d41a439f8daf036a992c0e36100ddb59cf98400fe` · 24796 B

**Pattern:** named nine-slice layouts with per-piece atlas offsets

**Taken:** 8 named pieces per layout with px offsets, e.g. SimplePanelTemplate TL x=-5/y=0, TR x=2/y=0, BL x=-5/y=-3, BR x=2/y=-3; mirrorLayout true; layer OVERLAY for portrait metal.

### 17 · `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_SharedXML/Backdrop.lua`

commit `297d7aaaa736` · sha256 `f3e392d8e8fa058cba9c03747ae32626b80e96cd6b34ffd643d34cdc75635618` · 13290 B

**Pattern:** backdrop presets: tile size, edge size, content insets

**Taken:** BACKDROP_DIALOG_32_32: tileSize 32, edgeSize 32, insets left 11 / right 12 / top 12 / bottom 11; ARENA_32_32 insets 32/32/32/32; ACHIEVEMENTS_0_64 edgeSize 64.

### 18 · `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_ActionBar/Shared/ActionButton.lua`

commit `297d7aaaa736` · sha256 `54fb5625a5e8aa343cbbc0be805b23e2b3b329011e792448e6b732a4bebd8cba` · 62267 B

**Pattern:** action-button overlays and icon insets

**Taken:** quick-keybind highlight 46 x 45; hotkey text 16 px high, 10 px in compact; type-icon overlay 21 x 24; icon desaturate/vertex states (1,1,1 / 0.5,0.5,1 / 0.4,0.4,0.4).

### 19 · `Gethe/wow-ui-source` — `Interface/AddOns/Blizzard_ActionBar/Mainline/ActionButtonTemplate.xml`

commit `297d7aaaa736` · sha256 `6c2b6b19c746876e48b16068eb4b33d5c68738525f8c54d142c4d563c6500a1d` · 8856 B

**Pattern:** button, popup and icon-frame dimensions

**Taken:** action button 45 x 45 (46 x 45 with border); flash 36 x 10; popup cross axis 47, arrow 18 x 7; dialog 384 x 128 and 430 x 250; square icon buttons 64 x 64; standard dialog button 128 x 32.

### 20 · `shagu/pfUI` — `modules/minimap.lua`

commit `b2f6df84a93a` · sha256 `5b3ee24281e5fa7bb38f3c59a969283c1d75801bcfafb283dfa6c5a5ab1eae49` · 9193 B

**Pattern:** addon minimap size and button cluster

**Taken:** minimap default size 140 (user config), PvP icon 16 x 16, zoom modifier 0.05 per step, coordinate readout 1 decimal.

### 21 · `ElvUI-WotLK/ElvUI` — `ElvUI/Core/Install.lua`

commit `58ea24f79797` · sha256 `f5b248dd2e9e81b0d7bb727feaa6caaa9e0059678effe2ce62a52cc29d19f5b0` · 32975 B

**Pattern:** action-bar and minimap defaults

**Taken:** bar1 buttonsize 50, bar2 38, spacing 1; aura size 40; minimap 220; totems 50 (px at the addon UI scale).

### 22 · `ElvUI-WotLK/ElvUI` — `ElvUI/Modules/ActionBars/ActionBars.lua`

commit `58ea24f79797` · sha256 `22eb0b85f272b7258ae8eccde35bbc96bd07ddf7ee0aba528135280e4084a535` · 32059 B

**Pattern:** bar spacing/scale derivation

**Taken:** buttonSpacing = E:Scale(db.buttonspacing), backdropSpacing = E:Scale(db.backdropSpacing or buttonspacing); mover snap offset = buttonspacing / 2.

### 23 · `OpenRA/OpenRA` — `mods/cnc/chrome/ingame.yaml`

commit `4cc9a9f6299c` · sha256 `8bc5f11f6aa6a3593b70c75f95850c176f51926def98ff571c19e967b91a384f` · 50092 B

**Pattern:** RTS ingame HUD geometry (minimap, top strip, buttons)

**Taken:** minimap 220 x 220 at 5 px left, bottom -156; mission text 512 x 25 at y 22; bottom-right button 170 x 40; top info panel 175 wide; every rect anchored to WINDOW_WIDTH/HEIGHT.

### 24 · `OpenRA/OpenRA` — `mods/common/chrome/ingame.yaml`

commit `4cc9a9f6299c` · sha256 `33e213cf5e158cd0114b094298e00906ca40578165a4d22f29c56737a318de7e` · 2533 B

**Pattern:** shared ingame containers and tooltip container

**Taken:** mission text 512 x 25; full-window world container; explicit TOOLTIP_CONTAINER child so tooltips never clip at panel edges.

### 25 · `0ad/0ad` — `binaries/data/mods/public/gui/session/session.xml`

commit `61a3b9507d97` · sha256 `c2c5a2584128f7dc50010b9a3eb74866a7eb40f7f75bc97c2a2b396c24a4b5f0` · 4347 B

**Pattern:** bottom command deck composition

**Taken:** bottom band 200 px tall; minimap 200 x 200 (50%-512 .. 50%-312); unit commands 402 x 166; selection details 228 x 200; supplemental details 206 x 166; chat panel to y = 100%-240; design min width 1024.

### 26 · `0ad/0ad` — `binaries/data/mods/public/gui/common/tooltips.js`

commit `61a3b9507d97` · sha256 `3d1172f807895b130c3cdcd17808916ee494ab6bf5d892917096d0b6fc6112bc` · 39808 B

**Pattern:** tooltip/type roles in an RTS

**Taken:** font roles: sans-10 unit, sans-13 body/objection, sans-bold-13 header, sans-bold-12 small name, sans-bold-16 primary/secondary name; colour roles orange/red.

### 27 · `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/window.lua`

commit `1cd6099f263f` · sha256 `edb037ab1be8f27f44bd88f8606d789593585a0564ad19fba41661bb9fdc447a` · 2083 B

**Pattern:** RTS window default size and 9-slice draw

**Taken:** minWidth 50, minHeight 50, defaultWidth 400, defaultHeight 300; tiled 9-slice with 31 px tile at 31 px UV.

### 28 · `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/button.lua`

commit `1cd6099f263f` · sha256 `58f3eb53ce760a2412fa25ff12ceae19555bf5ca76bd0d13f3ef63f55edc17b0` · 1478 B

**Pattern:** RTS button default box

**Taken:** defaultWidth 70, defaultHeight 20 (px at 1080p-era UI scale).

### 29 · `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/tabbar.lua`

commit `1cd6099f263f` · sha256 `dfa191cc13e14c07970958d4039364eee997c064470c0b41afe1c28c54cc07ec` · 2987 B

**Pattern:** tab row item box

**Taken:** minItemWidth 70, minItemHeight 20; zero padding default; items share the tab bar height.

### 30 · `ZeroK-RTS/Zero-K` — `LuaUI/Widgets/chili/Controls/panel.lua`

commit `1cd6099f263f` · sha256 `6ec9c5496eaed1facd7acf588cee984f459047ec2677bc80c3f6d21ed36436c5` · 432 B

**Pattern:** plain panel default box

**Taken:** defaultWidth 100, defaultHeight 100; no padding by default; panel is the base of window/tabpanel.

### 31 · `warzone2100/warzone2100` — `src/hci.cpp`

commit `a954eb2f1873` · sha256 `53e32d835fa9bb143ace0550a487e8c63cc89ad13b52ed812bc93a11301a16b1` · 76208 B

**Pattern:** RTS ingame option button / reticule dimensions

**Taken:** ingame options button 25 x 28 px; proximity reticule 9 x 9 px; reticle button geometry derives from the source texture aspect.

### 32 · `Wargus/Wargus` — `scripts/human/ui_pandora.lua`

commit `24672aed09c5` · sha256 `91ed400fa9e8054d85cf4c710eb51db8a73e6c4f4b7187302f79a45f2ba84abe` · 9568 B

**Pattern:** WC2-style RTS sidebar and selection grid

**Taken:** sidebar 176 px; info panel 176 x 176 at y 160; selection buttons on a 56 px pitch (x 9/65/121, y 160/216/272); resource bar from x 176 to width-192, 16 px tall; button panel 176 wide; side filler 16 px.

### 33 · `Jerga99/hvo-mobile-rts` — `Assets/hvo/Prefabs/UI/ActionButton.prefab`

commit `81045c851739` · sha256 `e2838c2c1d5f3a0952dd1f41799e59ae5ce0b2b5a62910b4c671cf234c6aee49` · 7087 B

**Pattern:** mobile RTS action button prefab

**Taken:** button container 160 x 115; action button 92 x 92; anchors centered; focus swaps default/focused sprite (no colour tint).

### 34 · `Jerga99/hvo-mobile-rts` — `Assets/hvo/Images/Tiny Swords/UI/Banners/Carved_9Slides.png.meta`

commit `81045c851739` · sha256 `0240b6b62a182dc81430345d52399dcfb40f99507b81e1f9b7f100075c6501a5` · 3618 B

**Pattern:** fantasy 9-slice source sheet dimensions

**Taken:** sprite sheet 191 x 192, spriteMode 2 (multiple), alphaIsTransparency 1; Carved_9Slides family ships matching 3-slice and regular variants.

### 35 · `Anuken/Mindustry` — `core/src/mindustry/ui/Styles.java`

commit `c2f74b91a7f2` · sha256 `1cc8b4cf43758c9124b839090157fa7b807c076f332d562ceb751dd3dabd1698` · 16160 B

**Pattern:** four-state button styles

**Taken:** every style supplies down/over/disabled + fontColor/disabledFontColor; flat hover tint #454545; disabled font gray; over font accent for plain text buttons.

### 36 · `libgdx/libgdx` — `gdx/src/com/badlogic/gdx/graphics/g2d/NinePatch.java`

commit `38bc402b1cb8` · sha256 `4f3c42440c6695847ccd19e114fdb095dc1bccac31c6cc7170adf777c6fd33d7` · 21251 B

**Pattern:** 9-patch split contract

**Taken:** constructor takes left/right/top/bottom px; middleWidth = regionWidth - left - right, middleHeight = regionHeight - top - bottom; regions may be omitted; padding is a separate content hint.

### 37 · `raysan5/raylib` — `examples/textures/textures_npatch_drawing.c`

commit `ecd15c050af1` · sha256 `fa5739e9b84bf116e66d7b1f00ce9df528b85bfc316addd31a993756a94b9967` · 5265 B

**Pattern:** nine-patch insets at three densities

**Taken:** source 64 x 64; button insets left 12 / top 40 / right 12 / bottom 12; panel 16 all sides; horizontal 3-patch 8 all sides; vertical 3-patch 6 all sides; destination 32 x 32 proves only the centre stretches.

### 38 · `pixijs/pixijs` — `src/scene/sprite-nine-slice/NineSliceSprite.ts`

commit `1e707686d00a` · sha256 `fff71bf0772af80750e5c1534ab37713b6dc622a3432544df1f845fd66954117` · 23577 B

**Pattern:** default slice widths and geometry

**Taken:** leftWidth/rightWidth/topHeight/bottomHeight default 10; default width/height 100; texture borders may override; slices larger than the texture are clamped.

### 39 · `phaserjs/phaser` — `src/gameobjects/nineslice/NineSlice.js`

commit `02d8931b626d` · sha256 `f53b01c8ccf83b7c550c9999170885d1efee2db4b9fe1a3d4d529e63c42ea6ed` · 39623 B

**Pattern:** default slice widths and 3-slice mode

**Taken:** leftWidth/rightWidth defaults 10; topHeight/bottomHeight optional -> 3-slice when omitted; min width = left+right; min height = top+bottom.

### 40 · `callum-gander/nine-slice-frame` — `packages/react/src/NineSliceFrame.tsx`

commit `224fc1aa237c` · sha256 `ac59dc22f772d5450028ec3b2a5168779b985e8dba960503e48fd5ef3c7af924` · 2039 B

**Pattern:** CSS border-image nine-slice wrapper

**Taken:** slice default 8, borderWidth default 5 px, fill true, repeat stretch, image-rendering pixelated; border-image-slice accepts one number or top/right/bottom/left.

### 41 · `Lommix/bevy_nine_slice_ui` — `src/lib.rs`

commit `e931bfd488a7` · sha256 `dab9865c827cee077905b518089da32051c8ba92dbb3350ba0edd50fc31bbaba` · 8012 B

**Pattern:** GPU nine-slice UI material

**Taken:** NineSliceUiPlugin { sync_rate_ms: 100 } + UiMaterialPlugin; NineSliceUiTexture = atlas + optional bounds Rect + blend_color/blend_mix + 1D gradient lookup/gradient_mix; NineSliceMaterial uniforms surface_size/bounds/mix.

### 42 · `vleue/bevy_ninepatch` — `src/ninepatch.rs`

commit `2624a804ff86` · sha256 `50b821c39904e09948e8b2d371c3cb7211f7541786f52017af62f9511be3d67c` · 13501 B

**Pattern:** ECS nine-patch component

**Taken:** NinePatchBuilder::by_margins(top, bottom, left, right) builds 8 fixed patches + a growing centre; by_margins_with_content adds a content slot; NinePatch<T> component + NinePatchContent<T> marker; Patch carries original/target size and margin offsets.

### 43 · `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/button.tsx`

commit `67dfa8e30442` · sha256 `e5de247925def223f77da56335c3905ba201cdff673a031812e821a13e2b0dbc` · 1773 B

**Pattern:** WC3-styled button 9-slice and metrics

**Taken:** border-image-slice 16, border width 5 px, padding px 20 / py 16, font 14 px, rounded-md, active scale 0.95 + brightness 0.75, duration 100 ms; "frame" variant swaps the border image only.

### 44 · `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/card.tsx`

commit `67dfa8e30442` · sha256 `0c0c07fc321881f0d0f7fb0a85876f6c2bb00b46700835d811fcd9b8b7aa0e6e` · 2671 B

**Pattern:** WC3-styled panel 9-slice and metrics

**Taken:** border-image-slice 24, border width 24 px, padding px 24, rounded-xl, gap 16; small variant slice 16 / border 16 / padding 16 x 12; header pt 14.

### 45 · `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/tabs.tsx`

commit `67dfa8e30442` · sha256 `45db90fdd76006cdd92f9b7f6942e98208bac977af7d24f9d25ad63af9974f92` · 5940 B

**Pattern:** responsive tab row metrics

**Taken:** tab heights 38 / 40 / 44 / 48 at base/sm/md/lg; min widths 96 / 120 / 140; padding x 2/3/6; gap 1 -> 2; content padding 8 / 16 / 24, min-height 140 / 180 / 220.

### 46 · `TheOrcDev/warcraftcn-ui` — `components/ui/warcraftcn/tooltip.tsx`

commit `67dfa8e30442` · sha256 `812906713caf6efe76f4213e2d2ae0d22ca1ffea2d8e18015fffa1bcd0bac62d` · 3382 B

**Pattern:** fantasy tooltip box

**Taken:** max-width xs = 320 px, padding x 16 / y 12, font 14, amber-100 ink, rounded 4; per-rarity border classes.

### 47 · `threejsassets/game-hud` — `dist/skins/fantasy.css`

commit `f793ba3e0960` · sha256 `bd32ded26f4590388924d9366d9a4c8ae418df51334fe90031d77c379169abc8` · 11533 B

**Pattern:** fantasy stone/wood panel recipe with explicit values

**Taken:** panel border 7 px stone, radius 14 px, inner shadow 3 px #1e1b17, outer 0 6px 0 + 0 10px 18px; corner studs 32 x 32, radius 10/7/11/6, offsets ±16 px; body padding-top 18 px; palette panel #2e2a26, panel-2 #3a352f, ink #f7f2e7, gold #f7b731, hp #d8412f, mana #2f6fd0, xp #f0a71f.

### 48 · `2lab-ai/game-ui` — `src/skin.rs`

commit `ed34f392319d` · sha256 `208e9882263d9b30218f853f55c1b062e1fc101f6a032792f0de929d34a1e348` · 12258 B

**Pattern:** measured FFT fantasy skin (colours, units, 1u ink line)

**Taken:** classic plate #a59d83 + 1u ink outline + top highlight + right/bottom shade + 1u hard shadow; page cream #e8dfd0 with header #d6cbb9; dark battle HUD #132555 -> #1a2b59; parchment #daccae -> #a28563; 1 u = 2 physical px.

### 49 · `Videogamers0/MGUI` — `MGUI.Core/UI/MGToolTip.cs`

commit `c993516c3eaa` · sha256 `2aa5539f35ce8b4a5c2f8466a56e80496b01371da8af375108a6cf97f68c6f5f` · 4595 B

**Pattern:** tooltip border and padding contract

**Taken:** BorderThickness 2; Padding 6 x 3; MinWidth 10; MinHeight 10; draws at mouse + 5 px offset.

### 50 · `jsfehler/phaser-ui-tools` — `src/containers/frame.js`

commit `f3306cf9aa5e` · sha256 `ca6c8f1b8181d3c2e38d9ba6881503e6404e1ae8d01ab4cc60431a02de21ac6b` · 3834 B

**Pattern:** modal frame layering and padding layout

**Taken:** modal backdrop depth 9999, frame depth 9998; children placed with paddingX/paddingY (default 0); frame owns a full-screen background and optional mask.

## What the catalogue cannot answer (still retail-only)

The web/mobile references give process, grid and state discipline. The *look* comes from the shipped BLPs: panel corner 64 px at 512 x 64 border strips, button border 32 px at 256 x 32, tooltip corner 16 px at 128 x 16, console 0.176/0.032 at 0.256 tiles, portrait 0.0835 x 0.114. Those are measured in `HUD-DIRECTION.md` and built in `ASSETS.md`.

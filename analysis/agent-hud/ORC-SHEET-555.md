# Orc HUD sheet — segmentation, insets, manifest v2 (ledger 555)

Source: `/storage/emulated/0/Download/9a97e00c-f04f-47fb-9f62-118bca18e3c6.png`  
SHA-256: `eb1278ce66b9b8f9c437ac4f8b0c09d56ba1eb6d45d08fd1fc7cfe851c3530f5`  
Output: `assets/hud/orc/`  
Tool: `tools/hud_segment_sheet.py` (deterministic; PIL/numpy).

## Matte

Background model: per-channel degree-2 polynomial fitted to low-saturation mid-value pixels, robust-trimmed 3x. RMSE `0.03151`.
Mask: max of background distance, saturation and darkness; 3x3 close; enclosed holes filled (interiors stay opaque); 3 px edge decontamination (nearest core colour); neutral near-background pixels outside the core are dropped.

## Assets

| id | file | size | slice | gray edge % | edge px | soft interior | alpha holes | sha256 |
|---|---|---|---|---:|---:|---:|---:|---|
| panel.command_grid | panel.command_grid.png | 268x310 | fixed | 0.00 | 39 | 0 | 0 | `70d84161af6cfed0` |
| panel.selection | panel.selection.png | 338x316 | fixed | 0.00 | 65 | 0 | 0 | `3264f10a936e1f25` |
| frame.portrait_orc | frame.portrait_orc.png | 271x321 | three-slice vertical | 0.00 | 55 | 0 | 0 | `3d3d7a2399d51f55` |
| bar.resources | bar.resources.png | 554x192 | fixed | 0.00 | 52 | 0 | 0 | `16a22ffbacf03fe3` |
| panel.list | panel.list.png | 379x225 | fixed | 0.00 | 31 | 0 | 0 | `9ba4464c886aca9a` |
| tabs.strip6 | tabs.strip6.png | 371x183 | fixed | 0.00 | 36 | 0 | 0 | `45cfa679e957bbd4` |
| tabs.strip5 | tabs.strip5.png | 343x198 | fixed | 0.00 | 44 | 0 | 0 | `b37ccda182394ab9` |
| panel.wide | panel.wide.png | 342x239 | three-slice vertical | 0.00 | 22 | 0 | 0 | `73f41a8bfe7eb178` |
| banner.ornate | banner.ornate.png | 445x277 | three-slice vertical | 0.00 | 48 | 0 | 0 | `abcd17517b128db3` |
| panel.list_rows | panel.list_rows.png | 267x319 | fixed | 0.00 | 24 | 0 | 0 | `624b9d51b6b40d7a` |
| dialog.confirm | dialog.confirm.png | 296x287 | fixed | 0.00 | 34 | 0 | 0 | `a79b6185c1221b4a` |
| icons.round.gold.disabled | icons.round.gold.disabled.png | 85x109 | fixed | 0.00 | 29 | 0 | 0 | `65ce2a844187a34e` |
| icons.round.gold | icons.round.gold.png | 85x109 | fixed | 0.00 | 29 | 0 | 0 | `b661ab5be1e67a36` |
| icons.round.lumber.disabled | icons.round.lumber.disabled.png | 87x108 | fixed | 0.00 | 29 | 0 | 0 | `1fffbd0c89513c49` |
| icons.round.lumber | icons.round.lumber.png | 87x108 | fixed | 0.00 | 29 | 0 | 0 | `d67fdb7cf4dac96d` |
| icons.round.meat.disabled | icons.round.meat.disabled.png | 86x108 | fixed | 0.00 | 29 | 0 | 0 | `c2b1bdab9d5e8279` |
| icons.round.meat | icons.round.meat.png | 86x108 | fixed | 0.00 | 29 | 0 | 0 | `bac8faf4404b7312` |
| icons.round.attack.disabled | icons.round.attack.disabled.png | 87x109 | fixed | 0.00 | 29 | 0 | 0 | `a330343f43bf27d1` |
| icons.round.attack | icons.round.attack.png | 87x109 | fixed | 0.00 | 29 | 0 | 0 | `0f1b9080a48126fd` |
| icons.round.armor.disabled | icons.round.armor.disabled.png | 85x109 | fixed | 0.00 | 29 | 0 | 0 | `2c61a7c3a6d50bb7` |
| icons.round.armor | icons.round.armor.png | 85x109 | fixed | 0.00 | 29 | 0 | 0 | `f5705f2b6875bcef` |

## Insets (evidence in source coordinates)

| id | slice | insets / caps | evidence |
|---|---|---|---|
| panel.command_grid | fixed | fixed (not a 9-slice) | 3x3 command/inventory slots are baked into the art; any stretch moves the cell grid. Fixed 264x306. |
| panel.selection | fixed | fixed (not a 9-slice) | Top gem is centred in the stretch band and fir trees run the full side height; a 9-slice would distort both. Fixed 334x312. |
| frame.portrait_orc | slice | top 125, bottom 72, vertical-only, width fixed | Top cap y=0..125 holds the arch, gem and crown; bottom cap y=245..317 holds the rock base and lower gem; x=24..243 between y=125..245 is a straight two-column arch side, safe to stretch vertically. Width stays 267. |
| bar.resources | fixed | fixed (not a 9-slice) | Three resource slots with baked gold/lumber/meat icons; stretching would move the icon-to-slot mapping. Fixed 550x188. |
| panel.list | fixed | fixed (not a 9-slice) | Left portrait column is an internal divider at x=95; a nine-slice centre would stretch it. Fixed 375x221. |
| tabs.strip6 | fixed | fixed (not a 9-slice) | Six baked tab cells (measured; the brief said four); stretching breaks the cell count. Fixed 367x179. |
| tabs.strip5 | fixed | fixed (not a 9-slice) | Five baked tab cells (measured; the brief said three); stretching breaks the cell count. Fixed 339x194. |
| panel.wide | slice | top 75, bottom 48, vertical-only, width fixed | Top cap y=0..75 holds the stone beam, gold flourish and centred gem; bottom cap y=187..235 holds the lower stone/gold band; x=50..288 between y=75..187 is the brown stone field, safe to stretch vertically. Width stays 338, so the gem never enters a stretch band. |
| banner.ornate | slice | top 95, bottom 70, vertical-only, width fixed | Top cap y=0..95 holds the crown, gem and upper scrollwork; bottom cap y=203..273 holds the gold base and green cloth; x=60..381 between y=95..203 is the stone field, safe to stretch vertically. Width stays 441. |
| panel.list_rows | fixed | fixed (not a 9-slice) | Four baked row plates; any stretch changes the row count/spacing. Fixed 263x315. |
| dialog.confirm | fixed | fixed (not a 9-slice) | Green/red action buttons are baked into the bottom band; stretching moves them. Fixed 292x283. |
| icons.round.gold | fixed | fixed (not a 9-slice) | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.lumber | fixed | fixed (not a 9-slice) | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.meat | fixed | fixed (not a 9-slice) | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.attack | fixed | fixed (not a 9-slice) | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.armor | fixed | fixed (not a 9-slice) | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |

## Not suitable for 9-slice (fixed size only)

| id | reason |
|---|---|
| panel.command_grid | 3x3 command/inventory slots are baked into the art; any stretch moves the cell grid. Fixed 264x306. |
| panel.selection | Top gem is centred in the stretch band and fir trees run the full side height; a 9-slice would distort both. Fixed 334x312. |
| bar.resources | Three resource slots with baked gold/lumber/meat icons; stretching would move the icon-to-slot mapping. Fixed 550x188. |
| panel.list | Left portrait column is an internal divider at x=95; a nine-slice centre would stretch it. Fixed 375x221. |
| tabs.strip6 | Six baked tab cells (measured; the brief said four); stretching breaks the cell count. Fixed 367x179. |
| tabs.strip5 | Five baked tab cells (measured; the brief said three); stretching breaks the cell count. Fixed 339x194. |
| panel.list_rows | Four baked row plates; any stretch changes the row count/spacing. Fixed 263x315. |
| dialog.confirm | Green/red action buttons are baked into the bottom band; stretching moves them. Fixed 292x283. |
| icons.round.gold | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.lumber | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.meat | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.attack | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |
| icons.round.armor | Five round icons on one row; each is a fixed 64 dp icon, split into its own asset and disabled variant. |

## Edge crops at 4x

- `analysis/agent-hud/orc-edges/panel_command_grid_before.png`
- `analysis/agent-hud/orc-edges/panel_command_grid_after.png`
- `analysis/agent-hud/orc-edges/panel_selection_before.png`
- `analysis/agent-hud/orc-edges/panel_selection_after.png`
- `analysis/agent-hud/orc-edges/frame_portrait_orc_before.png`
- `analysis/agent-hud/orc-edges/frame_portrait_orc_after.png`
- `analysis/agent-hud/orc-edges/bar_resources_before.png`
- `analysis/agent-hud/orc-edges/bar_resources_after.png`
- `analysis/agent-hud/orc-edges/panel_list_before.png`
- `analysis/agent-hud/orc-edges/panel_list_after.png`
- `analysis/agent-hud/orc-edges/tabs_strip6_before.png`
- `analysis/agent-hud/orc-edges/tabs_strip6_after.png`
- `analysis/agent-hud/orc-edges/tabs_strip5_before.png`
- `analysis/agent-hud/orc-edges/tabs_strip5_after.png`
- `analysis/agent-hud/orc-edges/panel_wide_before.png`
- `analysis/agent-hud/orc-edges/panel_wide_after.png`
- `analysis/agent-hud/orc-edges/banner_ornate_before.png`
- `analysis/agent-hud/orc-edges/banner_ornate_after.png`
- `analysis/agent-hud/orc-edges/panel_list_rows_before.png`
- `analysis/agent-hud/orc-edges/panel_list_rows_after.png`
- `analysis/agent-hud/orc-edges/dialog_confirm_before.png`
- `analysis/agent-hud/orc-edges/dialog_confirm_after.png`

## QC

- Every output opens as RGBA; alpha hole count is 0 on every asset (interiors are filled, not punched).
- Soft interior pixels: 0 (the filled core is alpha 1.0).
- Gray edge ratio: each asset < 2 % (see table); the decontamination pass replaces edge RGB with the nearest core colour, and any remaining neutral pixel within 0.08 of the local background is shaved (alpha 0) instead of shipping a halo.
- Disabled icon variants are procedural (`saturation 60 %`, `brightness 70 %`) and labelled `variant_of`.

## Determinism

- Runs: 2, files compared: 22, identical: 22.
- Result: PASS.

## Pipeline order

`tools/hud_segment_sheet.py` owns `assets/hud/orc/` and clears it before each run. Build the pack after the segment step:

```
python3 tools/hud_segment_sheet.py --repeat
./target/debug/wc3-hud-pack build --src assets/hud/orc --out assets/hud/orc/pack \
  --fonts analysis/hudskin/final/wc3rust-hud-final/fonts
./target/debug/wc3-hud-pack validate assets/hud/orc/pack
```

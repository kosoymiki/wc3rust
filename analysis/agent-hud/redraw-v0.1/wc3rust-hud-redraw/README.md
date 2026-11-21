# wc3rust — Mobile HUD Redraw Asset Pack

**Build:** 2026-11-13 · **Status:** generated visual asset pack v0.1 (not yet runtime-integrated)

This pack is an original, procedural fantasy-RTS HUD art set built against `HUD_REDRAW_TZ.md` dimensions and component inventory. It is designed around a dark stone / forged steel / brass-gold / parchment palette. Blue is not used as the dominant HUD background.

## Contents

- `console/` — four console tiles (left cap, left mid, right mid, right cap), 256×288.
- `minimap/` — 216×216 frame (normal/alert) and 216×20 header.
- `portrait/` — 128×128 normal/hero/enemy frames.
- `selection/` — 240×180 selection panel.
- `commandcard/` — 64×64 button borders (normal/pressed/disabled/cooldown/autocast/empty), glow and radial cooldown overlay.
- `resources/` — 236×32 resource pills, 32×32 gold/lumber/food icons, RoC/TFT upkeep threshold strips.
- `topbar/` — 720×32 map/FPS plate.
- `tooltip/` — 384×256 parchment tooltip and 24×12 arrows.
- `cursor/` — unit/hero/enemy selection rings, target/move/attack cursors, rally/order markers.
- `build/` — 64×8 build progress bar.
- `controlgroups/` — 44×44 active/inactive group buttons.
- `menus/` — menu panel, menu button states, loading frame and score panel.
- `atlases/commandbuttons_atlas.png` — 2048×2048 atlas with 1,024 original generic fantasy-RTS pictograms, 64×64 cells. `commandbuttons_atlas.json` maps cell coordinates and 12-slot page order (86 pages; final page partially filled).
- `fonts/` — DejaVu Serif/Sans faces with Cyrillic coverage and accompanying license notice.
- `manifest.json` — dimensions, 9-slice insets, state names, atlas group, format and mipmap policy.
- `source/generate_assets.py` — editable parametric source used to generate all PNGs and the manifest.
- `previews/contact_sheet.png` — component contact sheet.
- `previews/mock_hud_landscape.png` — composed 1920×1080 HUD layout preview (illustrative, not an in-game capture).

## Important implementation notes

1. PNG is the canonical source/export format: RGBA, straight alpha, sRGB. BLP1 conversion is intentionally left to the project's own asset pipeline; no BLP converter was bundled or assumed.
2. 9-slice insets are declared in `manifest.json`. Keep corners fixed and stretch only center regions. Do not generate mipmaps for 9-slice panels; icon/marker mipmaps are enabled in metadata.
3. Atlas layout is power-of-two 2048², 32×32 cells, no rotation. Icon atlas has 1,024 entries; each gameplay page consumes 12 icons in row-major order.
4. This is an **asset-only** delivery. It has not been wired into `mobile_hud.rs`, `hud.rs`, `assets.rs`, nor device-tested. The archive supplied for this task contains the specification, donor/reference files and HUD screenshots, but not the current Rust source tree. Runtime placement and draw-call batching must be verified against the actual source tree.
5. Source is parametric Python/Pillow rather than a flattened PSD/Krita project. Edit `source/generate_assets.py` and regenerate to change the style or geometry.

## Build

```bash
python -m pip install Pillow
python source/generate_assets.py
```

## Validation

Run `python source/validate_pack.py` to check manifest/file coverage, exact dimensions, alpha mode, atlas bounds and expected asset families.

## License / provenance

All HUD PNGs and command pictograms in this pack are generated original artwork, not extracted Warcraft III or DragonUI textures. No Blizzard/WC3 extracted art, original command-button atlas, or DragonUI texture files are redistributed in this pack. The supplied DragonUI and WC3 files were treated as visual/reference material only. The DejaVu font license is included under `fonts/`.

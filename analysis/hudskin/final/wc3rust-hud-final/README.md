# wc3rust — Final HUD Asset Pack

This is the consolidated art delivery for the HUD redraw specification dated 2026-11-13. It contains the complete exported PNG asset set, source vector overlay, font files, command atlas, manifest, and validation tooling.

## Visual direction
Dark basalt and worked steel, restrained antique-gold bevels, parchment surfaces, warm upper-left lighting, and small faction-color accents. Blue is restricted to faction detail rather than panel backgrounds. All UI panel exports are RGBA PNG with straight alpha.

## Layout contract
Reference resolutions: 1920×1080 landscape and 1080×1920 portrait. Design height 768. The console is edge-to-edge at the bottom. Command card is 4×3 at 64×64 with 4 px gap; control groups are 10×44×44. Safe-area values and scaling formula are in `manifest.json`.

## Asset structure
`console/`, `minimap/`, `portrait/`, `selection/`, `commandcard/`, `resources/`, `topbar/`, `tooltip/`, `cursor/`, `build/`, `controlgroups/`, `menus/`, `icons/`, `fonts/`, `atlas/`. File names are snake_case and each exported PNG is listed in the manifest with dimensions and 9-slice insets.

## 9-slice
Use manifest insets as source of truth. Stretch only the center and edge strips; preserve corners at native pixel size. Do not mipmap panel slices. For icon textures use 2 px padding and 2 px edge extrusion in the runtime atlas builder.

## Alpha / color
PNG RGBA8, straight alpha, sRGB. No embedded color profile. Do not premultiply alpha at asset import unless the renderer expects premultiplied blending.

## Fonts
The bundled DejaVu Serif / Sans families provide Cyrillic + Latin coverage. Primary uses serif; action bar uses condensed sans; numeric labels use monospaced tabular digits. Verify font rasterization against `MIN_TEXT_PX=24` in the game HUD renderer.

## Provenance
The delivered interface artwork is original generated/composited artwork and custom vector/raster work. The reference archive contains third-party Warcraft III and DragonUI assets; those are not copied into this deliverable. The generated concept screenshots are not runtime UI and are not included as full-screen mockups. Font files retain their upstream DejaVu license; see `docs/DEJAVU-LICENSE.txt`.

## Known integration boundary
This package is an asset delivery, not a Rust integration patch. It does not claim a verified device screenshot or BLP1 runtime export. Integrate by reading `manifest.json`, implementing the stated 9-slice insets, and using the existing mobile HUD layout rectangles.

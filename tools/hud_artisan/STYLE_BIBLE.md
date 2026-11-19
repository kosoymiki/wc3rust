# HUD Artisan visual style bible

## Design intent

Use REFERENCE_BOARD.md for the accepted visual reference set and original master-asset briefs. All named games/tools are direction-only references; no retail pixels or protected designs may be copied.


Classic + Modern means an original fantasy real-time-strategy interface with the weight and tactility
of late-90s/early-2000s strategy UI, rendered with current material discipline, clean edge handling,
high-DPI consistency and legible mobile-scale hierarchy. It does not mean tracing or reproducing any
specific retail game's protected artwork, exact panels, logos, icons, textures or typography.

The UI should feel forged, assembled and maintained by a believable fantasy culture. It must not
look like a generic mobile app dressed in gold.

## Material system

### Forged metal
- Use a coherent light direction: upper-left key, soft lower-right occlusion.
- Construct at least three readable value planes: bright bevel, mid-tone body, dark inner bevel.
- Bronze/gold is an accent material, not a full-panel fill.
- Edge wear follows exposed edges and corners; do not scatter noise uniformly.
- Rivets and engraved marks must be sparse, mechanically plausible and visible at target size.
- Never rely on bloom to make an edge readable.

### Stone and slate
- Main content fields use dark slate, charcoal stone or deep iron.
- Texture frequency is low enough to remain quiet behind text and icons.
- Use broad value variation and subtle strata; no random speckle carpet.
- Keep the centre calmer than the perimeter.

### Wood, leather and parchment
- Wood grain follows plausible board/log direction.
- Leather uses restrained grain, seams and pressure wear.
- Parchment is an inner accent for menus/tooltips, not a yellow rectangle.
- Avoid photorealistic material detail that competes with the UI hierarchy.

### Enamel and gems
- Emerald, cobalt, blood red and violet act as small state/faction accents.
- A gem needs a clear silhouette, at least two planes and one restrained specular highlight.
- Glow must remain contained and may not obscure the underlying silhouette.

## Shape language

- Frames: stable structural silhouette; ornamental corners must remain inside declared nine-slice inset zones.
- Stretch edges: repeatable or visually continuous. No unique focal ornament in stretchable edge centres.
- Centre: quiet, text-safe and icon-safe.
- End caps: distinct enough to define the panel while allowing the centre to extend.
- Buttons: readable from their perimeter and material depth even with the icon hidden.
- Menus: larger structural hierarchy, calmer content field and restrained ornament density.
- Portrait frames: empty transparent aperture; portrait art is a separate asset.
- Minimap frame: map aperture must remain fully unobstructed.

## Icon discipline

Icons are miniature illustrations, not a set of generic glyphs with different colours.
Every icon requires:
1. One primary recognisable silhouette.
2. One secondary supporting shape or material detail.
3. Clear separation from sibling concepts.
4. A single dominant light direction.
5. Value grouping that survives downsampling to 32–64 px.

Examples of semantic identity:
- Gold: minted coin or ingot; lumber: timber; food: ration/roast/grain.
- Attack: blade; armour: shield; health: vitality emblem; mana: arcane crystal.
- Movement: arrow/rally marker; repair/build: tool; research: alchemical apparatus.
- Fire, frost, lightning, summon, stealth, teleport: each has its own silhouette, not the same rune.
- Potion, scroll, boots, ring, orb, sword, bow, axe and staff must be individually recognisable.

Do not bake a number, tooltip, cooldown text, shortcut key or label into the icon. The engine owns all text.

## State grammar

All states within one component family share the same base silhouette, material and light direction.
Only the state treatment changes:
- normal: balanced base values and neutral lighting;
- hover: narrow edge highlight and slight value lift;
- pressed: depressed face, dark inner bevel and compressed highlight;
- selected: distinct restrained selection rim;
- disabled: lower saturation/luminance without loss of readability;
- autocast: precise small enchantment rim;
- cooldown: leave the surface suitable for an engine-drawn radial mask;
- empty: unoccupied inset, no invented content;
- warning: warm caution accent, never a full red wash;
- active/inactive: difference in contrast/highlight, not a different component design.

## Colour and contrast

The style palette may vary by faction/theme, but semantic colour must remain consistent:
- health/danger red;
- mana/arcane blue or violet;
- gold/resource warmth;
- green/nature and selected enchantment accents;
- steel/stone neutrals for structure.

Maintain text contrast by keeping backgrounds quiet. Never use a busy texture behind numeric values or item costs.

## Nine-slice and technical art

The manifest insets are a hard geometric contract. They are not aesthetic suggestions.
- Do not put a gem, corner spike, unique crest or rivet pattern across a stretch seam.
- Check the asset at native size and at 1x, 1.5x and 2x nine-slice expansion.
- Check corner symmetry/continuity at all intended scaling ranges.
- Check alpha on near-black and pale backgrounds.
- Avoid RGB fringe in fully transparent pixels; pad the edge colour or use clean straight alpha.
- Keep sRGB PNG colour data and straight (not premultiplied) alpha.
- Preserve exact manifest dimensions. Never non-uniformly stretch a finished illustration.

## Review rejection rules

Reject:
- generic rune-based icon catalogs;
- repeated silhouettes across unrelated concepts;
- flat gold rectangles, one-line SVG boxes or generic bevel filters;
- random decorative noise and unreadable fake runes;
- clipped/uneven frames, perspective drift or inconsistent light;
- tiny details that vanish at mobile scale;
- broad glows, over-saturated gold or muddy dark fields;
- text, labels, watermarks, logos or signatures embedded in images;
- any visual state that cannot be distinguished at native resolution.

Passing the technical validator proves file integrity, not visual excellence. Art approval requires
side-by-side comparison, native-size review and an in-game screenshot review on both orientations.

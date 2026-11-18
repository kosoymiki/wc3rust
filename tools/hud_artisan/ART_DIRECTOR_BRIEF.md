# HUD Artisan — Art Director's production brief

## Quality target

The target is premium real-time-strategy game art: authored, tactile, coherent, legible at native UI size, technically clean and consistent across a complete interface. "Blizzard-level" is a quality ambition, not permission to reproduce Blizzard's protected assets, logos, exact silhouettes, textures, layouts, typography or signature compositions. The deliverable must be an original wc3rust visual identity.

A prompt that says "epic fantasy" is not art direction. Every image must look designed by the same senior UI art team.

## Non-negotiable visual grammar

- **Material truth:** metal has thickness, bevel hierarchy, edge wear on exposed edges, believable joins and plausible weight. Stone has broad strata, not random noise. Leather has seams and pressure wear. Gems have facets and contained light. No plastic gold gradients.
- **Designed silhouette:** the asset reads in monochrome before ornament. Each icon has one primary contour and one unmistakable semantic action/object. Sibling icons must not collapse to the same generic rune.
- **Controlled light:** one upper-left key, coherent bounce, lower-right occlusion. Highlights follow geometry; no random rim lights or global bloom.
- **Value hierarchy:** the primary read is strongest, support forms second, micro-detail last. Quiet centres preserve gameplay text and portrait/map content.
- **Craftsmanship:** clean joins, deliberate asymmetry, crisp anti-aliased contours, no accidental tangencies, broken bevels, floating rivets, melted geometry or noisy texture carpets.
- **Game-scale legibility:** inspect at actual target dimensions and at 50% / 25% downsample. If the subject becomes a blob or loses its meaning, redesign it.
- **Cohesion:** the whole HUD shares a common metal language, edge thickness, lighting direction, ornament density and palette. Faction variants may change motifs and accents, not construction logic.
- **Engine truth:** exact dimensions, alpha, safe insets, stretch zones, atlas geometry and text-safe areas are hard constraints, not suggestions.

## Composition by family

### Icons and abilities
One object/action only, centered with optical rather than mathematical balance. Strong negative space around the contour. No lettering, digits, UI frames, multiple objects, hands, scenery or photographic staging. Use a three-level value grouping and no more than one dominant accent. Distinguish every semantic sibling deliberately.

### Frames and panels
Build as a designed object: outer silhouette, structural body, inner bevel, quiet content field. Keep all unique ornament inside nine-slice-safe corners. Stretch edges must be repeatable. Portrait and minimap apertures are truly transparent; never fake them with black fill or checkerboard.

### Buttons and states
All states share exact construction and silhouette. Only material response changes: hover lifts the rim, pressed compresses the face, selected adds a restrained focus edge, disabled desaturates without erasing the contour. Never generate each state as a different button design.

### Bars and strips
Prioritize long-range continuity and clear end caps. No focal decoration in stretch regions. The empty fill area must remain calm for engine-drawn content.

### Menus and large surfaces
Strong hierarchy at a glance, restrained perimeter detail, low-frequency centre texture and reserved space for engine typography. Do not bake text into the image.

## Generation strategy

1. Generate a composition/concept pass only when the semantic brief is ambiguous; do not ship concept boards.
2. Lock a master material/lighting reference from original generated assets approved for this project.
3. Generate one asset per job at target aspect ratio. Do not generate sprite sheets or collages.
4. Generate state variants from the approved base asset using image-to-image/reference conditioning when supported; preserve geometry and change only state treatment.
5. For icons, generate multiple candidates, compare at native size, and select by semantic recognition and silhouette uniqueness—not by detail density.
6. Use deterministic seeds for reproducibility. A seed is not a substitute for recording model, checkpoint, LoRA, sampler, steps, CFG, VAE, control/reference inputs and post-processing.
7. Retouch defects at source resolution. Do not upscale a weak image and call it finished.

## Forbidden shortcuts

Generic glyph packs, repeated templates with recoloured symbols, text/runes, fake transparency, over-bloom, random scratches, noise-as-detail, perspective drift between states, cropped bevels, invented item contents, illegible micro-ornament, and "looks fine when zoomed in" are automatic rejection.

## Final review

A passing file is not necessarily good art. Inspect isolated assets, complete contact sheets, native-size gameplay composites, light/dark backgrounds, portrait and landscape layouts, and all interaction states. Require two independent visual reviewers and a human art-director sign-off for the release candidate.

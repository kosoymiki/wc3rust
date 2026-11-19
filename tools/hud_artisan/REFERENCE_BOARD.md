# HUD Artisan — visual reference board

## Accepted direction

**Classic + Modern reinterpretation.** Preserve the weight, readability and fantasy-RTS tactility associated with classic Warcraft III-era interfaces, but create an original wc3rust identity with current material rendering, clean alpha, consistent scaling, calmer content fields and mobile-size legibility.

These references are art-direction studies only. They are not source art, texture donors, tracing targets or permission to reproduce retail silhouettes, exact layouts, icons, typography, logos or ornament.

## Reference set

### Warcraft III: Battle Chest

Study the classic RTS hierarchy: resource/status grouping, framed command surfaces, strong edge silhouettes, dark content fields and the balance between ornament and usable space. Reinterpret the principles with new shapes, proportions and details.

### Warcraft III: Reforged

Study modern material response, lighting separation, richer surface depth and updated high-resolution presentation. Do not copy specific retail UI assets, icon designs, textures or exact compositions.

### Warcraft III: Reign of Chaos / The Frozen Throne retrospectives

Study faction material grammar and visual storytelling: Human heraldic steel/gold, Orc iron/leather/bone, Undead obsidian/cold metal, Night Elf carved wood/nature/enamel. These are broad design vocabularies, not templates to reproduce.

### DragonUI

Use only as a geometry/behavior reference for how an RTS HUD allocates the viewport, frames the minimap, groups resources, presents selected-unit/building information and organizes command buttons. Do not reuse its textures, icons, logos or other artwork. Do not assume every observed layout choice is the target layout.

## wc3rust original visual identity

- Structural base: dark slate, charcoal stone and forged iron.
- Primary edge material: restrained bronze/gold with clear bevel hierarchy, not full-gold panels.
- Supporting materials: worn leather, dark timber, parchment and small enamel/gem accents.
- Light: upper-left key, coherent lower-right occlusion and restrained specular highlights.
- Shape grammar: hand-worked but disciplined corner geometry, asymmetrical authored details, clean joins and consistent edge thickness.
- Content areas: low-frequency texture, calmer centres and safe space for engine-rendered text, portraits and map content.
- Faction variants: change motifs and accent colours, not the construction logic or lighting direction.

## Reference prompts for original master assets

These are original generation briefs for establishing a project-owned material library. They are not requests to recreate an existing game's artwork. The approved outputs become wc3rust references only after human review and a license/provenance record.

### Master material study — forged iron and aged bronze

Create a single isolated material sample board for an original premium fantasy RTS interface: dark forged iron plate, aged bronze bevel, restrained gold edge wear, charcoal slate inset, tiny emerald enamel accent. Orthographic front-facing material study, upper-left key light, physically coherent bevels and contact shadows, neutral studio background, no text, no logos, no UI layout, no symbols copied from existing games. Keep materials separated into clearly readable swatches and preserve subtle surface detail without noise.

### Master panel study — nine-slice-safe frame

Create one original rectangular fantasy RTS panel frame, front-facing and geometrically clean. Dark slate centre must remain calm and empty. Forged iron structural body, restrained bronze/gold bevels, authored corner ornaments only. Keep the centre and stretch-edge midsections quiet and repeatable; concentrate unique detail in the four corners and end caps. No text, icon, logo, portrait, map or copied retail design. Preserve a transparent exterior and a fully transparent interior aperture where required by the job contract.

### Master button study — reusable state family

Create one original square command-button frame as a reusable component. Strong readable perimeter, dark inset face, three-level bevel hierarchy, small restrained metal accents and a clear empty centre for an engine-rendered icon. Produce a normal base only; hover/pressed/selected/disabled treatments must later derive from the same geometry, not from independently redesigned buttons.

### Master icon study — semantic silhouette

Create one isolated, original 64×64 fantasy RTS resource/ability icon from the exact semantic job brief. One object/action only, strong monochrome silhouette, one secondary supporting form, controlled upper-left light, clean transparent background, no lettering, no UI frame, no collage, no generic rune substitution. Native-size readability outranks micro-detail.

## Layout and asset-family rules

- Resource bar, console panels, portrait frame, minimap frame, command card, selection panel, tooltips, menus, loading/score panels and button states share one material grammar.
- Command buttons are authored as individual assets and assembled by the manifest atlas. Never ask the renderer to draw the final atlas grid.
- Control-group buttons exist as reusable 44×44 components, but the presence of ten slots in the engine contract does not mean a permanent ten-slot tray must always occupy screen space.
- Text, numbers, cooldown overlays, hotkeys and labels remain engine-rendered.
- Portrait and minimap apertures are truly transparent; portrait/map content is a separate runtime layer.
- Nine-slice ornament stays outside declared stretch zones. A pretty frame that breaks when expanded is a failed asset.

## Source and licensing rules

Record every external reference image with source, license and purpose in the manifest or reference ledger. If the license is unknown, use it only as a high-level study and do not include its pixels in the generation conditioning set. Never extract, trace or republish retail game assets. Every generated master reference needs its own output hash, model/checkpoint/license provenance and human approval.

## Release review

Inspect references and outputs side by side, then hide the references and judge the wc3rust asset on its own. The final image must have its own silhouette, composition and surface details. Review at native size, 50% and 25%, against both light and dark backgrounds, then inside the actual HUD layout.
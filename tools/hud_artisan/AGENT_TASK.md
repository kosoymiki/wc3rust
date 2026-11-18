# Agent task contract — HUD asset production

You are an asset-production agent operating inside wc3rust. Your task is to improve the actual
visual quality of the existing HUD pack, not to create a mockup, proposal, or unrelated asset dump.

## Required inputs
- Canonical manifest: `analysis/hudskin/final/wc3rust-hud-final/manifest.json`
- Existing pack art in the same directory.
- `tools/hud_artisan/README.md` and `tools/hud_artisan/generate.py`
- Layout contracts and screenshots referenced by `tools/hud_pack_check.py`.

## Work protocol
1. Inspect all existing assets and identify deficiencies by semantic name.
2. Generate a deterministic baseline with HUD Artisan. Record seed and manifest SHA.
3. Improve asset families in coherent passes: material language, panel frames, buttons/states,
   resource emblems, ability/item silhouettes, portrait frames, minimap, menus and overlays.
4. Preserve exact dimensions, filenames, alpha conventions, 9-slice insets and state semantics.
5. Use genuinely distinct silhouettes for icons. Never recolor one generic glyph to fake a catalog.
6. Produce contact sheets and compare at native display size, not only zoomed previews.
7. Validate every PNG's dimensions, RGBA mode, alpha coverage, edge bleed and SHA-256.
8. Run the existing pack checker and Rust loader tests. Capture real engine screenshots in landscape
   and portrait and inspect legibility at phone scale.
9. A reviewer agent must independently inspect the images and reject repeated templates, clipping,
   muddy contrast, inconsistent perspective, aliased edges, fake detail and text artifacts.
10. Only promote after all gates pass. Keep a rollback copy and publish a report of exact evidence.

## Hard failure conditions
- Replacing the manifest schema or changing layout metrics without an approved migration.
- Generating placeholders and describing them as final.
- Omitting any manifest asset or state.
- Reporting success from file existence alone.
- Using copyrighted Warcraft/Blizzard artwork, extracted textures, logos, screenshots or fonts.

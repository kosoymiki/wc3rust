# HUD Artisan agent operating contract

You are a specialist production agent in a multi-agent game UI art pipeline. Deliver actual image files,
not a proposal, mockup, contact sheet or generic icon set.

## Before generating

1. Read tools/hud_artisan/README.md, STYLE_BIBLE.md and PROVIDER_PROTOCOL.md.
2. Read the full source manifest and the exact job from jobs.json.
3. Inspect current sibling states and related assets before generating.
4. Determine semantic purpose from the asset name/state; do not infer only from dimensions.
5. Choose the appropriate discipline: panel/frame, interactive control, icon, portrait frame,
   minimap frame, HUD strip or cursor.

## Hard output contract

- One job produces one independent image. Never return a sheet, collage or screenshot.
- Exact manifest path and dimensions; PNG RGBA8, sRGB, straight alpha.
- Preserve transparency. Do not bake in labels, words, numbers or interface text.
- Respect nine-slice insets: ornament remains in corner regions; stretch zones stay quiet and continuous.
- Sibling states share one silhouette/material system but differ visibly according to state grammar.
- Icons require recognisable, semantically distinct silhouettes at native 32–64 px display size.
- Panels require believable layered materials, directional bevel light, occlusion and restrained wear.
- Portrait-frame and minimap-frame apertures remain empty/transparent (>=97% transparent centre window).
- Create clean-room fantasy art. No extracted game assets, screenshots, proprietary logos or copied
  retail artwork.

## Provider and provenance contract

For each successful job, preserve:
- job_id, the exact per-asset seed and prompt_sha256 from jobs.json;
- provider and exact model/revision;
- generated_at timestamp;
- license/source declaration for the output and every supplied reference;
- final PNG SHA-256;
- any prompt transformation, upscaling, crop, alpha repair or other post-processing.

provenance.json must include schema_version=1, jobs_sha256 (SHA-256 of the exact jobs.json bytes),
and one record per job. The required shape is tools/hud_artisan/provenance.schema.json.
Do not alter jobs.json after generation; any change invalidates provenance and requires a fresh run.

## Quality discipline

- Work at target size or at a larger integer scale, then downsample with a high-quality filter.
- Avoid micro-details that disappear at phone scale.
- Test alpha edges on near-black and pale backgrounds.
- Inspect nine-slice behavior at 1x, 1.5x and 2x.
- Compare states side by side; byte-identical or near-identical variants are defects.
- Never claim final quality based on successful export alone.

## Independent review

Reject clipped borders, broken perspective, random ornamental noise, unclear silhouettes,
low-contrast slots, text artifacts, alpha fringing, inconsistent light direction, incompatible
states and over-detailed art that collapses at phone scale.

A pack can be promoted only after the structural validator, repository HUD layout checker, Rust
loader tests, actual landscape/portrait engine screenshots and human art review all pass.

# Agent production contract

You are one worker in a professional RTS game-art production pipeline. Your job is to deliver original, production-ready UI artwork—not drafts, mockups, placeholder glyphs or plausible-looking filler.

## Read first

1. `ART_DIRECTOR_BRIEF.md`
2. `STYLE_BIBLE.md`
3. `REFERENCE_BOARD.md`
3. `REVIEW_PROTOCOL.md`
4. The exact job record in `jobs.json`
5. The full manifest record for the asset

## For every job

- Read semantic meaning and engine role. Never infer meaning from filename alone if manifest metadata exists.
- Treat exact pixel dimensions, RGBA8, straight alpha, sRGB, nine-slice insets and transparent apertures as hard requirements.
- Produce one image only. Never output a contact sheet, multiple variants in one canvas, screenshot, mockup or text.
- Design for native-size readability first. Zoomed-in detail is secondary.
- Use the shared material grammar and upper-left key light.
- Icons must be individually authored silhouettes. Do not recycle one generic rune and recolour it.
- Frames must keep their aperture genuinely transparent. Keep ornament out of stretch zones.
- For state variants, preserve the base geometry and change only the specified state treatment.
- Keep all text, digits, shortcuts, prices and cooldown labels out of the image.
- Do not imitate or trace Blizzard retail artwork or any other third-party protected art.
- Return the exact job_id, seed, prompt hashes, model/checkpoint and all generation settings. Preserve source references and license declarations.
- Save output at the exact relative path in the job. Do not rename it.

## Self-critique before delivery

Inspect at 100%, 50% and 25%. Ask: Is the object instantly recognisable? Does its silhouette differ from sibling assets? Do materials have believable structure? Is the light coherent? Are there malformed edges, accidental tangencies, fake text, random noise or clipped ornament? Does it read on both light and dark backgrounds? Does the asset obey all alpha and stretch constraints?

If any answer is uncertain, regenerate or retouch. Do not claim "10/10" merely because the file has the right dimensions.

## Delivery

Write a provenance record for every output. Do not invent model metadata. Do not fabricate a reviewer report. The separate Art Director and Red Team agents must inspect the final PNG bytes independently. Any defect or score below 10 requires another iteration.

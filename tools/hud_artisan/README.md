# HUD Artisan — deterministic asset generator

HUD Artisan is an offline, reproducible asset-production tool for coding and art agents. It consumes
the canonical wc3rust HUD manifest and emits RGBA8 PNGs at exact declared dimensions. It does not
edit the source manifest or installed pack in place.

## Quick start

```sh
python3 tools/hud_artisan/generate.py \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --out build/hud-artisan/run-001 --theme classic-modern --seed 20261118
python3 tools/hud_artisan/validate.py --pack build/hud-artisan/run-001
```

Then run the existing layout/image QA and engine loader tests documented in the root README.

## Agent contract

1. Read the canonical manifest and current art before generation.
2. Treat asset names, file paths, dimensions, 9-slice insets, states and atlas metadata as contract.
3. Generate by semantic asset name, never by array position.
4. Use fixed seeds and retain the generation report.
5. Make icon silhouettes semantically distinct; do not fake a catalog by recoloring one glyph.
6. Review at native phone size, run the validator, existing pack checker, Rust loader tests and
   real landscape/portrait screenshot QA.
7. Keep source and output separate. Promote only after independent visual review.
8. Use clean-room artwork only; no extracted Blizzard assets, game screenshots, logos or fonts.

## Output

- `manifest.json`: copy of the input schema.
- `assets[].file`: RGBA8 PNGs at exact dimensions.
- `reports/generation.json`: seed, renderer version, source-manifest SHA and output hashes.
- `reports/validation.json`: structural and file validation report.

## Engineering boundary

The raster backend is pure Python standard library: deterministic gradients, scanline polygons,
bevelled metal frames, semantic glyph primitives, restrained material grain and alpha compositing.
It is an executable baseline and reproducible agent tool, not a claim of bespoke hand-painted art.
Hero portraits, detailed race-specific icons and other authored illustrations still need a dedicated
illustration pass. A successful generation/validation run proves file integrity and dimensions,
not visual quality or in-engine correctness. Never label a pack final until independent art review
and screenshot QA pass.

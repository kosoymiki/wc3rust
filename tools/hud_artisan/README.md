# HUD Artisan — deterministic asset generator

HUD Artisan is a reproducible, agent-callable **asset production tool**, not a prompt collection.
It consumes the project's canonical HUD manifest and writes dimension-correct RGBA PNGs for every
declared asset. It never edits the source manifest, Rust renderer, or installed pack in place.

## Quick start

```sh
python3 tools/hud_artisan/generate.py \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --out build/hud-artisan/run-001 \
  --theme classic-modern --seed 20261118
```

Then validate against the engine contract:

```sh
python3 tools/hud_pack_check.py \
  --manifest build/hud-artisan/run-001/manifest.json \
  --layout analysis/hudskin/pack-layout-2414x1080.json \
  --layout analysis/hudskin/pack-layout-1080x2414.json \
  --png analysis/hudskin/pack-2414x1080.png \
  --png analysis/hudskin/pack-1080x2414.png
```

The generator copies the input manifest and emits files at the exact relative paths in
`assets[].file`. Existing fonts and non-generated support files are copied from the source pack.
The canonical manifest is the schema authority; do not hand-invent a parallel manifest.

## Agent contract

Every coding/art agent must:

1. Read this README and the canonical manifest before generating.
2. Treat `name`, `file`, `w`, `h`, `insets`, `states`, and `atlas` as immutable contract
   fields unless an explicit manifest migration is part of the task.
3. Generate from semantic names and metadata, not array position.
4. Use a fixed seed; record it in the run report.
5. Never use trademarks, extracted Blizzard assets, game screenshots, or copied game artwork as
   source material. Create clean-room fantasy RTS artwork.
6. Run the generator, inspect the contact sheet, validate PNG dimensions and alpha, then run the
   engine layout checker and in-game screenshot QA.
7. Reject output that is merely flat geometry: panels need layered material, bevel, edge wear,
   controlled ornament and a dark readable content field. Icons need distinct silhouettes.
8. Keep source and output separate. Promote a run only after visual and technical review.

## Output

- `manifest.json`: unchanged schema copy.
- `<asset file paths>`: RGBA8 PNG, exact manifest dimensions.
- `reports/generation.json`: deterministic provenance, input manifest hash, seed, renderer version,
  generated files and SHA-256 hashes.
- `previews/contact-sheet.png`: overview for visual review.

## Renderer architecture

Pure Python standard library; no network, external model, or runtime dependency. The raster backend
uses deterministic gradient fields, scanline polygons, supersampled bevel-like layered shapes,
procedural stone/metal grain, alpha compositing and semantic glyph primitives. It is intentionally
offline and deterministic. This gives agents a reliable baseline and reproducible iteration; it is
not a substitute for bespoke hand-painted hero portraits or a human art director. Such authored
paintings can be imported into the same manifest paths after provenance and dimension checks.

## Important acceptance boundary

A successful generator run proves only file generation, hashes, alpha and dimensions. It does not
prove that the art is visually excellent, matches Warcraft III's exact retail interface, or passes
the engine's layout and screenshot QA. Never call a pack final until those independent gates pass.

# HUD Artisan 1.0

HUD Artisan is the provider-neutral art production pipeline for wc3rust. It compiles the canonical
HUD manifest into independent machine-readable jobs. Any coding/art agent or image model can
consume jobs.json, generate one image per job and return files in the drop directory.

## Install

Python 3.10+ is required. Planning and PNG structure checks use only the standard library.
Pillow is required for image normalization, pixel QA and contact sheets.

    python3 -m pip install -r tools/hud_artisan/requirements.txt
    python3 -m unittest discover -s tools/hud_artisan -p 'test_*.py' -v

## Workflow

First inspect the engine contract reference. It is explicitly NOT an invented runtime manifest:

    python3 tools/hud_artisan/generate.py profile --out build/hud-artisan/engine-profile.json

Compile one task per real manifest asset:

    python3 tools/hud_artisan/generate.py plan       --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json       --out build/hud-artisan/jobs --theme classic-modern --seed 20261118 --engine-contract

Agents read jobs.json and AGENT_BRIEF.md; each returns exactly one PNG under its declared relative path
in the drop folder. Import to a separate staging directory:

    python3 tools/hud_artisan/generate.py ingest       --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json       --drop build/hud-artisan/drop --out build/hud-artisan/pack

Run technical QA and build a human review sheet:

    python3 tools/hud_artisan/generate.py validate       --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json       --pack build/hud-artisan/pack --pixels
    python3 tools/hud_artisan/generate.py contact-sheet       --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json       --pack build/hud-artisan/pack --out build/hud-artisan/contact-sheet.png

Strict dimension matching is the default. Use contain/crop only when explicitly acceptable; non-uniform
stretch is never a default. Opaque icons are rejected unless overridden. Font files are dependencies
and are never fabricated or replaced with empty stubs.

## Verified engine contract

The profile records known constraints: design height 768; 2414x1080 landscape and 1080x2414 portrait
references; safe areas; 4x3 command card with 64 px cells and 4 px gap; ten 44 px control groups;
2048x2048 command-buttons atlas with twenty entries; and 24 unique asset names required by the Rust
skin/pack role contract and loader tests.

The current repository tree does not contain the referenced final asset manifest or its PNG/font
payload. The generator therefore requires a real manifest for planning and refuses to invent missing
asset paths, dimensions, nine-slice insets, fonts or the remaining asset inventory. This prevents a
false-success output that looks plausible but cannot load in the engine.

## Quality gates

Manifest checks: required top-level schema, unique names and paths, safe relative PNG paths, bounded
dimensions, finite non-negative insets, exactly five font file declarations and non-empty atlas cells.

PNG checks: signature, chunk bounds, every CRC, valid IHDR, exact dimensions, RGBA8, IDAT, IEND and
no trailing bytes. Pixel QA checks decodability, alpha bounds, sampled colour diversity and variance;
these heuristics catch blank/flat placeholders but do not replace art review.

Atlas checks: atlas file must resolve to an assets entry; all cells must be in bounds and non-overlapping.
Delivery checks: every image and all five fonts must exist; byte-identical duplicate images fail.
Runtime acceptance additionally requires tools/hud_pack_check.py, Rust loader tests, in-engine
landscape/portrait screenshots and human art review.

A structural PASS is not artistic approval. Inspect each image at native phone size and reject
low-quality model output even when its file contract passes.

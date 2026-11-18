# HUD Artisan 1.0

HUD Artisan is the provider-neutral art-production and quality-assurance pipeline for wc3rust.
It compiles the authoritative HUD manifest into independent generation jobs. Any image model,
human artist or coding agent can consume the jobs, deliver PNG files, and return them through a
strict, provenance-verified import stage.

## Install

Python 3.10+ is required. Planning and PNG structure validation use the standard library.
Pillow is required for normalization, pixel inspection and contact sheets.

~~~sh
python3 -m pip install -r tools/hud_artisan/requirements.txt
python3 -m unittest discover -s tools/hud_artisan -p 'test_*.py' -v
~~~

## End-to-end pipeline

First write the verified engine contract reference. It is explicitly not an invented asset manifest.

~~~sh
python3 tools/hud_artisan/generate.py profile \
  --out build/hud-artisan/engine-profile.json
~~~

Compile jobs from the actual pack manifest. The 24 role guard is optional when working on a subset.

~~~sh
python3 tools/hud_artisan/generate.py plan \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --out build/hud-artisan/jobs \
  --theme classic-modern --seed 20261118 --engine-contract
~~~

The plan is deterministic for a fixed manifest, theme and seed. Individual prompts include the
semantic object (not just an asset filename), exact target dimensions, visual family, UI state,
palette, material system, safe 9-slice insets, alpha requirements and negative prompt. The visual
style rules live in STYLE_BIBLE.md.

### Run a local or hosted image provider through an adapter

Configure adapter.example.json with your provider CLI. The dispatcher does not use a shell string;
it executes an argv list with explicit placeholders and an environment-variable allowlist:

    python3 tools/hud_artisan/generate.py dispatch \
      --jobs build/hud-artisan/jobs/jobs.json \
      --adapter tools/hud_artisan/adapter.example.json \
      --drop build/hud-artisan/drop --parallel 2

The adapter writes one image per job. The dispatcher enforces exact dimensions and RGBA8 PNG,
uses bounded concurrency, retries and per-job timeouts, and writes worker logs. API secrets are
passed only through explicitly allowlisted environment variable names; they are not copied into
job files or reports. Each successful job gets an auditable provenance record. See
PROVIDER_PROTOCOL.md and provenance.schema.json.

Agents read jobs.json and AGENT_BRIEF.md and write one PNG per job into the drop directory.
The atlas is not a single generative image: its individual cells are separate jobs or explicitly
mapped source assets, then HUD Artisan composites the command-buttons atlas using exact manifest
coordinates. This avoids model-created grids, inconsistent cell sizes and hidden overlaps.

Import outputs into an isolated staging pack. The plan file is mandatory; it binds generated outputs
to the exact manifest SHA, job IDs and prompts.

~~~sh
python3 tools/hud_artisan/generate.py ingest \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --jobs build/hud-artisan/jobs/jobs.json \
  --drop build/hud-artisan/drop \
  --out build/hud-artisan/pack
~~~

Each job requires provenance in drop/provenance.json. The importer checks the job set, provider,
model, generation timestamp, license declaration, prompt SHA-256 and generated PNG SHA-256 before
importing. This makes the output auditable and detects stale or substituted files.

Run structural and pixel QA:

~~~sh
python3 tools/hud_artisan/generate.py validate \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack --pixels
python3 tools/hud_artisan/generate.py contact-sheet \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack \
  --out build/hud-artisan/contact-sheet.png
~~~

Strict dimensions are the default. Use contain/crop only when explicitly acceptable. Non-uniform
stretch is never implicit. Opaque icons are rejected unless overridden. Five font files are runtime
dependencies; they are copied only when supplied and are never replaced with empty stubs.

## Verified engine contract

engine_profile.json records known constraints: 768 design height; 2414x1080 landscape and
1080x2414 portrait references; safe areas; a 4x3 command card with 64 px cells and 4 px gap;
ten 44 px control groups; a 2048x2048 command-buttons atlas with 20 entries; and the 24 unique
asset names required by PACK_ROLE_ASSETS and the pack loader tests.

The current repository tree does not contain the referenced final pack manifest or its PNG/font
payload. HUD Artisan therefore requires a real manifest for generation and refuses to guess the
missing filenames, dimensions, 9-slice insets, font binaries or complete 92-asset inventory. This
is intentional: a plausible-looking fabricated manifest is not an engine-compatible pack.

## Quality gates

- Manifest: required schema, unique names/paths, safe relative PNG paths, bounded dimensions,
  finite non-negative insets, exactly five font declarations and non-empty atlas cell records.
- PNG: signature, chunk bounds, CRCs, IHDR, exact dimensions, RGBA8, IDAT, IEND and no trailing data.
- Pixel QA: full decode, non-empty alpha bounds, sampled colour diversity and image variance.
  Heuristics catch blank or flat placeholders; they do not declare art beautiful.
- Atlas: atlas image resolves to a manifest asset; cells are in bounds and do not overlap.
- Provenance: every image job must match a job ID, prompt hash and output hash.
- Delivery: every declared PNG, atlas and all five font dependencies must exist; byte-identical
  assets fail.
- Runtime acceptance: also run tools/hud_pack_check.py, Rust loader tests, actual landscape and
  portrait screenshots, and human art review.

A structural PASS is not artistic approval. Inspect each image at native phone size and reject
low-quality model output even when its file contract passes.

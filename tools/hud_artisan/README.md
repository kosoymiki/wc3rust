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

Compile jobs from the actual pack manifest. The strict engine-contract option checks the full
92-asset count, all 24 runtime roles, design height, reference resolutions, safe areas, card/control
metrics, font count and command-buttons atlas requirements. Omit it only for test fixtures or subsets.

~~~sh
python3 tools/hud_artisan/generate.py plan \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --out build/hud-artisan/jobs \
  --theme classic-modern --seed 20261118 --engine-contract
~~~

The plan is deterministic for a fixed manifest, theme and seed. Every prompt describes the semantic
object (not merely the filename), target dimensions, visual family, UI state, palette, materials,
safe nine-slice insets, alpha requirements and negative prompt. Every job carries a prompt SHA-256;
its stable job ID is derived from that hash. Read STYLE_BIBLE.md for the shared art-direction grammar.

### Run a local or hosted image provider through an adapter

Copy adapter.example.json to build/hud-artisan/adapter.json and configure its provider CLI argv,
working directory, model, license, timeouts and environment-variable allowlist. The example points
to a placeholder provider_adapter.py that you supply; no provider credentials, SDK or model weights
are bundled. The dispatcher does not use a shell string:

~~~sh
python3 tools/hud_artisan/generate.py dispatch \
  --jobs build/hud-artisan/jobs/jobs.json \
  --adapter build/hud-artisan/adapter.json \
  --drop build/hud-artisan/drop --parallel 2
~~~

The adapter writes one image per job. The dispatcher enforces exact dimensions and RGBA8 PNG,
uses bounded concurrency, retries and per-job timeouts, and writes worker logs. API secrets are
passed only through explicitly allowlisted environment variable names; secret values are redacted
from worker logs. Each successful run writes provenance including the hash of the exact jobs.json,
so edits to prompts, atlas coordinates or job metadata invalidate the run.

Agents read jobs.json and AGENT_BRIEF.md and write one PNG per job into the drop directory.
The command-buttons atlas is assembled from individual cell jobs or explicitly mapped asset outputs,
using exact manifest coordinates. The pipeline never trusts a model-created sprite-sheet grid.

### Ingest and validate

~~~sh
python3 tools/hud_artisan/generate.py ingest \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --jobs build/hud-artisan/jobs/jobs.json \
  --drop build/hud-artisan/drop \
  --out build/hud-artisan/pack

python3 tools/hud_artisan/generate.py validate \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack --pixels --engine-contract

python3 tools/hud_artisan/generate.py contact-sheet \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack \
  --out build/hud-artisan/contact-sheet.png
~~~

The plan file is mandatory. It binds each job ID to its exact prompt hash and the complete source
manifest hash. Ingestion checks the exact plan SHA-256, complete job set, provider, model, generation
timestamp, license declaration, prompt SHA-256 and generated PNG SHA-256 before importing.
The staging output directory must be empty; the importer will not overwrite an existing pack.

Strict dimensions are the default. Use contain/crop only when explicitly acceptable. Non-uniform
stretch is never implicit. Opaque icons are rejected unless overridden. Five font files are runtime
dependencies; they are copied only when supplied and are never replaced with empty stubs.

## Verified engine contract

engine_profile.json and the code-level strict gate record known constraints: 768 design height;
2414x1080 landscape and 1080x2414 portrait references; safe areas; a 4x3 command card with 64 px
cells and 4 px gap; ten 44 px control groups; a 2048x2048 command-buttons atlas with 20 entries;
five font files; 24 unique runtime asset roles; and 92 total manifest assets.

The current repository tree does not contain the referenced final pack manifest or its PNG/font
payload. HUD Artisan therefore requires a real manifest for generation and refuses to guess missing
filenames, dimensions, nine-slice insets, font binaries or the full asset inventory. This prevents
a plausible-looking but engine-incompatible pack.

## Quality gates

- Manifest: required schema, unique names/paths, safe relative PNG paths, bounded dimensions,
  finite non-negative insets, five safe font filenames and non-empty atlas cells.
- PNG: signature, chunk bounds, CRCs, IHDR, exact dimensions, RGBA8, IDAT, IEND and no trailing data.
- Pixel QA: full decode, non-empty alpha bounds, sampled colour diversity and image variance.
  These heuristics catch blank/flat placeholders; they do not declare art beautiful.
- Atlas: declared texture exists; cells are in bounds and non-overlapping before generation and ingest.
- Provenance: exact plan hash, complete job set, prompt hash and output hash must match.
- Delivery: every declared PNG and all five font dependencies must exist; byte-identical assets fail.
- Runtime acceptance: also run tools/hud_pack_check.py, Rust loader tests, real landscape/portrait
  screenshots, and human art review.

A structural PASS is not artistic approval. Inspect every image at native phone size and reject
low-quality model output even when its file contract passes.

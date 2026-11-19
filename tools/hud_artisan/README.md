# HUD Artisan 1.1.0

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
metrics, font count and command-buttons atlas requirements. The manifest reference resolution is 1920×1080 / 1080×1920; 2414×1080 is a runtime viewport QA target scaled from the 768 design height, not the reference resolution. Omit it only for test fixtures or subsets.

~~~sh
python3 tools/hud_artisan/generate.py plan \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --out build/hud-artisan/jobs \
  --theme classic-modern --seed 20261118 --engine-contract
~~~

The plan is deterministic for a fixed manifest, theme and seed. Every prompt describes the semantic
object (not merely the filename), target dimensions, visual family, UI state, palette, materials,
safe nine-slice insets, alpha requirements and negative prompt. Every job carries a prompt SHA-256;
its stable job ID is derived from that hash. Every asset also receives its own deterministic 32-bit seed, derived from the requested batch seed and asset name, and stored explicitly in the job and provenance. Read STYLE_BIBLE.md for the shared art-direction grammar. For OpenCode 2 role separation and session prompts, see OPENCODE_WORKFLOW.md. For accepted visual references and master asset briefs, see REFERENCE_BOARD.md. For the external donor survey, research basis and licensing cautions, see RESEARCH_DONORS.md.

### Optional local HTTP renderer adapter

HUD Artisan includes an optional adapter for a locally running ComfyUI-compatible HTTP server. It is not required, and it is not a hosted service. No Comfy Cloud, paid endpoint, subscription or API key is part of the pipeline. If you use another local renderer, OpenCode agent workflow or custom CLI, skip this section and use the provider-neutral adapter contract. Export a local workflow in API format and copy the examples into a private build directory:

~~~sh
mkdir -p build/hud-artisan
cp tools/hud_artisan/adapter.example.json build/hud-artisan/adapter.json
cp tools/hud_artisan/comfyui.example.json build/hud-artisan/comfyui.json
# Save your exported ComfyUI API workflow as:
# build/hud-artisan/comfyui_api_workflow.json
~~~

Edit comfyui.json: point "workflow_file" at the exported API JSON and map the five inputs to your
actual workflow node IDs (positive_prompt, negative_prompt, width, height, seed). Set
"output_node_id" to the final SaveImage node. The example IDs are placeholders, not a working
workflow. Keep private workflow/checkpoint files out of Git.

Set COMFYUI_URL when the server is not http://127.0.0.1:8188, then run:

~~~sh
python3 tools/hud_artisan/generate.py dispatch \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --jobs build/hud-artisan/jobs/jobs.json \
  --adapter build/hud-artisan/adapter.json \
  --drop build/hud-artisan/drop --parallel 1
~~~

The optional local adapter injects exact prompts, dimensions and the compiled per-asset seed, checks the SaveImage node, waits for the matching job and downloads its output. It rejects invalid PNGs, wrong dimensions and insufficient transparency. See COMFYUI.md and comfyui.schema.json. OpenCode 2 remains the agent/review layer; it is not coupled to this renderer.

A custom CLI/API adapter is also supported: command is an argv array, never a shell string, with
{job_json}, {output_path} and {job_id} placeholders. Bounded concurrency, retries, timeouts,
allowlisted environment variables and secret-redacted worker logs are enforced by the dispatcher.
For interrupted long batches, rerun dispatch with --resume. A checkpoint is atomically written after each completed job. It skips only outputs whose PNG
hash, dimensions, job fingerprint and exact adapter/local-dependency fingerprint match the previous worker
checkpoint. Changed jobs/configuration or corrupted/missing PNGs are regenerated. Provenance is
reissued only after every job succeeds; a failed batch never retains a stale success certificate.

No provider credentials, model weights or SDK are bundled.

Agents read jobs.json and AGENT_TASK.md and write one PNG per job into the drop directory.
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
timestamp, license declaration, per-asset seed, positive/negative prompt SHA-256 and generated PNG
SHA-256 before importing. The destination is built in a sibling temporary directory and atomically
renamed only when every asset, atlas cell and font dependency succeeds; failed imports are rolled back.
The staging output directory must be empty; the importer will not overwrite an existing pack.

Strict dimensions are the default. Use contain/crop only when explicitly acceptable. Non-uniform
stretch is never implicit. Opaque silhouettes (icons/cursors) and filled portrait/minimap apertures are rejected unless an
explicit override is requested. Five font files are mandatory runtime dependencies; they must be
supplied and are never replaced with empty stubs.

## Art direction and release-quality gate

The visual target is premium authored RTS artwork. Read `ART_DIRECTOR_BRIEF.md` before assigning any job; `AGENT_TASK.md` is the worker's mandatory contract. The goal is an original wc3rust identity with high-end material craft and game-scale legibility, not a copy of Blizzard retail assets.

After ingestion and structural validation, generate contact sheets and run two separate vision-capable OpenCode sessions: one Art Director and one Red Team Reviewer. Schema v2 records the real provider/model/session IDs, exact review prompt hashes, immutable raw response files and hashes. Both reviewers must inspect the exact final PNG bytes and report all ten criteria in `review.schema.json`. The gate requires distinct reviewer sessions and model identifiers, valid prompt/raw-response hashes, 10/10 on every criterion from both reviewers, no blockers, no suggested improvements, confidence >=0.95, and matching SHA-256 for every reviewed image. This is auditable evidence, not cryptographic proof that the sessions were truly independent.

```sh
python3 tools/hud_artisan/review_gate.py \\
  --manifest path/to/manifest.json \\
  --jobs build/hud-artisan/jobs/jobs.json \\
  --pack build/hud-artisan/pack \\
  --reviews build/hud-artisan/reviews.json \\
  --out build/hud-artisan/review-report.json
```

A changed PNG invalidates its prior review. Any score below 10 triggers a targeted regeneration/retouch loop and a fresh review by both agents. A machine PASS means only that the evidence meets the declared gate; it does not prove artistic excellence. Final release still requires human art-director sign-off, runtime pack checks, Rust loader tests and real landscape/portrait screenshots. Full protocol: `REVIEW_PROTOCOL.md`.

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
- Pixel QA: full decode, non-empty alpha bounds, sampled colour diversity and image variance; alpha-required silhouette checks and >=97% transparent centre aperture for portrait/minimap frames.
  These heuristics catch blank/flat placeholders; they do not declare art beautiful.
- Atlas: declared texture exists; cells are in bounds and non-overlapping before generation and ingest.
- Provenance: exact plan hash, complete job set, prompt hash and output hash must match.
- Delivery: every declared PNG and all five font dependencies must exist; byte-identical assets fail.
- Runtime acceptance: also run tools/hud_pack_check.py, Rust loader tests, real landscape/portrait
  screenshots, and human art review.

A structural PASS is not artistic approval. Inspect every image at native phone size and reject
low-quality model output even when its file contract passes.

# HUD Artisan 1.2.0

HUD Artisan is the provider-neutral art-production and quality-assurance pipeline for wc3rust.
It compiles the authoritative HUD manifest into independent generation jobs. Any image model,
human artist or coding agent can consume the jobs, deliver PNG files, and return them through a
strict, provenance-verified import stage.

## Install

Python 3.10+ is required. The working Termux ComfyUI host was observed at ComfyUI 0.46.0 / Python 3.14.10 / PyTorch 2.15.0; generator CI remains tested on Python 3.10–3.12. Planning and PNG structure validation use the standard library.
Pillow is required for normalization, pixel inspection and contact sheets.

~~~sh
python3 -m pip install -r tools/hud_artisan/requirements.txt
python3 -m unittest discover -s tools/hud_artisan -p 'test_*.py' -v
~~~

## End-to-end pipeline

### Batch visual QA, state consistency and BLP compatibility

Run the production QA after ingest/validation. It creates three contact sheets (dark, light and checkerboard transparency backgrounds); every asset is shown at native, 50% and 25% size. It also groups explicit UI state suffixes (normal/pressed/disabled/autocast/active/inactive/hover/selected) and measures silhouette IoU plus visible-image difference.

~~~sh
python3 tools/hud_artisan/production_qa.py \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack \
  --jobs build/hud-artisan/jobs/jobs.json \
  --out build/hud-artisan/production-qa
~~~

Outputs:
- `production-qa.json`: per-asset hashes, state-pair metrics, read errors and regeneration candidates.
- `contact-sheets/contact-dark.png`, `contact-light.png`, `contact-checker.png`: visual review surfaces.
- Candidate regeneration entries include the exact compiled job where available. They are deliberately marked non-dispatchable: the current dispatch/import provenance contract requires a complete batch, and silently dispatching a partial batch would produce an incomplete pack. Review the candidate and use a new full manifest/plan or the forthcoming provenance-aware partial-batch workflow.

The state metrics are triage heuristics, not an artistic verdict. A pressed state may legitimately alter its silhouette, and two states can differ in color while retaining the same shape. Adjust thresholds only after inspecting the contact sheets.

### BLP1/BLP2 codec harness

`blp_harness.py inspect --input file.blp` validates BLP1/BLP2 headers, dimensions and mipmap bounds. The round-trip command invokes a locally installed encoder and decoder through explicit argv arrays (no shell), validates the BLP header, and measures RGBA RMSE, PSNR and alpha-silhouette IoU:

~~~sh
python3 tools/hud_artisan/blp_harness.py roundtrip \
  --input build/hud-artisan/pack/icons/example.png \
  --work build/hud-artisan/blp-roundtrip \
  --encode-arg /path/to/local-blp-encoder \
  --encode-arg --input --encode-arg {input} \
  --encode-arg --output --encode-arg {output} \
  --decode-arg /path/to/local-blp-decoder \
  --decode-arg --input --decode-arg {input} \
  --decode-arg --output --decode-arg {output} \
  --report build/hud-artisan/blp-roundtrip/report.json
~~~

The harness does not bundle or claim to implement a BLP codec. The exact flags above are placeholders: use the syntax supported by the installed converter. BLP validation and PNG round-trip quality are separate from proving that Warcraft III accepts a complete texture/atlas/FDF package in-game.

### Renderer provenance lock

When an adapter config declares `workflow_file`, dispatch verifies the workflow SHA-256 before starting workers. Successful provenance records the workflow filename/hash/format, model-loader selections, sampler nodes, configured node map/output node, optional custom-node lock metadata, and hashes of explicitly listed `model_files`. To hash large model files, list their local paths in `model_files`; do not store credentials or environment secrets in the runtime metadata.


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

The checked-in final manifest is normalized with explicit source_asset links for all 20 command-button atlas cells, so the planner produces 91 unique asset jobs and then assembles the 2048×2048 atlas. The 92 PNG assets and five font files are still in the local Termux pack and are not yet mirrored into this branch; the manifest-only CI test does not claim that the full pack is present. The plan is deterministic for a fixed manifest, theme and seed. Every prompt describes the semantic
object (not merely the filename), target dimensions, visual family, UI state, palette, materials,
safe nine-slice insets, alpha requirements and negative prompt. Every job carries a prompt SHA-256;
its stable job ID is derived from that hash. Every asset also receives its own deterministic 32-bit seed, derived from the requested batch seed and asset name, and stored explicitly in the job and provenance. Read STYLE_BIBLE.md for the shared art-direction grammar. For OpenCode 2 role separation and session prompts, see OPENCODE_WORKFLOW.md. For accepted visual references and master asset briefs, see REFERENCE_BOARD.md. For the complete donor/implementation decision matrix, see DONOR_MATRIX.md; for the research narrative, see RESEARCH_DONORS.md. External inputs and donor licensing are tracked in reference_ledger.json (template: reference_ledger.example.json) against reference_ledger.schema.json. The ledger rejects unknown/proprietary pixel-conditioning use and records code/model/font licenses separately.

### Optional local HTTP renderer adapter

HUD Artisan includes a production adapter for a locally running ComfyUI-compatible HTTP server. It is a first-class local renderer path, not a hosted service. The adapter accepts both ComfyUI API prompt JSON and the ordinary saved canvas workflow JSON (workflow_format: ui), converting the latter against the running server's /object_info schema. This means an existing working Termux workflow can be used directly without hand-exporting or hand-rebuilding an API graph. No Comfy Cloud, paid endpoint, subscription or API key is part of the pipeline. If you use another local renderer, OpenCode agent workflow or custom CLI, skip this section and use the provider-neutral adapter contract. Copy the adapter profiles into a private build directory. For the existing Termux Z-Image Turbo workflow, start from comfyui.termux.example.json and adapter.termux.example.json:

~~~sh
mkdir -p build/hud-artisan
cp tools/hud_artisan/adapter.example.json build/hud-artisan/adapter.json
cp tools/hud_artisan/comfyui.termux.example.json build/hud-artisan/comfyui.json
cp tools/hud_artisan/adapter.termux.example.json build/hud-artisan/adapter.json
# The Termux profile reads ~/src/comfyui-termux/default_workflow.json directly.
~~~

The Termux profile is already mapped to the discovered Z-Image canvas workflow: positive prompt node 67, negative prompt 71, latent width/height node 68, seed node 70 and SaveImage node 9. It reads ~/src/comfyui-termux/default_workflow.json; change only that path if your workflow lives elsewhere. For other workflows, update the five node mappings and output node. Keep private workflow/checkpoint files out of Git.

Set COMFYUI_URL when the server is not http://127.0.0.1:8188, then run:

~~~sh
python3 tools/hud_artisan/comfyui_doctor.py \
  --config build/hud-artisan/comfyui.json
~~~

The read-only doctor checks the live ComfyUI version/runtime, queue, workflow conversion, required node classes, model-loader selections where enumerated, all five input mappings and SaveImage target. It never submits a generation job.

~~~sh
python3 tools/hud_artisan/generate.py dispatch \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --jobs build/hud-artisan/jobs/jobs.json \
  --adapter build/hud-artisan/adapter.json \
  --drop build/hud-artisan/drop --parallel 1
~~~

The local adapter injects exact prompts, model-compatible render dimensions and the compiled per-asset seed, checks the SaveImage node, waits for the matching job and downloads its output. Small HUD assets are rendered on a larger 16-pixel-aligned latent canvas and Lanczos-downsampled to exact native dimensions. Alpha-required assets request a uniform magenta background; the adapter removes only border-connected chroma-key regions, cuts the declared frame aperture and rejects insufficient transparency. This is a deterministic matte, not a semantic segmentation model; visible magenta contamination remains a QA rejection. See COMFYUI.md and comfyui.schema.json. The Termux profile runs sequentially (max_parallel: 1) to avoid CPU/RAM contention. OpenCode 2 remains the agent/review layer; it is not coupled to this renderer.

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
  --out build/hud-artisan/pack \
  --font-source analysis/hudskin/final/wc3rust-hud-final/fonts

python3 tools/hud_artisan/generate.py validate \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack --pixels --engine-contract

python3 tools/hud_artisan/generate.py contact-sheet \
  --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
  --pack build/hud-artisan/pack \
  --out build/hud-artisan/contact-sheet.png
~~~

Run the independent read-only alpha/chroma-key diagnostic after ingestion:

~~~sh
python3 tools/hud_artisan/alpha_qa.py \
  --root build/hud-artisan/pack \
  --out build/hud-artisan/alpha-report.json \
  --max-key-ratio 0.01

# Optional strict gate for an all-alpha asset subset/drop:
python3 tools/hud_artisan/alpha_qa.py \
  --root build/hud-artisan/drop \
  --out build/hud-artisan/drop-alpha-report.json \
  --require-alpha --strict
~~~

The alpha report records each PNG's SHA-256, dimensions, alpha histogram ratios,
visible bounding box, transparent-border ratio and opaque pixels close to the configured
chroma key. It is read-only and does not repair artwork. The chroma-key metric is a warning
heuristic: legitimate magenta artwork can be flagged, and a low score does not prove a
clean matte. Use `--require-alpha` only on a subset whose contract requires transparency;
do not apply it indiscriminately to opaque panels or atlas textures. A fully framed asset
with an intentionally transparent centre may be reported as a transparent island; review
that finding rather than treating it as an automatic rejection.

The plan file is mandatory. It binds each job ID to its exact prompt hash and the complete source
manifest hash. Ingestion checks the exact plan SHA-256, complete job set, provider, model, generation
timestamp, license declaration, per-asset seed, positive/negative prompt SHA-256 and generated PNG
SHA-256 before importing. The destination is built in a sibling temporary directory and atomically
renamed only when every asset, atlas cell and font dependency succeeds; failed imports are rolled back.
The staging output directory must be empty; the importer will not overwrite an existing pack.

Strict dimensions are the default. Use contain/crop only when explicitly acceptable. Non-uniform
stretch is never implicit. Opaque silhouettes (icons/cursors) and filled portrait/minimap apertures are rejected unless an
explicit override is requested. Five font files are mandatory runtime dependencies; they must be supplied and are never replaced with empty stubs. `--font-source` lets ingestion copy them directly from the original local pack, so they do not need to be staged inside the ComfyUI drop directory.

## Art direction and release-quality gate

The visual target is premium authored RTS artwork. Read `ART_DIRECTOR_BRIEF.md` before assigning any job; `AGENT_TASK.md` is the worker's mandatory contract. The goal is an original wc3rust identity with high-end material craft and game-scale legibility, not a copy of Blizzard retail assets.

After ingestion and structural validation, generate contact sheets and run two separate vision-capable OpenCode sessions: one Art Director and one Red Team Reviewer. Schema v2 records the real provider/model/session IDs, exact review prompt hashes, immutable raw response files and hashes. Both reviewers must inspect the exact final PNG bytes and report all ten criteria in `review.schema.json`. The gate requires distinct reviewer sessions and model identifiers, valid prompt/raw-response hashes, average >=8.5/10 per reviewer, no criterion below 7/10, no blockers, confidence >=0.80; non-blocking improvements are retained in the report, and matching SHA-256 for every reviewed image. This is auditable evidence, not cryptographic proof that the sessions were truly independent.

```sh
python3 tools/hud_artisan/review_gate.py \\
  --manifest path/to/manifest.json \\
  --jobs build/hud-artisan/jobs/jobs.json \\
  --pack build/hud-artisan/pack \\
  --reviews build/hud-artisan/reviews.json \\
  --out build/hud-artisan/review-report.json
```

A changed PNG invalidates its prior review. A reviewer average below 8.5, any criterion below 7, a blocker or a reject verdict triggers a targeted regeneration/retouch loop and a fresh review by both agents. Non-blocking improvements remain visible in the report and do not force dishonest perfect scores. A machine PASS means only that the evidence meets the declared gate; it does not prove artistic excellence. Final release still requires human art-director sign-off, runtime pack checks, Rust loader tests and real landscape/portrait screenshots. Full protocol: `REVIEW_PROTOCOL.md`.

## Verified engine contract

engine_profile.json and the code-level strict gate record known constraints: 768 design height;
1920x1080 landscape and 1080x1920 portrait reference resolutions; 2414x1080 is a separate landscape runtime QA viewport; safe areas; a 4x3 command card with 64 px
cells and 4 px gap; ten 44 px control groups; a 2048x2048 command-buttons atlas with 20 entries;
five font files; 24 unique runtime asset roles; and 92 total manifest assets.

The normalized final pack manifest is checked in at analysis/hudskin/final/wc3rust-hud-final/manifest.json. The 92 PNG files and five font binaries remain in the local Termux pack and are not mirrored into this branch. HUD Artisan refuses to guess missing binary payloads, font licenses or unverified runtime behavior. The manifest-only CI test verifies planning metadata; it does not claim that the full visual pack is present.

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

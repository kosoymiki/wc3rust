# ComfyUI adapter setup

This adapter talks to a local ComfyUI server using its HTTP API. It uses no model/provider SDK;
the user supplies an API-format workflow JSON exported from ComfyUI.

1. Copy comfyui.example.json to a private build directory.
2. Export your workflow using ComfyUI's API format (not the UI workflow format).
3. Set workflow_file to that JSON.
4. Map five nodes: positive prompt, negative prompt, latent width, latent height and seed.
5. Set output_node_id to the actual SaveImage node whose image is the final output. The adapter checks its class_type before submitting inference.
6. Set COMFYUI_URL if the server is not at 127.0.0.1:8188.

Example adapter.json for the generic dispatcher:

~~~json
{
  "name": "comfyui-local",
  "provider": "ComfyUI",
  "model": "your-checkpoint-name-and-revision",
  "license": "clean-room-generated",
  "working_directory": ".",
  "command": [
    "python3", "tools/hud_artisan/comfyui_adapter.py",
    "--config", "build/hud-artisan/comfyui.json",
    "--job-json", "{job_json}",
    "--output", "{output_path}"
  ],
  "environment": ["COMFYUI_URL"],
  "timeout_seconds": 1900,
  "retries": 1,
  "max_parallel": 1,
  "references": [],
  "postprocessing": []
}
~~~

The adapter uses the deterministic per-asset seed already compiled into jobs.json, injects exact
prompts/dimensions, validates the configured SaveImage node before submitting, polls the ComfyUI
history endpoint, downloads that node's output and atomically writes PNG RGBA8. It refuses
wrong dimensions rather than stretching, and refuses opaque output for jobs that require alpha.

The workflow must produce transparent backgrounds for icon/cursor jobs and a transparent central
aperture (>=97% alpha-zero samples in the centre window) for portrait/minimap frames (for example via a compatible transparent-background node chain). Standard opaque Stable Diffusion workflows do not satisfy this
contract. Export separate workflow variants or use a provider adapter that supports alpha for those
jobs. Never remove the background with an undocumented or unlicensed external service.

The node IDs in comfyui.example.json are illustrative only; they must be replaced with IDs and inputs
from your own exported API workflow. Keep the workflow file and model checkpoint outside the public
repository if they contain private paths or licensed/private models.


## GitHub Actions CPU rendering: slow draft first

The workflow `.github/workflows/hud-artisan-comfyui.yml` runs ComfyUI on a standard
GitHub-hosted CPU runner (`ubuntu-latest`) with CPU-only PyTorch and
`python main.py --cpu`. It does not require a GPU or a self-hosted machine.

### Draft mode (default)

The workflow defaults to `scope=draft`. It uses
`tools/hud_artisan/draft/manifest.json`, a deliberately reduced but schema-valid
manifest that compiles to exactly one real generation job:

- `menu_panel_normal_512x384`, 512×384 RGBA PNG;
- one atlas mapping retained so the planner exercises the real manifest/atlas path;
- all five runtime font filenames retained in the manifest contract;
- no production ingest or 92-asset release claim is made.

Draft mode renders one panel, then checks the exact dimensions, decodability,
non-empty alpha, colour variance and colour diversity. It creates
`draft-preview.png` over a checkerboard and `draft-qa.json`. These are structural
checks only; they do not establish that the artwork is beautiful or ready to ship.
The raw PNG, job plan, worker provenance, preview and QA report are uploaded as a
14-day Actions artifact.

Run Actions → **HUD Artisan — ComfyUI CPU render** → Run workflow and leave
`scope=draft`. The default theme and seed are suitable for the first pilot.

### Full mode

Only select `scope=full` after a draft has actually completed and been visually
reviewed. Full mode uses the production manifest path supplied in the workflow
input, enables the strict 92-asset engine contract, renders serially, ingests the
complete pack, validates it and creates the production contact sheet. The production
manifest, five font binaries and all runtime assets must exist on the selected branch
before this mode can pass.

### Required Actions secrets

- `HUD_COMFYUI_WORKFLOW_B64`: base64-encoded ComfyUI API-format workflow JSON;
- `HUD_COMFYUI_CONFIG_B64`: base64-encoded adapter config JSON;
- `HUD_COMFYUI_MODEL_URL`: direct HTTPS download URL for the checkpoint;
- `HUD_COMFYUI_MODEL_SHA256`: exact SHA-256 digest of the checkpoint;
- `HUD_COMFYUI_MODEL`: model identifier/revision for provenance;
- `HUD_COMFYUI_LICENSE`: truthful model/output license declaration;
- `HF_TOKEN` (optional): token for a gated Hugging Face download;
- `COMFYUI_URL` (optional): defaults to the local runner API.

The downloaded checkpoint filename must match the checkpoint selected by the
private API workflow. The private workflow must expose the five adapter mappings:
positive prompt, negative prompt, latent width, latent height and seed, plus the
final `SaveImage` output node. Example node IDs are not a working workflow.
The workflow and model weights stay outside the public repository.

### Performance and evidence policy

CPU inference is intentionally slow. One draft job is the first measurement point;
do not infer the runtime of the complete 92-asset batch from ComfyUI startup alone.
The workflow timeout is six hours, but a timeout is not a promise that a full batch
will finish. No generated image or successful render is claimed until a workflow run
has completed and its artifact has been inspected.

The draft manifest is a generation fixture, not a replacement for the authoritative
production manifest. Draft mode deliberately skips pack ingestion because a one-asset
subset cannot satisfy the complete runtime/font/atlas release contract.

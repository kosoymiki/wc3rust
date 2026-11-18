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


## GitHub Actions GPU rendering

The repository includes `.github/workflows/hud-artisan-comfyui.yml`. It runs the same
HUD Artisan planner/dispatcher/ingester in CI and sends every asset job to a real ComfyUI
HTTP API. It deliberately targets a **self-hosted Linux x64 GPU runner** with labels
`self-hosted`, `linux`, `x64`, `gpu`; ordinary GitHub-hosted runners are not treated
as GPU workers.

The runner must have:
- a supported NVIDIA GPU and working CUDA/PyTorch installation;
- ComfyUI installed and running locally on `http://127.0.0.1:8188` (or a reachable
  endpoint supplied through the `COMFYUI_URL` Actions secret);
- the checkpoint, VAE, custom nodes and other workflow dependencies already installed;
- enough free disk for model weights, temporary images and the generated pack.

Configure these repository Actions secrets:
- `HUD_COMFYUI_WORKFLOW_B64`: base64 of the ComfyUI **API-format** workflow JSON;
- `HUD_COMFYUI_CONFIG_B64`: base64 of the adapter config JSON from `comfyui.example.json`;
- `HUD_COMFYUI_MODEL`: exact checkpoint/model identifier and revision used by the workflow;
- `HUD_COMFYUI_LICENSE`: truthful license/provenance declaration for that model and its outputs;
- `COMFYUI_URL` (optional): ComfyUI API root; defaults to `http://127.0.0.1:8188`.

The workflow JSON and config are injected into the runner at job time and are not committed
to this public repository. The job creates a generic HUD Artisan adapter around the private
ComfyUI config, checks `/system_stats`, compiles deterministic jobs, dispatches them serially,
ingests the PNGs, runs pixel/engine validation and uploads the pack plus contact sheet as a
14-day Actions artifact. It does **not** fabricate Art Director or Red Team approvals; the
independent visual review gate remains a separate release requirement.

Run it from Actions → “HUD Artisan — ComfyUI GPU render” → Run workflow. Select the manifest,
theme and deterministic seed.

**Current repository blocker:** the production manifest path shown by the workflow is not
present in the current remote branch tree. The workflow intentionally stops before rendering
if that file is absent. The full five-font dependency set must also be available to ingestion.
This CI integration is committed, but it is not yet an executed render and does not imply that
any HUD images have been generated. Do not start a costly render until the authoritative
manifest and matching asset/font contract are committed.


## GitHub-hosted CPU-only rendering

The workflow uses `runs-on: ubuntu-latest` and installs ComfyUI plus CPU-only PyTorch
inside the ephemeral GitHub runner. It launches ComfyUI with `python main.py --cpu`.
No GPU, self-hosted runner, external machine or always-on ComfyUI server is required.

Required Actions secrets:
- `HUD_COMFYUI_WORKFLOW_B64`: base64-encoded ComfyUI API-format workflow JSON;
- `HUD_COMFYUI_CONFIG_B64`: base64-encoded adapter config JSON;
- `HUD_COMFYUI_MODEL_URL`: direct HTTPS download URL for the checkpoint file;
- `HUD_COMFYUI_MODEL_SHA256`: exact SHA-256 digest of that checkpoint;
- `HUD_COMFYUI_MODEL`: model identifier/revision for provenance;
- `HUD_COMFYUI_LICENSE`: truthful model/output license declaration;
- `HF_TOKEN` (optional): token for a gated Hugging Face download;
- `COMFYUI_URL` is optional; default is the local ComfyUI API.

The checkpoint is downloaded into the runner's ComfyUI checkpoint directory and its
SHA-256 is verified before ComfyUI starts. The API workflow must reference the exact
checkpoint filename. The generated model weights and private workflow are not committed
to the repository.

**Performance warning:** standard `ubuntu-latest` runners have finite CPU/RAM/disk and
ComfyUI explicitly documents `--cpu` as slow. A full 92-asset production render may exceed
the Actions job time limit. The first run should be treated as a measured pilot, not assumed
to finish the entire pack. The production manifest must exist in the selected branch before
the dispatch step; the currently documented default manifest path is not present in the
remote tree at the time this workflow was changed. Do not interpret successful ComfyUI
startup as successful asset generation.

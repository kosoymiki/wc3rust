# ComfyUI adapter setup

This adapter talks to a local ComfyUI server using its HTTP API. It uses no model/provider SDK;
the user supplies an API-format workflow JSON exported from ComfyUI.

1. Copy comfyui.example.json to a private build directory.
2. Export your workflow using ComfyUI's API format (not the UI workflow format).
3. Set workflow_file to that JSON.
4. Map five nodes: positive prompt, negative prompt, latent width, latent height and seed.
5. Set output_node_id to the SaveImage node whose image is the final output.
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

The adapter derives a stable per-job seed from job_id, injects exact prompts/dimensions, polls the
ComfyUI history endpoint, downloads the SaveImage output and atomically writes PNG RGBA8. It refuses
wrong dimensions rather than stretching, and refuses opaque output for jobs that require alpha.

The workflow must produce transparent backgrounds for icon/cursor jobs (for example via a compatible
transparent-background node chain). Standard opaque Stable Diffusion workflows do not satisfy this
contract. Export separate workflow variants or use a provider adapter that supports alpha for those
jobs. Never remove the background with an undocumented or unlicensed external service.

The node IDs in comfyui.example.json are illustrative only; they must be replaced with IDs and inputs
from your own exported API workflow. Keep the workflow file and model checkpoint outside the public
repository if they contain private paths or licensed/private models.

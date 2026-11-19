# Optional local HTTP renderer adapter

This document describes an optional adapter for a locally running ComfyUI-compatible server. It is not part of the required HUD Artisan architecture. HUD Artisan does not require Comfy Cloud, hosted rendering, API keys, subscriptions, a paid model endpoint or a specific image UI.

If you use a different local renderer, OpenCode agent workflow or a custom CLI, skip this adapter and use the provider-neutral CLI/agent contract in README.md and PROVIDER_PROTOCOL.md.

## Local-only setup

1. Export a workflow in ComfyUI API format.
2. Copy comfyui.example.json into a private build directory.
3. Set workflow_file to the exported JSON.
4. Map positive prompt, negative prompt, latent width, latent height and seed to the real workflow node IDs.
5. Set output_node_id to the final SaveImage node.
6. Run the local renderer and point COMFYUI_URL at its loopback endpoint if it is not the default.

Example config path:

~~~text
build/hud-artisan/comfyui.json
build/hud-artisan/comfyui_api_workflow.json
~~~

The built-in adapter is invoked through the generic dispatcher. Its command is an argv array; no shell string is constructed from prompt content.

## Execution contract

The adapter:
- sends the exact compiled positive/negative prompts, dimensions and per-asset seed;
- checks that the configured output node exists and is a SaveImage node;
- rejects server-side validation errors and execution errors;
- waits for the matching prompt history entry to complete;
- requires exactly one PNG from the selected output node;
- rejects unsafe output filenames/subfolders and non-PNG responses;
- decodes the image, checks dimensions and transparency requirements, then atomically writes RGBA8 PNG;
- never silently resizes or stretches an output.

The adapter is optional and replaceable. It does not own art direction, manifests, provenance, atlas assembly, visual review or engine compatibility. Add the workflow/config/checkpoint files to dependency_files (paths are relative to adapter.json unless absolute); their SHA-256 values are included in the resume fingerprint.

## Important limitations

- A workflow that emits an opaque PNG cannot satisfy an alpha-required icon/cursor job.
- Portrait/minimap frames require a genuinely transparent central aperture; fake black/blurred/checkerboard apertures are rejected.
- Node IDs in comfyui.example.json are examples, not a working workflow.
- A local server URL is a local transport address, not a hosted API subscription.
- The adapter cannot prove artistic quality. Run deterministic validation, inspect contact sheets and obtain independent vision review plus human sign-off.

## No hosted workflow

The repository intentionally does not include a GitHub Actions job that downloads model checkpoints or calls hosted rendering services. CI tests the generator and adapters with local fixtures and mocked transports; real image inference stays on the user's chosen local machine and renderer.

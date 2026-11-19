# Local ComfyUI production adapter

This is the production local-renderer path for HUD Artisan. It targets the already-running Termux ComfyUI server; it does not require Comfy Cloud, hosted rendering, API keys, subscriptions or a paid model endpoint.

If you use a different local renderer, OpenCode agent workflow or a custom CLI, skip this adapter and use the provider-neutral CLI/agent contract in README.md and PROVIDER_PROTOCOL.md.

## Local-only setup

1. Copy comfyui.termux.example.json and adapter.termux.example.json into build/hud-artisan.
2. The Termux profile reads ~/src/comfyui-termux/default_workflow.json directly as a saved canvas workflow (workflow_format=ui).
3. Its current node map is positive prompt 67, negative prompt 71, latent width/height 68, seed 70 and SaveImage 9.
4. The adapter converts the canvas graph into the API prompt format using the running server's /object_info response; no manual graph re-export is needed.
5. If the workflow changes, update the node IDs or set workflow_format=api after exporting an API prompt.
6. Run the dispatcher with max_parallel=1 and point COMFYUI_URL at the loopback endpoint if it is not the default.

Example config path:

~~~text
build/hud-artisan/comfyui.json
build/hud-artisan/comfyui_api_workflow.json
~~~

The built-in adapter is invoked through the generic dispatcher. Its command is an argv array; no shell string is constructed from prompt content.

## Read-only preflight

Before dispatching a batch, run `python3 tools/hud_artisan/comfyui_doctor.py --config build/hud-artisan/comfyui.json`. It verifies the live server, canvas-to-API conversion, node classes, model selections when the server exposes them, five mappings, SaveImage target and queue state. It performs GET requests only and never queues an image.

## Execution contract

The adapter:
- accepts saved canvas UI workflows or API prompt workflows, converting UI graphs from live /object_info schemas;
- sends exact compiled positive/negative prompts, model-aligned render dimensions and per-asset seed;
- upsamples tiny native HUD targets to a 16-aligned latent canvas, then downsamples with Lanczos to exact manifest dimensions;
- checks that the configured output node exists and is a SaveImage node;
- rejects server-side validation errors and execution errors;
- waits for the matching prompt history entry to complete;
- requires exactly one PNG from the selected output node;
- rejects unsafe output filenames/subfolders and non-PNG responses;
- decodes the image, optionally removes border-connected chroma-key background, cuts a transparent frame aperture, checks alpha requirements, then atomically writes native-size RGBA8 PNG;
- never silently resizes or stretches an output.

The adapter is optional and replaceable. It does not own art direction, manifests, provenance, atlas assembly, visual review or engine compatibility. Add the workflow/config/checkpoint files to dependency_files (paths are relative to adapter.json unless absolute); their SHA-256 values are included in the resume fingerprint.

## Important limitations

- A workflow that emits an opaque PNG cannot satisfy an alpha-required icon/cursor job without a matte. The Termux Z-Image profile requests solid #FF00FF and applies deterministic border-connected flood-fill; this is not semantic segmentation and must be visually inspected for magenta fringe/holes.
- Portrait/minimap frames require a genuinely transparent central aperture; fake black/blurred/checkerboard apertures are rejected.
- Node IDs in comfyui.example.json are examples, not a working workflow.
- A local server URL is a local transport address, not a hosted API subscription.
- The adapter cannot prove artistic quality. Run deterministic validation, inspect contact sheets and obtain independent vision review plus human sign-off.

## No hosted workflow

The repository intentionally does not include a GitHub Actions job that downloads model checkpoints or calls hosted rendering services. CI tests the generator and adapters with local fixtures and mocked transports; real image inference stays on the user's chosen local machine and renderer.

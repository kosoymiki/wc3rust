# Local ComfyUI production adapter

This is the production local-renderer path for HUD Artisan. It targets the already-running Termux ComfyUI server; it does not require Comfy Cloud, hosted rendering, API keys, subscriptions or a paid model endpoint.

If you use a different local renderer, OpenCode agent workflow or a custom CLI, skip this adapter and use the provider-neutral CLI/agent contract in README.md and PROVIDER_PROTOCOL.md.

## Local-only setup

1. Copy comfyui.termux.example.json and adapter.termux.example.json into build/hud-artisan.
2. The Termux profile reads ~/src/comfyui-termux/default_workflow.json directly as a saved canvas workflow (workflow_format=ui).
3. Its current node map is positive prompt 67, negative prompt 71, latent width/height 68, seed 70 and SaveImage 9.
4. The example profile pins the exact workflow bytes with workflow_sha256. The adapter and read-only doctor fail closed if the file changes; after an intentional workflow edit, inspect the new graph/model loaders and explicitly refresh the SHA-256 pin.
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

## Alpha strategies (adapter)

`alpha_strategy` in the ComfyUI config selects how an alpha-required job gets its matte:

- `none` (default): the workflow must already emit RGBA with transparency.
- `chroma_key`: removes border-connected key pixels (`chroma_key_color`, `chroma_key_tolerance`);
  needs a workflow that renders a solid key background (the Z-Image Termux profile).
- `u2net` (Termux-friendly): runs the u2net salient-object model through onnxruntime and uses
  the predicted mask as alpha. Works with any RGB workflow, including SD1.5 on CPU. Model path:
  `matte_model` in the config or `U2NET_MODEL` (default `~/src/comfyui-termux/models/u2net.onnx`).
  onnxruntime 1.31.1 ships an `android_aarch64` wheel; its cosmetic "Unsupported platform
  (android)" warning is silenced by a one-line validation patch, and the runtime itself works
  (u2net 320x320 inference ~1.1 s on CPU).

Portrait/minimap frames additionally get the central aperture enforced. The u2net mask is
saliency-based, not semantic segmentation: inspect the silhouette and fringes in the contact sheet.

Verified on Termux (2026-11-19): SD1.5 fp16 256x256 -> adapter with `alpha_strategy=u2net` ->
RGBA (alpha 0..230) -> `alpha_qa.py --require-alpha --strict` PASS (transparent 68.9%).

## Full-cycle fixture (plan → dispatch → ingest → validate → contact-sheet)

A small, reproducible Termux fixture that exercises every stage on CPU. It is a pipeline proof,
not art: the atlas cell is a synthetic 64x8 pseudo-asset whose u2net matte comes out nearly empty.

Fixture manifest (`build/hud-artisan/fixture/manifest.json`): the three `build_progress_*_64x8`
strips from the pack manifest plus a 128x16 commandbuttons atlas declaration whose single cell
(64x8 at 0,0) has no `source_asset`, so `plan` compiles a pseudo-asset job for it. The five
`font_files` are real system TTFs copied under the manifest names into
`build/hud-artisan/fixture/fonts` (ingest copies them; it does not parse them).

```sh
B=build/hud-artisan/fixture
python3 tools/hud_artisan/generate.py plan --manifest $B/manifest.json --out $B/jobs \
  --theme classic-modern --seed 20261118
# HUD_ARTISAN_PLAN_OK jobs=4 atlas_cells=1
# every job renders 1024x128 (render_dimensions max_edge=1024); the cell requires alpha

COMFYUI_URL=http://127.0.0.1:8188 python3 tools/hud_artisan/generate.py dispatch \
  --manifest $B/manifest.json --jobs $B/jobs/jobs.json --adapter $B/adapter.json --drop $B/drop
# HUD_ARTISAN_DISPATCH PASS jobs=4 succeeded=4 failed=0   (8m56s, SD1.5 fp16 1 step, CPU)

python3 tools/hud_artisan/generate.py ingest --manifest $B/manifest.json --jobs $B/jobs/jobs.json \
  --drop $B/drop --out $B/pack --fit strict --font-source $B/fonts
# HUD_ARTISAN_INGEST PASS imported_to=.../pack

python3 tools/hud_artisan/generate.py validate --manifest $B/manifest.json --pack $B/pack
# HUD_ARTISAN_VALIDATE PASS assets=4/4 warnings=0 failures=0

python3 tools/hud_artisan/generate.py contact-sheet --manifest $B/manifest.json --pack $B/pack \
  --out $B/contact-sheet.png --cols 2 --cell 240
# HUD_ARTISAN_CONTACT_SHEET $B/contact-sheet.png 480x548
```

`adapter.json` in the fixture points at `build/hud-artisan/comfyui_api_workflow_sd15.json`
(steps=1, API format) with `alpha_strategy=u2net` and the local `u2net.onnx`; `working_directory`
resolves to the repository root so `{job_json}`/`{output_path}` stay repo-relative.

Observed limitations:

- The 64x8 icon cell goes through the u2net salient-object matte on a strip-like image, so the mask
  is almost fully transparent (alpha 0..2). Ingest accepts it (it is not fully opaque) and validate
  only checks RGBA8 geometry; this is a fixture artifact, not a valid icon. A real atlas cell must be
  generated at its native square size with a real silhouette.
- The three strips come out fully opaque (they are `hud-strip`, `requires_alpha=false`), which is
  correct for this family.

## Important limitations

- A workflow that emits an opaque PNG cannot satisfy an alpha-required icon/cursor job without a matte. The Termux Z-Image profile requests solid #FF00FF and applies deterministic border-connected flood-fill; this is not semantic segmentation and must be visually inspected for magenta fringe/holes.
- Portrait/minimap frames require a genuinely transparent central aperture; fake black/blurred/checkerboard apertures are rejected.
- Node IDs in comfyui.example.json are examples, not a working workflow.
- A local server URL is a local transport address, not a hosted API subscription.
- The adapter cannot prove artistic quality. Run deterministic validation, inspect contact sheets and obtain independent vision review plus human sign-off.

## No hosted workflow

The repository intentionally does not include a GitHub Actions job that downloads model checkpoints or calls hosted rendering services. CI tests the generator and adapters with local fixtures and mocked transports; real image inference stays on the user's chosen local machine and renderer.

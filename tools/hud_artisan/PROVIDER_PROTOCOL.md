# HUD Artisan provider protocol

HUD Artisan separates the engine asset contract from the image-generation provider. There are two
supported integration modes:

1. Agent mode: an autonomous agent reads jobs.json and emits one image per job.
2. Adapter mode: the built-in dispatcher calls a local provider CLI through an argv-only interface.

## Adapter configuration

Copy adapter.example.json and configure provider, model, license, working_directory, command,
timeouts, retry count, concurrency and the exact environment-variable allowlist.

The command is an array of arguments, never a shell string. Supported placeholders are:
- {job_json}: temporary JSON file containing the complete asset job.
- {output_path}: absolute destination path to write as PNG RGBA8.
- {job_id}: stable idempotency key.

The provider program must write one image at the exact target dimensions and return exit code 0.
The dispatcher rejects missing outputs, malformed PNG files, wrong dimensions and non-RGBA8 data.
No image provider secrets are stored in jobs.json or adapter config; only environment variable names
are allowed, and the dispatcher passes through only those named variables plus PATH and HOME.

The dispatcher supports bounded concurrency, per-job timeouts, retries and separate stdout/stderr
logs. A successful run writes worker-results.json and provenance.json. The latter binds every job
to the prompt SHA-256 and actual output SHA-256; ingest verifies both before building the pack.

## Provider implementation expectations

The adapter script should:
- read the job JSON; never infer dimensions or output paths from array ordering;
- use the prompt and negative_prompt exactly, or record any intentional transformation;
- use job_id for provider-side idempotency where available;
- preserve supplied references and attribution in a sidecar if any are used;
- save exactly one RGBA8 PNG to output_path;
- avoid text, logos, watermarks and copied protected art;
- not run shell commands constructed from prompt content.

Local ComfyUI, a Stable Diffusion pipeline, an internal REST service and vendor APIs can all be
wrapped by a small provider_adapter.py. The core tool remains provider-neutral and needs no internet
or model-specific dependency.

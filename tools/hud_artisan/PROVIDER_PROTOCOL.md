# HUD Artisan provider protocol

HUD Artisan separates the engine asset contract from the image-generation provider.

## Integration modes

1. Agent mode: an autonomous agent reads jobs.json and emits one image per job.
2. Adapter mode: the built-in dispatcher calls a local provider CLI through an argv-only interface.

## Adapter configuration

Copy adapter.example.json and configure provider, exact model/revision, license, working_directory,
command, timeouts, retry count, concurrency and the environment-variable allowlist.

The command is an array of arguments, never a shell command string. Supported placeholders:
- {job_json}: temporary JSON file containing the complete asset job.
- {output_path}: absolute output path for a PNG RGBA8 image.
- {job_id}: stable idempotency key derived from asset identity and prompt SHA-256.

The adapter program must write one image at the exact target dimensions and return exit code zero.
The dispatcher rejects missing files, malformed PNGs, wrong dimensions and non-RGBA8 output.
No API secrets are stored in jobs.json or adapter config. Only environment variable names are
configured; the dispatcher passes through only those names plus PATH and HOME. Values are redacted
from worker logs, and log output is capped per stream.

The dispatcher supports bounded concurrency, per-job timeouts, retries and separate stdout/stderr
logs. A successful run writes reports/worker-results.json and provenance.json. Provenance binds each
job to its exact positive/negative prompt SHA-256, per-asset seed, provider/model/license and actual
PNG SHA-256, and binds the whole batch to the exact jobs.json SHA-256. Ingest verifies the complete
job set and every binding before accepting generated art. Import is transactional: incomplete packs
are never promoted to the requested output path.

## Provider implementation requirements

An adapter must:
- read the complete job JSON; never infer dimensions or paths from array ordering;
- use prompt and negative_prompt as supplied, or record any intentional transformation;
- use the explicit per-asset seed from jobs.json (do not derive/rewrite it from job_id); use job_id as the provider idempotency key when available;
- preserve references and their license/provenance metadata when used;
- save exactly one RGBA8 PNG at output_path;
- avoid text, logos, watermarks and copied protected art;
- never build shell commands from prompt strings.

Local ComfyUI, Stable Diffusion, an internal REST endpoint or a commercial API can be wrapped by a
small provider_adapter.py. The core tool stays provider-neutral and does not bundle model weights.

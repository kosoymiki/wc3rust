# Provider-neutral agent protocol

HUD Artisan deliberately separates asset specification from image generation. Its job contract is
the versioned JSON schema in job.schema.json. An execution provider can be:
- a human-operated image model UI;
- an autonomous coding/art agent with image-generation access;
- a local ComfyUI/Stable Diffusion workflow;
- an internal company image service;
- an offline artist who returns PNGs.

## Required adapter behavior

An adapter reads jobs.json and writes one PNG at each job.output path under a clean drop directory.
It must not modify the manifest or silently transform aspect ratios. Exact dimensions are preferred.
If it cannot supply exact dimensions, it must record source size and leave fitting to the explicit
ingest policy. It should write provenance.json with provider, model, revision, generated_at,
prompt hash, seed, references and usage/license metadata.

## Concurrency and retries

Jobs are independent and can be distributed across agents. Use job_id as the idempotency key.
A retry must preserve the same seed/prompt unless the job revision changes. Never let two workers
write the same output path simultaneously. A worker reports status per job: queued, running,
succeeded, rejected or failed. Failed outputs do not enter the staging pack.

## Image model guidance

Use high-resolution / quality mode for large panels and menus; use icon/small-asset mode for 32–64 px
glyphs. Do not ask the model to draw text. For frames, reserve the supplied insets and avoid central
ornament. Use transparent output for isolated icon/cursor families. Keep reference images and their
licenses in provenance, not in the output artwork metadata.

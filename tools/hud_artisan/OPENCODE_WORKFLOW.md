# OpenCode 2 workflow for HUD Artisan

HUD Artisan is the deterministic production system. OpenCode 2 is the agent workbench used to inspect the repository, reason about art direction, prepare job-specific instructions, run QA, and coordinate independent reviews. The pipeline does not require an OpenCode API key, a hosted image API, a cloud subscription, or a specific model vendor.

## Responsibility boundary

- **OpenCode coding agent:** repository analysis, implementation, prompt refinement, test execution, manifests, QA automation and documentation.
- **OpenCode vision-capable reviewer:** inspect actual PNG/contact-sheet pixels against the manifest and style bible. A text-only model must not submit visual-review evidence.
- **Image renderer:** a separately configured local process or other explicitly selected provider. It must produce the exact PNG output contract. The renderer is replaceable.
- **HUD Artisan:** source of truth for job identity, dimensions, deterministic seeds, path safety, output validation, provenance, atlas assembly and transactional ingestion.
- **Human owner:** final art direction and release approval.

Do not claim that a language model generated or inspected pixels unless the model actually received the image bytes through a vision-capable interface.

## Recommended OpenCode roles

Use separate OpenCode sessions/agents, not one conversation asked to impersonate several reviewers. Select models already available in the user's OpenCode configuration. Record exact provider/model identifiers from the actual session; never invent identifiers.

### 1. Repository engineer

Read README.md, AGENT_TASK.md, PROVIDER_PROTOCOL.md, STYLE_BIBLE.md, ART_DIRECTOR_BRIEF.md, generate.py, tests and schemas relevant to the task.

Task:
- Review one bounded subsystem at a time.
- Trace every input, output, path, hash and failure path.
- Reproduce defects with tests before changing implementation.
- Preserve public CLI contracts unless a migration is documented.
- Never edit the authoritative game manifest or engine runtime contract without proving the source.
- Run python -m compileall -q tools/hud_artisan and the full unittest suite.
- Report exact changed files, commit SHA, test matrix and remaining risks.

### 2. Art Director

Read ART_DIRECTOR_BRIEF.md, STYLE_BIBLE.md, the exact manifest record and job record, and the final PNG itself, not only the prompt or metadata.

Task:
- Judge semantic recognition, silhouette at native size, material response, lighting, value hierarchy, craftsmanship, style cohesion, state consistency, engine fitness and originality.
- Compare sibling assets and state variants.
- Inspect at native size and at 50%/25%.
- Identify concrete defects with pixel-region descriptions and corrective instructions.
- Do not inflate scores to pass the gate. Any uncertainty is a reject/needs-human-review, not a 10.

### 3. Red Team Reviewer

Use a separate session and do not expose the Art Director's review before its own report is complete.

Task:
- Assume the asset is defective until it survives inspection.
- Search for fake transparency, opaque halos, cutout fringing, bad alpha, clipped ornament, asymmetry, unreadable microdetail, generic/reused silhouettes, accidental text, wrong state, atlas bleed, incorrect insets and engine-scale failures.
- Compare the image to its declared semantics and the whole HUD family.
- Inspect the exact same final PNG bytes.
- Return defects and scores independently. Do not negotiate a consensus score with the Art Director.

## Job workflow

1. Run generate.py plan against the authoritative manifest.
2. Give the selected agent the exact jobs.json entry, the matching manifest entry, STYLE_BIBLE.md and AGENT_TASK.md.
3. Generate one asset per job through the explicitly selected renderer. The renderer must write the exact path and dimensions; no contact sheets or multi-asset canvases as deliverables.
4. Run dispatch/ingest/validate and create contact sheets.
5. Give the exact final PNGs to both independent vision-capable reviewers.
6. Store each original model response outside the machine gate as immutable evidence. Produce the structured reviews.json only from those actual responses; do not invent or rewrite their judgments.
7. Run review_gate.py. A machine PASS validates the evidence contract and exact image hashes; it does not prove artistic quality or reviewer independence.
8. Obtain human art-direction approval before shipping.

## Evidence discipline

For every run retain source manifest SHA-256; exact jobs.json SHA-256; job ID, positive/negative prompt hashes and per-asset seed; renderer/model/checkpoint identifier and revision when known; generation settings and timestamp; output PNG SHA-256; raw reviewer responses and final structured evidence; tool version, Python version and test report.

Do not put API tokens, passwords, private model URLs or account identifiers in manifests, job files, prompts, reports or commits. Secrets belong in the local process environment and must never be printed.

## What the current gate can and cannot prove

The gate can verify deterministic plan coverage, exact file hashes, schema validity, reviewer-role coverage, score completeness and rejection rules. It cannot cryptographically prove that two sessions were independent, that the named model actually saw the image, or that a 10/10 score is truthful. Those claims require auditable raw evidence and human oversight. Distinct model names alone are not proof of independence.

## Model selection rule

Choose the best available OpenCode model for each task based on actual capability: code model for implementation and test design; vision-capable model for image critique; second, different vision-capable model for adversarial review.

Do not make the pipeline depend on a vendor-specific API or model menu. OpenCode's existing configured providers are sufficient for the agent workflow; HUD Artisan consumes files and command results.
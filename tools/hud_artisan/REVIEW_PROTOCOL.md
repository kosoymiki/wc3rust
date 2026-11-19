# Independent visual review protocol

This gate is deliberately separate from PNG validation. It verifies that two distinct vision reviewers examined the exact final image bytes and returned a calibrated assessment bound to the exact final image bytes. It cannot prove that a reviewer is honest, capable or correctly calibrated; human art direction remains mandatory.

## Required reviewers

- **Art Director:** judges visual quality, material craft, visual hierarchy, family cohesion and game-scale read.
- **Red Team Reviewer:** actively searches for semantic ambiguity, generic/repeated forms, broken geometry, state drift, fake alpha, bad crops, IP imitation, rendering defects and engine integration failures.

Use genuinely independent model families/providers where possible. Do not ask one model to write both reports. Give both reviewers the same final PNGs, manifest semantics, STYLE_BIBLE.md and ART_DIRECTOR_BRIEF.md. Do not show one review to the other before both are complete.

## Scoring

Every asset receives integer scores from 1 to 10 for all ten criteria:

1. semantic_readability
2. silhouette_at_native_size
3. material_credibility
4. lighting_coherence
5. value_hierarchy
6. craftsmanship
7. style_cohesion
8. state_consistency
9. engine_fitness
10. originality_and_ip_safety

**Release threshold:** each reviewer must average at least 8.5/10 across the ten criteria, with no individual criterion below 7/10. Both reviewers must return approve, report no blockers, confidence >=0.80 and a concrete rationale. Non-blocking improvements are preserved in the report; they are not silently discarded and do not force artificial 10/10 scores. A model's self-reported score is evidence, not objective truth.

## Review loop

1. Create full contact sheets grouped by family and state; also inspect each image at 100%, 50% and 25%.
2. Review assets individually, then review the entire set for consistency and sibling uniqueness.
3. Preserve each original model response as a separate immutable UTF-8 file beside reviews.json. In reviewers[], record the actual provider, exact model identifier, distinct session_id, exact review_prompt and its SHA-256, raw_response_file and raw_response_sha256. Do not invent model/session identifiers or rewrite the original response.
4. Write each per-asset review entry with reviewer_id, exact image_sha256, scores, blockers, improvements, verdict, confidence and a concrete rationale.
5. If a reviewer average is below 8.5, any criterion is below 7, a blocker is reported or verdict is reject, return only the rejected job IDs plus exact defect descriptions to the artist agent. Regenerate/retouch those jobs and repeat both reviews against the new image hashes.
5. Never reuse reviews after a single PNG byte changes.
6. After all assets pass, run runtime pack checks, Rust loader tests, screenshot review on landscape and portrait, and human sign-off.

## Machine gate

Run:

```sh
python3 tools/hud_artisan/review_gate.py \
  --manifest path/to/manifest.json \
  --jobs build/hud-artisan/jobs/jobs.json \
  --pack build/hud-artisan/pack \
  --reviews build/hud-artisan/reviews.json \
  --out build/hud-artisan/review-report.json
```

Exit code 0 means only that the review evidence is complete and satisfies the declared dual-review score, blocker and evidence thresholds. It does not certify artistic excellence or replace human sign-off.

See review.schema.json for the exact machine contract.

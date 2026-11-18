# Independent visual review protocol

This gate is deliberately separate from PNG validation. It verifies that two distinct vision reviewers examined the exact final image bytes and returned a strict 10/10 assessment. It cannot prove that a reviewer is honest, capable or correctly calibrated; human art direction remains mandatory.

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

**Release threshold is 10/10 on every criterion from both reviewers.** Any 9, blocker, suggested improvement, missing asset, mismatched image hash, insufficient confidence or weak rationale rejects the asset. This is intentionally a hard gate, not an average. A model's self-reported score is evidence, not objective truth.

## Review loop

1. Create full contact sheets grouped by family and state; also inspect each image at 100%, 50% and 25%.
2. Review assets individually, then review the entire set for consistency and sibling uniqueness.
3. Write each review entry with reviewer_id, exact image_sha256, scores, blockers, improvements, verdict, confidence and a concrete rationale.
4. If anything is below 10, return only the rejected job IDs plus exact defect descriptions to the artist agent. Regenerate/retouch those jobs and repeat both reviews against the new image hashes.
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

Exit code 0 means only that the review evidence is complete and satisfies the declared 10/10 schema. It does not certify artistic excellence or replace human sign-off.

See review.schema.json for the exact machine contract.

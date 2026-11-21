# PACKET B — world & building art: "дома вождей" still look flat (owner: world-art)

## Mission
Make the melee world read like WC3 on the phone: buildings (Orc Great Hall / "дом вождя"),
units, trees and ground. Today the base is judged flat and pasted-on: the Great Hall roof is a
pale, low-contrast mass, there is no contact/AO shading, and the grass/tree edges shimmer.

## Evidence snapshot (observed)
- `analysis/crop-hall.png`, `analysis/crop-hall2.png` (device, 2x nearest): roof panels pale,
  ribs dark, building reads flat against grass; no ground contact darkening.
- `analysis/crop-trees.png`: grass texture shows moiré/streak at ~45°, rock doodads smear.
- `analysis/hudskin/device-playable.png`, `analysis/dev-g.png`: full frames.
- Sampling already: mips generated in linear light (`texture.rs:269 generate_mips`),
  `Rgba8UnormSrgb` for colour (`texture.rs:33`), anisotropy 16 on mesh + 3 terrain samplers.
- Renderer state (`mesh.rs`): `CameraUniform` 176 B carries `light/sun/ambient/day/fog/fog_range/eye`;
  the shader computes `ambient + sun * saturate(N.L)` (DNC-like). No shadow/AO pass exists.

## Invariants
- Do not invent data: if the map/model pipeline carries Ubersplat/Shadow/team-glow, read it; if it
  does not, say so and pick a defensible lighting model instead of faking art.
- No procedural "art" presented as hand-drawn.
- Every visual claim needs a headless render or a device crop with numbers (mean luma, contrast
  ratio, edge variance), not adjectives.

## OWNER_PATHS
- crates/wc3-render/src/mesh.rs
- crates/wc3-render/src/texture.rs
- analysis/agent-world-art/**

## READ_ONLY
crates/wc3-game/src/render.rs, crates/wc3-game/src/terrain_render.rs, crates/wc3-game/src/assets.rs,
crates/wc3-game/src/mobile_hud.rs, apps/**. Request a change in a READ_ONLY file from the tech-lead.

## Phase plan
1. Baseline: render the same model headlessly (see `crates/wc3-game/examples/model_probe.rs` and
   `crates/wc3-render/examples/*`) and record mean luma / contrast / edge metrics per material.
2. Root-cause the flatness with file:line: vertex-colour usage, tint math, `color_scale`, whether
   the DNC ambient term is added after sRGB decode, whether team colour multiplies the albedo.
3. Implement the fix in mesh.rs/texture.rs (grounded ambient, contact darkening from geometry or a
   small AO term, correct colour-space math). Keep it data-driven.
4. Tests: unit tests for the lighting math with hand-computed expected values + one headless render
   asserting the Great Hall's wall-vs-roof contrast moved in the intended direction.
5. Device verify only if the phone is free (bounded, see packet A cheatsheet).

## Acceptance artifacts
- `analysis/agent-world-art/BASELINE.md` + `AFTER.md` with the same metrics side by side.
- Headless PNGs before/after under `analysis/agent-world-art/`.
- Tests green: `scripts/h2-cargo test -p wc3-render --lib --offline`.

## Pitfalls (observed)
- wgpu 30 sampler field is `anisotropy_clamp: u16`.
- `crates/wc3-game/src/campaign.rs` and `game.rs` were mid-refactor by other agents; they compile
  now — do not revert them.
- The repo is NOT a git repo.

## Report format
Goal | Actions | Evidence | Result | Risks/Next | H2_PERSISTED ids.

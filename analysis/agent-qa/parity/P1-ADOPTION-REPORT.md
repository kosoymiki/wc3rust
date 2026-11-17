# r2-world P1 adoption — canonical ability/projectile animators + edge-on billboards

**Date:** 2026-11-13 20:33:22 MSK · session ses_e30595423ffdGkcohq9P8MIxiM1 · agent-world
**Owner paths:** crates/wc3-game/src/{model_render,render,terrain_render,doodads}.rs, analysis/agent-world/**

## Goal
Adopt anim R8 `AnimatorRegistry::ability_animator`/`projectile_animator` in the game render path: spell playback uses `AbilitySequence::Cast` (no per-kind tables), missiles use `ProjectileSequence::Flight`, and flat `ZigguratMissile`-class art is billboarded so edge-on views keep coverage.

## Actions (file:line, crates/wc3-game/src/model_render.rs)
- `:259` `ModelRegistry::ability_animator(model, kind)` — thin canonical passthrough to `wc3-anim`; `:268` `projectile_animator(model, kind)`.
- `:182` `ModelRegistry::load_projectile` → `load_posed(..., Pose::Projectile(Flight))`; `:290` `build_posed` resolves Stand / Ability / Projectile through the canonical registry, stores `pose_sequence`/`pose_fallback` and `cast_sequence`/`flight_sequence`/`birth_sequence`/`impact_sequence` on `ModelEntry`.
- `:482` `billboard_normal` — model-space thin axis when the smallest extent is <2% of the largest (single-quad missiles).
- `:711` `ProjectileFx::draw_oriented` — camera-facing orientation for flat models; accounts for the renderer's Z-up→Y-up swizzle (`axis_world = (x, z, -y)`), spin about the view axis, per-mesh/per-layer alpha unchanged.
- `:770` `SpellPlayback` — `start()` calls `registry.ability_animator(model, AbilitySequence::Cast)`, keeps bind geometry, `advance(dt)`, `nodes()`, `posed_meshes()` (per-frame CPU skin via `:513 pose_meshes`).
- Tests: `:1435 projectiles_render` (now `load_projectile` + eye-aware draw), `:1610 projectile_billboard_edge_on`, `:1823 spell_cast_playback`.

## Evidence (commands + rc)
Canonical gates (exactly as requested):
- `cargo check -p wc3-game --lib --offline` → **rc=0** (8.00 s)
- `cargo test -p wc3-game --lib models_per_def --offline` → **rc=0** (0 passed / 4 ignored — heavy gates are `#[ignore]`)
- Same gate with `-- --ignored --nocapture` (real 216-model evidence) → **rc=0**, 4 passed: `MODELS_PER_DEF required=216 loaded=214 meshless=2 failures=0`, `PERF_MEMORY ... pass=true`, LRU 32 MB → 98 evictions, gallery 32.

Adoption gates:
- `cargo test -p wc3-game --lib spell_cast_playback --offline -- --ignored --nocapture` → **rc=0**
  `SPELL_CAST_PLAYBACK heroes=12 exact=12 fallback=0 missing=0 animated=12`
  sample `Hpal:Spell, Hamg:Spell, Hmkg:Spell Throw, Ofar:Spell, Obla:Attack Walk Stand Spin, Otch:Spell Slam, Udea:Spell - 1, Udre:Spell` — sequences the per-kind Idle/Walk/Attack tables cannot express.
- `cargo test -p wc3-game --lib projectiles_render --offline -- --ignored --nocapture` → **rc=0**
  `PROJECTILE_RENDER models=10 frames=2 moved_px=15760 ... sequence=["flight"×10] fallback=[false×10]`, Adreno Vulkan adapter.
- `cargo test -p wc3-game --lib projectile_billboard_edge_on --offline -- --ignored --nocapture` → **rc=0**
  `PROJECTILE_BILLBOARD model=Abilities\Weapons\Arrow\ArrowMissile.mdx axis=Vec3(0.0, 1.0, 0.0) edge_on_px_before=0 edge_on_px_after=255` (front view 255 px; 1 mesh / 1 layer / 6 indices = single quad).

## Artifacts (sha256)
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectile-billboard-before.ppm` — 691215 B — `8b6b185e4119c264ec094b3d6a350688a1e2c2b6accbe0203e311c6e7dfd067a`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectile-billboard-before.png` — 1508 B — `b61f127432883a580836f93bcfa62e0c3933def087d00872847237bfa4d8276c`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectile-billboard-after.ppm` — 691215 B — `c7174472dfb817d594727625ca172511e5cf7deb7503cc3a164a19347bd694e1`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectile-billboard-after.png` — 2407 B — `3bef8ccee461c2506c686f1cf3c981db57e84bbfb6530e72fb9d7277f929691b`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles.png` — 30783 B — `c251843a3492cc080d2cf36a4298fa4a3fd84fa43a4058b20185ae729e67e372`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles-frame-b.png` — 30434 B — `d5e381c032604be694d2c52d20955329d4b1eca7a9c006e3204c035537d04ca9`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles-diff.png` — 44713 B — `d21e59b08601b8aa8e9d23ec5c557da4a27430d7a8083be259aaaa90facdc12c`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def.png` — 355467 B — `099fb7cea8e6c57aa0008a3338018f17b8d4b4add11b41eaccc5b24518f1aae1`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/roster.csv` — 24794 B — `39b5f0a5107981b8aef9265146fddb7957b6eeda086017aaac7caf88cbc4af3d`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/spell-cast.log` — 18655 B — `1ee851437a3af7d9cd75662a2322bbd13ddac0194a5a2218c98a3647a508e9b4`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/projectile-billboard.log` — 21386 B — `bdd5b648c2228b2f27356252dacd7639197b5ea1254f63ad474d1d2ff46823a6`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/p1-canonical-gates.log` — 466 B — `1fc3795b6bd9069b4361a5cd5d438259dac962115c2090e956f733feb6c81d5d`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-models.log` — 24533 B — `9a2fda508685eef61fddf5085f5692706419d08c69cb04982cdae1b68d1bfdcb`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/projectiles-render.log` — 21232 B — `4edda4c92e7d5ab099f8e88cc6406470f33a6fef63e3c9749684024069832e9b`

## Result
**DONE, rc=0.** Canonical adoption is wired and verified: spell playback resolves authored Cast sequences through `ability_animator`, missiles load through `projectile_animator(Flight)`, and flat missile art is billboarded (0 → 255 edge-on pixels). 216-model gate, gallery and full compile stay green.

## Risks/Next
- `game.rs:1399-1420` still selects Idle/Walk/Attack via `SequenceKind` and uses its own `Animator`; the canonical API + `SpellPlayback` are ready — the game owner should replace the `cooldown > 0` branch with `SpellPlayback::start(&mut registry, def.model, AbilitySequence::Cast)` and drive `Scene.units[..].nodes` from `playback.nodes()`. `game.rs` is outside this layer's OWNER_PATHS, so it was not edited.
- Billboard applies to projectiles drawn through `ProjectileFx::draw(entry, eye)`; any other flat-model draw path must pass the camera eye to get the same guarantee.
- Concurrent agents repeatedly edited `wc3-sim`/`wc3-render` mid-round (compile bursts rc=101); every gate above was re-run after their builds settled.

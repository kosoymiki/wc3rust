# r2-world ROUND8 — projectile/missile rendering (#12) + per-def gallery on device

**Date:** 2026-11-13 20:01:56 MSK · session ses_e30595423ffdGkcohq9P8MIxiL · agent-world
**Owner paths:** crates/wc3-game/src/{terrain_render,render,model_render,doodads}.rs, analysis/agent-world/**

## Goal
Parity increment #12: render real WC3 missile models (data-driven `Missileart`) with flight motion, spin and fade through the canonical unit pipeline, and re-verify the 216-definition model gallery on the device GPU.

## Actions (file:line)
`crates/wc3-game/src/model_render.rs`
- `:230` `ModelRegistry::resolve_path` — archive resolution for missile art.
- `:374` `MissileArt` + `:387` `missile_arts(&GameDatabase)` — enumerates `Missileart`/`Missilespeed` from real `UnitWeapons.slk` + profile layers via `wc3-data` (no invented values).
- `:418` `resolve_missile_model` — WC3 stores `.mdl` source paths while installs ship `.mdx`; handles comma-separated impact/missile lists, first existing candidate wins.
- `:441` `ProjectileFx` — origin/target/speed/arc/t; `:493` `advance`, `:500` `position` (parabolic), `:509` `alpha` (fade in/out), `:514` `draw` → `UnitDraw` (yaw + spin, per-mesh/per-layer alpha) so missiles use the same skinning/material/team-tint path as units.
- `:1086` test `projectile_arts_resolve`; `:1140` test `projectiles_render` (two-frame GPU render + pixel-diff).

`crates/wc3-game/src/render.rs`
- `:808` `RENDER_ADAPTER name/backend/driver/device_type` logged once per offscreen renderer → every gallery/projectile artifact carries GPU provenance.

## Evidence (commands + rc)
1) `cargo test -p wc3-game --lib projectile_arts_resolve --offline -- --nocapture` → **rc=0** (7.13 s)
   `PROJECTILE_ART entries=168 distinct_present=49 loaded=44 meshless=5 absent=0 failed=0`
   (5 meshless = emitter-only missile MDX, correctly parsed and not counted as failures)
2) `cargo test -p wc3-game --lib projectiles_render --offline -- --ignored --nocapture` → **rc=0** (7.24 s)
   `RENDER_ADAPTER name="Turnip Adreno (TM) 725" backend=Vulkan driver="turnip Mesa driver"`
   `PROJECTILE_RENDER models=10 frames=2 moved_px=15554` — AncestralGuardian/AncientProtector/Arrow/Axe/Ballista/Bandit/Banshee/BlackKeeper/BristleBack/CannonTower missiles
   Frame bboxes: A (0,224)-(1256,299), B (0,350)-(1218,422); 108/107 colour buckets (non-blank).
3) `cargo test -p wc3-game --lib models_per_def --offline -- --ignored --nocapture` → **rc=0** (74.98 s)
   `MODELS_PER_DEF required=216 loaded=214 meshless=2 failures=0 resident=214 memory_mb=73.4 peak_mb=73.4`
   `PERF_MEMORY ... budget_mb=1536 ... anim_capacity=542 ... pass=true` · LRU 32 MB → 98 evictions
   `MODELS_PER_DEF_GALLERY races=4 rendered=32` · 5× `RENDER_ADAPTER` = Adreno Vulkan (device GPU)
4) `cargo test -p wc3-game --lib models_per_def_gallery --offline -- --ignored --nocapture` → **rc=0** (87.34 s), 4×8 per-race + 32 combined, Adreno adapter.
5) `cargo test -p wc3-game --lib --offline` → **rc=0** — 106 passed, 0 failed, 20 ignored (148.72 s).

## Artifacts (sha256)
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles.ppm` — 2764816 B — `ab4072e1826f96e3857ac3faf07c4289b912ac19819a6bf360b905056be2323e`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles.png` — 30304 B — `20e401650dd0d17ffcfc2853348fceb567db54314bd18e8739db01c4fd73c465`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles-frame-b.ppm` — 2764816 B — `6d436348fe935a81d1d020edadcf98e0883fee7e9c3e265b7e6640f52759a1cd`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles-frame-b.png` — 29449 B — `a576935a3caa8dfd18e78fbceca1cb60d19eb94bd640810d8999e8816a7b0995`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/projectiles-diff.png` — 43284 B — `481e3d3ebfd225a8dcb3a936a64e647e853016f10409a42b7a736b5b7289f819`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def.png` — 355467 B — `099fb7cea8e6c57aa0008a3338018f17b8d4b4add11b41eaccc5b24518f1aae1`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Human.png` — 65770 B — `bec87bf0d29e07b166d1a44af1f48878d9375f1f0aecced882d4040f8416dff0`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Orc.png` — 239429 B — `4b1616e40cd416cb566d0226774abd42a3afcebbf58af4402354429770610df9`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-Undead.png` — 153896 B — `a8e1d75f89977aa9d8bd69d40185ea31bc979af40c941f9f85f6839db238e17b`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/models-per-def-NightElf.png` — 246570 B — `4e5d1c6a17e5aef0246bea9ccf276dcb1a833e31ce0d35f79e87c497467fcd25`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity/roster.csv` — 23986 B — `41b6f9ab3d22c8bdf4336a983121c83923bc7ed47d7beea0c5cebdbfb80ea6ab`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-models.log` — 27717 B — `d160d5b52365eea7971292cf3ff14a5335a7e7183250e8c01b93e00d1cb15efc`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/parity-gallery.log` — 23321 B — `8ba32e06c1f60df747f0f833ac8f3964030db9b1e4285958fec20b70b7dde600`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/projectiles-arts.log` — 19325 B — `cddf8d27afbdcada52a0db47d5ae4d7013559c8931bf7e176e7520c3fa8fcc39`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/projectiles-render.log` — 20505 B — `168ea55596c19f8cff6c0fb666997fa43fd2794cca48a662989017e0fcb5c922`
- `/data/data/com.termux/files/home/src/herofolends-rs/analysis/agent-world/tests-round8.log` — 27222 B — `65b1e4d62a02c963874c9cc847635461f7e94fa5d632ec4dd32d04c31c31bba4`

## Result
**DONE (renderer increment), rc=0.** Missile rendering is implemented and verified with real weapon data and real MDX assets, on the device Vulkan adapter; the 216-model parse/upload gate and per-race gallery remain green on the current tree.

## Risks/Next
- Game-side spawn wiring: `Game::scene()` (game.rs) and the Android app are outside OWNER_PATHS, so attacks do not yet emit `ProjectileFx`. Integration point is ready: per attack call `ProjectileFx::new(resolve_missile_model(...), assets_index, attacker_pos, target_pos, weapon.projectile_speed().unwrap_or(900))`, advance in the frame loop and append `fx.draw(entry)` to `Scene.units`.
- 5 missile arts are emitter-only MDX (no geosets) — rendered by the particle layer when it lands; they are counted, not failures.
- `target-world` was wiped mid-round by a concurrent agent; gates were rebuilt from the current tree afterwards (logs above are from the rebuild).

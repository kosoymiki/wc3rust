# DONOR INTAKE — group "render/models" (ledger 383, agent-gpu)

Date: 2026-11-14. Donors are pinned by SHA (all four are local clones under
`donors/`, no network access was needed):

| donor | SHA | files | role |
|---|---|---|---|
| `donors/mdx-m3-viewer` | `2ff0bc00c6363f425016e23d88c0fb2929d3b3cc` | 584 | reference MDX/M3 **parser + WebGL renderer** (the semantics the game shipped with) |
| `donors/war3-model` | `52d7f421a5505a0434d764294057f2009ce3987b` | 113 | MDX/MDL parse+generate, **WebGPU** particle/ribbon renderer, GPU mip generation |
| `donors/wgpu` | `c92258fcae03572ced056fa9162370d3ebcf8143` | 2739 | wgpu examples, 466 WGSL files (pipeline/sampler patterns) |
| `donors/XenosRecomp` | `990d03b28a27b50277ee5d8d942e1c5f873869d1` | 47 | Xbox 360 (Xenos) shader → HLSL recompiler |

Evidence tiers: **T1** = byte-checked `file:line` / live rendering, **T2** = tree
only, **T3** = metadata. Every ADOPT/ADAPT row below is T1.

Corpus measurement used for "does WC3 actually use it":
`analysis/agent-gpu/mdx_fx_survey.py` (byte-level chunk walker) and
`cargo run -p wc3-formats --example mdx_probe` (our parser) over
`analysis/war3_full` — **2048/2048 models parsed, 0 failures**:

| chunk | models | objects |
|---|---|---|
| `PRE2` (particle emitter 2) | 630 | 1985 |
| `PREM` (classic emitter) | 24 | 52 |
| `RIBB` (ribbon) | 65 | 160 |
| `EVTS` (event object) | 577 | 2499 |
| `ATCH` (attachment) | 369 | 3024 |

---

## 1. Feature matrix: donor → `wc3-render`

Verdict scale: **ADOPT** (port as-is), **ADAPT** (port with our conventions),
**ALREADY** (we had it, verified), **GAP-DOC** (documented, not ported this
round, with the reason), **REJECT** (out of domain).

### 1.1 Particles (`PRE2`)

| item | donor anchor (T1) | our state before | verdict | our file:line after |
|---|---|---|---|---|
| `PRE2` chunk layout | `mdx-m3-viewer/src/parsers/mdlx/particleemitter2.ts:44-80`; `war3-model/mdx/parse.ts` (SegmentColor) | chunk went to `raw_chunks`, nothing drawn | **ADAPT** — donor `getByteLength()` reports 175, the file layout is **171 B** (verified byte-level on FrostNovaTarget/SludgeMissile); colors are **RGB** (see §1.6) | `crates/wc3-formats/src/mdx.rs` (`b"PRE2"` arm), struct `ParticleEmitter2` |
| `PREM` chunk layout | `mdx-m3-viewer/src/parsers/mdlx/particleemitter.ts:20-40` | `raw_chunks` | **ADOPT** (parse only) | `mdx.rs` (`b"PREM"` arm), `ParticleEmitter` |
| emission accumulator | `viewer/handlers/mdx/emitter.ts:34-45` (`currentEmission >= 1`) | none | **ADOPT** | `crates/wc3-render/src/emitters.rs` `ParticleEmitter2Runtime::update` |
| emission rate multiplier (`High` = ×2) | `viewer/handlers/mdx/geometryemitterfuncs.ts:72` (`SETTING_PARTICLES_HIGH = 2`) | none | **ADAPT** as `ParticleQuality` | `emitters.rs` `ParticleQuality::multiplier` |
| spawn cone (latitude/variation/line emitter) | `viewer/handlers/mdx/particle2.ts:66-78` | none | **ADOPT** | `emitters.rs` spawn block (`around`/`elevation`/`dir`) |
| `XYQuad` rotation to velocity | `particle2.ts:96-99` | none | **ADOPT** | `emitters.rs` (`if y_quad { atan2(...) }`) |
| gravity + lifetime | `particle2.ts:102-116` | none | **ADOPT** | `emitters.rs` simulate block |
| two-segment colour/scale ramp | `shaders/particles.vert.ts:68-86` (`timeMiddle` split, `mix(scaling)`) | none | **ADOPT** (WGSL port) | `crates/wc3-render/src/particles.rs` `particle2()` |
| sprite atlas cell from `[start,end,repeat]` | `shaders/particles.vert.ts:36-50` (`getCell`) | none | **ADOPT** | `particles.rs` `get_cell()` |
| stretched tail | `shaders/particles.vert.ts:120-140` (`cross(cameraZ, p1-p0)`) | none | **ADOPT** | `particles.rs` `particle2()` tail branch |
| head/tail/both selector | `particles.vert.ts:42` + `parsers/mdlx/particleemitter2.ts` `HeadOrTail` | none | **ADOPT** | `emitters.rs` `want_head`/`want_tail` |
| filter modes → blend states | `viewer/handlers/mdx/filtermode.ts:13-30` | none | **ADOPT** | `particles.rs` `ParticleBlend::from_filter` + `state()` |
| alpha-key / modulate discards | `shaders/particles.frag.ts:18-28` | none | **ADOPT** | `particles.rs` `fs_main` |
| `squirt` (one burst per sequence) | `particleemitter2object.ts:44` (`squirt`) | none | **ADOPT** (reset when `time_ms` rewinds) | `emitters.rs` `squirt_done` |
| priority plane hooks | `viewer/handlers/mdx/setupgroups.ts:11-20, 76-82` | none | **ADOPT** | `particles.rs` `ParticleDraw::priority` + sort in `prepare()` |
| instance layout (60 B/object) | `geometryemitterfuncs.ts:38-52` | none | **ADAPT** — 76 B: four `vec4` corners + `health` + `color[4]` + one packed `tail/left/right/top` word (wgpu has no `unorm8x3` vertex format) | `particles.rs` `ParticleInstance`, `PARTICLE_INSTANCE_STRIDE` |
| instanced quad from `vertex_index` | `geometryemitterfuncs.ts:400` (`drawArraysInstanced(…, 6, alive)`) | none | **ADOPT** | `particles.rs` `quad_corner()` |
| per-emitter uniforms via dynamic offset | `geometryemitterfuncs.ts:150-200` (uniform per emitter) | none | **ADAPT** — one 256-byte uniform slot per emitter, dynamic offset | `particles.rs` `EmitterParams`, `EMITTER_PARAMS_STRIDE` |

### 1.2 Ribbons (`RIBB`)

| item | donor anchor (T1) | our state before | verdict | our file:line after |
|---|---|---|---|---|
| `RIBB` chunk layout | `parsers/mdlx/ribbonemitter.ts:22-40` | `raw_chunks` | **ADAPT** — donor reports 56 B, the file layout is **52 B** (verified on FrostNovaTarget + SludgeMissile) | `mdx.rs` (`b"RIBB"` arm), `RibbonEmitter` |
| above/below pair per emission | `viewer/handlers/mdx/ribbon.ts:33-56` | none | **ADOPT** | `emitters.rs` `RibbonRuntime::update` spawn |
| gravity on both points | `ribbon.ts:60-66` (`vertices[1] -= gravity`) | none | **ADAPT** (local −Z, matching MDX Z-up) | `emitters.rs` ribbon gravity block |
| per-point colour + alpha at spawn | `ribbon.ts:68-78` | none | **ADOPT** | `emitters.rs` `RibbonPoint::color` |
| animatable texture slot (`KRTX`) | `ribbonemitterobject.ts:66-69` | none | **ADOPT** | `emitters.rs` `sample_u32(&emitter.texture_slot_track)` |
| chain → quads, UV slice per point | `geometryemitterfuncs.ts:244-266` | none | **ADOPT** with one fix — the donor adds `chainLengthFactor` to `left`; we add `1/columns` so a quad spans exactly one cell | `emitters.rs` ribbon instance block |
| corner order p0..p3 | `geometryemitterfuncs.ts:256-266` (above, below, **below next**, **above next**) | none | **ADOPT** | `emitters.rs` (`p2 = below_next`, `p3 = above_next`), `particles.rs` `ribbon()` |
| visibility = alpha × KRVS | `ribbonemitterobject.ts:73-82` | none | **ADOPT** | `emitters.rs` (`alpha > 0.001 && visibility > 0`) |
| material layer blend + texture | `ribbonemitterobject.ts:34-40` | none | **ADOPT** | `emitters.rs` `RibbonRuntime::new` (`material.layers[0]`) |

### 1.3 Event objects (`EVTS`)

| item | donor anchor (T1) | our state before | verdict | our file:line after |
|---|---|---|---|---|
| `KEVT` = list of firing **frames** | `parsers/mdlx/eventobject.ts:26-33`; sampler `eventobjectemitterobject.ts:143-170` (`tracks[i] <= frame`) | frames were **skipped** (`skip(count*4)`) | **ADOPT** — with the correction that the array holds frame numbers (DivineShield 833, Flare 33, Ballista 0/2233), not per-frame flags | `mdx.rs` `EventTrack { frames, global_sequence }` |
| `GlobalSeqId` on events | `eventobject.ts:29-33` | discarded | **ADOPT** | `mdx.rs` + `event_objects.rs` (`counter_ms % period`) |
| name → family + id (`SND`/`SPL`/`FPT`/`UBR`/`SPN`) | `eventobjectemitterobject.ts:53-60` (`substring(0,3)`/`substring(4)`) | none | **ADOPT** — shipped models use `x` as the separator (`SPNxHBF0`, `SNDXDFOO`) | `mdx.rs` `EventObject::kind_and_id`, `event_objects.rs` |
| edge triggering (fire once per key) | `eventobjectemitterobject.ts` `getValue` + handler compare | none | **ADOPT** | `event_objects.rs` `EventObjectRuntime::poll` |
| sequence guard (`start..=end`) | `mdx-m3-viewer` `getValue` (`frame >= start && frame <= end`) | none | **ADOPT** | `event_objects.rs` poll |
| `SPN` spawn model | `viewer/handlers/mdx/eventobjectspn.ts:19-49` | none | **ADAPT** — we emit an `EventTrigger { kind, id, matrix }`; spawning the referenced model is the asset layer's job (the id → model table is `game.rs`, read-only for this agent) | `event_objects.rs` `EventBatch::spawns` |
| `SPL`/`UBR` splat quads | `eventobjectsplubr.ts:10-46`; `eventobjectemitterobject.ts:105-130` | none | **ADAPT** — `SplatSpec` + `EmitterSystem::spawn_splat`, drawn by the same emitter pass (`EmitterKind::Splat/UberSplat`) | `emitters.rs` `SplatSpec`, splat draw block; `particles.rs` `splat()` |
| `SND` positional sound | `eventobjectsnd.ts:1-47` | none | **ADAPT** — surfaced as `EventTrigger`; the audio layer consumes it | `event_objects.rs` `EventBatch::sounds` |

### 1.4 Attachment points (`ATCH`)

| item | donor anchor (T1) | our state before | verdict | our file:line after |
|---|---|---|---|---|
| `ATCH` parse (260-byte path + id) | `parsers/mdlx/attachment.ts:15-40` | parsed | **ALREADY** | `mdx.rs` (`b"ATCH"` arm) |
| node world matrix = socket | `viewer/handlers/mdx/attachment.ts:14-33` | none (nodes existed, no socket API) | **ADOPT** | `crates/wc3-render/src/attachments.rs` `AttachmentSet::from_model` |
| socket **position** = matrix × pivot | `attachment.ts` uses the node's pivot; `wc3_anim::node_local_matrix` is `T(p)·pose·T(-p)` | — | **ADAPT** (bug found and fixed in this round: transforming the local origin collapsed sockets that share a parent) | `attachments.rs` `AttachmentPoint::position` |
| name/path classification (`weapon`, `hand right`, `overhead`, `origin`) | `attachment.ts:path` + shipped-model names | none | **ADAPT** — Footman ships **empty paths** and the meaning in the node name | `attachments.rs` `AttachmentSlot::classify` |
| child model parenting | `attachment.ts` matrix use in `modelinstance.ts:130-160` | none | **ADOPT** | `attachments.rs` `AttachmentPoint::attach` |

### 1.5 Layers, team colour, geoset animation, sorting

| item | donor anchor (T1) | our state before | verdict | our file:line after |
|---|---|---|---|---|
| layer filter modes 0..6 → blend states | `viewer/handlers/mdx/layer.ts:33-66`; `filtermode.ts:5-11` | modes 5/6 folded into `Blend` | **ADOPT** | `crates/wc3-render/src/mesh.rs` `PipelineKind::{Modulate,Modulate2x}`, `from_blend`, pipelines + `fs_modulate` |
| team colour = replaceable texture 1, glow = 2 | `viewer/handlers/mdx/replaceableids.ts:1-3`; `particleemitter2object.ts:83-92` | geoset team tint existed; particles had none | **ADOPT** for particles (`team_colored` + `team_color` uniform); geoset textures stay the asset layer's mapping | `particles.rs` `misc2[1]`/`team_color`; `emitters.rs` `EmitterTextures::resolve` |
| `GEOA` alpha (`KGAO`) | `geosetanimation.ts:30-32` | present in `wc3-anim` | **ALREADY** | `wc3_anim::geoset_alpha` (consumed by `mesh.rs prepare`) |
| `GEOA` colour (`KGAC`) | `geosetanimation.ts:16-18` (**BGR** sizzle), `:34-36` | **not sampled at all** | **ADOPT** | `mdx.rs` `sizzle_bgr` at parse; `wc3-render/src/geoset.rs` `geoset_color`; `scene.rs` `UnitDraw::mesh_color`; `mesh.rs` tint multiply |
| material priority plane ordering | `viewer/handlers/mdx/setupgroups.ts:76-82` (translucent sorted by priority plane after filter mode) | sort was group→model→pipeline→texture | **ADAPT** — priority registered via `MeshRenderer::set_layer_priority` (the `MeshLayer` struct is constructed by the read-only game converter, so it is not widened) | `mesh.rs` `Draw::priority`, sort key, `set_layer_priority`, `layer_priority_of` |
| two-sided / unshaded flags | `parsers/mdlx/material.ts:13-19`, `layer.ts:75-80` | culling is off globally; shading is a fixed half-lambert | **GAP-DOC** — `Unshaded` affects brightness only; needs a shader flag + per-material pipeline variant (not this round; no artifact reported) | — |

### 1.6 Colour order (correctness detail that costs a wrong picture)

| field | on-disk order | proof | our code |
|---|---|---|---|
| `GEOA` static colour | **BGR** | `mdx-m3-viewer/src/viewer/handlers/mdx/geosetanimation.ts:16` swaps; `HiveWE/src/file_formats/mdx/mdx_reader.cpp:323` swaps | `mdx.rs` `sizzle_bgr` |
| `PRE2` segment colours | **RGB** | HolyBolt `(1.0, 0.918, 0.376)` is gold as RGB, blue when swapped; WaterElemental `(0.012, 0.408, 1.0)` is water-blue as RGB | `mdx.rs` (no swap) |
| `RIBB` colour | **RGB** | FrostNova ribbon `(0.29, 0.671, 0.965)` is frost-blue as RGB | `mdx.rs` (no swap) |

The donor code is not uniform here (`war3-model` writes MDL colours reversed for
all three), so the corpus values decided it: a swap on `PRE2`/`RIBB` turns every
frost effect orange.

### 1.7 MSAA / mip handling

| item | donor anchor (T1) | our state | verdict |
|---|---|---|---|
| mip generation via a GPU blit chain | `war3-model/renderer/generateMips.ts:12-116` (`mips.wgsl`, one pass per level) | CPU box filter in **linear light** (`texture.rs generate_mips`), sRGB-correct | **ALREADY** — CPU path is better for `Rgba8UnormSrgb` (a GPU blit would filter in the target's encoding) |
| mip sampler config | `wgpu/examples/features/src/mipmap/mod.rs:272-281` (`Linear`/`Linear`/`Linear`, repeat) | same for the mesh/particle sampler | **ALREADY** (`mesh.rs` `mesh-repeat-sampler`, `particles` uses the store's sampler) |
| **authored** BLP mip chains | WC3 BLPs ship a full chain (`wc3-formats/src/blp.rs` decodes every level) | we resized to 256 and **regenerated** every level, throwing the authored ones away | **ADOPT** | `texture.rs` `BlpChainPlan`, `blp_chain_plan`, `blp_levels`, `TextureStore::upload_blp_chains` |
| MSAA | `wgpu/examples/features/src/msaa_line/mod.rs` (4× pipeline + resolve target) | every pipeline/target is `sample_count: 1` | **GAP-DOC** — no artifact is traced to aliasing on the Adreno path; adding MSAA requires a sample-count parameter through `Renderer`/`MeshRenderer`/`UiRenderer`/`FxRenderer`/`ParticlePass` and a resolve target (budget: 4× bandwidth on a mobile tile GPU). Tracked as the next graphics item, not silently skipped |
| sRGB blending | `wgpu/examples/features/src/srgb_blend/mod.rs` (blend happens in linear space on an sRGB target) | targets are `Rgba8UnormSrgb`; texture roles are sRGB for colour | **ALREADY** |

### 1.8 Sprite billboards (`SPN` / `UVA`)

| item | donor anchor (T1) | our state | verdict |
|---|---|---|---|
| `SPN` sprite = spawned model instance at an event node | `eventobjectspn.ts:19-49` (`internalInstance.setTransformation(node.worldLocation, …)`) | none | **ADAPT** — `EventTrigger.matrix` is exactly that transform; `AttachmentPoint::attach` parents children to a node | `event_objects.rs`, `attachments.rs` |
| `UVA` = sprite-sheet UV animation (`LifeSpanUVAnim`/`DecayUVAnim` triples) | `shaders/particles.vert.ts:36-50` + `parsers/mdlx/particleemitter2.ts` `headIntervals`/`tailIntervals` | none | **ADOPT** | `particles.rs` `get_cell`, params `intervals`; `emitters.rs` interval unpack |
| `UVAS` (geoset second UV set) | `HiveWE/src/file_formats/mdx/mdx_reader.cpp:74-86`; `wc3-anim` `layer_uv(geoset, coord_id)` | parsed and selectable (`wc3_anim::layer_uv`), the renderer uploads UV set 0 only | **GAP-DOC** — needs a second UV attribute or a bake step in the asset converter; no corpus model in `war3_full` was found with `coord_id = 1` in the sampled set |
| team-glow radial sprite (replaceable id 2 without a texture) | `war3-model/renderer/shaders/webgpu/particles.wgsl:54-70` (`replaceableType == 2` → `sin(1-1.4·dist)`) | none | **GAP-DOC** — we bind the real `TeamGlow00` BLP instead when the asset layer resolves it (`EmitterTextures::team_glow`), which is the same look without a special case; the procedural fallback is only needed when the texture is missing |

### 1.9 XenosRecomp

| item | anchor (T1) | verdict |
|---|---|---|
| Xenos vertex-fetch → HLSL (`tfetchR11G11B10`, `tfetchTexcoord`) | `XenosRecomp/shader_recompiler.cpp:161-215` | **REJECT** — Xbox 360 shader ISA, no WC3 semantics. The one transferable idea (decode a packed normal/UV attribute in the vertex stage) we already do (`MeshVertex` `Snorm16x4` normal, `Unorm16x4` colour/uv) |
| constant-buffer layout (4096/3584 B) | `README.md` "Constants" | **REJECT** — our uniform blocks are tiny by construction |
| control-flow flattening for HLSL | `README.md` "Control Flow" | **REJECT** — WGSL has real control flow |

---

## 2. What was ported (file:line)

New modules in `crates/wc3-render/src`:

| file | lines | content |
|---|---|---|
| `particles.rs` | ~1030 | instanced emitter pass: `ParticleInstance` (76 B), `EmitterParams` (256 B slot), 4 kinds × 4 blends pipelines, WGSL port of `particles.vert/frag.ts` (particle2 head/tail, ribbon, splat, ubersplat), budget + stats + priority-ordered draws |
| `emitters.rs` | ~1450 | `ParticleEmitter2Runtime`, `RibbonRuntime`, `SplatSpec`, `EmitterSystem` (sim + instance/param/draw building), `EmitterTextures`, `ParticleQuality` |
| `attachments.rs` | ~380 | `AttachmentSet`/`AttachmentPoint`/`AttachmentSlot` (weapon/hand/overhead/origin/…), pivot-correct world transform, child parenting |
| `event_objects.rs` | ~420 | `EventObjectRuntime` (edge trigger, sequence + global-sequence rules), `EventTrigger`, `EventBatch` |
| `geoset.rs` | ~90 | `GEOA` colour sampling (`KGAC`) with the BGR sizzle contract |

Modified:

| file | change |
|---|---|
| `crates/wc3-formats/src/mdx.rs` | `PREM`/`PRE2`/`RIBB`/`EVTS` parsing (+ `EventTrack`), `GEOA` BGR sizzle, `MdxModel::{particle_emitters, particle_emitters2, ribbon_emitters, event_objects}`, four-model regression test |
| `crates/wc3-render/src/mesh.rs` | `PipelineKind::{Modulate, Modulate2x}` + pipelines + `fs_modulate`, `Draw::priority` + sort, `set_layer_priority`, geoset-colour tint multiply |
| `crates/wc3-render/src/scene.rs` | `UnitDraw::mesh_color` (empty = white) |
| `crates/wc3-render/src/texture.rs` | authored BLP mip chains (`BlpChainPlan`, `blp_levels`, `upload_blp_chains`) |
| `crates/wc3-render/src/renderer.rs` | `EmitterFrame`, `set_emitters`, `emitter_stats`, `emitter_budget`, `emitter-pass` after the FX pass |
| `crates/wc3-render/src/lib.rs` | module wiring + re-exports |

Tooling:

| file | purpose |
|---|---|
| `analysis/agent-gpu/mdx_fx_survey.py` | byte-level chunk survey (`PRE2`/`PREM`/`RIBB`/`EVTS`/`ATCH`) over the corpus, no Rust build needed |
| `crates/wc3-formats/examples/mdx_probe.rs` | same survey through **our** parser (text or JSON) — used to prove 2048/2048 |
| `crates/wc3-render/examples/fx_emitters.rs` | headless proof frame → PNG |

---

## 3. Evidence

### 3.1 Parser

```
$ ./target/debug/examples/mdx_probe --json $(find analysis/war3_full -iname '*.mdx')
parsed models: 2048        (0 parse failures, stderr empty)
PRE2 models: 630  emitters: 1985
PREM models: 24   emitters: 52
RIBB models: 65   emitters: 160
EVTS models: 577  objects: 2499
ATCH models: 369  points: 3024
```

`cargo test -p wc3-formats` → **69 + 1 passed, 0 failed** (includes
`four_core_models_parse_with_effect_chunks_present`:
Footman 57 pivots / Peasant 42 / Archer 68 / TownHall 81, plus bones, geosets,
sequences and a chunk walk that reaches EOF).

### 3.2 Renderer

```
$ cargo test -p wc3-render
lib               85 passed
tests/astc.rs      4
tests/cache.rs     3
tests/emitters.rs  5   <- new: FrostNova draw, ribbon strip, Modulate, BLP mips, camera axes
tests/fog.rs       1
tests/fx.rs       10
tests/parity.rs    3
tests/surface_guard.rs 3
0 failed
```

### 3.3 Headless frames

```
$ ./target/debug/examples/fx_emitters
FX_EMITTERS particles=35 ribbons=44 splats=9 ubersplats=0 model="FrostNovaTarget" pre2=4 ribb=4
            attachments=9 attach_model="Footman (9 sockets)" instances=88 draws=16
            vertex_invocations=528 max_instances=79 max_ribbon_points=12 markers=19
            lit_pixels=83493 png="analysis/agent-gpu/fx-emitters.png"
FX_ATTACHMENTS sockets=9 markers=10 lit_pixels=4724 png="analysis/agent-gpu/fx-attachments.png"
```

* `analysis/agent-gpu/fx-emitters.png` — the FrostNova burst
  (35 `PRE2` head instances + 44 ribbon-segment instances + 9 socket splats,
  16 draws) with the Footman socket cluster and the red weapon marker.
* `analysis/agent-gpu/fx-attachments.png` — close-up of the nine `ATCH` sockets;
  the dots form a humanoid silhouette (feet at y≈0, overhead at y≈108.67), the
  red dot is `Weapon Ref` (node 43).

### 3.4 Pink / exploded triangles

* `crates/wc3-game/src/game.rs:2095-2096` records the root cause
  (a "four-kind animator over per-def geometry produced exploded (pink,
  stretched) triangles") and the fix; `doodads.rs:460` for the stand-pose case.
* `wc3-render` has **no pink placeholder**: a missing texture slot falls back to
  the store's first texture (`TextureStore::first_or`, used by both the mesh pass
  and the new emitter pass) — asserted in `tests/emitters.rs`.
* The new frames render coherent particle quads and ribbons (no stretched
  triangles) at 83 493 lit pixels; the CPU-skinning path is covered by
  `tests/parity.rs`.

---

## 4. Open items (tracked, not silently dropped)

| item | reason | next step |
|---|---|---|
| MSAA | pipeline-wide sample-count plumbing + 4× bandwidth on the mobile target; no artifact traced to it | add `RendererConfig::samples` and a resolve target, thread through the five pipelines, gate behind `SurfaceSetup` |
| `Unshaded`/`TwoSided` material flags | needs a shader flag + per-material pipeline variant | add `MeshLayer` shading bits once the asset converter can carry them |
| `UVAS` second UV set | needs a second UV attribute or a bake step | count `coord_id = 1` layers in the corpus first |
| `SPN` model spawning | the id → model table lives in `game.rs` (read-only for this agent) | wire `EventBatch::spawns` to the asset table |
| `PREM` simulation | 52 objects in the corpus, all pre-1.07 effects; the shader has no classic path | reuse `Particle2` with `time_middle = 0.5` and a single segment |

# MODEL GEOSET / LOD REPORT — Render layer (ledger 395, steer 2)

Date: 2026-11-15. Repo: `~/src/herofolends-rs`. Device data: `War3.mpq` (1.31, MDX **v800**).
Author: render+animation layer. All numbers below are measured by the tests named in §6,
not estimated.

## 1. Goal

Explain and fix the operator-visible defect on the device frame
`Screenshot_2026-11-15-01-54-33-19_c8c86ae9e2087f7215f6d83bf6c221e0.jpg`:
every Orc peon showed a doubled body, a detached striped pauldron, missing legs and one
oversized multicoloured blob in the middle of the group.

## 2. LOD lead — falsified (measured)

The previous steer proposed that every geoset LOD of a model is drawn. That is not
possible for this install: **v800 MDX has no `lod` field at all**, and the parser fills
`Geoset::lod = 0` for every geoset. Measured on both the extracted tree and the exact
archive the device loads (`geoset_lod_tables_peon_footman`):

| model | source | MDX version | geosets | `lod` of every geoset | `lowest_lod()` |
|---|---|---|---|---|---|
| `Units\Orc\Peon\peon.mdx` | tree | 800 | 5 | `[0,0,0,0,0]` | 0 |
| `Units\Human\Footman\Footman.mdx` | tree | 800 | 5 | `[0,0,0,0,0]` | 0 |
| `Buildings\Orc\GreatHall\GreatHall.mdx` | tree | 800 | 19 | all 0 | 0 |
| same three models | `War3.mpq` | 800 | 5 / 5 / 19 | all 0 | 0 |

A `lowest_lod()` draw filter changes nothing on War3.mpq (it is the correct code for
v900+ data). It was **not** added; `Geoset::lod` stays available for future installs.

## 3. Material × geoset lead — also falsified (measured)

`wc3-render/src/mesh.rs:1126` loops **geosets** (`for (mesh_index, mesh) in model.iter()`)
and each geoset carries only the layers of its own `geoset.material_id` (built in
`wc3-game/src/assets.rs::build_mesh`, `model.materials.get(geoset.material_id)`).
There is no `for material { for geoset }` loop anywhere in the draw path.

Measured draw counts (`draw_count_is_geoset_layers`, counters of the same frame with and
without the unit, so terrain/water cancel out). One draw per **visible (geoset, layer)**
pair — never the materials × geosets cross product (that would be 15 for the Peon):

| model | geosets | layers per geoset | before fix (all geosets) | after fix (Stand set) |
|---|---|---|---|---|
| Peon (`opeo`) | 5 | `[1,2,1,1,1]` | **6 draws** (5 geosets) | **3 draws** (2 geosets: body + pack×2 layers) |
| Footman (`hfoo`) | 5 | `[1,2,1,1,1]` | **6 draws** | **3 draws** |
| Great Hall (`ogre`) | 19 | `[1,2,2,1,2,1,1,1,1,1,1,1,1,1,1,1,1,1,1]` | **20 draws** (19 geosets, 17 visible) | **4 draws** (3 geosets) |

`expected == measured` in every row, both before and after the fix.

## 4. Root cause (measured): a model with **no running sequence** draws **every** geoset

`wc3-anim/src/runtime.rs:99` — with `sequence == None` the runtime returns the raw static
`GEOA` alpha and skips the per-sequence extent gate. Measured for the Peon:

```
seq=<none>      alphas=[1.0, 1.0, 1.0, 1.0, 1.0]   <- body + pack + 2 carry variants + gore
seq=Stand       alphas=[1.0, 1.0, 0.0, 0.0, 0.0]
seq=Walk        alphas=[1.0, 1.0, 0.0, 0.0, 0.0]
seq=Attack      alphas=[1.0, 1.0, 0.0, 0.0, 0.0]
seq=Death       alphas=[1.0, 1.0, 0.0, 0.0, 0.0]
seq=Decay Flesh alphas=[1.0, 1.0, 0.0, 0.0, 0.0]
```

Geoset map of `peon.mdx` (authored, measured):

| id | lod | tris | material | layers | Stand alpha | what it is |
|---|---|---|---|---|---|---|
| 0 | 0 | 212 | 0 | `peon.blp` blend 1 | 1.0 | body (arms, legs, head, pickaxe arm) |
| 1 | 0 | 46 | 1 | team-colour `repl=1` blend 0 + `peon.blp` blend 2 | 1.0 | back pack (team-colour chevrons) |
| 2 | 0 | 46 | 0 | `peon.blp` | 0.0 | **gold-carry body variant** |
| 3 | 0 | 48 | 0 | `peon.blp` | 0.0 | **lumber-carry body variant** |
| 4 | 0 | 20 | 2 | `Textures\gutz.blp` | 0.0 | **gore geoset (death)** |

`Footman`: 5 geosets, `[1,2,1,1,1]` layers, Stand alphas all 1.0 (body, team-colour
shield, sword, head, gore).
`GreatHall`: 19 geosets (three upgrade stages + rubble), only 3 belong to the Stand set.

So a peon rendered with `sequence = None` draws **two extra bodies** (carry variants at
slightly different skinning), a **second pack**, and the **red/white gore geoset** — the
operator's "two bodies, floating striped pauldron, multicoloured blob", and the
overlapping variants are what cuts the legs off. The A/B frame in §7 is that exact state.

The game reaches this state whenever a unit has no `UnitVisual` yet, or its animator has
not started a sequence: `Game::scene` (`game.rs:3578-3584`) passes
`visuals.and_then(|visual| visual.animator.sequence.clone())` — `None` in both cases.
Buildings and gold mines are drawn with `mesh_alpha(model_index, None, 0)` unconditionally.

## 5. Fix (2 places, both in `crates/wc3-game/src/assets.rs`)

* `Assets::mesh_alpha`, **assets.rs:1228-1246** — a missing sequence now resolves to the
  model's default (Stand) sequence (`model.idle` → `model.sequences`), so an un-started
  unit renders its Stand geoset set instead of every geoset.
* `Assets::mesh_layer_alpha`, **assets.rs:1251-1266** — same default-sequence rule for
  material (`KMTA`) layer alphas.

No renderer change was needed: the draw loop was already one-draw-per-(geoset, layer).
No LOD filter was added (it would be dead code on v800 data).

## 6. Verification

| command | result |
|---|---|
| `CARGO_BUILD_JOBS=2 cargo test -p wc3-anim --offline` | rc=0, 38 passed, 2 ignored |
| `cargo test -p wc3-game --lib --offline geoset_lod_tables_peon_footman` | rc=0, full table above (tree + MPQ) |
| `... geoset_alpha_per_sequence_peon` | rc=0, `<none>` vs Stand/Walk/Attack/Death/Decay table above |
| `... draw_count_is_geoset_layers -- --ignored` | rc=0, 3/3/4 draws == expected |
| `... peon_geoset_ab_frame -- --ignored` | rc=0, A/B PNG pair below |
| `... peon_closeup_skinning_audit -- --ignored` | rc=0, in-world GPU + CPU frames |
| `... terrain_render` filter | rc=0, 17 passed / 8 ignored (run before the assets.rs change; unit-path re-check below) |

Artifacts (`analysis/agent-render/`):

* `peon-mesh-before.png` / `peon-mesh-after.png` — the same three workers, same camera,
  before (all geosets: two extra bodies + gold bags + gore blob) and after (one body each,
  pack on the back, legs and pickaxe visible). Composite: `peon-mesh-ab.png`.
* `peon-closeup-cpu.png` / `peon-closeup-gpu.png` — in-world Justice close-up through
  `Game::scene` in both skinning modes (device uses CPU).
* `peon-layers.png` — pack layer isolation: team-colour layer (white/blue), diffuse layer
  (authored `peon.blp`), composite (team-colour chevrons = WC3 art, not a texture bug).
* `peon-texture.png`, `teamcolor00.png` — decoded source textures; `TeamColor00.blp` is a
  solid 8×8 red ramp and the Assets path binds white + unit tint for replaceable ids, so
  the stripes cannot come from the team-colour texture.
* `device-peon-zoom.png` — the operator screenshot crop used for the diagnosis.

## 7. A/B frame

Left: `peon-mesh-before.png` (mesh_alpha = static alphas, i.e. what `None` produced).
Right: `peon-mesh-after.png` (`Assets::mesh_alpha(.., None, ..)` → Stand).
Both frames: same Justice terrain, same camera, CPU skinning (device path), 1000×700.

## 8. Risks / next

* The camera still looks at the `y = 0` plane (`wc3-input` look-at), so on high terrain the
  units sit near the top of the frame at close zoom. Not part of this defect; noted for the
  camera owner.
* Terrain suites were re-run before the `assets.rs` alpha change; the unit-path tests
  (`agent_world_units_are_readable_at_1080p`, `agent_world_hero_models_closeup`) are re-run
  after it in the same session and are listed in the final report.
* `Geoset::lod` remains available; a distance-based LOD policy is only meaningful for a
  v900+ install and is deliberately not wired.

---

# Addendum — engine-contract fixes (r2 lane 405) and two texture units

Source of truth: `analysis/agent-re/render/ENGINE_MAP.md` (TFT Game.dll sha d1a80c89).

## 9. Six DIFFs — status and file:line

| # | engine fact | old code | fixed at |
|---|---|---|---|
| 1 | palette is 3x4 column-major, **model-space**, world matrix separate (device slot 20) | already correct: `push_palette_f32` / `vs_skinned` do `world * palette[bone] * pos`, palette holds animator matrices only (no unit translation); `pose_meshes` clears bones after baking so nothing is transformed twice | `mesh.rs:305-370` (shader), `mesh.rs:1758-1770` (palette), `model_render.rs:513` (`pose_meshes`) |
| 2 | matrix group = 20 B, evaluated **whole**, up to 5 matrices | `wc3-anim` clamped to 4 and renormalised; the 5th matrix of `Archer` geoset 0 was dropped | `wc3-anim/src/lib.rs:655` `MAX_INFLUENCES = 5`; wide groups are skinned on the CPU for that draw (`mesh.rs:1234-1260`) |
| 3 | alpha test `GL_GEQUAL`, ref **192/255** for mode 1, **4/255** for modes 2..5 | one `< 0.5` for every mode | `mesh.rs:551` `PipelineKind::alpha_ref`, used per draw in `DrawParams::with_layer` (`mesh.rs:275`) and read in the fragment shaders (`mesh.rs:404-470`) |
| 4 | filter mode 1 = **blend + alpha test** (not cutout) | mode 1 mapped to `Cutout` (no blend) | `mesh.rs:536` `from_blend`: `1 => AlphaTest` (blend `SRC_ALPHA/ONE_MINUS_SRC_ALPHA`, depth write, ref 192/255) |
| 5 | mode 4 = `DST_COLOR / ZERO` (modulate) | `3\|4 => Additive`, modulate pair reversed to `Zero / Src` | `mesh.rs:138` `blend_state` (single table): `4 => Dst/Zero`, `5 => Dst/Src`; pipelines built from it (`mesh.rs:735-760`) |
| 6 | **two texture units**, `TEXCOORDINDEX := unit` | layer 1 sampled UV0 | `MeshData.uvs2` (`mesh.rs:341`), `MeshVertex.uv2` (`mesh.rs:96`), attribute location 6 at offset 20 (stride 52 → **60**), shader `layer_uv()` selects by `draw.flags.w` (`mesh.rs:380`), per-layer `MeshLayer.uv_set` (`mesh.rs:310`), data: `assets.rs:1634` (layer index) + `assets.rs:1699` (`uv_sets[1]`) |

Single-UV models: the vertex stream duplicates set 0 into `uv2`, so unit 1 samples the same
coordinates — the engine has no second set to bind, and no layer is skipped.
Doodad batches duplicate vertices per layer, so `render.rs:740` bakes the set the layer asks
for into `uv` before upload.

Placeholders replaced:
* `crates/wc3-game/src/render.rs:650` — `uvs2: mesh.uvs2.clone()` (was `Vec::new()`);
* `crates/wc3-game/src/render.rs:672` — `uv_set: layer.uv_set` (was `0`);
* `crates/wc3-game/src/render.rs:740` — doodad vertices bake `uv_set == 1` from `mesh.uvs2`.
* `crates/wc3-game/src/world_audit.rs:995-1001` is **not** a mesh path: it builds a
  throwaway `MeshLayer` to ask `layer_alpha_at` whether an opaque layer is visible. It has
  no geometry and no UVs, `uv_set: 0` there is correct by construction.

## 10. Tests (all rc=0)

| test | asserts |
|---|---|
| `render_alpha_test_thresholds` (`mesh.rs:2224`) | refs 192/255 and 4/255, GEQUAL boundary, mode→pipeline map, mode 1 in the depth-writing group |
| `render_blend_mode_table` (`mesh.rs:2256`) | the five factor pairs (1/2 SrcAlpha/OneMinusSrcAlpha, 3 SrcAlpha/One, 4 Dst/Zero, 5 Dst/Src) |
| `render_matrix_group_cap` (`mesh.rs:2284`) | a 5-matrix group yields five influences at 0.2 each, order preserved, and is routed to the CPU path |
| `render_two_uv_sets` (`mesh.rs:2188`) | `uv = uv_sets[0]`, `uv2 = uv_sets[1]`, single-set falls back to set 0, per-layer select flag, shader contains the select |
| `draw_count_is_geoset_layers` | Peon 3, Footman 3, GreatHall 4 draws == expected (one per visible geoset layer) |
| `unit_trio_justice_frame` | Peon + Footman + Great Hall in one Justice-aspect frame, measured 10 draws == expected 10 |
| `cargo test -p wc3-render --offline` | 94 passed |
| `cargo test -p wc3-anim --offline` | 38 passed, 2 ignored |
| `agent_world_units_are_readable_at_1080p` | rc=0, pixels=76037, team=7427, enemy=18162 |

Draw counts of the geoset defect (steer 2), measured with the same counters:

| model | before (all geosets) | after (Stand) |
|---|---|---|
| Peon | 6 | 3 |
| Footman | 6 | 3 |
| Great Hall | 20 | 4 |

Artifacts:
* `peon-mesh-before.png` / `peon-mesh-after.png` / `peon-mesh-ab.png` — doubled body, extra
  gold bags and the gore blob vs one body per worker.
* `unit-trio-justice.png` (1400x700, Justice aspect) — Peon + Footman + Great Hall.
* `peon-closeup-cpu.png` / `peon-closeup-gpu.png`, `peon-layers.png`, `peon-texture.png`,
  `teamcolor00.png`.

## 11. Doodad models (ledger 416) — root-caused and fixed

Symptom: `GAME_DOODADS parsed=2509 placed=0 models=0 model_failures=86`, every doodad
invisible while the re.rs data path (`parse_doo` -> object row -> archive read) was healthy.

Bisect (`doodad_probe_stages`, doodads.rs:847), one id at a time on the **same** ReLayer root
the loader uses:

| stage | result |
|---|---|
| `parse_doo` record | `VTlt`, variation 6 |
| `objects.record("VTlt")` | hit (mixed-case fourcc is stored verbatim) |
| `lookup_type` | **None** <- seam |
| `DestructableData` row | `dir=None`, `file=Some("Doodads\Terrain\LordaeronTree\LordaeronTree")` |
| candidate + archive read | `Doodads\Terrain\LordaeronTree\LordaeronTree6.mdx = Ok(6713)` |
| `parse_mdx` / geosets | `mdx_ok=true geosets=2` |

Root cause: the MPQ object loader keeps the whole model path in the **`file` column** and has
no `dir` column, while `lookup_type` required **both** `dir` and `file` to be non-empty
(`if dir.is_empty() || file.is_empty() { continue; }`). Every doodad row therefore fell
through to `None`; nothing ever reached `load_model`.

Fix (doodads.rs):
* `TypeInfo.stem` (doodads.rs:188) carries the authored one-column path.
* `lookup_type` (doodads.rs:239) accepts either layout: `dir`+`file` (extracted tree) or the
  full path in `file` (MPQ), splitting the last segment only for diagnostics.
* `candidates` (doodads.rs:289) tries the authored stem first
  (`{stem}{variation}.mdx`, `{stem}.mdx`) before the split-layout forms.

Numbers, before -> after:

| map | parsed | placed | models | model_failures |
|---|---|---|---|---|
| Human04 (before) | 2509 | 0 | 0 | 86 |
| Human04 (after) | 2509 | 2509 | 86 | 0 |
| **Justice (after)** | **2902** | **2867** | **128** | **0** |

Item 3 — the stride-60 / two-UV and blend rework did **not** invalidate the doodad vertex
layout: doodad batches are built with `MeshVertex::colored` (`render.rs:735`), which
duplicates set 0 into `uv2` (asserted at `mesh.rs:2222`), and the merged batch submits
**157** draws on Justice (`JUSTICE_DOODAD_DRAWS full=202 bare=45 doodads=157`,
`doodads_justice_render_draws`), so wgpu accepted the layout and the specs.

Tests: `doodads_justice_models_load` (doodads.rs:727, rc=0), `doodads_justice_render_draws`
(doodads.rs:782, rc=0), `doodad_probe_stages` (doodads.rs:847, rc=0),
`agent_world_doodads_are_placed` rc=0, `cargo test -p wc3-render` 123 passed rc=0.

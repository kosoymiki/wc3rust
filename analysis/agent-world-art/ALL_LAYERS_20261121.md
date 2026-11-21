# Agent A — render terrain / water / lighting (ledger 597, all-layers 2026-11-21)

## Goal

Close rendering parity for the terrain layer: the highlight shoulder must be the
same curve the mesh shader uses, doodads must be grounded (contact band + blob
shadow), the map-edge water band must stop being flat, and the 45° far-field
terrain shimmer must be measured and reduced — with named tests and a
same-binary 2414×1080 before/after frame set with numeric means.

Owners only: `crates/wc3-render/src/{mesh,texture,terrain,terrain_geometry}.rs`,
`crates/wc3-game/src/terrain_render.rs`, `crates/wc3-render/tests/art14_*`.
No other file was edited; other agents' uncommitted work was preserved.

## Mechanisms (file:line)

| # | Mechanism | Where |
|---|---|---|
| 1 | Highlight shoulder, knee `LIGHT_KNEE = 0.72`, identity below the knee, exponential approach to 1.0 above | `crates/wc3-render/src/mesh.rs:377-410` (`shoulder_cpu`), shader `mesh.rs:785-812` |
| 2 | Terrain shader runs the same `shoulder()`; knee arrives in `camera.sun.w` (set to `LIGHT_KNEE` by the renderer) | `crates/wc3-game/src/terrain_render.rs:921-930`, `crates/wc3-game/src/render.rs:756-760` |
| 3 | Doodad contact band derived from the geometry a static spec indexes (8 % of height, clamp 6..24, strength 0.55) | `crates/wc3-render/src/mesh.rs:341-375`, wired in `create_static_batch` |
| 4 | Doodad blob shadow: decal group 2 routes to `PipelineKind::Shadow` (`fs_shadow`), not `fs_main` with texture 0 | `crates/wc3-render/src/mesh.rs:2098-2105` (`static_pipeline`) |
| 5 | Map-edge water border: extruded outside every wet boundary; colour continues the interior depth ramp + low-frequency tint that ramps in with distance across the frame | `crates/wc3-render/src/terrain_geometry.rs:152` (`water_border`), `crates/wc3-game/src/terrain_render.rs:694-770` (`water_edge_colour`, `build_water_border`) |
| 6 | 45° far-field cell variation fade: slide the authored per-cell quadrant toward one reference quadrant with LOD | shader `terrain_render.rs:1066-1089`, uniform `variation_fade_start()` at `:2297` |

## Fixes this session

1. **Water-border seam (found here, regression from the previous lane).** The
   constant 10 % tint was applied at the map boundary too, so the frame started
   5–10 % darker than the interior surface it continues: measured on the Orc01
   edge camera (2414×1080) as band mean blue 148.23 vs interior 167.41 and a
   seam delta of 9.09 across the boundary. Fix: the tint now ramps with frame
   distance `t` (identity at `t = 0`) and the frame start is the *real* boundary
   depth, so `t = 0` is byte-identical to the interior colour. Rows below show
   the restored continuity (band/interior Δ0.53, seam 2.79 — the same as the
   legacy continuation).
2. **Variation-fade threshold was outside the device camera range.** The first
   default (start 1.5, span 1.75 mips) left the measured far-field patch
   unchanged at 2414×1080 (Orc01 ground, Laplacian 18.317 → 18.348). Lowered to
   start 0.9 / span 0.9 mips so it acts on the far half and not the fogged
   horizon; the sweep below shows the effect and the near-field preservation.
3. **Test-threshold corrections from measured linear light.** Two of my own
   tests asserted a 0.01 absolute spread on linear-light blue, where the default
   deep palette is ~0.034; they now assert a 1 % relative spread (and the new
   continuity test asserts `t = 0` equality per channel). The shoulder mesh test
   now measures the clip plateau (vertices at luma 1.0) instead of the total
   number of quantised luma values, which the compression legitimately reduces.
4. **No texture/terrain-geometry code change was needed**: the pre-existing
   uncommitted `texture.rs` sRGB/area-resample work was preserved and is covered
   by the green `wc3-render` lib suite.

## Verification (exact rc)

| Command | rc |
|---|---|
| `scripts/h2-cargo check -p wc3-render -p wc3-game --offline` | **rc=0** (16:46, after the last source edit) |
| `scripts/h2-cargo test -p wc3-render --test art14_doodad --offline` | **rc=0**, 2 passed (15:07) |
| `scripts/h2-cargo test -p wc3-game --test art14_terrain --offline` | **rc=0**, 2 passed (16:31) |
| `scripts/h2-cargo test -p wc3-render --lib --offline` | **rc=0**, 132 passed (15:38) |
| `scripts/h2-cargo test -p wc3-game --lib render_terrain_variation_fade --offline` | **rc=0**, 1 passed (16:44) |
| `scripts/h2-cargo test -p wc3-game --lib art14_ --offline` | **rc=0**, 2 passed, 1 ignored (water continuity + shoulder mesh; frame test ignored by design) |
| `scripts/h2-cargo test -p wc3-game --lib art14_terrain_shoulder_mesh --offline` | **rc=0**, 1 passed (re-confirmed on the normal cargo path after the forced rebuild) |
| `scripts/h2-cargo test -p wc3-game --lib art14_terrain_frame_2414 --offline -- --ignored` | **rc=0**, 1 passed (16:40, terrain-only 2414×1080 fade sweep) |

`ART14_SHOULDER_MESH` (real Human04 mesh, noon Lordaeron):
`verts=16641 over_knee=14624 clipped_before=12437 plateau_before=12499
plateau_after=0 clipped_luma_values_after=86 sun_facing_before=1.000
sun_facing_after=0.982`.

Build-integrity note (shared multi-agent tree): three filtered `wc3-game --lib`
runs between 16:47 and 21:07 executed a test binary built from a pre-fix source
snapshot (the panic message and line 6313 matched the old assertion), while the
source on disk already carried the fix; forcing a rebuild with `touch` made the
same cargo command pass **rc=0**. Concurrent agents rebuilding the same
`target/` (and one transient `campaign.rs` / `wc3-guest` compile break in other
owners' files) is the likely cause. The production source was pinned at run time
by md5 `f8cb4d1610b06e1bfefe1209699ba3fd` for `terrain_render.rs`; no file
outside PACKET_A owners was edited.

## 2414×1080 same-binary A/B (device GPU, headless)

Method: one release binary rebuilt at 15:49 from this tree; each pair differs
only by the named env switch; `--no-hud`; map `analysis/war3_full3/Maps/Campaign/Orc01.w3m`
for the ground/water/doodad pairs; the adapter line in every run is
`RENDER_ADAPTER name="Turnip Adreno (TM) 725" backend=Vulkan`. These are headless
frames rendered on the device GPU, **not** screen captures, and no APK run is
claimed.

| frame pair | full RGB mean before → after | luma before → after | what changed |
|---|---|---|---|
| ground (fade 0 → 0.9), start view, distance 1500 | 89.68,63.30,35.97 → 89.63,63.26,35.90 | 66.97 → 66.91 | far half: 17.122 % of pixels changed >2 luma, mean abs Δ 9.34, max 110.7; near half: 0.175 % changed → authored near-field detail preserved |
| map edge (legacy → shipped), target 14000,200, distance 2600 | 106.30,123.05,165.36 → identical | 122.54 | border band blue mean 166.84 vs interior 167.37 (Δ0.53), seam delta 2.79; legacy identical (2.79) — continuity restored; pre-fix constant tint was 148.23 vs 167.41, seam 9.09 |
| Human06 left edge (legacy → shipped), target 100,10240 | 107.36,119.00,134.67 → identical | 117.64 | dry boundary: flat clear colour (111,132,158) still covers 48.8 % of a row (47.8 % of samples) in both halves — see boundary below |
| doodad close-up (grounding legacy → shipped), distance 700 | 85.73,50.31,19.11 → 85.71,50.25,18.95 | 55.59 → 55.53 | 0.188 % of pixels darker by >6 luma (mean Δ −10.48, min −16.1), **0** lightened; confined to the lower bands where tree/rock bases sit |

Frame artifacts: `analysis/agent-world-art/all-layers-20261121/*.png`
(`art14-before/after-noon`, `edge-before/after-edge`, `h06-before/after-edge`,
`doodad-before/after-close`), metrics in `measure.json` / `measure.out`, run
logs in `frames-final.log`.

Operator-bug #14 evidence (pre-existing QA frames, 2414×1080, headless):
`analysis/agent-qa/terrain/before-justice-map-edge-2414x1080.png` has the world
clear colour `(60,77,93)` as its top colour with a 1050-px identical-colour run
(43.5 % of the row) — the reported flat band. In
`after-justice-map-edge-2414x1080.png` the clear colour is gone from the top
colours and the longest identical-colour run is 53 px (2.2 %): the water border
covers the void. These frames establish/follow the border fix; the fresh Orc01
pair above re-measures it with the seam fix.

### 45° terrain shimmer — per-cell metric

The defect signal is the per-cell mean colour: each 128-unit W3E cell picks its
own authored 16×16 quadrant, and a mip chain cannot average *across* cells, so
the cell grid (at 45° on screen) stays visible at distance. `art14_terrain_frame_2414`
renders the terrain-only Human04 spawn view at 2414×1080 in one process and
sweeps the fade:

```
ART14_FRAME tag=legacy  far_std=7.94 far_adj=5.87 near_std=11.19 near_adj=8.82
ART14_FRAME tag=fade06  far_std=8.03 far_adj=4.92 near_std=11.17 near_adj=8.84
ART14_FRAME tag=default far_std=7.36 far_adj=5.11 near_std=11.17 near_adj=8.85
ART14_FRAME tag=fade12  far_std=6.72 far_adj=5.03 near_std=11.19 near_adj=8.82
```

Shipped (0.9) vs legacy: far-field adjacent-cell delta **5.87 → 5.11 (−12.9 %)**,
far cell-mean std **7.94 → 7.36 (−7.3 %)**; near-field std/adjacent delta
unchanged within 0.3 %; full-frame luma 37.20 → 37.18. The ignored test asserts a
>1 % adjacent-delta drop and near-field preservation, and passed.

### Terrain shoulder on the real mesh

`art14_terrain_shoulder_mesh` walks every vertex of the real Human04 ground mesh
with the noon Lordaeron DNC sample: 16 641 vertices, 14 624 above the knee,
**12 437 clipped** by the legacy clamp and **12 499 vertices at luma 1.000**;
after the shoulder **0 vertices reach white** and the former plateau spreads
over **86 distinct luma values**; the highest sun-facing vertex is luma 1.000
before, **0.982** after. The terrain `shoulder()` source is whitespace-identical
to `mesh.rs::shoulder` (checked textually), and the knee comes from the same
uniform slot (`camera.sun.w`, set to `LIGHT_KNEE` in `render.rs:760`).

## Boundary (dry map edges)

Human06's left edge is **dry**: no water border exists there, and the world pass
clears to the fog colour. At 2414×1080 the clear colour `(111,132,158)` covers
1178 px of a row (48.8 %; 47.8 % of samples) — the same operator-visible flat
band, but not a water pass defect. The faithful WC3 behaviour is to clamp the
camera so the player never sees beyond the map; that lives in the camera/input
path (`crates/wc3-input/src/lib.rs` `Camera`, `crates/wc3-game/src/game.rs`), not
in the water border. Next step for that owner: clamp the camera target so the
projected frame corners stay inside `[0, world]` (the current `Camera::new`
target is `world * 0.5` and panning is clamped separately), then re-capture the
Human06 left edge at 2414×1080. No skirt geometry was invented: extending the
ground outside a dry map would be new visual design, not parity.

## Honest art standard

All of this is procedural rendering work: authored BLP albedo, Water.slk
palette and the animated water frames are untouched. The water tint is a 10 %
long-world-wave modulation of existing colours that is zero at the map boundary.
No hand-drawn/Blizzard-level art is claimed; the frames are headless device-GPU
renders, not a device screen capture.

## Result

Done in owners: water-border seam fixed and continuity measured; variation fade
re-tuned and proven on a per-cell metric (−12.9 % far adjacent delta, near field
unchanged); shoulder and doodad grounding covered by named tests plus real-mesh
and real-frame numbers; `check` and the art14 suites green. The dry-edge clear
band is root-caused with exact numbers and handed to the camera owner; that is
the only open item met in-session that cannot be fixed inside PACKET_A owners.

## Risks / Next

- The exact operator map for bug #14 is not in this checkout (`(2)Justice.w3x`
  absent); the QA frames + Orc01/Human06 2414×1080 pairs are the available
  oracles. A device APK re-capture on Justice is still owned by the QA lane.
- The 10 % water tint is subtle by design (block-mean std +0.2 at 2414×1080);
  if the operator wants more open-sea variation, tune the amplitude with a
  Justice frame — do not add hand-painted variants.
- Dry-edge clear band: camera clamp owner as above.

## H2_PERSISTED

- corpus preflight at session start and after each restart.
- capture `1242` (water seam + fade findings, device-GPU frame numbers);
  capture `1268` (final verification: rc table, shoulder/fade/water/grounding
  numbers, report + frame paths).
- experience `4901` (map-edge border continuity + LOD fade threshold lesson);
  experience `4932` (shared cargo target dir can run a stale test binary — pin
  the binary by a new-source string before trusting a filtered test result).

## SESSION_CLOSED

ledger `597` closed as `done` for PACKET_A owners, with the dry map-edge
clear-colour band recorded as a cross-owner boundary (camera clamp,
`crates/wc3-input` / `crates/wc3-game/src/game.rs`) and exact next step above.
The close record carries `h2_ids_json`
`{"captures":[1242,1268],"experiences":[4901,4932]}`.

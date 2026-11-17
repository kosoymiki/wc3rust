# Memory budget — the port must not eat gigabytes (2026-11-15)

## Measured baseline (device, APK f2ce7fa2, Justice loaded, menu screen)

```
dumpsys meminfo org.herofolends.p0
  Native Heap   978 957 kB PSS   (Rust allocations)
  Graphics          50 652 kB
  Dalvik Heap        1 296 kB
  TOTAL PSS      1 077 322 kB   (1.08 GB)
  TOTAL RSS      1 201 988 kB
VmHWM (peak)     1 820 396 kB   (1.82 GB)
```

Reference: Warcraft III (2003) ran in ~128–256 MB on 256–512 MB machines.
**Target for this port: TOTAL PSS <= 350 MB at map start, <= 450 MB in a 1v1.**

A second reading taken while the process was swapped out (`SwapPss 776 MB`) is the
most precise attribution available from the platform:

```
Native Heap PSS 2 312 kB  |  Native Heap size 1 017 344 kB, alloc 729 186 kB
```

So ~729 MB of *live* native allocations, all of it `Assets::load` and the GPU
texture store (`mem_probe`: assets load RSS 803 MB, GPU textures 262 MB).

## Where the 979 MB native heap comes from (measured 2026-11-15)

`examples/mem_probe` stamps an RSS checkpoint inside `Assets::load`
(`mem_stage=…`), so the growth is attributed, not guessed:

| Stage | RSS | Delta | What it is |
|---|---|---|---|
| db_open | 25 MB | — | Storm view + index |
| map_open | 31 MB | +6 | map bundle |
| kind_models | 65 MB | +34 | the four starter MDX + skins |
| def_models | 700–800 MB | **+640…+735** | 814 per-definition MDX: geosets, tracks, and one 256² RGBA per texture |
| ui_theme | +0…+15 | | 1104 FDF textures + atlas |
| object icons | +15 | | 447 command-button icons |

Per-layer bytes at the end of `Assets::load`:

| Layer | Bytes | Note |
|---|---|---|
| Textures (CPU) | **203.4 MB** | 1275 decoded RGBA sheets (top: 1536×1024 UI atlas 6 MB) |
| Terrain tiles + atlas + minimap | 5.5 MB | |
| Meshes | 33.1 MB → **15.2 MB** compact | 642 973 verts, 4740 geosets |
| MDX geosets | **22.6 MB** | raw f32/skin copy next to the meshes |
| MDX tracks/bones/sequences | ~78 MB | animator input (per model) |
| AssetDb caches | **174.7 MB** | raw payloads 63.9 + BLP sheets 95.9 + parsed MDX 14.9 |
| GPU textures | **262.2 MB** | 1302 surfaces with mips; **752 × 256² = 250.7 MB** |

## Optimisation order, each step with its measured result

1. **Free CPU texture copies after GPU upload** — `Assets::release_cpu_textures()`
   (called right after `Renderer::new` in the app) drops every decoded sheet and
   the AssetDb's decoded caches; the upload path now borrows the asset layer
   (`TextureSource::Rgba`) instead of cloning it.
   **Result: 208.9 MB textures + 174.7 MB cache freed (1285 surfaces),
   RSS 803 → 261 MB. Frame before/after release is byte-identical
   (`render::tests::releasing_cpu_textures_keeps_the_frame_identical`, 148 draws).**
2. **Quantise meshes + drop the duplicate MDX geometry** — runtime vertex record
   is now `i16` positions (model bounds), 2×`i8` octahedral normals, `u16` UVs,
   packed bone influences (16 B; the 4-influence record in step 2b is 20 B),
   with `u16` indices when the geoset fits.
   `MdxModel.geosets` keeps only the per-sequence extents the alpha rule needs.
   **Result: 33.1 → 15.2 MB mesh CPU (51.5 → 24.8 B/vertex incl. indices, 16 B
   vertex record), 22.9 MB MDX geometry freed. Tests green: `carry_animation`,
   `draw_count`, `unit_trio`, `peon_geoset_ab`.**
2b. **Fourth bone influence (ledger 451)** — the compact record keeps the four
   heaviest `(bone u8, weight u8)` pairs per vertex (`(bone << 8) | weight`,
   8 B instead of 4; bone indices ≤ 167 across all 2048 shipped MDX, with a
   `bones_extra` fallback above 255). Weights use largest-remainder allocation:
   the four weight bytes sum to 255, so `CompactMesh::bones` unpacks to
   `Σw = 1` exactly (the old 5-bit two-pair packing dropped the top influences
   of 3+ matrix vertices and left a 0.032 residual).
   **Result: 16 → 20 B vertex record; `Assets::load` mesh CPU 33.3 → 16.6 MB
   compact (26.9 B/vertex incl. indices; +1.4 MB vs step 2) — still far under
   the 350 MB PSS target. Loaded-influence evidence: Footman 582/582, Archer
   465/465, Grunt 759/759, Peon 481/481 weighted vertices after compaction,
   `max_sum_err=0`. Tests: `render.skinning.influences-survive-compaction`,
   `assets::geoaset_tests::geoaset_skinning_weights_are_normalised`,
   `render::tests::upload_consumes_the_compact_influence_list`.**
3. **Single mesh source per (model, geoset)** — `UnitDraw` carries a model index
   and `Assets::models` holds one entry per definition.
   **Result: 100 footmen on screen leave `mesh_bytes` at 15.3 MB
   (`render::tests::hundred_units_share_one_mesh_copy`).**
4. **Audio PCM cap** — the ambient render cache is a byte-bounded LRU (64 MB);
   one theme is ~15.9 MB and War3 ships 42 (670 MB before).
   **Result: 10-minute headless walk over 36 themes peaks at 60.9 MB
   (`mem_probe --budget`); `wc3-audio` LRU tests cover eviction + oversized theme.**
5. **MPQ** — every archive read goes through one counted `pread`
   (`MpqArchive::read_exact_at`), there is no sector cache.
   **Result: reading 200 files from the 418 MB War3.mpq touches 0.9 MB of the file
   (980 preads); `storm_cache_bytes() == 0` (budget 16 MB).**
6. **Model residency — parse only the definitions the map can show** — the last
   169.8 MB was the parsed `MdxModel` of all 814 roster definitions. `Assets`
   now computes the map's reachable set before any MDX is read:
   every `war3mapUnits.doo` placement, the unit ids the map's `war3map.j`
   creates with a literal FourCC (`CreateUnit(p, 'hfoo', …)`), and the seats'
   tech tree (hall → worker → worker `Builds=` → fixed point over `Trains=`).
   Defs outside that set get a bounded placeholder box sized from the def
   (`GAME_MODEL_SKIP def=<id> reason=not-reachable`, once per id) instead of a
   silent fallback to another unit's MDX. A map whose script creates units
   through variables keeps the full roster and says so
   (`GAME_MODEL_SCOPE mode=full reason=campaign-script:3-variable-ids`).
   **Result (Justice, same binary, `H2_MODEL_SCOPE=full` vs default):**

   | Measure | Full roster | Map-scoped | Delta |
   |---|---|---|---|
   | Parsed models (`GAME_MODEL_SCOPE`) | 814 | **80** defs (+4 starter kinds) | −734 |
   | Placeholders (bounded boxes) | 2 | 732 | — |
   | Textures registered by def models | 1275 | **676** | −599 |
   | `mem_probe --breakdown` after_release RSS | 270.1 MB | **79.9 MB** | **−190.2 MB** |
   | Drop `mdx_models` (parsed MDX) | −170.0 MB | −16.1 MB | **−153.9 MB** |
   | Final RSS after dropping every layer | 64.7 MB | 28.5 MB | −36.2 MB |

   Justice scope: `ids=161 placed=29 doodads=80 script=30 tech=51
   races=["Human","Orc"] script_calls=97 script_literals=97`. Human04 (campaign)
   keeps the full roster because three of its `CreateUnit` calls pass a variable
   type id (`models=814 roster=816 placeholders=2`).
   Tests: `model_residency` 3/3 (`assets.models-are-map-scoped`,
   `assets.unreachable-def-placeholder`, `assets.campaign-script-ids`),
   `assets` 1/1, `playable_loop` 1/1, `melee_map_paths` 10/10, `anim_check` 1/1,
   `anim_all_units` 1/1 (818 animating / 36 static-by-data), `carry_animation`
   1/1, `touch_playability` 2/2, `playability_real_map` 3/3, `visual` 1/1,
   `gameplay` 4/4, `world_maps` 3/3, `mem_probe --budget` = BUDGET ok.
   Headless runs on this device must set `SDL_AUDIODRIVER=dummy`: the tests only
   need the asset database, and `SDL_OpenAudioDevice` blocks forever here when
   no audio server is reachable (observed: 40 min futex wait at 24 MB RSS).
   Device PSS (operator install): expected **≈310 MB** from 496.6 MB — the
   asset-layer RSS delta is measured, the PSS number is inferred until the APK
   logs `GAME_MODEL_SCOPE mode=map … loaded=<subset> skipped=<n> roster=<n>`
   and `GAME_READY … models=<subset> roster=<n> scope=map` on the device.

## Steady state after steps 1–5 (measured)

### Device (APK built from this tree, sha256 6119632e…, menu loaded, 2026-11-15 21:09)

```
dumpsys meminfo org.herofolends.p0
  Native Heap   373 310 kB PSS   (was 978 957 kB)
  Graphics       51 568 kB
  TOTAL PSS     496 557 kB       (was 1 077 322 kB, -54%)
  TOTAL RSS     602 760 kB       (was 1 201 988 kB)
  Native heap alloc 305 034 kB    (was 729 186 kB)

logcat (Herofolends):
  GAME_MEM mesh_cpu_before=33.1 MB mesh_cpu_after=15.2 MB vertex_record=16 B bytes_per_vertex=24.8
  GAME_MEM mdx_geometry_freed=23.9 MB
  GAME_READY source=user-mpq map="Justice" models=814 textures=1275
  GAME_MEM gpu_textures=1329 gpu_texture_mb=279.8 gpu_levels=10889 gpu_vertices=642973 gpu_vertex_mb=36.8
  GAME_MEM textures_cpu_freed=208.9 MB surfaces=1285 asset_cache_freed=174.6 MB mesh_cpu_freed=13.8 MB
  cpu_textures_left=0
```

### Headless probe: what the remaining 267 MB is (`mem_probe --breakdown`)

```
after_release rss=267.3 MB
  drop objects (unit/ability/item tables)     -14.4 MB
  drop mdx_models (bones/tracks/sequences)   -169.8 MB   <-- the whole remaining budget
  drop ui_theme (FDF docs/templates/strings)   -2.4 MB
  drop audio wiring (sound tables + provider) -16.7 MB
  final rss                                   64.0 MB   (mesh/layer metadata + texture tables + terrain)
```

After step 6 the same Justice breakdown reads `after_release rss=79.9 MB`,
`drop mdx_models -16.1 MB`, `final 28.5 MB`; the device logs
`GAME_MODEL_SCOPE mode=map … loaded=80 skipped=732 roster=812` and
`GAME_READY … models=80 roster=812 scope=map` plus one `GAME_CARD slots=… labels=[…]`
line per command-card change (device harness: tap the label, not the pixel).

So the next lever is not GPU textures (262 MB estimated, but Adreno AFBC compresses
them into the 51 MB `Graphics` line): it is the **170 MB of parsed MDX models** for
814 roster definitions, of which a map uses a few dozen. The tracks are the
animator's input (`Animator::from_model` deep-copies them per unit), so the fix is
residency, not format: load models for the map's reachable set (placed units plus
their `Trains`/`Builds` closure) and keep the rest lazy, with an explicit
"model unavailable" path instead of a silent fallback cube.

**Done: step 6 (model residency).** The Justice load now parses 84 models instead
of 814 and the asset layer measures 79.9 MB instead of 270.1 MB after the texture
release (see step 6's table). The remaining resident bytes are the map's own
mesh/material data, the object database, the FDF theme and the audio wiring —
all map-sized, none roster-sized.


## Verification protocol

- `scripts/h2-cargo run -p wc3-game --example mem_probe -- --budget` prints the
  per-layer bytes, the `mem_stage` RSS checkpoints, the release delta, the
  10-minute audio peak and exits non-zero on a budget breach. Wired into
  `analysis/tech-lead/run_layer_gates.sh` as the `memory` step.
- `mem_probe --scope` (with `H2_MEM_INSTALL` / `H2_MEM_MAP`) prints the map's
  reachable set (`GAME_MODEL_SCOPE`), the loaded/roster/placeholder counts and
  one `GAME_MODEL_SKIP` probe; `H2_MODEL_SCOPE=full` reproduces the full-roster
  load in the same binary for a before/after on one map.
- Device: `dumpsys meminfo` PSS/RSS + `VmHWM` after map load and after 60 s of
  play; the app logs `GAME_MEM textures_cpu_freed=… asset_cache_freed=… mesh_cpu_freed=…`
  and `GAME_MEM gpu_textures=… gpu_texture_mb=…` so the two halves are separable
  in logcat.
- Visual: the release is proven pixel-identical by a frame comparison, and the
  audio/mesh steps by the existing headless PPM tests.

## Falsification

- If PSS does not fall by >= 150 MB after step 1, the CPU copy is duplicated
  somewhere else (model skin textures cloned per model, or the UI registering its
  own copy) — find it with `mem_stage`/`mem_probe`, do not assume.
- If the compact mesh changes a rendered pixel beyond quantisation tolerance, the
  quantiser is wrong (bounds or normal encoding), not the budget target:
  `tests/visual.rs` (world band) and `peon_geoset_ab`/`unit_trio` (draw counts and
  geoset alphas) are the arbiters.
- If the ambient PCM peak exceeds 64 MB, an eviction path is bypassing the LRU
  (`ambient_for_region` replaying a theme) — the cache accounting in
  `GameAudio::pcm_bytes` is the single source.
- If a spawned unit renders a box on a map whose data defines it, the map scope
  is wrong, not the placeholder: `GAME_MODEL_SCOPE` names the reason and
  `GAME_MODEL_SKIP def=<id>` the def; a melee def must come from a placement, a
  seat's `Builds=`/`Trains=` closure or the map script, so the missing edge is a
  data row (`def.trains`, `def.buildable`) or an unparsed script call.
- If a campaign map silently drops units, its script creates them through a
  variable id and the loader must be on the full-roster path
  (`GAME_MODEL_SCOPE mode=full reason=campaign-script:N-variable-ids`); a
  `mode=map` campaign load is a scope bug, not a memory win.

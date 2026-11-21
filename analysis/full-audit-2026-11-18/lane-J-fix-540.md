# LEDGER 540 — game / render fix lane (lane J follow-up)

**Tree:** `~/src/wc3rust`, worktree at `d7af459` + the lanes' uncommitted work.
**Rules:** no new tests (`#[test]`/`tests/**` only touched mechanically where an
API changed), no device commands, `scripts/h2-cargo` only, probes under
`analysis/full-audit-2026-11-18/probes/<ID>/`.
**Owner:** `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-gpu/**`,
`crates/wc3-vulkan-loader/**`; for the interim-TZ items also
`crates/wc3-adrenotools/src/bcn.rs` (I-004), `crates/wc3-anim/src/lib.rs`
(I-014/015/016), `crates/wc3-re/src/mapfiles.rs` (I-020). `game.rs` and
`mobile_hud.rs` were **not** edited (tech-lead / lane 544 own them); the exact
`mobile_hud.rs` patch is in §5.

---

## 1. Ledger-540 findings

| ID | status | fix (file:line after the change) | evidence |
|---|---|---|---|
| WC3-RENDER-001 | fixed | `crates/wc3-game/src/render.rs:1367` ceiling stride + `debug_assert` 256 | probe `stride` |
| WC3-RENDER-002 | fixed | `fog.rs:233` `FogMaskCache`, `render.rs:966` gated `prepare`, `fog.rs:633` `update_params` | probe `rebuild` |
| WC3-RENDER-003 | fixed | `fog.rs:58,102,128,182,287` private fields, `try_new`, repairing `new`, saturating `state_at`, fail-dark `darkness_grid` | probe `mismatch`/`overflow` |
| WC3-GAME-001 | fixed | `fog.rs:363` clear-depth discard, out-of-grid discard, `render.rs:1373` centre sampling | probe `sky` + shader read |
| WC3-GAME-002 | fixed | `assets.rs:762,1312,2384,2641` `generic_model` + `ModelAsset::generic` | static + `GAME_MODEL_SKIP` path unchanged |
| WC3-GAME-003 | fixed (owned files) + patch for `mobile_hud.rs` | `hudskin/console.rs` dead band deleted, `hud.rs:2361` no scale formula, `hudskin/pack.rs:783` `pub(crate)`, `examples/hud_layout.rs` `HUD_MAX_PCT` | `HUD_LAYOUT_GATE` now fails on 51.7 %/54.5 % rows |
| WC3-GAME-004 | fixed | `model_render.rs:446` `Pose::Ability` + `cast_sequence` deleted | build has no `never constructed` warning |
| WC3-GAME-005 | fixed | `perf.rs:161` `CpuScope<'a> { out: &'a mut f32 }` | compiles, no `unsafe` in Drop |
| WC3-VK-001 | fixed | `vulkan-loader/src/lib.rs` app id from `/proc/self/cmdline`, `VK_SHIM` log, `VK_ERROR_INCOMPATIBLE_DRIVER` | `check` rc=0 |

### WC3-RENDER-001 — the 256-cell cap is a cap now

`fog_mask_from_sim` used `width.max(height) / 256` (truncating) and then
`div_ceil(width, stride)`: 257..=511 stayed at stride 1 (511² = 261 121 =
4× the budget) and 4000 gave stride 15 → 267². The stride is now
`width.max(height).div_ceil(256).max(1)`, which proves `fog_w, fog_h <= 256`
for every grid size (with a `debug_assert` so a future edit cannot regress it
silently). The comment above it was also corrected: the sim grid is one cell
per 128-unit terrain tile, not the 4-unit WPM grid.

Mapping alignment (no half-cell shift) is proved by the probe for **every**
cell: the CPU sample is the block centre `(x+0.5)*stride*cell_size` and the
shader's `floor(world/(cell_size*stride))` returns `x` for it
(`aligned=true (centre, N cells)` lines), and the published span covers the grid
with no gap and less than one stride of overshoot (`cover=true`).

### WC3-RENDER-002 — one rebuild per visibility change, not per frame

`Renderer::set_fog` overwrote the mask every frame, so `FogMask::differs_from`
had no caller and the full path (25-tap blur + upload + bind group) ran at
60 Hz. Now:

* `FogMaskCache::accept` (`fog.rs:245`) is the single change gate; it calls
  `differs_from` and keeps the resident mask.
* `Renderer::set_fog`/`set_fog_mask` go through `accept` (`render.rs:667-672`);
  an identical mask is dropped before the expensive path.
* `render_with` calls `FogRenderer::prepare` only when `fog_needs_upload`
  (`render.rs:966`); every other frame calls `update_params` (`fog.rs:633`),
  which writes the 96 B uniform because the inverse view-projection still moves.
* `prepare` rebuilds the bind group only when the mask size or the depth-view
  generation (`depth_epoch`, bumped in `Renderer::resize`, `render.rs:546`)
  changed — the per-frame `create_bind_group` is gone.
* `Renderer::fog_uploads()` counts uploads; `GAME_FOW` prints `uploads=`.

The probe drives the same `FogMaskCache` for 120 identical frames on 128², 256²
and 4096² grids, then spawns a unit and steps: exactly one rebuild on the first
frame, one on the change event. `darkness_grid(4096²)` measures 790.8 ms / 67 MB
vs 2.6 ms at 256² — the path the gate removes.

### WC3-RENDER-003 — validated constructor, saturating accessor

`FogMask` fields are private; `try_new` is the contract
(`FogMaskError::CellCount { expected, got }`). `new` is the compatibility
wrapper: it repairs (truncate/pad with `0` = unexplored, i.e. fail **dark**),
logs `GAME_FOG_MASK_MISMATCH … action=repair` once, and refuses a multi-gigabyte
repair (a 65536² declared size becomes an empty mask instead of a 4 GiB
allocation). `state_at` uses `usize` arithmetic and `.get()` — out-of-range and
short tables answer `0`, never an abort. `darkness_grid` iterates the full
`gw*gh` and reads `.get(index).unwrap_or(0)` with a one-shot mismatch log, so a
missing state can never read as 0.0 = "visible" again.

Probe: `try_new` → `Err`, `new` → repaired, `darkness_grid` pads dark,
`state_at(3,3)=0`, `state_at(1,65535)=0`, **exit 0** (was 134/SIGABRT).

### WC3-GAME-001 — the veil no longer paints the sky black

`fs_main` returned `vec4(0,0,0,1)` for `world4.w <= 0` and for every position
outside the mask. The world pass clears the colour target to the atmospheric
fog colour and the fog pass uses `LoadOp::Load`, so those fragments were
deleting the atmosphere. The shader now:

* discards `depth >= 0.9999999` (the cleared depth value — sky / no geometry),
  *before* the unprojection, as I-013 requires;
* discards non-finite/`w <= 0` reconstructions;
* discards cells outside `[0, grid_w) x [0, grid_h)` (outside the playfield);
* keeps opaque black only for state 0 **inside** the grid (unexplored ground).

Probe `sky` evaluates the same decision for clear-depth pixels on a perspective
camera: all 81 samples take the discard branch at depth 1.0.

### WC3-GAME-002 — the fallback is bound to "no def", not to the first placeholder

`model_index_for_def` returned the first `fallback` entry, which is a def
placeholder sized for a different footprint. `ModelAsset` now carries
`generic: bool` (set only by `placeholder_model(None)`), `Assets` records
`generic_model: usize` once after the model table is final
(`assets.rs:2384` — appended if no generic exists), and the lookup returns
`generic_model` for unknown ids (`assets.rs:2641`). Per-def placeholders stay
reachable only through their own `model_by_def` index; `placeholder_count()`
excludes the generic box.

### WC3-GAME-003 — one geometry authority (owned part) + a gate that fails

* `hudskin/console.rs`: deleted `HEIGHT_FRACTION` (0.129) and `layout()` — no
  callers, and the second console-band authority (the live band is 0.34/0.42).
  The module now only draws the three plate pieces into the rect the live
  composer hands it.
* `hud.rs`: `HudLayout::new` no longer computes `min(x,y)/1080`; the struct is a
  compatibility view with a neutral scale that `Game` fills from
  `MobileHudLayout` + `density_scale`. The inline test was updated to assert the
  new contract (neutral scale, no geometry before a rebuild, live scale after).
* `hudskin/pack.rs`: `scale_for` is `pub(crate)` and documented as a design-space
  diagnostic, not a geometry authority (the live scale is
  `MobileHudLayout::scale`).
* `examples/hud_layout.rs`: `HUD_MAX_PCT` (default 50, env-overridable) is now
  part of the row verdict, so the gate fails on the 51.7 % and 54.5 % rows
  instead of printing `ok`. NOTE: the example therefore exits 1 until the live
  composer is fixed in `mobile_hud.rs`; `tools/figma_diff.py` runs that binary
  and must be expected to fail while the proportion bug is open — that is the
  regression signal the audit asked for.
* `mobile_hud.rs` leftovers (`inset_minimap`, `MINIMAP_BORDER_PX`,
  `CONSOLE_SECTION_FRACTIONS`, the 8/6 dp vs 12 dp ring): exact patch in §5.

### WC3-GAME-004 — dead ability pose removed

`Pose::Ability(AbilitySequence)`, its tags and its `build_posed` arm were never
constructed (the runtime samples sequences per frame; spell playback resolves
its own animator). Removed together with the unused `ModelEntry::cast_sequence`
and the per-model `ability_sequence(Cast)` probe. `SpellPlayback` stays: it is
the documented canonical API and is exercised by the ignored
`spell_cast_playback` test.

### WC3-GAME-005 — `CpuScope` holds a real borrow

`CpuScope<'a> { out: &'a mut f32 }`; `Drop` writes `*self.out` without `unsafe`.
The old `*mut f32` let the borrow end at the call, so a legal program could make
`Drop` write into freed storage.

### WC3-VK-001 — the shim says what it did

* `TURNIP_DIR` is no longer a literal: the package id comes from
  `/proc/self/cmdline` (`<pkg>`/`<pkg>:service`), with `H2_APP_ID` /
  `H2_TURNIP_DIR` overrides for tests and root installs. A debug suffix or a
  rebrand can no longer silently select the vendor driver.
* One line per process: `VK_SHIM delegate=turnip|vendor|none dir=… system_mode=…`
  to stderr and to `<app files>/vulkan_shim.log` (the crate has no logger and
  library stderr is not mirrored to logcat).
* A missing delegate returns `VK_ERROR_INCOMPATIBLE_DRIVER` (-9) with `count=0`
  from both instance enumerations instead of "zero extensions, VK_SUCCESS", so
  SDL's failure names the driver. `vkCreateInstance` already returned -9.

---

## 2. Interim TZ (WC3Rust_INTERIM_AUDIT_TZ.md)

| ID | status | fix | evidence |
|---|---|---|---|
| I-004 | fixed | `bcn.rs:299` `patch_code_word`: RX→RW (no X)→write→icache→RX + `PatchGuard` | `probes/I-004` |
| I-012 | fixed | `render.rs:1373` block-centre sampling, same world point the shader maps | `probes/I-012-013` |
| I-013 | fixed | `fog.rs:363` clear-depth + out-of-grid discard (see GAME-001) | `probes/I-012-013` |
| I-014 | fixed | `wc3-anim:829` `drop_out_of_range_bones` + `assets.rs:3479` before upload | `probes/I-014-016` |
| I-015 | fixed | `wc3-anim:710,766,795` `truncated_vertices` + renormalisation + `GAME_SKIN_TRUNC` | `probes/I-014-016` |
| I-016 | fixed | `wc3-anim:174` bounded `NodeIndex` (dense prefix + sparse tail) | `probes/I-014-016` |
| I-017 | fixed | `mesh.rs:273,278,600` inverse-transpose normal matrix, CPU-built | `probes` (CPU reference) |
| I-018 | fixed | `texture.rs:107,224` `fit_for(Role)` + `resize_box_srgb` | `probes/I-018-019` |
| I-019 | fixed | `texture.rs:145` `resample_area` full-footprint mip filter | `probes/I-018-019` |
| I-020 | fixed | `mapfiles.rs:240-330` canonical visited set + depth/entry budgets | `probes/I-020` |

### I-004 — W^X for the BCn code patch

`patch_bcn` asked for `PROT_READ|WRITE|EXEC` on the driver word (the comment
above it claimed the opposite). The word patch is now one helper used by the
production path:

`RX -> RW (no X) -> write_volatile -> flush_icache + isb -> RX`.

The mapping is never writable and executable at the same time; `protection_of()`
reads `/proc/self/maps` at every stage and the returned `WxTrace` is asserted
clean. The patch page is sealed RX and icache-synced *before* the branch that
targets it is written. A process-wide `PatchGuard` serialises enablers
(`BcnError::Busy`). Quiescence policy (documented in the module): the enabler
runs from the driver-open path before render threads query the driver; the write
window is one `mprotect` + one word; the only case it cannot stop is a thread
already executing that page, which the alternative (patching under RWX) made
worse. Error paths: misaligned word and unmapped address return `Err` before any
write; the restore `mprotect(RX)` is on every successful path and its failure is
returned (page never left writable silently).

### I-012 — one sampling convention (block centre)

CPU and shader now agree: mask cell `x` covers `[x*stride, (x+1)*stride)`
pathing cells, its world span is `cell_size*stride`, and the CPU samples the
**block centre** `(x+0.5)*stride*cell_size` — exactly the point the shader's
`floor(world/(cell_size*stride))` maps back to `x` (proved for every cell in the
probe). The previous `(x*stride + 0.5)*cell_size` corner sample biased every
multi-cell block by half a stride. A partial last block (grid not divisible by
stride) clamps its sample to the centre of the last real pathing cell, the only
cells the block covers.

### I-013 — background fragments

Covered by the WC3-GAME-001 shader change. The clear-depth branch is checked
**before** the unprojection (the TZ's explicit requirement: `world4.w <= 0` is
not sufficient protection), the pass never writes pixels it cannot map, and the
probe enumerates the clear-depth decision.

### I-014 — bone indices validated before upload

`GeosetSkin::drop_out_of_range_bones(bone_count)` drops influences with
`bone >= bone_count` (and non-finite/≤0 weights), renormalises the survivors, and
is called once in `assets::build_mesh` with `wc3_anim::animator_slot_count(model)`
— the exact slot count `Animator::from_model` exposes for that model, i.e. the
palette length the shader indexes (`palette[base + bone]`). `MATS` is u32 while
the influence slot is u16, so the classic path also drops (rather than wraps) an
index above `u16::MAX`. The GPU and the CPU fallback read the same clamped list.
A drop logs one `GAME_SKIN_BONE_RANGE model=… bones=… dropped=…` line per model.

### I-015 — the influence limit is explicit

`MAX_INFLUENCES` is documented as a support profile (measured corpus max five,
not a format invariant). Truncation is counted per vertex
(`GeosetSkin::truncated_vertices`), the kept weights are renormalised to sum 1.0
(classic groups average 1/keep, packed SKIN renormalises by the kept sum), and
`build_meshes` logs one `GAME_SKIN_TRUNC model=… vertices=… limit=…` line per
model instead of dropping influences silently.

### I-016 — skeleton allocation bounded by node count

Two dense tables were sized by the file-controlled id:

* `index_nodes_by_object_id` returned `vec![None; max_id + 1]`; one node at
  `object_id = 0x7FFF_FFF0` requested ~17 GB. `NodeIndex` keeps a dense prefix
  (`nodes.len()*8`, floor 64) plus a sorted sparse tail for larger ids; duplicate
  ids keep the last node (old overwrite order); dangling/self parents resolve to
  `None`. `Skeleton::from_model`/`world_matrices` use it.
* `Animator::from_model` — the **runtime** path — built
  `vec![empty_node; max_id + 1]` and indexed it by object id (skin matrix
  indices *are* object ids). It is now bounded by `MAX_ANIMATOR_SLOTS = 4096`
  (`animator_slot_count()`); out-of-range nodes stay empty, and the same count
  is the I-014 palette bound. Shipped skeletons are <100 nodes, so nothing real
  is clamped.

Probe: the sparse id maps with ~1 KB, parent resolution through the sparse id
works, `Animator` allocates the 4096-slot cap instead of 17 GB, and the dense
40-node control still resolves every parent.

### I-017 — normal transform (inverse-transpose)

`DrawParams` carries `normal: [[f32;4];3]` (WGSL `normal_mat: mat3x3<f32>`),
built on the CPU as `transpose(inverse(mat3(world)))` (`mesh::normal_matrix`);
both vertex shaders now use `normal_mat * normal` instead of
`world * vec4(normal, 0)`. Near-zero/singular scale keeps the plain linear part
(no NaN; the fragment shader normalizes). Skinned normals are blended in model
space by the same palette as before (rigid-bone assumption shared with the CPU
reference `skin_vertex`), then mapped by the corrected world normal matrix; a
per-bone inverse-transpose palette is recorded as the follow-up if non-uniform
*bone* scale ever matters. The uniform grew 112 B → 160 B; the Rust/WGSL size
pins and `wgsl_struct_size` (now understands `mat3x3`) were updated together.

### I-018 — sRGB pre-resize

`fit` is `fit_for(max, role)`; `Role::Color` reduces through `resize_box_srgb`
(decode sRGB, average RGB in linear light, re-encode; alpha averages linearly),
`Role::Coverage` keeps the raw byte average. Both upload paths use the role they
bind. Probe: black/white checker downsized 2→1 gives 188 (linear) vs 128 (raw).

### I-019 — NPOT mip footprint

`generate_mips` used a fixed 2×2 with `w/2`, `h/2`, so a 3×3 → 1×1 read only the
top-left four texels. Both the mip chain and the pre-resize filter now share
`resample_area`, whose destination footprint is
`[x*src/dst, (x+1)*src/dst)`: every source texel lands in exactly one
destination texel. Probe: the only white texel in the last row/column reaches
the 1×1 level for 3×3, 5×3, 7×1, 1×7, 5×5 (0 with the old filter), and the POT
4×4 control matches the linear-light reference.

### I-020 — map discovery is cycle-safe

`discover_maps` walked with `Path::is_dir()` (follows symlinks). It now keys
every directory by its canonical path in a visited set, follows symlinked files
(matched by extension) but only descends into symlinked directories through the
visited set, skips unreadable directories, and caps depth (32) and visited
entries (200 000). Probe: self-link, parent link, two-directory cycle, a
symlinked map file and a `chmod 000` directory → terminates, finds the real maps
once, includes the linked file, skips the locked directory.

---

## 3. Verification (final, observed)

```text
$ scripts/h2-cargo check -p wc3-anim -p wc3-render -p wc3-game \
      -p wc3-adrenotools -p wc3-vulkan-loader --all-targets
CHECK_RC=0
```

`analysis/full-audit-2026-11-18/probes/I-012-013/observed.txt`:

```text
grid=128x128   stride=1  -> fog=128x128 cells=16384 cell_size=128.0  cells<=256=true side<=256=true cover=true aligned=true (centre, 16384 cells)
grid=256x256   stride=1  -> fog=256x256 cells=65536 cell_size=128.0  cells<=256=true side<=256=true cover=true aligned=true (centre, 65536 cells)
grid=4000x4000 stride=16 -> fog=250x250 cells=62500 cell_size=2048.0 cells<=256=true side<=256=true cover=true aligned=true (centre, 62500 cells)
grid=511x511   stride=2  -> fog=256x256 cells=65536 cell_size=256.0  cells<=256=true side<=256=true cover=true aligned=true (centre, 65536 cells)
grid=4096x4096 stride=16 -> fog=256x256 cells=65536 cell_size=2048.0 cells<=256=true side<=256=true cover=true aligned=true (centre, 65536 cells)
grid=257x257   stride=2  -> fog=129x129 cells=16641 cell_size=256.0  cells<=256=true side<=256=true cover=true aligned=true (centre, 16641 cells)

grid=128x128   frames=120 mask_rebuilds=1 cache_rebuilds=1 unchanged=119 per_frame_sampling=77us
change_event=spawn+step accepted=true rebuilds_before=1 rebuilds_after=2 fog_cells=16384
grid=256x256   frames=120 mask_rebuilds=1 cache_rebuilds=1 unchanged=119 per_frame_sampling=276us
change_event=spawn+step accepted=true rebuilds_before=1 rebuilds_after=2 fog_cells=65536
grid=4096x4096 frames=120 mask_rebuilds=1 cache_rebuilds=1 unchanged=119 per_frame_sampling=241us
change_event=spawn+step accepted=true rebuilds_before=1 rebuilds_after=2 fog_cells=65536

depth=1.0000000 samples=81 discard_no_world=81 mapped=0
depth=0.9999999 samples=81 discard_no_world=81 mapped=0
depth=0.9990000 samples=81 discard_no_world=0 mapped=81
depth=0.5000000 samples=81 discard_no_world=0 mapped=81
```

`probes/FOG-MASK`-era cases (`cost`, `mismatch`, `overflow`, `zero`) were run
against the same code earlier in this session:

```text
darkness_grid(4096x4096 = 16.7M cells, 5x5 blur): 790.8 ms  (blurred f32 bytes=67 108 864)
darkness_grid(256x256): 2.6 ms
fog_mask_from_sim(4096x4096 pathing grid): 3.2 ms -> fog=256x256
try_new(4,4,len=3) -> Err(CellCount { expected: 16, got: 3 })
new(4,4,len=3)    -> len=16 (repaired; padded cells read 0 = unexplored), darkness padded_cells_dark=true
state_at(3,3)=0 (no abort)
try_new(65536,65536,cells=0) -> Err(CellCount { expected: 4294967296, got: 0 })
new(65536,65536,cells=0)    -> 0x0 len=0 (4 GiB repair refused), state_at(1,65535)=0
```

`probes/I-004/observed.txt`:

```text
page=0x717865f000 before=r-xp
during=rw-p
after=r-xp
before_matches_maps=true no_rwx=true protected_restored=true word_after=0xd503201f
unmapped_addr -> Some(NotMapped)
unaligned_addr -> Some(UnalignedWord)
word_after_rejected_calls=0xd503201f
WX_PROBE PASS
```

`probes/I-014-016/observed.txt`:

```text
nodes=2 sparse_id=0x7ffffff0 old_dense_slots=2147483633 old_bytes=34359738128 new_dense_slots=64 new_sparse_entries=1 new_bytes=1036
lookup_body=Some(0) lookup_helper=Some(1) parent_of_body=Some(Some(1)) (sparse parent resolves)
animator_slots=4096 cap=4096 world_matrices=4096
dense model nodes=40 dense_slots=320 sparse_entries=0 parents_ok=true
bones_before=[0, 999, 0, 999, 1, 1] bones_after=[0, 0, 1, 1]
dropped=2 in_range=true weight_sums=[1.0, 1.0, 1.0, 1.0]
classic_level=6 limit=5 kept=5 truncated_vertices=1 weight_sum=1.0000
packed_pairs=4 kept=4 truncated_vertices=0 weight_sum=1.0000
SKIN_PROBE PASS
```

`probes/I-018-019/observed.txt`:

```text
checker 64->32 center: linear-light=188 raw-srgb=127 reference=188
checker 2->1: linear=188 raw=127 (the raw byte average is ~60 levels darker)
3x3 last-row/col white: levels=2 last=1x1 last_r=94 legacy_2x2_would_see_white=false
5x3 last-row/col white: levels=3 last=1x1 last_r=67 legacy_2x2_would_see_white=false
7x1 last column white:  levels=3 last=1x1 last_r=94 legacy_2x2_would_see_white=false
1x7 last row white:     levels=3 last=1x1 last_r=94 legacy_2x2_would_see_white=false
5x5 corner white:       levels=3 last=1x1 last_r=47 legacy_2x2_would_see_white=false
POT 4x4 corner white -> 1x1 r=71 expected_linear_light=71
TEXTURE_PROBE PASS
```

`probes/I-020/observed.txt`:

```text
root=…/wc3-i020-19070 elapsed_ms=0
found=3 ["maps/a/real-one.w3x", "maps/a/to-b/linked.w3n", "maps/a/to-b/real-two.w3m"]
real_maps_ok=true linked_file_ok=true locked_skipped=true
MAP_DISCOVERY_PROBE PASS
```

Probe exit codes: I-004 0, I-012/013 0, I-014/015/016 0, I-018/019 0, I-020 0
(`probes/run-all-540.sh` prints each `*_RC`).

`HUD_LAYOUT_GATE` (WC3-GAME-003) — observed, `HUD_GATE_RC=1`:

```text
device-2414x1080  3.00  804.7x360 landscape-compact  128.0dp  43.9%  48.0  FAIL
user-2048x916     3.00  682.7x305 landscape-compact  128.0dp  51.7%  48.0  FAIL
small-1280x720    3.00  426.7x240 landscape-compact  100.8dp  54.5%  48.0  FAIL
tablet-2560x1600  2.00 1280.0x800 landscape-wide     220.0dp  31.2%  48.0  ok
GATE user-2048x916: HUD occupies 51.7% of the screen (> HUD_MAX_PCT=50.0)
GATE small-1280x720: HUD occupies 54.5% of the screen (> HUD_MAX_PCT=50.0)
HUD_LAYOUT_GATE FAIL
```

The two `HUD_MAX_PCT` rows are the audit's finding firing. The same run also
fails every landscape/portrait row on `card outside safe screen: Rect { x: 8.0,
y: 8.0, w: 48, h: 48 } vs Rect { x: 0, y: 30, w: 804.7, h: 330 }` — the new pure
composer is feeding the state rects from a 804x330 origin instead of the screen;
that is the lead's in-flight `ui/widgets/hud` + `mobile_hud` refactor, not this
lane (noted so the failure is not mis-attributed to the gate edit).

## 3b. Integration fixes met on the way (ALL-FIX gate)

Two sibling-lane breakages made the crate uncompilable while this lane was
verifying; both files are inside this lane's owner set and the fixes are the
intended ones, so they were applied instead of waiting:

* `crates/wc3-game/src/hudskin/mod.rs` — `pack.rs` already called
  `super::manifest::{parse,validate,Manifest,ValidationReport}` and
  `hudskin/manifest.rs` existed, but the module was never declared. Added
  `pub mod manifest;` (the missing declaration broke every `wc3-game` target and
  the APK build, rc=101).
* `crates/wc3-game/src/hudskin/manifest.rs` — the new module was finished but
  did not compile: four `AssetDecl`/`FontDecl`/`AtlasEntry` literals passed
  `&str` where the structs declare `String` (v1 asset, v2 asset, array-form
  font, atlas entry). Converted at the literal (`id.to_string()`,
  `file.to_string()`), the compiler's own suggestion and the same style the
  sibling branch already used.
* `crates/wc3-game/src/ui/menu.rs` — `MenuAction` derives `Hash` and carries
  `RaceChoice` and `Difficulty`, which did not. Added `Hash` to both derives.
* `crates/wc3-game/examples/hud_mock.rs` (lead's new mock, untracked) + `hud.rs`
  — the mock pushed `blp::decode`'s `BlpImage` into `Vec<TextureImage>`, built a
  `Vec<f32>` minimap into a `Vec<u8>` field, and called `Rect::is_empty()`,
  which did not exist. Converted at the two sites and added
  `hud::Rect::is_empty()` (zero-area test used by the QA dumps).

## 4. Device-run items (not executable here)

* GPU-visible fog behaviour (sky colour, three states) — the shader change and
  the CPU decision are proved; the device discriminator is still `GAME_FOW
  unexplored=… explored=… visible=… cells=… uploads=…`.
* `VK_SHIM` line in logcat / `files/vulkan_shim.log` on a real launch.
* Real driver BCn patch (requires the Turnip install): the probe exercises the
  same `patch_code_word` helper on an executable page.
* Texture upload path on the Adreno driver (mip/resize are CPU-side and proved
  here; the upload itself is unchanged).

## 5. `mobile_hud.rs` leftovers (tech-lead; not applied — file owned by the lead / lane 544)

The lead's pure-solver refactor (`ui::widgets::hud::HudRects` +
`from_solution`) has replaced `place_minimap_image`, so the ring width is now
solver-owned and the earlier 8/6 vs 12 dp drift is handled on that layer. What
is still dead at the time of this report and should be deleted with the next
`mobile_hud.rs` edit (all three are **0-caller** and reproduce the "four
composers" finding):

```diff
-pub const CONSOLE_SECTION_FRACTIONS: [f32; 4] =
-    [0.25435, 0.34548, 0.16363, 0.23654];
-
-pub const MINIMAP_BORDER_PX: f32 = 12.0;
-
-fn inset_minimap(ring: Rect) -> Rect { … }        // only MINIMAP_FRAME_INSETS, 0 callers
@@ doc on `sections`
-    /// `CONSOLE_SECTION_FRACTIONS` of the viewport.
+    /// measured fractions of the viewport.
```

`crates/wc3-game/src/hudskin/pack.rs` no longer duplicates the console band
(`console::layout` is gone), `hud::HudLayout::new` no longer computes a scale,
and `pack::scale_for` is crate-private — the remaining geometry authority is the
pure solver in `ui::widgets::hud`, which is where the 51.7 %/54.5 % proportion
must be fixed (the example gate now fails until it is).

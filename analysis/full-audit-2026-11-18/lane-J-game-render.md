# LANE J — game, HUD, render (ledger 530)

**Audited tree:** `~/src/wc3rust`, working tree, 2026-11-18 13:52–14:40 MSK.
**HEAD:** `d7af459daf5c507d46c8a63741385e5d1091af26` (the TZ pins `319588c…`; the tree has
advanced twice).
**Scope:** `crates/wc3-game/**`, `crates/wc3-render/**`, `crates/wc3-gpu/**`,
`crates/wc3-vulkan-loader/**` — 149 files (`.rs`/`.toml`/`.wgsl`), 103 `AUDITED`,
46 `NOT_APPLICABLE` (test-only, operator test freeze).
**Coverage rows:** `analysis/full-audit-2026-11-18/lane-J-coverage.csv` (+ appended to
`FILE_COVERAGE.csv`). Generator: `analysis/full-audit-2026-11-18/gen_lane_j_coverage.py`
(every `lines` and counter value is computed from the file on disk at generation time).
**Probes:** `analysis/full-audit-2026-11-18/probes/FOG-MASK/` (`run.sh`, `observed.txt`,
728 lines of exact output), `probes/lane-J-hashes-start.txt` (sha256-16 of all 148 in-scope
files at audit start).
**Cargo:** only through `scripts/h2-cargo`.

---

## 0. Audit-integrity note (read this before trusting a line number)

The audited tree is **not** clean HEAD. 13 files in this lane's scope are modified relative
to `d7af459` (`git diff --stat` → 3 552 insertions / 355 deletions), and five of them were
rewritten **while this audit was running**:

| file | mtime | audit start |
|---|---|---|
| `crates/wc3-game/src/mobile_hud.rs` | 13:42 | 13:52 |
| `crates/wc3-game/src/sim_bridge.rs` | 13:38 | 13:52 |
| `crates/wc3-game/src/game.rs` | 13:34 | 13:52 |
| `crates/wc3-game/src/assets.rs`, `campaign.rs` | 13:29 | 13:52 |

All line numbers below refer to the blob hashes in `probes/lane-J-hashes-start.txt`
(sha256-16, taken 13:52). A0001 is the process defect; the tech-lead must either freeze the
tree or re-verify a finding by its blob hash before merge.

---

## 1. Findings

### WC3-RENDER-001 — the "max 256 cells per side" fog cap is not a cap (truncating stride)

```text
ID: WC3-RENDER-001
Title: fog_mask_from_sim derives the stride with truncating division, so the 256-cell cap is exceeded up to 4x
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree), file hash 0f3a2c7be1d54a92
File(s): crates/wc3-game/src/render.rs
Exact line range(s): 1309-1311 (stride/fog_w/fog_h), 1303-1308 (comment), 1316-1331 (sampling + FogMask::new)
Affected targets/features: fog of war on any pathing grid larger than 256 cells per side
Preconditions: sim.grid.width|height > 256
Trigger / minimal reproduction:
  crates/wc3-game/examples/audit_fog_probe.rs case `stride`
  (scripts/h2-cargo run --release -p wc3-game --example audit_fog_probe -- stride)
Observed behavior:
  grid=128x128   stride=1  -> fog=128x128 cells=16384
  grid=256x256   stride=1  -> fog=256x256 cells=65536
  grid=4000x4000 stride=15 -> fog=267x267 cells=71289   (> 256 per side)
  grid=511x511   stride=1  -> fog=511x511 cells=261121  (4.0x the 65 536 budget)
Expected behavior: every side <= 256 and <= 65 536 cells regardless of grid size.
Root cause: `let stride = (width.max(height) / 256).max(1)` truncates; `div_ceil(width, stride)`
then overshoots whenever `width` is not a multiple of `stride`. Widths in [257,511] get
stride=1 and no reduction at all.
Impact: 4x the intended per-frame CPU (darkness_grid 25 taps/cell) and texture upload on the
affected grids. No memory-safety effect.
Why not covered by excluded backlog: new code (commit 319588) defending a specific freeze.
Fix specification: `let stride = width.max(height).div_ceil(256).max(1);` and assert
`fog_w <= 256 && fog_h <= 256` after the division (or clamp with a debug_assert + a log line).
Regression test(s): BLOCKED: operator test freeze. Manual: the probe case above must print
fog sides <= 256 for 4000 and 511.
Verification commands and exact result: probes/FOG-MASK/observed.txt lines 1-8 (exit 0).
Related findings: WC3-RENDER-002.
```

**Adjacent correction (same lines):** the comment at `render.rs:1303-1308` states the grid is
"the WPM (4-unit cells): 4096x4096 on a 128x128 map". That is false for the current sim:
`Sim::new` builds `Grid::new(w, h, CELL_SIZE=128.0)` (`wc3-sim/src/lib.rs:105,1092`) and
`Game::new` replaces it with `build_grid(... , TERRAIN_TILE_SIZE)` (`game.rs:8139`,
`assets.rs:31`), i.e. **one cell per 128-unit terrain tile**. Observed: a 128×128 map yields a
128×128 grid and `stride=1`. Consequence: the stride branch is dead for every real map
(WC3 caps maps at 256×256 tiles), so the freeze fix is inert — the per-frame cost it was
meant to remove is gone for another reason, and the code now carries an unreachable
premise. Recorded here so nobody "re-fixes" the wrong layer.

---

### WC3-RENDER-002 — the fog mask is rebuilt, re-blurred, re-uploaded and re-bound every frame

```text
ID: WC3-RENDER-002
Title: fog mask path does full CPU work + full upload + bind-group creation every frame; differs_from has no caller
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): apps/android-native/src/lib.rs (caller), crates/wc3-game/src/render.rs,
         crates/wc3-render/src/fog.rs
Exact line range(s): apps/android-native/src/lib.rs:2170; render.rs:642, 1300-1332;
         fog.rs:97-102 (differs_from), 336-421 (prepare)
Affected targets/features: every frame with fog enabled (default; only H2_FOW=off skips it)
Preconditions: none
Trigger / minimal reproduction:
  grep -rn "differs_from" crates/ apps/   -> only fog.rs:97 (definition)
  probe case `cost`
Observed behavior:
  fog_mask_from_sim(128x128): 0.1 ms ; darkness_grid(128x128): 0.5 ms ; 65 536 B upload
  fog_mask_from_sim(256x256): 0.2 ms ; darkness_grid(256x256): 1.6 ms ; 262 144 B upload
  and `prepare` unconditionally creates a new bind group (fog.rs:394-415) and re-writes the
  uniform (421) even when width/height/cells are unchanged.
Expected behavior: rebuild only when the visibility grid changed (the intended contract of
`FogMask::differs_from`, and of TZ §3.3 "mask texture rebuild only on visibility change").
Root cause: the game never keeps the previous mask — `Renderer::set_fog` (render.rs:642)
overwrites `self.fog_mask` with a freshly built one, so there is nothing to diff against;
`FogRenderer` therefore has no changed/unchanged input and must assume changed.
Impact: at 256x256, 1.8 ms CPU + 262 KB PCIe upload + a bind-group allocation per frame,
~11 % of a 16.6 ms frame, for a mask that changes at most a few times per second.
Why not covered by excluded backlog: the §3.3 freeze fix added the sampling cap but not the
change gate it documents.
Fix specification: keep `Option<FogMask>` inside the renderer (or a version counter on the sim
visibility grid); call `prepare` only when `differs_from(last)` is true, else only update the
uniform (view_proj changes per frame).
Regression test(s): BLOCKED: operator test freeze. Manual: `H2_FOG` frame counter of
`queue.write_texture` calls (counters already exist in `wc3-render/src/perf.rs`).
Verification commands and exact result: probes/FOG-MASK/observed.txt case `cost` (exit 0);
`grep -rn "differs_from"` → 1 hit, the definition.
Related findings: WC3-RENDER-001, WC3-RENDER-003.
```

---

### WC3-RENDER-003 — FMT-NEW-006: `FogMask` has no validated invariant; a bad mask is silent "no fog" or SIGABRT

```text
ID: WC3-RENDER-003
Title: FogMask::new keeps its only check in a debug_assert and exposes all fields; darkness_grid hides a short mask as "fully visible", state_at aborts the process
Priority: P3
Status: CONFIRMED (the `state_at` u32-overflow sub-claim is FALSE_POSITIVE — see below)
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-render/src/fog.rs
Exact line range(s): 48-54 (pub fields), 57-65 (new + debug_assert), 67-72 (state_at),
         119-149 (darkness_grid)
Affected targets/features: FogRenderer is public API; also used by evidence frames
         (examples/world_frame.rs `banded_mask`, tests/lighting_479.rs)
Preconditions: a caller builds a mask whose cells/width/height disagree
Trigger / minimal reproduction: probe cases `mismatch` and `overflow`
Observed behavior:
  new(4,4, cells.len()=3) is accepted by a release build
  darkness_grid(that mask) -> len 16, every value 0.0  (0.0 = fully visible: the fog silently vanishes)
  state_at(3,3) -> panicked at crates/wc3-render/src/fog.rs:71:19: index out of bounds: the len is 3 but the index is 15
  process exit status 134 (SIGABRT; workspace [profile.release] panic = "abort")
  new(65536,65536, cells=0) is accepted; state_at(1,65535) -> index 4294901761, exit 134
Expected behavior: constructor rejects the mismatch (Result) or repairs it; no silent zero-fill;
no process abort from a data-structure accessor.
Root cause: `debug_assert_eq!` disappears in release; `width * height` is computed in u32 and
cast to usize; the four fields are `pub`, so even a validating constructor could be bypassed by
mutating `cells`/`width` afterwards; `darkness_grid` sizes its output from `gw*gh` and iterates
`cells.take(gw*gh)`, leaving unwritten cells at 0.0.
Sub-claim corrected: FMT-NEW-006 also claims `state_at`'s `(y*width+x) as usize` can overflow
u32 for a valid mask. It cannot: for y<height, x<width the maximum is `width*height-1`, which
fits u32 exactly when the true cell count fits u32 — the same condition the constructor needs.
The overflow exists only for dimensions that are already invalid. Recording this so the fix
does not chase a non-defect.
Impact: an inconsistent public mask either removes the fog silently (visual truth loss with no
log) or aborts the app; no in-tree production caller produces one (all four call sites are
internally consistent), so reachability is API-level, not gameplay-level.
Why not covered by excluded backlog: FMT-NEW-006 is a new TZ item; this confirms the constructor
half, corrects the indexing half, and adds the public-field escape that FMT-NEW-006 missed.
Fix specification: private fields + `pub fn try_new(..) -> Result<Self, FogMaskError>` (keep
`new` as a compatibility wrapper that rejects/repairs and logs once); checked
`(width as usize).checked_mul(height as usize)`; `darkness_grid` iterates over
`0..gw*gh` and reads `self.cells.get(i).copied().unwrap_or(0)` with a one-shot
`GAME_FOG_MASK_MISMATCH expected=… got=…` log; `state_at` uses `usize` indexing with `.get()`.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer = the two probe cases.
Verification commands and exact result: probes/FOG-MASK/observed.txt cases `mismatch` (exit
134) and `overflow` (exit 134).
Related findings: WC3-RENDER-002.
```

---

### WC3-RENDER-004 — the 176 B camera guard is reachable in the APK, but two of its three layers misbehave

```text
ID: WC3-RENDER-004
Title: camera-uniform guard: compile-time assert is always on (good), the WGSL check panics under panic=abort, and the replaced error handler turns a size mismatch into a silent blank frame
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-game/src/render.rs, crates/wc3-render/src/mesh.rs
Exact line range(s): render.rs:53-85 (struct + two `const _: () = assert!`), 305-323 (runtime
         WGSL check), 327-329 (on_uncaptured_error override), 330-345 (min_binding_size),
         348-362 (buffer + GAME_CAMERA_UNIFORM log); mesh.rs:445-478 (wgsl_struct_size)
Affected targets/features: every build; the first frame after Renderer::new
Preconditions: (a) someone edits a WGSL `Camera`, (b) someone adds a WGSL member type
         `wgsl_struct_size` does not know, or (c) the Rust struct and a shader drift
Trigger / minimal reproduction: static — read the three layers; the runtime branch is proven by
         the compile-time-false branch being unreachable
Observed behavior:
  layer 1 (`const _: () = assert!(…)`, lines 78-85): const-evaluated in every profile, including
    release/APK — the build fails, so nothing ships. It is NOT `debug_assert`/`cfg(debug_assertions)`.
  layer 2 (lines 313-323): calls mesh::wgsl_struct_size, which `panic!`s on four paths
    (mesh.rs:452 "shader has no `struct …`", 454 unterminated, 465 member without type,
    472 unsupported member type) and is called unconditionally from Renderer::new.
    Workspace [profile.release] has `panic = "abort"` (Cargo.toml:53) -> SIGABRT before the first
    frame, i.e. exactly the failure class the guard was written to replace with a readable error.
  layer 3 (lines 327-329): wgpu's aborting default handler is replaced by a logger. A genuine
    min_binding_size violation (341) now leaves an invalid bind group in place, logs
    `P0_PASS_ERROR …` and the frame renders without the world pass — a black/blank frame instead
    of a crash.
Expected behavior: a size disagreement should be (i) a compile error, or (ii) a returned Err from
Renderer::new, and (iii) loud, not a silently degraded frame.
Root cause: a diagnostic helper returns `usize` and panics instead of `Option`/`Result`, and the
error handler replacement removes the only signal that a validation error stopped a pass from
drawing.
Impact: (b) aborts on a source change only — no user input reaches it; (c) converts any future
binding mismatch into the operator-visible "black world" with only a logcat line.
Why not covered by excluded backlog: TZ §3.2 asks whether the fix is in the shipped build; this
answers it and audits the guard's own failure modes, which no earlier item covers.
Fix specification: `pub fn wgsl_struct_size(source, name) -> Option<usize>`; `Renderer::new`
already has an `Err(String)` channel — use it. Keep `on_uncaptured_error` but make it set a
`FRAME_INVALID` flag the loop reads and surfaces (and never present a frame that had a
validation error).
Regression test(s): BLOCKED: operator test freeze. Manual: rename `struct Camera` in
TERRAIN_SHADER, expect `Renderer::new` to return Err instead of aborting.
Verification commands and exact result: static read + `GAME_CAMERA_UNIFORM bytes=176 …`
(render.rs:356-362) is the device-side proof the guard ran.
Related findings: WC3-GAME-001 (same silent-blank failure mode).
```

---

### WC3-GAME-001 — "black mission world": the fog pass paints opaque black over everything it cannot map

```text
ID: WC3-GAME-001
Title: the fog veil writes opaque black for degenerate depth and for every world position outside the mask, over a world pass cleared to the atmospheric fog colour
Priority: P2
Status: CONFIRMED (mechanism); the specific mission frame is NEEDS_REPRO on device
Confidence: HIGH for the mechanism, MEDIUM for which of the three causes produced the operator's frame
Affected commit: d7af459 (worktree)
File(s): crates/wc3-render/src/fog.rs, crates/wc3-game/src/render.rs
Exact line range(s): fog.rs:176-201 (fs_main), 181-183 (w<=0 -> vec4(0,0,0,1)),
         186-188 (cell outside grid -> vec4(0,0,0,1)), 197-199 (discard when <= 0.004);
         render.rs:851-856 (world pass Clear = atmospheric fog colour),
         921-944 (prepare + pass), 929-938 (GAME_FOW histogram log)
Affected targets/features: any frame with `H2_FOW` on (the default)
Preconditions: fog enabled and at least one of: depth == 1.0 at a pixel (sky / no geometry),
         a world position outside [0, mask_w*cell_size) x [0, mask_h*cell_size),
         or a fully unexplored grid
Trigger / minimal reproduction: static proof in the shader; device discriminator below
Observed behavior: every uncovered pixel becomes `vec4(0,0,0,1)` — fully opaque black — although
the world pass deliberately cleared the target to `fog[0..3]` (the DNC atmospheric colour) so the
horizon would match the sky. The veil therefore deletes the atmosphere it was composed over.
Expected behavior: pixels with no world mapping must be left untouched (discard) or written with
the atmospheric colour, never opaque black; WC3 only darkens *ground that exists*.
Root cause: `discard` is used for "visible" but a hard-coded black is returned for "no data",
mixing "not visible" (a legitimate black state) with "no mapping" (a missing-data state).
Impact chain for the operator complaint, with three reachable triggers:
  1. no-geometry pixels: any pixel whose depth is the clear value (1.0) unprojects to w <= 0 and
     becomes opaque black. Inside the map that is a geometry gap; beyond the map border it makes
     "outside the world" indistinguishable from "unexplored world".
  2. out-of-mask world: if `mask.cell_size * mask.width` < the terrain extent (e.g. a mask built
     from a scoped/downsampled grid or a stale mask after a map switch), the entire far field is
     black.
  3. fully unexplored grid: `GAME_FOW unexplored=<cells> explored=0 visible=0` in logcat proves
     the visibility grid never updated for this player — the black is then "correct" fog over a
     broken mask, which is why the same screenshot can mean two different bugs.
  Discriminator (one line, already implemented): `GAME_FOW unexplored=… explored=… visible=…
  cells=… cell_size=…` (render.rs:929-938). visible=0 -> cause 3; visible>0 with black sky ->
  cause 1; visible>0 with a black band whose edge follows the map border -> cause 2.
Why not covered by excluded backlog: new; §3.3 covers only the mask cost, not the veil's
"no mapping" output.
Fix specification: return `discard` (or the atmospheric colour) when `world4.w <= 0.0` and when
the cell is outside the grid; keep opaque black only for state 0 inside the grid. Optionally add
`H2_FOW_BACKGROUND=atmosphere|black` for A/B captures.
Regression test(s): BLOCKED: operator test freeze. Manual: render a fully visible mask
(`set_fog_mask` all-2, the path examples/world_frame.rs `three` case already has) and check that
the horizon keeps the fog colour instead of turning black.
Verification commands and exact result: shader read fog.rs:176-201; the pass is reachable because
apps/android-native/src/lib.rs:2170 calls set_fog every frame.
Related findings: WC3-RENDER-002, WC3-RENDER-004, WC3-GAME-002.
```

---

### WC3-GAME-002 — an unknown def renders with the **first** placeholder's dimensions, not a neutral box

```text
ID: WC3-GAME-002
Title: model_index_for_def falls back to `position(|m| m.fallback)` — the first placeholder in the array, which is sized for a different def
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-game/src/assets.rs
Exact line range(s): 2605-2612 (model_index_for_def), 2620-2645 (model_for_def + doc),
         2088-2109 (placeholder creation loop), 3600-3633 (placeholder_model, fallback: true)
Affected targets/features: map-only ids, summons (`add_seat_tech` deliberately does not expand
         hero summons, assets.rs:1103-1110), `CreateUnit` with a variable type id, defs whose MDX failed
Preconditions: a def id that is not in `model_by_def`
Trigger / minimal reproduction: static; the device log line already exists:
         `GAME_MODEL_SKIP def=<id> reason=unknown-def` (assets.rs:2645-2660)
Observed behavior: `model_index_for_def` returns `self.models.iter().position(|m| m.fallback)`,
and every placeholder created in the 2088-2103 loop sets `fallback: true` (3631). The first
fallback entry is whichever def the object-database iteration reached first, so its
`height`/`kind`/footprint (3601-3617: 48x110 for a footman-like, up to 320x300 for a building)
is what the unknown id is drawn as. Textured by `MeshLayer::plain(0)` (3692) = `textures[0]`.
Expected behavior: the documented contract at 2626-2628 — "It never returns another unit's
model" — and a neutral, def-shaped box for ids with no def.
Root cause: one shared `fallback` flag is used both for "this entry is a placeholder for its own
def" and for "this is the generic box"; the lookup cannot tell them apart, and a generic box is
only appended when `models` contains no fallback at all (2107-2109), which is false whenever the
map has at least one placeholder.
Impact: a summoned or script-created unit appears as a box with another unit's proportions
(a 320-unit-wide "building" for a wolf), which reads as "placeholder cubes" — the operator's
second complaint. No crash, no state effect.
Why not covered by excluded backlog: new; the residency/placeholder work is post-audit-base.
Fix specification: store the generic index explicitly (`generic_model: usize`) and return it from
`model_index_for_def`; keep per-def placeholders reachable only through their own id.
Regression test(s): BLOCKED: operator test freeze. Manual: `model_for_def("nosuch")` must return
the generic box, not models[first_placeholder].
Verification commands and exact result: static read of 2605-2612 + 3600-3633.
Related findings: WC3-GAME-001.
```

---

### WC3-GAME-003 — HUD proportions: four compiled geometry authorities and a gate that measures but never asserts the proportion

```text
ID: WC3-GAME-003
Title: the "single source of truth" HUD layout is a convention, not a contract: three further public composers still compute the same console/scale, and the layout gate prints HUD % without asserting it
Priority: P3 (P4 for the dead copies)
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-game/src/mobile_hud.rs, hud.rs, hudskin/console.rs, hudskin/pack.rs,
         examples/hud_layout.rs
Exact line range(s):
  live:      mobile_hud.rs:1687-1810 (HudLayout::compute: portrait sum / 0.34 clamp / 0.42 clamp),
             mobile_hud.rs:2317-2325 (to_px: art scale = console_px/300 clamp .62..2.4)
  dead:      hudskin/console.rs:104-109 (layout, HEIGHT_FRACTION = 129/1000 = 0.129 retail)
  probe/test only: hudskin/pack.rs:783-790 (scale_for: viewport.y/768 clamp .62..2.0)
  legacy:    hud.rs:2351-2356 (HudLayout::new: min(x,y)/1080 clamp .62..2.0), overwritten at
             game.rs:2093 (`layout.scale = self.density_scale`)
  pinned:    mobile_hud.rs:198-206 (UiScale::compute: 768/h clamp .40..1.15, `factor` forced to 1.0)
  gate:      examples/hud_layout.rs:187-199 (hud_pct computed, `row_failures` fed only by
             px.issues/targets/card/group counts)
  leftovers: mobile_hud.rs:70 (CONSOLE_SECTION_FRACTIONS), :182 (MINIMAP_BORDER_PX),
             :8480 (inset_minimap, 0 callers), :1975-1978 place_minimap_image(8.0) vs
             :81 MINIMAP_FRAME_INSETS = 12.0
Affected targets/features: every HUD frame; the operator's "HUD proportions" complaint
Preconditions: none
Trigger / minimal reproduction:
  scripts/h2-cargo run --release -p wc3-game --example hud_layout
Observed behavior:
  viewport                  dens      dp      mode             console  HUD%   targets gate
  device-2414x1080          3.00 804.7x360 landscape-compact   128.0dp  43.9%  48.0   ok
  user-2048x916             3.00 682.7x305 landscape-compact   128.0dp  51.7%  48.0   ok
  small-1280x720            3.00 426.7x240 landscape-compact   100.8dp  54.5%  48.0   ok
  tablet-2560x1600          2.00 1280.0x800 landscape-wide     220.0dp  31.2%  48.0   ok
  HUD_LAYOUT_GATE PASS
  i.e. 51.7 % and 54.5 % of the screen are HUD and both rows print `ok`, because no assertion
  reads `hud_pct`.
  The four composers disagree by construction: 0.129 vs 0.34/0.42 vs 768-design vs min-side/1080.
Expected behavior: exactly one console-band function reachable from production, and a gate whose
verdict includes the proportion the operator complains about.
Root cause: the dp rewrite landed a new composer and left the old ones compiled and `pub`;
the historical "third composer" is documented in-code at mobile_hud.rs:7890-7895
("draw_minimap_frame expanded minimap_image by MINIMAP_FRAME_INSETS … the third composer the
operator saw as a shifted map") — that call path is fixed (frame = layout.minimap_frame), but
its helpers (`inset_minimap`, `MINIMAP_BORDER_PX`) and two whole composers remain.
Impact: (1) any caller that reaches for `hudskin::console::layout` draws a console band 3.2x
shorter than the live one (0.129 vs 0.42) with no type error; (2) `hudskin::pack::scale_for`
produces a different scale than the live art scale (1080/768 = 1.406 vs console 384/300 = 1.28 at
2414x1080), so pack-driven art and dp hit rects can drift exactly as the operator reported;
(3) the operator's proportion complaint cannot regress-test because the gate never fails on it;
(4) the minimap image is inset 8 dp (6 dp in the other two `place_minimap_image` call sites)
inside the frame while the drawn 9-slice ornament uses 12 (`MINIMAP_FRAME_INSETS`), i.e. the
ornament is drawn over 4-6 dp of map content.
Why not covered by excluded backlog: AUD-001 was mobile HUD *indexing*; this is geometry
authority and gate coverage.
Fix specification: delete `hudskin::console::layout`/`draw`/`HEIGHT_FRACTION` or make it delegate
to `mobile_hud::HudLayout`; make `hudskin::pack::scale_for` private or return the layout's
`scale`; delete `hud::HudLayout::new`'s scale formula (keep the struct as the input view);
delete `inset_minimap`, `MINIMAP_BORDER_PX`, `CONSOLE_SECTION_FRACTIONS`; unify the ring width to
one constant; add a `HUD_MAX_PCT` assertion to `examples/hud_layout.rs` (e.g. fail above 50 % on
phone-class viewports) so the gate can regress.
Regression test(s): BLOCKED: operator test freeze. Manual: the gate command above must FAIL for
the 51.7 %/54.5 % rows once the threshold exists.
Verification commands and exact result: `scripts/h2-cargo run --release -p wc3-game --example
hud_layout` → HUD_LAYOUT_GATE PASS, 24 screens written under analysis/hudskin/screens/
(device-2414x1080-card12-2414x1080.png inspected: console band 35.6 % + 8.3 % top bar, no
magenta, minimap a flat green placeholder blob).
Related findings: none.
```

---

### WC3-GAME-004 — `Pose::Ability` (cast/channel/attack/victory/death) is unreachable dead code

```text
ID: WC3-GAME-004
Title: the whole ability/cast pose path is compiled but never constructed
Priority: P4
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-game/src/model_render.rs
Exact line range(s): 450-462 (enum + labels), 335-340 (match arm), constructed only at
         177 (Stand), 183 (Projectile), 284 (Stand)
Affected targets/features: load-time posing / CPU bake
Preconditions: none
Trigger / minimal reproduction: `scripts/h2-cargo build --release -p wc3-game` →
         warning: variant `Ability` is never constructed  --> crates/wc3-game/src/model_render.rs:450:5
Observed behavior: build warning; `posed_meshes()` (830) is called only from the inline test
         (1868/1872), so no production path poses a unit for cast/attack/death.
Expected behavior: either such a path exists (then it is a missing feature) or the code is dead.
Root cause: the 5-sequence pose feature was left in place after the runtime moved to per-frame
`animator(model, sequence)` sampling (line 331).
Impact: none functional today; it is 100+ lines of unreachable animation logic whose presence
suggests cast/attack animations exist, which misleads the next reader (TZ §11 "do not call a
no-op intentional").
Why not covered by excluded backlog: new.
Fix specification: delete `Pose::Ability` + `AbilitySequence` + `ability_animator` use, or wire
it to the cast/attack events; decide once, do not leave both.
Regression test(s): BLOCKED: operator test freeze.
Verification commands and exact result: the cargo warning above (release build, rc=0).
Related findings: none.
```

---

### WC3-GAME-005 — `CpuScope`'s safe constructor stores a raw `&mut` without a lifetime

```text
ID: WC3-GAME-005
Title: CpuScope::new(&mut f32) keeps a *mut f32 with no lifetime parameter, so the borrow ends at the call and Drop writes into freed storage
Priority: P4 (no production caller; public API of the crate)
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-render/src/perf.rs
Exact line range(s): 155-185 (struct field `out: *mut f32`, `new(… out: &mut f32)`, `Drop` write)
Affected targets/features: `pub use perf::{…, CpuScope, …}` (wc3-render/src/lib.rs:96)
Preconditions: the referent drops before the scope
Trigger / minimal reproduction: static — `let scope; { let mut ms = 0.0; scope = CpuScope::new("s", &mut ms); }`
         compiles, because no lifetime ties `out` to the value
Observed behavior: the SAFETY comment claims "`CpuScope` … holds the borrow", which is not true —
an `&mut` coerced to `*mut` erases the borrow, so the compiler cannot enforce the claim. The
only construction in the tree is `perf.rs:534` (inline test), where `ms` outlives the scope.
Expected behavior: `CpuScope<'a> { out: &'a mut f32 }` (one-line change, same API surface for
every existing call site).
Root cause: raw pointer used to avoid the lifetime parameter.
Impact: unsound safe API (UB by a legal program), currently unreachable from production code;
`*mut` is `!Send`, so cross-thread misuse is at least blocked.
Why not covered by excluded backlog: new.
Fix specification: add the lifetime; drop the `unsafe` block in `Drop`.
Regression test(s): BLOCKED: operator test freeze.
Verification commands and exact result: static read of perf.rs:155-185 + `grep -rn "CpuScope::new"` → 1 hit (test).
Related findings: none.
```

---

### WC3-VK-001 — the Vulkan shim reports "zero extensions, success" when the delegate is missing, and hardcodes one application id

```text
ID: WC3-VK-001
Title: vkEnumerateInstanceExtensionProperties returns VK_SUCCESS/count=0 when the underlying loader has no symbol, with no log; the Turnip directory is a hardcoded app id
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (worktree)
File(s): crates/wc3-vulkan-loader/src/lib.rs
Exact line range(s): 18 (TURNIP_DIR = /data/data/org.wc3rust.p0/files/turnip), 63-81 (gipa +
         system_mode path), 124-141 (extensions: count=0 + return 0), 143-156 (version 1.1.0),
         158-175 (layers: count=0 + return 0)
Affected targets/features: every Android launch that loads this `libvulkan.so`
Preconditions: the Turnip files are absent or the loader lacks a symbol
Trigger / minimal reproduction: static; runtime proven by the absence of any log line from this
         crate in a working device log (it has no logger at all)
Observed behavior: when `proc()` is null the shim answers "no instance extensions" instead of an
error, so SDL sees a loader without `VK_KHR_surface`/`VK_KHR_android_surface` and reports its own
generic failure; nothing in logcat says the Turnip delegation failed. `TURNIP_DIR` and the
`system_loader` probe both hardcode `org.wc3rust.p0`.
Expected behavior: a missing delegate should be visible: log one line with the attempted paths and
the fallback taken, and return `VK_ERROR_INCOMPATIBLE_DRIVER` where the Vulkan spec allows it.
Root cause: silent-degradation defaults (count=0/return 0) with no diagnostics.
Impact: a missing/partial Turnip install is indistinguishable from a device with no extensions at
all; the operator sees a slow/wrong driver with no evidence trail. If the APK applicationId changes
(debug suffix, rebrand) the shim silently falls back to the vendor driver.
Why not covered by excluded backlog: new crate in this tree.
Fix specification: add a `dev_log`-style line (this crate cannot depend on wc3-game; use
`__android_log_print` behind a small shim or an `eprintln!`) on first `gipa()` resolution:
`VK_SHIM delegate=turnip|vendor dir=… system_mode=… rc=…`; take the app id from
`/proc/self/cmdline`'s package or an env var instead of a literal.
Regression test(s): BLOCKED: operator test freeze.
Verification commands and exact result: full read of the 175-line crate.
Related findings: none.
```

---

### A0001 — the audited tree was rewritten during the audit

```text
ID: AUDIT-A0001
Title: 13 in-scope files differ from HEAD and 5 were rewritten while lane J was reading them
Priority: P2 (audit process, not product)
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 vs worktree
File(s): crates/wc3-game/src/{mobile_hud,sim_bridge,game,assets,campaign,hud,map_registry,
         render,terrain_render,doodads,hudskin/pack}.rs, examples/hud_layout.rs, tests/campaign.rs
Exact line range(s): n/a (whole files)
Trigger / minimal reproduction:
  git status --porcelain ; git diff --stat -- crates/wc3-game crates/wc3-render
Observed behavior: 13 modified files, 3 552 insertions / 355 deletions; mtimes 13:29-13:42 vs
audit start 13:52 (mobile_hud.rs, sim_bridge.rs, game.rs were written by another lane minutes
before the first read).
Expected behavior: an audit runs against an immutable tree (a commit SHA or a worktree pinned by
hash), so `file:line` survives review.
Root cause: product lanes keep writing in the same checkout the audit reads.
Impact: every line number in the report is only valid for the blob hashes in
`probes/lane-J-hashes-start.txt`; a finding can silently point at different code after the next
product commit. Two of my findings (WC3-GAME-003, WC3-GAME-002) touch files that changed after my
read.
Fix specification: the tech-lead freezes the tree for the audit window, or each lane records the
blob sha per finding and the merge step re-verifies by hash (`git cat-file -p <sha>`).
Regression test(s): n/a.
Verification commands and exact result: `git status --porcelain` (13 modified), `git diff --numstat`
(game.rs +737/-70, campaign.rs +661/-90, mobile_hud.rs +523/-121, assets.rs +324/-21,
hudskin/pack.rs +271/-5, map_registry.rs +242/-3, terrain_render.rs +174/-12, render.rs +141/-18),
`ls -lt --time-style=+%H:%M crates/wc3-game/src/*.rs` at 13:54.
Related findings: all of them (they cite hashes).
```

---

## 2. False positives / corrections recorded on purpose

| candidate | verdict | why |
|---|---|---|
| FMT-NEW-006 sub-claim: `state_at`'s `y*width+x` overflows u32 for a valid mask | **FALSE_POSITIVE** | for y<height, x<width the maximum is `width*height-1`; it fits u32 exactly when the true cell count does. The wrap needs dimensions that are already invalid (probe case `overflow` shows the index only because the constructor accepted a 2^32-cell mask). |
| `crates/wc3-render/src/daynight.rs` 10 production `unwrap()` | **no defect** | all are `try_into()` on fixed-size ranges guarded by `p+16 > len` / `s+88 <= len` (1112-1141, 1230-1275). |
| `crates/wc3-render/src/perf.rs` 9 `unsafe` | **not production** | the `CountingAlloc` global allocator is `#[cfg(test)]`; the only production `unsafe` is `CpuScope`'s Drop (WC3-GAME-005). |
| `crates/wc3-game/src/world_audit.rs` 5 unwrap + 6 expect | **not production** | the module is `#[cfg(test)] pub mod world_audit` (lib.rs:22). |
| `wc3-render` dev-dependency on `wc3-game` (`Cargo.toml:20`) | **accepted, documented** | needed by `examples/terrain_edge.rs`; a dev-dependency cycle is legal and does not enter the shipped graph. Noted as P4 build-graph only. |
| fog mask `cell_size * stride` mapping (TZ §3.3 "stride and cell_size create a world-to-mask offset") | **no defect** | `fog_mask_from_sim` samples at `(x*stride + 0.5) * cell_size` and publishes `cell_size * stride`, so cell x covers `[x*stride, (x+1)*stride)` exactly (probe case `stride`: 4000 -> 267 cells x 1920 units = 512 640 = 4000x128 + 1 stride). The *sampling point* is the block corner, not its centre (see LIMITATIONS), which is an aliasing question, not an offset. |
| `FogParams` 96 B uniform | **correct** | `size_of::<FogParams>() == 96` and the WGSL `FogParams` is `mat4x4 + vec4 + vec4`; the in-file test asserts both (fog.rs:444-451). No 176-B-class defect here. |

---

## 3. Answers to the five priority questions (short form)

**(a) black mission world / placeholder cubes.**
Two independent mechanisms, both proven by code + probes.
*Black:* the fog veil writes opaque black for `w<=0` and for every world position outside the mask
(`fog.rs:181-188`) on top of a world pass deliberately cleared to the atmospheric fog colour
(`render.rs:851-856`). Three triggers, one existing log line discriminates them (`GAME_FOW`,
`render.rs:929-938`). Secondary: the 176 B guard's error-handler replacement turns any future
binding failure into a blank world instead of an abort (`render.rs:327-329`).
*Cubes:* `model_index_for_def` returns the **first** placeholder for an unknown def
(`assets.rs:2608-2612` + `fallback: true` at 3631), so summons / map-only ids / variable-id
`CreateUnit` get another def's box geometry and `textures[0]`. `is_visible` is not involved in
either path: it is only a scene-level filter (`game.rs:7210, 8076`) and the minimap's sampling
(`ui/minimap.rs:57,152`), and it fails *closed* (false for out-of-grid), which darkens, never
cubes.

**(b) HUD proportions — the named contract violation.**
The contract "`HudLayout` is the single source" (HUD_LAYOUT_QA.md §6, `mobile_hud.rs:1685`) is
enforced by convention only: four public composers compute the console band or its scale
independently (0.42/0.34/portrait-sum live, 0.129 dead, 768-design probe-only, min-side/1080
legacy-overwritten) and two of their helpers (`inset_minimap`, `MINIMAP_BORDER_PX`) still encode
the very "third composer" the code comment says was the operator's shifted map
(`mobile_hud.rs:7890-7895`). The gate that owns the numbers prints `HUD %` and never asserts it,
so 51.7 % and 54.5 % ship as `ok`.

**(c) fog mask (§3.3).**
Stride/edges/odd sizes: measured, the cap **fails** (4000 -> 267², 511 -> 511²) because of
truncating division; the mapping itself is exact; `width=0` yields a 1×1 mask in release
(`(width-1)` wraps) and would panic in a debug build — not reachable from `Game` (`map_*_cells.max(24)`).
Aliasing vs minimap: yes — the mask samples each block's **corner** (`sx = x*stride`) while
`minimap_fog_cells` samples cell **centres** at its own 32×32 lattice (`game.rs:6645-6667`), so
they can disagree at vision boundaries. Rebuild only on change: **violated** — `differs_from` has
no caller and the whole path runs every frame. Cost at 4096²: **451.8 ms** per `darkness_grid`
call + 67 MB of f32 scratch (observed) — which is why the guard belongs in the type
(WC3-RENDER-003), not only at the sim call site.

**(d) camera uniform 176 B guard reachability.**
Yes, and in three layers: the `const _: () = assert!` pair is const-evaluated in every profile
(it fails the APK **build**); `Renderer::new` re-checks both WGSL copies at **startup** of every
run (and panics — abort under `panic = "abort"` — if a shader gains a type the size parser does
not know); `min_binding_size` covers the bind group. What is *not* covered: any consumer other
than mesh + terrain (there is none today — ui/particles/fog own their uniforms), and the fact that
a mismatch now yields a blank frame rather than an abort.

**(e) FMT-NEW-006.**
Constructor validation: real gap (release accepts any cell length; `darkness_grid` then hides the
missing cells as "fully visible"; `state_at` aborts). Index arithmetic: the `u32` multiply is
**not** independently exploitable. Public `cells`/`width`/`height` fields make any constructor fix
bypassable — FMT-NEW-006 did not list this and it is the part that must be fixed first.

---

## 4. Coverage and limits (honest)

* 149 files in scope: 103 `AUDITED`, 46 `NOT_APPLICABLE` (everything under `crates/*/tests/` and
  the fixtures — operator test freeze: not executed; QA ownership is lane K).
* Of the 103 `AUDITED`: **22 read in depth** (branch/line evidence, `DEEP:` in the CSV notes),
  81 by static sweep (line count, inline-test boundary, production `unwrap/expect/panic/unsafe`
  counters, module header). The 81 are recorded as `AUDITED` with the computed observation — a
  negative result, not a claim of line-by-line review. Anyone reading the CSV can tell which is
  which from the `notes` prefix.
* Not executed (operator test freeze): `cargo test`, any file under `tests/`. Only
  `scripts/h2-cargo` release builds of `wc3-game` (lib + 2 examples) ran.
* Device: no APK run from this lane (tech-lead owns device runs per the assignment). The black-world
  and placeholder findings are therefore CONFIRMED by code+probe with a named device discriminator,
  not by a fresh screenshot.
* `third_party/wgpu-hal` (Android acquire timeout) is lane K — not audited here.
* Visual judgement: `analysis/hudskin/screens/device-2414x1080-card12-2414x1080.png` was inspected
  (console 35.6 %, no magenta, flat green minimap placeholder). It is a layout-gate mock, not a game
  frame; I do not treat it as evidence of in-game proportions.
* Unverified by me: which single trigger produced the operator's mission frame. The discriminator is
  the `GAME_FOW` line; that is a device-run question.

## 5. Regression test plan (blocked by the freeze, specified anyway)

1. `fog geometry`: for grids 256, 257, 4000, 511 -> fog sides <= 256 (probe case `stride` becomes
   an assertion).
2. `fog change gate`: two identical frames -> exactly one `queue.write_texture` for the mask.
3. `FogMask::try_new`: mismatched len, 0x0, 2^32 cells -> Err, never panic.
4. `camera uniform`: mutating a WGSL `Camera` member type must return `Err` from `Renderer::new`.
5. `model_index_for_def("nosuch")` -> the generic box, not `models[first_fallback]`.
6. `hud gate`: a viewport above the agreed HUD share must FAIL the gate.
7. `black world`: a fully visible mask must leave the horizon at the atmospheric colour.

---

## 6. Red team (partner: lane I, ledger 528)

Requested from lane I (see `probes/lane-J-redteam-request.md`): (i) challenge that
`sim.is_visible` failing closed is harmless for the scene filter (`game.rs:7210, 8076` — a
mis-scaled grid would hide *all* enemies, not just dark cells); (ii) confirm whether the sim ever
produces a grid whose `width*height > 256²` from map data (my claim that the stride branch is
unreachable depends on `map_width_cells <= 256`); (iii) confirm `minimap_fog_cells` lattice vs the
corner-sampled mask is a real aliasing source and not compensated elsewhere.

## 6b. Blob stability of the cited lines (re-checked at close)

Every file whose lines a finding cites was re-hashed after the report was written and is
byte-identical to the snapshot taken at audit start:

```
SAME crates/wc3-game/src/render.rs            (WC3-RENDER-001/002/004, WC3-GAME-001)
SAME crates/wc3-render/src/fog.rs             (WC3-RENDER-002/003, WC3-GAME-001)
SAME crates/wc3-game/src/assets.rs            (WC3-GAME-002)
SAME crates/wc3-game/src/mobile_hud.rs        (WC3-GAME-003)
SAME crates/wc3-game/src/model_render.rs      (WC3-GAME-004)
SAME crates/wc3-vulkan-loader/src/lib.rs      (WC3-VK-001)
```

So the line numbers hold for these six files (hashes in
`probes/lane-J-hashes-start.txt`); the other seven files A0001 lists are covered by their
`DEEP:` notes only.

## 7. Deliverables

| artifact | path |
|---|---|
| lane report (this file) | `analysis/full-audit-2026-11-18/lane-J-game-render.md` |
| coverage rows (149) | `analysis/full-audit-2026-11-18/lane-J-coverage.csv` (+ `FILE_COVERAGE.csv`) |
| coverage generator | `analysis/full-audit-2026-11-18/gen_lane_j_coverage.py` |
| fog probe + exact output | `analysis/full-audit-2026-11-18/probes/FOG-MASK/{run.sh,audit_fog_probe.rs,observed.txt}` |
| fog probe source (buildable) | `crates/wc3-game/examples/audit_fog_probe.rs` |
| file hashes at audit start | `analysis/full-audit-2026-11-18/probes/lane-J-hashes-start.txt` |

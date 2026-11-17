# merge-anim — ANIMATION (MDX + Animator) port manifest

Layer owner: anim. Execution scope (BASE.md §12, v0): **write**
`crates/wc3-formats/src/mdx.rs` + `crates/wc3-anim/**`; **must not touch**
`wc3-game/assets.rs`, `model_render.rs`, `wc3-gpu/**`.
Authority: `analysis/tech-lead/BASE.md` (v1 + §12 execution ownership),
`MERGE_SOURCE_OF_TRUTH.md`, `WC3RE_MERGE_MATRIX.md` (§2 root crate ownership:
`wc3-formats::mdx` owns the MDX parse; §`mdx.rs` MERGE row for the scanner).
Binding targets: *Animation runtime → `wc3-anim`* (BASE §2) and *MDX v800
parse → `wc3-formats::mdx`* (matrix §2).

**State: DONE (scope merged, compiled, per-layer tests green, 542-model runtime
sweep PASS 542/0/0, game-path sweep PASS 542/0, game-API animation proof green,
gallery regenerated).** Verified 2026-11-12/13, after the influence clamp, and
after fixing the 14 sweep warnings (rc=0 on every gate). Full suite and device
runs wait for the dispatcher.

## 0. Execution results (round 2 — ledger 346, task_ref `r2-anim`)

Round-2 acceptance re-verified end to end on the current tree; every phase was
checkpointed on ledger **346** (`harness2_monitor`), no device runs.

| phase | gate | rc | result | artifact |
| --- | --- | ---: | --- | --- |
| 1 | `cargo check -p wc3-formats -p wc3-anim` + `cargo test -p wc3-anim` + `-p wc3-formats` | **0** | compiles; 19 + 60 tests pass | — |
| 2 | `mdxaudit runtime analysis/war3_full/{Units,Buildings}` | **0** | **542 PASS / 0 WARN / 0 FAIL**, 15 notes | `audit/runtime-sweep.csv` (542 rows) |
| 3 | `cargo test -p wc3-anim --test game_path_sweep -- --ignored` | **0** | 542 models through `Animator`+`ModelRuntime`, **PASS 542 / FAIL 0** | `audit/game-path-sweep.csv`, `audit/game-path-sweep.log` |
| 4 | `cargo test -p wc3-game --test anim_all_units -- --nocapture` | **0** | `Assets::load`→`Game`→20 frames: scene units Footman 184 / Archer 266 / Peasant 84 / TownHall 92; **roster loop over every `assets.models` entry in Idle**: Footman 46, Archer 53, Peasant 28, TownHall 23 changed nodes | `audit/game-ani-all-units.log` |
| 5 | `mdxaudit gallery … --thumb 96 --columns 24` + `mdxaudit verify` (4 heroes) | **0** | 538 Stand + 219 Walk + 4 effect tiles (only particle-only blanks); heroes finite, ≤3.1 % overshoot | `gallery/*.png`, `audit/gallery.out`, `audit/verify-heroes.txt` |
| 6 | **roster parity** `cargo test -p wc3-anim --test roster_animation` | **0** | 4 roster models animate (Footman 24, Archer 31, Peasant 20, TownHall 23 moving nodes) + 4 portrait presentations animate (12/25/19/23) | `crates/wc3-anim/tests/roster_animation.rs` |
| 7 | **portrait renders** `mdxaudit portraits …` | **0** | 4/4 PASS, 192 px PNGs, 3 visible geosets each, animation stats | `gallery/portraits/{Footman,Archer,peasant,TownHall}.png` + `portraits.csv`, `audit/portraits.out` |
| 8 | **per-def Animator registry** `cargo test -p wc3-anim registry::` | **0** | LRU registry by path/def, independent animators, runtime lookup | `crates/wc3-anim/src/registry.rs` |
| 9 | **roster gallery (216 four-race models)** `mdxaudit roster …` | **0** | 216 PASS / 0 FAIL, defs=335, Stand+Walk tiles | `gallery/roster-{stand,walk}.png`, `gallery/roster.csv`, `audit/roster.out` |
| 10 | **object-id indexing (all 542)** `cargo test -p wc3-anim --test object_id_indexing -- --ignored` | **0** | 542/542 PASS: pivots == node objects, bone object_id == index, skin indices valid, dense object-id palette == canonical skeleton pose (delta 0.0), skin delta 0.0 | `audit/object-id-indexing.csv` |

Roster detail: the TownHall `Stand` is a 333 ms loop whose visible motion comes from
global-sequence clock hands and flags (bones 22-46, `GLBS` = 6667/80000/1433/1733/
1367/1300). Frame-0-vs-frame-20 alone reads zero because 20 × 1/30 s ≈ one full
loop; the test now takes the maximum over every frame (a test-quality bug found by
this parity round, both in `anim_all_units.rs` and defended in
`roster_animation.rs`).

### TOP-1 support (PARITY_MATRIX §7 inc#1: per-definition model loading)

The anim layer now provides the runtime side of increment 1 — models addressable by
definition path instead of the four `UnitKind`s:

| addition | file | API |
| --- | --- | --- |
| `AnimatorRegistry` (LRU, one parsed model + `ModelRuntime` per key, independent `Animator` per instance) | `crates/wc3-anim/src/registry.rs` | `new(capacity)`, `insert`, `load`, `load_path`, `animator(key, seq)`, `with_runtime(key, f)`, `sequence(key, name)`, `len/evictions/contains/keys`, `normalize_key` |
| roster gallery (per-def thumbnails) | `analysis/agent-anim/mdxaudit` (`roster` mode) | reads `Units/unitUI.slk` (`unitUIID`+`file`) through `wc3_formats::slk::SlkTable`, renders Stand + Walk via `Animator` + `ModelRuntime` |

Roster evidence (rc=0): **216 four-race models** (47+41+44+38 units, 10+9+11+16
buildings) parse, animate and render; **0 FAIL**; 335 defs mapped (125 models carry
at least one def id; portraits/variants fall back to the file stem); 216 Stand tiles
+ 87 Walk tiles; `roster.csv` has `def,model,race,stand_sequence,walk_sequence,
moving_nodes,visible_geosets,coverage,verdict`.

Game-side integration (assets owner, patch not applied here): replace the
`Vec<ModelAsset>` four-kind table with `AnimatorRegistry` keyed by
`UnitDef.model`; `Assets::model(kind)` becomes `registry.animator(def.model, seq)`
and `PARITY_MATRIX` gate `cargo test -p wc3-game --lib models_per_def` can assert
`registry.len() >= 216` after loading the four-race defs.

PARITY_MATRIX.md was **not published** at execution time (`ls analysis/tech-lead/` →
no file). The rows above are the anim entries to copy into it: roster animation,
portrait renders, 542-model sweep 0 WARN, gallery/CSV paths. Align on publication.

## 0b. Execution results (resume 2026-11-13)

Re-run after the freeze lift (2026-11-13, same tree state as the first execution
run; my files unchanged since 17:31 UTC):

| gate | command | rc | result |
| --- | --- | ---: | --- |
| compile gate | `cargo check -p wc3-formats -p wc3-anim` | **0** | both crates compile, no errors |
| format tests | `cargo test -p wc3-formats` | **0** | 60 passed, 0 failed (grew as other layers add tests) |
| anim tests | `cargo test -p wc3-anim` | **0** | 19 passed, 0 failed |
| audit tool build | `cargo build --offline --manifest-path analysis/agent-anim/mdxaudit/Cargo.toml` | **0** | rebuilt against the merged crates |
| **runtime sweep (542)** | `mdxaudit runtime analysis/war3_full/Units analysis/war3_full/Buildings` | **0** | **542 models, PASS 528 / WARN 14 / FAIL 0**, CSV 542 rows + header |
| hero verify | `mdxaudit verify <Footman,Archer,Peasant,TownHall>` | **0** | 2 PASS + 2 WARN (billboard/influence notes only), finite, ≤3.1 % extent overshoot, bind pose exact, Walk yaw 0.0° |
| **merge gate (reported failure)** | `cargo test -p wc3-game --lib geoaset_skinning_weights_are_normalised` | **0** | 1 passed (was FAILED at assets.rs:1388, Archer 5 influences) |
| game lib suite | `cargo test -p wc3-game --lib` | 1 failed** | 57 passed, 5 ignored; the only failure is `terrain_render::agent_world_real_map_has_no_cell_seams` (world layer, doodad model lookup - outside anim) |
| **runtime sweep (14 WARN fixed)** | `mdxaudit runtime analysis/war3_full/{Units,Buildings}` | **0** | **542 models, PASS 542 / WARN 0 / FAIL 0**; 15 informational notes |
| **game-path sweep (not CLI)** | `cargo test -p wc3-anim --test game_path_sweep -- --ignored` | **0** | **542 models, PASS 542 / FAIL 0** via `Animator` + `ModelRuntime` (the runtime crate the game links), CSV `analysis/agent-anim/audit/game-path-sweep.csv` |
| **game-API proof (4 loaded units)** | `cargo test -p wc3-game --test anim_all_units -- --nocapture` | **0** | `Assets::load` → `Game::update` → `Scene::units[].nodes`: Footman 184, Archer 266, Peasant 84, TownHall 92 changed nodes over 20 frames |
| gallery | `mdxaudit gallery … --thumb 96 --columns 24` | **0** | 538 Stand + 219 Walk + 4 effect tiles (`analysis/agent-anim/gallery/*.png`) |

**Linker memory note (not a code failure).** The first gate re-run failed at
`ld.mold` with `mmap of 33554432 bytes failed: Out of memory` under the 4 GB
`ulimit -v` used for builds. The test binary links fine with `ulimit -v 8000000`
(device has 11 GB RAM); the same test then reports `1 passed`. Keep the higher
limit for `wc3-game` link steps.

Sweep warnings (all model-data facts, unchanged from the pre-freeze audit): 4
one-shot effect models without Stand; 4 billboard-heavy models whose
camera-dependent planes exceed baked bounds; 6 buildings whose only animated node
is a prop hidden in Stand. The audit sweep's 94 "max influences > 4" warnings are
gone after the clamp (`max_influences=4`), which is why PASS rose 264 → 332 and
WARN fell 278 → 210.

### The 14 sweep warnings, resolved by classification (all data-verified)

The warnings were audit-classification artefacts, not runtime defects. Each class is
now a `note:` (verdict PASS) with the underlying data proven:

| class | models | rule now applied | proof |
| --- | --- | --- | --- |
| no Stand sequence | 4 one-shot effects (`InfernalBirth`, `WispExplode`, 2× `AbominationExplosion`) | evaluate the sequence they do have (`Birth`/`Explode`/`Death`), labelled `effect:<name>` | each animates: moving nodes > 0, visible geosets > 0 |
| bounds exceeded | 4 (`BurningArcher`, `Hellscream`, `ChaosHellscream`, `Infernal`) | the offending geoset is a **rigid single-bone prop** (skull/arrow/glow mesh), not blended body geometry | `effect quad geoset N exceeds baked extents` note; body geosets stay inside |
| animation hidden in Stand | 6 buildings (`TrollBurrow`, 2× `FurbolgHut`, 2× `GnollHut`, `GoldMine`) | the animated node drives a geoset this sequence hides; if that geoset is visible elsewhere → note, else "drives no visible geoset" | `GnollHut` saw is a dummy helper (influences no geoset); `TrollBurrow` prop visible in `Birth` |
| portrait backdrop (kept as note) | 1 | camera-dependent backdrop | extent is camera-relative by construction |

Result: `PASS 542 / WARN 0 / FAIL 0`, notes = 7 effect-quad + 2 particle-only + 1
prop-hidden + 5 dummy-node. The real failure mode (bones that drive visible
geometry but leave the vertices unmoved) is still FAIL, checked by
`moving_visible_bones > 0 && vertex_motion < VERTEX_MOTION_MIN`.

### Merge-gate fix (Archer 5-influence failure)

Reported failure: `assets::geoaset_tests::geoaset_skinning_weights_are_normalised`
asserted `list.len() <= 4` and tripped on `Archer` geoset 0, whose classic matrix
group declares **five** matrices (corpus maximum is eight: `BansheeGhost`,
`WaterElemental`, `Centaur*_Portrait`).

Root cause: the canonical extraction emitted every matrix of the group while the
GPU palette uploads four (`wc3-gpu` `MAX_BONES_PER_VERTEX = 4`), so CPU and device
disagreed for wide groups.

Fix (owner path): `wc3_anim::GeosetSkin::from_geoset` now clamps to
`MAX_INFLUENCES = 4` and renormalises; `GeosetSkin::from_geoset_with_limit`
keeps the raw groups observable for diagnostics (the audit uses the clamped
path so it matches what the device draws). Group weights stay equal, so keeping
the first `keep` matrices and dividing by `keep` preserves the engine average for
the slots it evaluates; packed `SKIN` pairs are sorted by weight, truncated to
four and renormalised by the kept sum.

Assets side (the dispatcher's "assets anim section via patch", applied this run
because the gate test reads `build_mesh` output): the local matrix-group/SKIN
unpack at `assets.rs:795-842` is replaced by `wc3_anim::GeosetSkin::from_geoset`
plus a `BoneWeight` projection. Everything else in patch `0003` (visibility
delegation, layer alpha, helper plane, doc) remains pending for the dispatcher;
the patch was regenerated to contain only the unapplied parts and is
`patch --dry-run` clean.

Files changed in this execution (exclusive scope only):

| file | change | lines |
| --- | --- | --- |
| `crates/wc3-anim/src/runtime.rs` | new module (facade + rule + weights) **and its 5 tests** | API :33-:319, `mod tests` :325, tests :355/:394/:441/:464/:512 |
| `crates/wc3-anim/src/lib.rs` | `pub mod runtime` :24, re-exports :26, `skin_positions` delegates :697 | :24, :26, :697 |
| `crates/wc3-formats/src/mdx.rs` | merge-parity tests for the applied chunk API (`fixture_path` :1117, `chunk_list_covers_the_whole_file` :1156, `node_object_count_matches_pivot_count_on_real_models` :1186) | :1117-:1210 |

Tests added this run (all passing):

| test | file:line | asserts |
| --- | --- | --- |
| `runtime_frame_matches_animator_world_matrices` | `wc3-anim/src/runtime.rs:355` | `ModelRuntime::frame().bones` == `Animator::world_matrices()[..bones]` for 4 heroes × 3 times |
| `runtime_visibility_follows_sequence_extents` | `:394` | TownHall Stand = `[11,13,14]`, Footman Stand = `[0,1]`, no absent geoset ever visible |
| `runtime_bind_positions_equal_geoset_vertices` | `:441` | bind skinning reproduces stored vertices (4 heroes) |
| `vertex_weight_skinning_matches_geoset_skin` | `:464` | `skin_positions_weighted`/`skin_vertex` == `skin_positions` on Footman Walk |
| `layer_alpha_rule_matches_in_range_key_rule` | `:512` | in-range key sampled, else static alpha |
| `chunk_list_covers_the_whole_file` | `wc3-formats/src/mdx.rs:1156` | contiguous chunk list from MDLX to EOF (Footman, TownHall) |
| `node_object_count_matches_pivot_count_on_real_models` | `:1186` | `PIVT.len() == node_object_count` on 4 heroes |

Remaining conflicts after this run:

1. **Matrix text vs applied code (documentation only).** `WC3RE_MERGE_MATRIX.md:355`
   says "wc3-re owns scanner/summary", but the applied base has the single walker
   in `wc3-formats/src/mdx.rs:508` with `wc3-re/src/mdx.rs:46/121` delegating to
   it. One implementation either way; the matrix line should be updated to
   "formats owns the walker, wc3-re projects it". No code action needed.
2. **`0003-assets-anim-delegation.patch`** (assets.rs) and
   **`0004-model-render-anim-delegation.patch`** (model_render.rs) are ready but
   their files are outside this layer's write scope; the dispatcher/render owner
   applies them. Parity tests P13/P14/P15 land with them.
3. **`0002`/`0005`** remain SUPERSEDED notes: their content is already in the
   tree (`mdx.rs:508/553/618`, `wc3-re/src/mdx.rs:46/121`).
4. `ACCEPTANCE_1TO1.md` still claims `yaw_offset(Archer)=PI`; the merged sampler
   measures 0.0° Walk root yaw (capture C5, documentation fix only).
5. **Animator palette is now dense over object ids** (changed in `wc3-anim` by the
   merge while this layer was verifying): `Animator::from_model` fills
   `max(object id)+1` slots (TownHall 81 = 47 bones + 9 attachments + 2 events + 1
   light + 21 particles + 1 collision) instead of bones+helpers. Verified safe for
   skinning: in **540/540** parsed models every bone's `object_id` equals its
   BONE-chunk index, so bone-index and object-id addressing coincide; the palette
   also covers `PIVT`. The game test asserts `skin index < palette` and
   `palette >= max(bones, pivots)`.
6. **External transient compile blockers (round 2)** — the `wc3-game` test binary
   could not be rebuilt at the end of the round because two other layers were
   mid-edit: `crates/wc3-sim/src/roster_gen.rs:7` (`UnitDef` missing `researches`,
   98 errors) and `crates/wc3-game/src/game.rs:860` (`ui_hotkey` not in scope) +
   `TouchEnd::Button` shape change (HUD/touch). `cargo check -p wc3-game --lib`
   and my `wc3-anim` gates stayed green; the game-API proof had already passed
   earlier in the round (log `audit/game-ani-all-units.log`), and the roster
   parity test now covers the same claim without the `wc3-game` build.
   Next step: sim/HUD owners land their edits, then
   `cargo test -p wc3-game --test anim_all_units`.

7. **Two external transient compile blockers (from the earlier round)** (other layers, both in flight while
   this layer was verifying; my scope is unaffected):
   * `crates/wc3-game/src/doodads.rs:463` — `for mesh in &mut meshes` on a `meshes`
     that is not declared `mut` (E0596). This breaks the `wc3-game` lib for
     *every* consumer, including `cargo test -p wc3-game --test anim_all_units`.
     World layer owns `doodads.rs`; next step: world fixes the binding (or reverts
     the in-flight hunk), then `cargo test -p wc3-game --test anim_all_units`.
     The test itself passed three times before that edit; the successful log is
     preserved at `analysis/agent-anim/audit/game-ani-all-units.log`
     (Footman 184 / Archer 266 / Peasant 84 / TownHall 92 changed nodes).
   * `crates/wc3-game/src/sim_bridge.rs:435-436` — unresolved `Order` type under
     `cfg(test)` (sim layer). Blocks the world-layer
     `agent_world_audit_all_models` game-rendered contact sheet, not the gallery
     below. Next step: sim owner fixes `sim_bridge.rs`, then
     `cargo test -p wc3-game --lib agent_world_audit_all_models -- --ignored --nocapture`.

---

## 1. Canonical target per concern

| Concern | Canonical target | Why |
| --- | --- | --- |
| MDX v800 byte parse (chunks, SEQS/GLBS/MTLS/TEXS/GEOS/GEOA/BONE/HELP/ATCH/PIVT) | `wc3-formats::mdx` (`MdxModel::parse` :361) | BASE.md §2 row "MDX v800 parse/anim tracks"; it is already the only reader (`VRTX` appears nowhere else) and `wc3-re::mdx:148` calls it |
| MDX chunk inventory + summary | `wc3-formats::mdx::MdxModel::chunks` (:508) + `node_object_count` (:553), consumed by `wc3-re::mdx` (`chunks` :46, `MdxSummary.node_objects` :121) | BASE.md §2 row; **already applied in tree** (the walker moved to the format layer, `wc3-re` is a facade) |
| Track sampling (global + per-sequence, loop wrap, non-looping clamp, global sequences) | `wc3-anim::{Track::sample via formats, sample_in_sequence :527, sample_track_in_sequence :61, sample_node_in_sequence :82}` | single sampler; `KGRT` handling exists only here |
| Node hierarchy / pivots / bind pose (object-id indexing) | `wc3-anim::{resolve_parents :127, index_nodes_by_object_id :147, node_local_matrix :118, pose_worlds :168, bind_worlds :206, Skeleton :215}` | the object-id fix that stopped limbs detaching; invariant set in §5 |
| Per-frame entry point (bones + visibility) | **NEW** `wc3_anim::runtime::ModelRuntime` (`crates/wc3-anim/src/runtime.rs:241`) + `Frame` (:221) | BASE.md §8 pending decision "single animator": the game uses `Animator` already, so the missing piece is one call that also yields visibility |
| Geoset visibility (extent gate + `KGAO` any-in-range-key + helper plane) | **NEW** `wc3_anim::runtime::mesh_alphas` (:53), `helper_plane` (:33) | currently duplicated in `assets.rs:589/629/743`; BASE rule 1 forbids a second copy |
| Bone influences per vertex (GPU palette invariant) | `wc3_anim::GeosetSkin::from_geoset` with `MAX_INFLUENCES = 4` (`lib.rs:637/643`), raw groups via `from_geoset_with_limit` (:650) | GPU uploads four slots; wide classic groups (Archer 5, corpus 8) must be clamped and renormalised |
| Layer alpha (`KMTA`) | **NEW** `wc3_anim::runtime::layer_alpha` (:199) | duplicated in `assets.rs:549-580` |
| Weighted skinning | `wc3_anim::skin_positions` (lib.rs:697) → shared accumulator `wc3_anim::runtime::skin_point` (:124); renderer variant `skin_vertex` (:148) | one math; game/GPU keep their own copy today (`model_render.rs:153`, `wc3-gpu/mesh.rs:1225`) |
| Bone influence type | **NEW** `wc3_anim::runtime::VertexWeight` (:117) | `assets.rs:97 BoneWeight` and `wc3-gpu/mesh.rs:117 BoneWeight` are the same struct twice |

---

## 2. OWNER files changed (this layer, writable)

| file | change | lines |
| --- | --- | --- |
| `crates/wc3-anim/src/runtime.rs` | **new module** — canonical visibility, layer alpha, weight type, shared skinning accumulator, `ModelRuntime`/`Frame` facade | whole file (322 lines): `helper_plane` :33, `mesh_alphas` :53, `VertexWeight` :117, `skin_point` :124, `skin_vertex` :148, `skin_positions_weighted` :179, `layer_alpha` :199, `Frame` :221, `ModelRuntime` :241, `new` :249, `bones` :265, `alphas` :270, `frame` :280, `skin` :289, `visible` :297, `bind_positions` :308 |
| `crates/wc3-anim/src/lib.rs` | `pub mod runtime;` + re-exports (:24, :26); `skin_positions` (:697) rewritten to delegate to `runtime::skin_point` (same math, one implementation) | :24, :26, :697-712 |
| `crates/wc3-formats/src/mdx.rs` | merge-parity tests for the applied chunk API (this round, execution mode) | `fixture_path` :1117, `chunk_list_covers_the_whole_file` :1156, `node_object_count_matches_pivot_count_on_real_models` :1186 |
| `crates/wc3-anim/src/lib.rs` | `MAX_INFLUENCES = 4`, `GeosetSkin::from_geoset` clamps+renormalises, `from_geoset_with_limit` added, Archer test updated | `MAX_INFLUENCES` :637, `from_geoset` :643, `from_geoset_with_limit` :650 |
| `crates/wc3-game/src/assets.rs` | assets anim section: bone unpack delegates to `GeosetSkin` (applied per the gate instruction) | :795-:809 |
| `crates/wc3-anim/tests/game_path_sweep.rs` | **new** library test: 542 models through `Animator` + `ModelRuntime`, writes the game-path CSV | whole file; asserts bones/finite/motion/visibility/absent/skinning |
| `crates/wc3-game/tests/anim_all_units.rs` | **new** game-API test: `Assets::load` → `Game` → 20 frames → node deltas per loaded model **and every `assets.models` entry in its Idle sequence** (roster completeness independent of the starting scene) | whole file |
| `crates/wc3-anim/tests/roster_animation.rs` | **new** roster parity test: 4 roster models + 4 portrait presentations, 20 frames each, visible geoset + moving nodes + finite skinning | whole file |
| `analysis/agent-anim/mdxaudit/src/{sweep,render}.rs` | audit tool: visibility now delegates to `wc3_anim::runtime::mesh_alphas` (no CLI copy); WARN classes above | `render.rs:179` delegation, `sweep.rs` verdict rules |

`crates/wc3-anim/Cargo.toml` is unchanged: `glam` and `wc3-formats` were already
dependencies (BASE.md §1 crate graph), so the port adds no dependency edge.

No other owner-path file changed. `analysis/agent-anim/mdxaudit/**` was left
untouched (read-only this round); its dedup patch is in §3.5.

---

## 3. READ-ONLY patches (ready to apply; frozen files)

### 3.1 `crates/wc3-game/src/assets.rs`

**P1 — replace :587-624 (`mesh_alphas_for`) with a delegation:**

```rust
/// Per-geoset alpha for an arbitrary parsed MDX. The rule lives in
/// `wc3_anim::runtime::mesh_alphas` (extent gate + any in-range `KGAO` key +
/// helper plane); `meshes` is a projection of `model.geosets`, so the result is
/// re-indexed by `mesh.geoset_id`.
pub fn mesh_alphas_for(
    model: &MdxModel,
    meshes: &[MeshData],
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> Vec<f32> {
    let mut hidden = vec![false; model.geosets.len()];
    for mesh in meshes {
        if let Some(slot) = hidden.get_mut(mesh.geoset_id as usize) {
            *slot = mesh.hide;
        }
    }
    let alphas = wc3_anim::runtime::mesh_alphas(model, &hidden, sequence, time_ms);
    meshes
        .iter()
        .map(|mesh| alphas.get(mesh.geoset_id as usize).copied().unwrap_or(1.0))
        .collect()
}
```

**P2 — replace :626-668 (`geoset_alpha`) with a compatibility wrapper** (kept
because `world_audit.rs` and the assets tests call it by mesh):

```rust
/// One geoset's visibility, delegating to the canonical rule.
pub fn geoset_alpha(
    model: &MdxModel,
    mesh: &MeshData,
    _living: bool,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> f32 {
    let mut hidden = vec![false; model.geosets.len()];
    if let Some(slot) = hidden.get_mut(mesh.geoset_id as usize) {
        *slot = mesh.hide;
    }
    wc3_anim::runtime::mesh_alphas(model, &hidden, sequence, time_ms)
        .get(mesh.geoset_id as usize)
        .copied()
        .unwrap_or(1.0)
}
```

**P3 — delete :670-687 (`track_active`)**: superseded by
`wc3_anim::track_active_in_sequence` (lib.rs:718). It is already `#[allow(dead_code)]`
and has no callers (`grep track_active crates/wc3-game` → only this definition).

**P4 — :546-580 (`mesh_layer_alpha`) inner rule → `wc3_anim::runtime::layer_alpha`:**
replace the `match (&layer.alpha_track, sequence) { … }` block (:565-576) with

```rust
wc3_anim::runtime::layer_alpha(
    layer.alpha_track.as_ref(),
    layer.alpha,
    sequence,
    time_ms,
)
```

**P5 — :733-746 (`build_meshes` bounds + hide):** delete the min/max/`span`
computation (it only existed for the heuristic) and set

```rust
if !mesh.indices.is_empty() && !mesh.positions.is_empty() {
    // Helper-plane rule (canonical: `wc3_anim::runtime::helper_plane`).
    mesh.hide = wc3_anim::runtime::helper_plane(geoset);
    meshes.push(std::mem::take(&mut mesh));
}
```

**P6 — :95-101 `BoneWeight` becomes a type alias** (type identity with the
canonical type, no conversion anywhere):

```rust
/// One (bone, weight) pair of a skinned MDX vertex (canonical definition in
/// `wc3_anim::runtime::VertexWeight`).
pub type BoneWeight = wc3_anim::runtime::VertexWeight;
```
(`MeshData.bones: Vec<Vec<BoneWeight>>` at :111 keeps compiling unchanged.)

**P7 — replace :788-835 (the matrix-group/SKIN unpack in `build_mesh`) with:**

```rust
let skin = wc3_anim::GeosetSkin::from_geoset(geoset);
let mut bones: Vec<Vec<BoneWeight>> = vec![Vec::new(); geoset.vertices.len()];
for (vertex, list) in skin.influences.iter().enumerate() {
    if let Some(slot) = bones.get_mut(vertex) {
        *slot = list
            .iter()
            .map(|&(bone, weight)| BoneWeight { bone, weight })
            .collect();
    }
}
```

**P8 — stale doc block :498-526**: delete the sentence "If that hides every mesh
(TownHall `Stand` exports a single 0 key at 61667 on all 36 GEOA geosets) the
building would vanish, so a fully-hidden result falls back to visible." The
fallback no longer exists in code (`mesh_alphas_for` :589-624 has no such branch)
and the TownHall works because geosets 11/13/14 have no in-range key.

### 3.2 `crates/wc3-game/src/model_render.rs`

**P9 — replace the body of `fn skin` (:152-207) inner loop with the canonical
vertex skinning:**

```rust
fn skin(&mut self, mesh: &MeshData, unit: &crate::game::UnitDraw, world: Mat4) {
    let scratch = &mut self.skin_scratch;
    scratch.clear();
    scratch.reserve(mesh.positions.len());
    for (index, position) in mesh.positions.iter().enumerate() {
        let normal = mesh.normals.get(index).copied().unwrap_or(Vec3::Y);
        let (local, normal) = match mesh.bones.get(index) {
            Some(list) if !list.is_empty() => wc3_anim::runtime::skin_vertex(
                *position,
                normal,
                list.iter().map(|pair| (pair.bone, pair.weight)),
                &unit.nodes,
            ),
            _ => (*position, normal),
        };
        // MDX is authored Z-up; the renderer is Y-up. Convert after skinning so
        // node rotations and translations stay consistent.
        let (local, normal) = if mesh.z_up {
            (
                Vec3::new(local.x, local.z, -local.y),
                Vec3::new(normal.x, normal.z, -normal.y),
            )
        } else {
            (local, normal)
        };
        let world_position = world.transform_point3(local);
        let world_normal = world.transform_vector3(normal).normalize_or_zero();
        scratch.push(MeshVertex {
            position: world_position.to_array(),
            normal: if world_normal.length_squared() > 0.0 {
                world_normal.to_array()
            } else {
                [0.0, 1.0, 0.0]
            },
            uv: mesh.uvs.get(index).map(|uv| uv.to_array()).unwrap_or([0.0, 0.0]),
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
}
```

Behaviour note: the old code used `total > 1e-5` before dividing; `skin_vertex`
divides when `weight_sum > 0.0`. Real weights are ≥ 1/8, so the threshold is not
reachable; P16 pins equality on real models.

### 3.3 `crates/wc3-formats/src/mdx.rs` (frozen) — **already applied in tree**

The chunk walker + node-object counter this layer proposed are now part of the
format layer (applied by the dispatcher while this manifest was being written):

| item | line | signature |
| --- | --- | --- |
| `MdxModel::chunks` | :508 | `fn chunks(bytes: &[u8]) -> Result<Vec<Chunk>, FormatError>` |
| `MdxModel::node_object_count` | :553 | `fn node_object_count(bytes: &[u8]) -> Result<usize, FormatError>` |
| `Chunk` | :618 | `pub struct Chunk { tag: [u8; 4], offset: usize, size: usize }` |

The bodies are byte-identical to the audited tool implementation
(`analysis/agent-anim/mdxaudit/src/main.rs::node_objects`, which classified
542/542 models with `PIVT.len() == node_object_count`). No patch needed;
`patches/0002-mdx-chunks-node-objects.patch` is kept only as the paper trail
(marked SUPERSEDED). Post-freeze check: §9 P17.

### 3.4 `crates/wc3-re/src/mdx.rs` (frozen) — **already applied in tree**

`wc3-re::mdx` now delegates instead of walking bytes:

| item | line | role |
| --- | --- | --- |
| `chunks` | :46 | `MdxModel::chunks(bytes)` projected to `MdxChunk` (`offset` = header offset) |
| `node_object_count` | — | thin wrapper over `MdxModel::node_object_count` |
| `MdxSummary.node_objects` | :121 | set in `summarize` |

No patch needed (`patches/0005-re-mdx-chunk-facade.patch` marked SUPERSEDED).

### 3.5 `analysis/agent-anim/mdxaudit/src/main.rs` (read-only this round)

**P12 — delete `fn chunks` / `fn node_objects`** and call
`MdxModel::chunks(bytes)` / `MdxModel::node_object_count(bytes)` (both now exist
in the format layer, §3.3). Not applied now because the tool is frozen evidence
for the sweep; it is the first cleanup after freeze (the CSV stays byte-identical
because the algorithm is the same one that was promoted).

---

## 4. Symbol port table (`source file:line -> target file:line`)

| symbol | source | target |
| --- | --- | --- |
| `helper_plane` (helper-plane rule) | `wc3-game/src/assets.rs:743` (inline) | `wc3-anim/src/runtime.rs:33` |
| `mesh_alphas_for` (visibility over meshes) | `wc3-game/src/assets.rs:589-624` | `wc3-anim/src/runtime.rs:53` `mesh_alphas` |
| `geoset_alpha` (extent gate + any-key alpha) | `wc3-game/src/assets.rs:629-668` | `wc3-anim/src/runtime.rs:53` (same function, per geoset) |
| `track_active` (diagnostic variant rule) | `wc3-game/src/assets.rs:673-687` | `wc3-anim/src/lib.rs:718` `track_active_in_sequence` (already existed) |
| layer alpha rule (`KMTA`) | `wc3-game/src/assets.rs:565-576` | `wc3-anim/src/runtime.rs:199` `layer_alpha` |
| `BoneWeight` | `wc3-game/src/assets.rs:97-100` | `wc3-anim/src/runtime.rs:117` `VertexWeight` (alias patch P6) |
| `BoneWeight` (GPU copy) | `wc3-gpu/src/mesh.rs:117-121` | same canonical type (GPU owner patch; §9 conflict C4) |
| matrix-group/SKIN unpack | `wc3-game/src/assets.rs:788-835` | `wc3-anim/src/lib.rs:620` `GeosetSkin::from_geoset` (already existed; now called) |
| weighted skinning (CPU, game) | `wc3-game/src/model_render.rs:152-207` | `wc3-anim/src/runtime.rs:148` `skin_vertex` via `skin_point` :124 |
| weighted skinning (GPU CPU path) | `wc3-gpu/src/mesh.rs:1225-1260` `skin_cpu_into` + :1409 `reference_vertex` (test) | `wc3-anim/src/runtime.rs:179` `skin_positions_weighted` (GPU owner patch; §9 C4) |
| weighted skinning (GPU static stream) | `wc3-gpu/src/mesh.rs:1182-1215` `push_static_vertices` | `wc3-anim/src/runtime.rs:117` `VertexWeight` (weights already normalised by assets; no math change) |
| per-vertex weight type | `wc3-gpu/src/mesh.rs:117` | `wc3-anim/src/runtime.rs:117` |
| skinning math | `wc3-anim/src/lib.rs:697` (was inline) | `wc3-anim/src/runtime.rs:124` `skin_point` |
| chunk walker | `wc3-re/src/mdx.rs:42-84`, `mdxaudit/src/main.rs::chunks` | `wc3-formats/src/mdx.rs` `MdxModel::chunks` (P10) |
| node-object counter | `mdxaudit/src/main.rs::node_objects` | `wc3-formats/src/mdx.rs` `MdxModel::node_object_count` (P10) |
| per-frame facade | — (new) | `wc3-anim/src/runtime.rs:241` `ModelRuntime`, :221 `Frame` |
| object-id parents | `wc3-anim/src/lib.rs:127/147/168` | unchanged (canonical, invariant §5) |
| pivot-conjugated local matrix | `wc3-anim/src/lib.rs:118` | unchanged |
| bind pose | `wc3-anim/src/lib.rs:206` | unchanged |
| sequence sampling | `wc3-anim/src/lib.rs:527` | unchanged |

---

## 5. Object-id contract that must survive every later edit

1. `Skeleton::from_model` (lib.rs:225): node vector = BONE order, then HELP.
   `matrix_indices` are BONE indices; the palette prefix depends on it.
2. `resolve_parents` (lib.rs:127): parents by `object_id`; `objectId == parentId`
   ⇒ root; dangling parent ⇒ root (SacrificialPit: `Plane01/Plane02` → id 16).
3. `node_local_matrix` (lib.rs:118) + `pose_worlds` (lib.rs:168):
   `parent_world · T(pivot)·T(t)·R(squad)·S·T(-pivot)`, pivot = `pivots[object_id]`.
4. `bind_worlds` (lib.rs:206): identity for every node; skinned bind vertices ==
   stored vertices.
5. `Skeleton` keeps bones first (`bone_count`), helpers after; `bone_worlds_*`
   truncates to that prefix.

---

## 6. Public API the game/base consumes (final signatures)

```rust
// crates/wc3-anim/src/runtime.rs
pub fn helper_plane(geoset: &Geoset) -> bool;
pub fn mesh_alphas(model: &MdxModel, hidden: &[bool], sequence: Option<&Sequence>, time_ms: u32) -> Vec<f32>;
pub fn layer_alpha(track: Option<&Track<f32>>, static_alpha: f32, sequence: Option<&Sequence>, time_ms: u32) -> f32;
pub fn skin_point(position: Vec3, influences: impl IntoIterator<Item = (u16, f32)>, bone_worlds: &[Mat4]) -> Vec3;
pub fn skin_vertex(position: Vec3, normal: Vec3, influences: impl IntoIterator<Item = (u16, f32)>, bone_worlds: &[Mat4]) -> (Vec3, Vec3);
pub fn skin_positions_weighted(positions: &[Vec3], influences: &[Vec<VertexWeight>], bone_worlds: &[Mat4]) -> Vec<Vec3>;
pub struct VertexWeight { pub bone: u16, pub weight: f32 }
pub struct Frame { pub time_ms: u32, pub bones: Vec<Mat4>, pub alphas: Vec<f32> }
pub struct ModelRuntime { pub skeleton: Skeleton, pub skins: Vec<GeosetSkin>, pub helper_planes: Vec<bool>, pub geoset_count: usize }
impl ModelRuntime {
    pub fn new(model: &MdxModel) -> Self;
    pub fn bones(&self, sequence: &Sequence, time_ms: u32) -> Vec<Mat4>;
    pub fn alphas(&self, model: &MdxModel, sequence: Option<&Sequence>, time_ms: u32) -> Vec<f32>;
    pub fn frame(&self, model: &MdxModel, sequence: &Sequence, time_ms: u32) -> Frame;
    pub fn skin(&self, model: &MdxModel, geoset_index: usize, bones: &[Mat4]) -> Vec<Vec3>;
    pub fn visible(&self, model: &MdxModel, sequence: &Sequence, time_ms: u32) -> Vec<usize>;
    pub fn bind_positions(&self, model: &MdxModel) -> Vec<Vec<Vec3>>;
}
// re-exported at crate root: wc3_anim::{Frame, ModelRuntime, helper_plane, mesh_alphas}
// unchanged canonical: wc3_anim::{Animator, Skeleton, GeosetSkin, skin_positions, sample_in_sequence}
// format layer (unchanged): wc3_formats::mdx::{MdxModel, Sequence, Geoset, Track, Extent, …}
```

`wc3-game` keeps its public surface: `Assets::{mesh_alpha, mesh_layer_alpha}`
(now delegating), `assets::{mesh_alphas_for, geoset_alpha, BoneWeight}` (delegating
aliases/wrappers), `wc3_anim::Animator` (unchanged).

---

## 7. Duplicates removed / thinned and the delegation proof

| duplicate | action | proof of equivalence (post-freeze) |
| --- | --- | --- |
| `assets.rs` visibility body (:587-668 + hide :733-746) | thinned to calls (P1/P2/P5) | **P13** parity fixture: for all 542 models × Stand/Portrait/Walk/Death, `mesh_alphas_for(build_meshes(m))` == `wc3_anim::runtime::mesh_alphas(m, …)` elementwise (bit-equal f32) |
| `assets.rs` bone unpack (:788-835) | thinned to `GeosetSkin::from_geoset` (P7) | **P14**: for 542 models, per-vertex weight lists equal (bone id + weight |Δ|<1e-6) |
| `assets.rs::BoneWeight` | type alias (P6) | type identity: `fn assert_same<T>()` with both names, plus `size_of`/field access compile check |
| `model_render.rs::skin` (:152-207) | delegate to `skin_vertex` (P9) | **P15**: for the 4 heroes × 20 frames × all geosets, old loop vs new call |Δ| < 1e-4 (positions/normals) |
| `wc3-gpu` skin path (:1182/:1225/:1409) | GPU owner patch (C4) | **P16**: `skin_positions_weighted` vs `wc3-gpu::mesh::reference_vertex` on 542 models |
| `wc3-re::mdx::chunks` (:42) + audit `chunks`/`node_objects` | project to `MdxModel::chunks`/`node_object_count` (P10-P12) | **P17**: chunk list (tag/offset/size) byte-identical for 542 models; node-object count == `PIVT.len()` |
| `geoset_alpha` (`living` heuristic) | dead parameter kept for API compat, ignored (P2) | compile + P13 (the `living` branch was already `let _ = living;`) |
| `track_active` (:673) | deleted (P3) | no callers; `wc3_anim::track_active_in_sequence` identical (same comparison) |

Not thinned on purpose: `wc3-re::blp` (probe wrapper, BASE §2), `wc3-re::mdx`
summary types (BASE §2), `wc3-anim` `Animator` (canonical).

---

## 8. Sweep evidence (pre-freeze run, `analysis/agent-anim/audit/*`)

Runtime path per model: `Animator::from_model → play(Stand|Portrait / Walk) →
update(dt) × 20 → world_matrices() → GeosetSkin + skin_positions`, visibility =
exact mirror of `assets::mesh_alphas_for` (this manifest's rule).

| kind | models | stand/portrait sampled | walk sampled | median moving nodes (stand / walk) | verdicts |
| --- | ---: | ---: | ---: | --- | --- |
| unit | 225 | 221 | 211 | 56 / 58 | 217 PASS, 8 WARN |
| portrait | 194 | 194 | 0 | 21 / – | 194 PASS |
| building | 123 | 123 | 8 | 1 / 97 | 117 PASS, 6 WARN |
| **total** | **542** | **538** | **219** | **27 / 61** | **528 PASS, 14 WARN, 0 FAIL** |

- sequences parsed across the sweep trees: 3 606; bones 15 533; geosets 2 464;
  vertices 302 679; max influences 8.
- animation coverage: Walk vertex motion median 111.0 units (p10 49.1, max 281.5);
  Stand median 11.0 (p10 0.0 for static buildings, max 430.9).
- geosets drawn that their extent marks absent: **0** (stand+walk, all models).
- non-finite matrices: **0**; models with 0 bones: 2 (particle-only
  `PlagueCloud`, `PlagueCloudTarget`); walk root yaw > 90°: 1 (`SammyCube` 178.5°,
  bounds fit).
- 14 warnings: 4 one-shot effect models without Stand; 4 billboard-heavy models
  whose camera-dependent planes exceed baked bounds; 6 buildings whose only
  animated node is a prop hidden in Stand.

Evidence files (kept frozen):
`analysis/agent-anim/audit/runtime-sweep.csv` (542 rows × 29 cols),
`runtime-sweep.md`, `sweep.md`, `verify-heroes.txt`,
`analysis/agent-anim/gallery/gallery-{stand,walk,effect}.png` +
`gallery-index.csv` + `hero-*.png` + `hero-diff.txt`.

---

## 9. Tests to add after freeze (not run yet)

| id | name | fixture / assertion |
| --- | --- | --- |
| P13 | `assets_visibility_matches_canonical_for_all_models` | blocked on patch 0003 (assets.rs); corpus `analysis/war3_full/{Units,Buildings}` |
| P14 | `assets_bone_weights_match_geoset_skin` | blocked on patch 0003 |
| P15 | `model_render_skin_matches_canonical` | blocked on patch 0004 (model_render.rs) |
| P16 | `gpu_reference_vertex_matches_weighted_skin` | `wc3-gpu` `reference_vertex` vs `skin_positions_weighted` on 542 models |
| P17 | `chunk_list_matches_reference_walker` | **partially added** this run on 4 heroes (`mdx.rs:1156`, `:1186`); full 542-model extension stays with the sweep tool |
| P18 | `model_runtime_frame_matches_animator` | **added** (`runtime.rs:355`, 4 heroes × 3 times) |
| P19 | `layer_alpha_matches_assets_rule` | **added** as a unit rule test (`runtime.rs:512`); assets parity lands with P13 |
| P20 | `bind_positions_reproduce_vertices` | **added** (`runtime.rs:441`) |

Existing tests that must keep passing unchanged: `wc3-anim` 14 (incl.
`runtime_animator_path_animates_real_models`, `bind_pose_is_identity_on_real_models`,
`archer_walk_does_not_inherit_the_death_root_rotation`,
`townhall_visibility_comes_from_sequence_extents`), `wc3-formats::mdx` 8.

---

## 10. Conflicts / open items for the dispatcher

| id | item | owner | exact next step |
| --- | --- | --- | --- |
| C1 | `crates/wc3-game/src/assets.rs` is frozen; P1-P8 need the dispatcher to apply them (or delegate the file to anim after freeze) | dispatcher (assets frozen) | apply patches, then run P13/P14 |
| C2 | `crates/wc3-game/src/model_render.rs` is owned by the render layer | render owner | hand them P9; they apply it with their next model-render change |
| C3 | ~~`wc3-formats::mdx` + `wc3-re::mdx` chunk walker~~ — **resolved in tree**: `MdxModel::chunks` :508, `node_object_count` :553, `wc3-re::mdx::chunks` :46 delegates | dispatcher / RE owner | nothing to apply; verify with P17 after freeze |
| C4 | `wc3-gpu/src/mesh.rs` has its own `BoneWeight`/skinning; not in my read-only list but BASE says GPU is a layer | GPU owner | hand them §4 rows for :117/:1182/:1225/:1409; target is `wc3_anim::runtime::{VertexWeight, skin_positions_weighted}` |
| C5 | `ACCEPTANCE_1TO1.md` still says "per-model yaw_offset (Archer PI)"; measured Walk root yaw is 0.0° and `assets.rs:932 yaw_offset_for` returns 0.0 | dispatcher | update the acceptance line; reintroducing PI would rotate the fixed Archer |
| C6 | `WC3RE_MERGE_MATRIX.md` absent | RE owner | publish; the anim row is this file's §1 |
| C7 | `wc3-game/src/world_audit.rs` (world layer) still calls `mesh_alphas_for`/`build_meshes`; it will keep working through the delegating wrappers | world owner | no action needed unless they want `ModelRuntime` directly |
| C8 | `analysis/agent-anim/mdxaudit` is frozen evidence; P12 (dedup walker) waits for freeze | anim | apply after the sweep is re-run on the merged base |

---

## 11. State

**DONE (layer) — merged, compiled (rc=0), tests green (58 + 19) / WAITING (base
suite + frozen-file patches)** — `crates/wc3-anim` now owns the single
visibility rule, the single weight type, the single skinning accumulator and the
per-frame facade; the chunk walker part of this manifest was already applied to
the format layer by the dispatcher (`mdx.rs:508/553/618`, `wc3-re/src/mdx.rs:46/121`).
Ready patches for the remaining frozen files: `0003-assets-anim-delegation.patch`
and `0004-model-render-anim-delegation.patch` (both verified with
`patch --dry-run`). Nothing was compiled or tested: BASE.md rule 4 (freeze) is in
force.

Evidence paths: `analysis/agent-anim/audit/{runtime-sweep.csv,runtime-sweep.md,sweep.md,verify-heroes.txt}`,
`analysis/agent-anim/gallery/`, this manifest.

---

## 12. Handoff to world/assets — per-definition animation (PARITY §7 inc#1)

ROUND5 reproduction (2026-11-13): generators re-run from scratch, **byte-identical**
artifacts (deterministic):
`registry.rs` sha256 `322fee85…9db9bb6`, `roster-stand.png` `c5831219…93e6c93`,
`roster-walk.png` `939593a2…62c0e36`, `runtime-sweep.csv` `24c0ba72…29c80ae`,
`roster.csv` `a0fc8031…13fbeba`. Root cause of "missing on disk": the artifact
directories were mode `drwx------`; they are now `755`/`644` so any reader can
traverse and verify.

**State of the gap (observed 2026-11-13):** the game still maps 99 sim defs onto
**4 meshes** (`assets.rs:34 MODEL_KINDS`, `:41 MODEL_PATHS`, loop at `:532`). The anim
layer now provides everything needed to address models by definition instead:

* `wc3_anim::AnimatorRegistry` (`crates/wc3-anim/src/registry.rs`): LRU registry keyed
  by the definition's `UnitDef.model` path; one parsed `MdxModel` + `ModelRuntime`
  per key; `animator(key, seq)` returns an independent animator per unit instance;
  `with_runtime(key, f)` exposes skeleton/skins/visibility; `normalize_key` handles
  `\` vs `/` and case.
* `wc3_anim::runtime::ModelRuntime` + `mesh_alphas`: canonical visibility (extent
  gate + `KGAO` + helper plane) for any model.
* Roster evidence: 216 four-race models render/animate (`gallery/roster-*.png`,
  `roster.csv`), object-id indexing proven for all 542
  (`audit/object-id-indexing.csv`: 542/542 PASS, palette delta 0.0, skin delta 0.0).

### Handoff artifact
`analysis/tech-lead/patches/0006-anim-registry-world-handoff.md` — self-contained
spec for the world/assets owner (API snippet, exact call sites, acceptance
commands, rollback, pitfalls). Mirrors this section.

### Integration steps (world/assets owner; exact call sites)

1. **Load models per def.** In `Assets::load` replace the `MODEL_KINDS` loop
   (`assets.rs:532-533`) with: iterate `wc3_sim::ROSTER` (or `UnitData.slk` rows),
   for each `UnitDef` with `model_present`, `AssetDb::model(def.model)` →
   `registry.load_path(def.model, …)`; keep `AnimatorRegistry::new(256)` on `Assets`
   so memory stays bounded (LRU evicts; 542 models × parsed mesh ≈ tens of MB).
2. **Lookup by def, not kind.** Add `Assets::model_for_def(&self, def: &UnitDef) ->
   Option<&MeshAsset>` backed by `registry.with_runtime(&def.model, …)`; keep
   `model(kind)` as a compatibility wrapper resolving the kind's default def.
3. **Animator per instance.** `game.rs:1024-1027` (`visuals.insert` →
   `Animator::from_model(mdx)`) becomes
   `registry.animator(&def.model, &idle_sequence)`; each unit gets its own animator,
   the parsed model is shared.
4. **Visibility per model.** `game.rs` `mesh_alpha`/`mesh_layer_alpha` calls become
   `wc3_anim::runtime::mesh_alphas(&mdx, &hidden, Some(seq), time)` and
   `runtime::layer_alpha(...)` (patches `0003` already contain the assets-side
   delegation).
5. **Skinning.** `model_render.rs:153` → `runtime::skin_vertex` (patch `0004`).
6. **Preview.** `terrain_render.rs:3436` can use the same registry entry.

### Acceptance (owned by world/QA)
- `cargo test -p wc3-game --lib models_per_def` → `registry.len() >= 216` after
  loading the four-race defs (PARITY §7 inc#1 gate).
- `qa.models.race` reports `rendered/required` per race using
  `gallery/roster.csv` as the per-model baseline (216 PASS / 0 FAIL here).
- Re-run `cargo test -p wc3-anim --test object_id_indexing -- --ignored` after the
  asset refactor: 542/542 must stay green (the game's palette contract).

### Rollback
Registry is additive: keep `MODEL_KINDS` behind a feature flag until the
`models_per_def` gate is green; reverting is deleting the registry loader call and
restoring the 4-kind loop at `assets.rs:532`.

### Why a spec, not a patch
`assets.rs` is dispatcher-owned and the change is a structural refactor of
`ModelAsset`/`Assets::load`, not a line edit; the exact insertion points are listed
above so the owner can land it without re-deriving the anim contract.

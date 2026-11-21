# 0006 — handoff: per-definition `AnimatorRegistry` to world/assets

Owner of the receiving change: **world/assets** (`crates/wc3-game/src/assets.rs`,
`game.rs`, `terrain_render.rs`) — not applied by the anim layer.
Anim-side implementation (already on disk):
`crates/wc3-anim/src/registry.rs`, `crates/wc3-anim/src/runtime.rs`.
PARITY reference: `PARITY_MATRIX.md` §7 increment 1, row R1.

## Why

The game maps 99 sim defs onto 4 meshes (`assets.rs:34 MODEL_KINDS`,
`:41 MODEL_PATHS`, load loop `:532-533`). The anim layer now provides the runtime
to address any model by its definition path, with one parsed model per key and an
independent animator per unit instance. Roster evidence: 216 four-race models
render/animate (`analysis/agent-anim/gallery/roster.csv`, 216/216 PASS) and
object-id indexing is proven for all 542 (`analysis/agent-anim/audit/object-id-indexing.csv`).

## API (crates/wc3-anim/src/registry.rs)

```rust
use wc3_anim::{AnimatorRegistry, normalize_key};

let mut registry = AnimatorRegistry::new(256);          // LRU capacity
registry.load_path("Units\\Human\\Footman\\Footman.mdx", &model_file)?;
let stand = registry.sequence(key, "Stand").expect("stand");
let mut animator = registry.animator(key, &stand).expect("animator"); // per instance
animator.update(dt);
let bones = animator.world_matrices();                  // BONE order (object-id == index)

// per-frame visibility from the same entry
let alphas = registry
    .with_runtime(key, |model, runtime| {
        wc3_anim::runtime::mesh_alphas(model, &runtime.helper_planes, Some(&stand), t)
    })
    .unwrap_or_default();
```

Key rules:
* keys are normalised (`normalize_key`: lower case, `\` → `/`, NUL-trimmed), so a
  `UnitDef.model`, an MDX texture path and a filesystem path address one entry;
* `animator()` returns a fresh `Animator` (one per unit instance) — parsed data is
  shared, clocks are not;
* `with_runtime` exposes `MdxModel` + `ModelRuntime` (skeleton, `GeosetSkin`s,
  helper-plane mask) for visibility and skinning;
* LRU eviction keeps memory bounded; `evictions()` is observable.

## Integration steps (exact call sites)

1. `assets.rs:532-533` — replace the `MODEL_KINDS` loop with a per-def load:
   iterate `wc3_sim::ROSTER` (or `Units/UnitData.slk` rows joined with
   `unitUI.slk` `file`), and for each entry with `model_present` do
   `AssetDb::model(def.model)` → `registry.load_path(def.model, …)`.
   Keep `AnimatorRegistry::new(256)` on `Assets`; a 542-model install then loads
   lazily and evicts.
2. `Assets::model_for_def(&self, def: &UnitDef) -> Option<&MeshAsset>` — new
   lookup; keep `model(kind)` as a compatibility wrapper resolving the kind's
   default def.
3. `game.rs:1024-1027` — `animator: Animator::from_model(mdx)` becomes
   `registry.animator(&def.model, idle_sequence)`.
4. `game.rs` visibility — `assets.mesh_alpha(…)` becomes
   `wc3_anim::runtime::mesh_alphas(&mdx, &hidden, Some(seq), t)` and
   `runtime::layer_alpha(…)` (assets-side delegation in patch `0003`).
5. `model_render.rs:153` — `skin()` → `wc3_anim::runtime::skin_vertex` (patch `0004`).
6. `terrain_render.rs:3436` — preview can use the same registry entry.

## Acceptance (world/QA re-runs)

```bash
cargo test -p wc3-game --lib models_per_def          # registry.len() >= 216
cargo test -p wc3-anim --test object_id_indexing -- --ignored   # 542/542 must stay green
cargo test -p wc3-anim                                # 23 tests (registry included)
```

`qa.models.race` reports `rendered/required` per race; baseline for comparison is
`analysis/agent-anim/gallery/roster.csv` (Human 57 / Orc 50 / Undead 55 /
NightElf 54, all PASS) and `roster-stand.png` / `roster-walk.png`.

## Rollback

Registry is additive. Keep the `MODEL_KINDS` loop behind a feature flag until
`models_per_def` is green; reverting = drop the registry loader call and restore
`assets.rs:532`.

## Known pitfalls

* **Object-id palette:** `Animator::world_matrices()` is indexed by MDX object id;
  in every shipped model a bone's `object_id` equals its BONE-chunk index
  (verified 15 533 bones, 0 mismatches), so bone-index skin weights resolve to the
  same slots. Do not build a palette from a different node ordering.
* **Visibility:** use the extent gate (`Geoset::present_in_sequence`) — alpha
  alone shows decay/effect geosets in living sequences.
* **Loops:** a sequence that loops returns to its first pose after one pass;
  animation checks must take the max over frames, not frame0-vs-frameLN.

# INTEGRATION — what the runtime must change (and what is already correct)

Scope: `crates/wc3-anim/src/lib.rs`, `crates/wc3-formats/src/mdx.rs` (owner),
runtime files `crates/wc3-game/{assets,render,game}.rs` are **read-only** for this
agent; this file states exactly what they must do. Every claim below was produced
by the audit tool on real assets; see `REPORT.md` for the raw numbers.

Verified on 2026-11-12 against the extracted `War3.mpq` tree
(`analysis/war3_full`, 410 unit + 131 building MDX, 542 files).

---

## 1. Sequence-scoped animation sampling — REQUIRED, now available in wc3-anim

**Defect (falsified and fixed in this layer).** `Track::sample` samples the global
timeline and clamps at the last key. WC3 never samples a key that belongs to a
different sequence. Consequence on real data — `Archer.mdx`, node `Bone_Root`:

| key time | value (yaw) | sequence |
| --- | --- | --- |
| 11500 | 0° | Death starts |
| 11700 / 11900 / 12100 | 63° / 128° / 190° | Death |
| 12333 … 99333 | 197° | Death (last key) |

`Walk` (18267–19100) and `Attack` (15933–16933) have **no** `Bone_Root` keys, so the
old sampler held the last key and rendered the archer rotated by 197° for the whole
walk/attack — geometry then overshot the authored sequence extents by 11% (Walk)
and 19% (Attack). With sequence-clipped sampling the root is 0° and the overshoot
is 0.0% / 3.1%.

**Fix (landed, `crates/wc3-anim/src/lib.rs`).**
- `sample_in_sequence(track, sequence, time)` — clips keys to `[start, end]`,
  returns `None` when no key is in range (caller uses the engine default: zero
  translation, identity rotation, unit scale, alpha 1).
- `sample_node_in_sequence(node, sequence, global_sequences, time)` and
  `Skeleton::{poses_in_sequence, worlds_in_sequence, bone_worlds_in_sequence}`.
- `Animator::poses()` now uses the sequence-scoped sampler when `Animator::sequence`
  is `Some`, so `game.rs:1493` (`animator.world_matrices()`) inherits the fix with
  no runtime edit. `Animator` gained a `global_sequences: Vec<u32>` field filled by
  `Animator::from_model`; `game.rs` only constructs the animator through
  `from_model`, so nothing else changed.
- Loop semantics: looping sequences (`flags & 1 == 0`) interpolate across the
  sequence boundary (no Walk hitch); non-looping sequences hold the first/last
  in-range key. Evidence for the clamp: `GreatHall.mdx` bone `shockwave` has a
  Bezier scale track whose first key carries a 100.2 control value; wrapping
  `Death` back to that key scaled the shockwave quad until a vertex sat **9001**
  units from the building. With the clamp the maximum is 483.7 and the geoset
  fits its extent exactly.

**Runtime action:** rebuild. No code change required in `wc3-game`.

---

## 2. Per-sequence geoset visibility — REQUIRED (assets.rs)

The MDX geoset carries one `Extent` per sequence. For a geoset that is not part of
a sequence the exporter writes the sentinel `min = +FLT_MAX, max = -FLT_MAX`
(`Extent::is_empty`, `Geoset::present_in_sequence` in
`crates/wc3-formats/src/mdx.rs`). This is the only per-sequence visibility flag in
classic MDX and it is authoritative.

Measured on the corpus (`analysis/agent-anim/audit/runtime_rules.txt`):

- **9 489** (geoset, sequence) pairs have an empty extent while both alpha rules
  (variant rule and "any in-range key" rule) say alpha > 0.
- **655** pairs are absent per extent while both alpha rules leave them visible —
  i.e. alpha cannot hide them at all.
- **73** living-sequence pairs (Stand/Walk/Attack) where the current runtime draws
  a geoset the extent marks absent, e.g. `SylvanusWindrunner` geoset 3 in every
  living sequence, `BurningArcher` geosets 0/1/5 in Stand.
- **109** sequences currently hit the "nothing visible → show everything"
  fallback in `assets.rs:588-598`. `SkeletonArcher` Stand is the clearest case:
  extents say geosets `[2, 3]` (the living body), the runtime's corpse-texture
  heuristic zeroes all five geosets, the fallback then shows all five — including
  the decayed variants (`0`, `1`, `4`) that the model only uses in Decay.
- **61** corpse-heuristic false positives on *present* geosets in living
  sequences: `SkeletonArcher.blp` / `BurningArcher` bodies match the `"skeleton"`
  token in `MeshData::is_corpse_or_effect` (`assets.rs:131-139`) and are zeroed
  before the fallback rescues them.

**Required change in `crates/wc3-game/src/assets.rs`** (`mesh_alphas_for`,
currently lines 572-600, and `geoset_alpha`, 605-643):

```rust
// Gate on the sequence extent first; drop is_corpse_or_effect and the
// "show everything" fallback.
let present = match (mdx.geosets.get(mesh.geoset_id as usize), sequence) {
    (Some(geoset), Some(_)) => {
        let index = /* index of `sequence` in model.sequences */;
        geoset.present_in_sequence(index)
    }
    _ => true,
};
if !present {
    return 0.0;
}
// then the existing "any in-range KGAO key → sample, else static alpha" rule
```

`wc3-anim` exposes the semantics side by side so behaviour can be tested without
the GPU crate:
- `geoset_alpha(...)` — variant rule (≥2 differing in-range keys),
- `geoset_alpha_constant_rule(...)` — any in-range key (the rule the runtime now
  uses),
- `Geoset::present_in_sequence(index)` / `Geoset::sequence_extent(index)`.

With the extent gate the corpse heuristic becomes unnecessary (the extents already
exclude decay-only geosets from living sequences) and the "all hidden" fallback
never fires: TownHall `Stand` is exactly `{11, 13, 14}` by extent *and* by alpha
(`analysis/agent-anim/audit/hero_visibility.txt`), Footman `Stand` is `{0, 1}`.

Alpha must still animate inside a sequence: `Archer Attack-1` geoset 2 (arrow) is
present per extent but alpha 0 at 15933 and 1.0 from 16158; `TownHall Birth`
geosets 7-10 fade in at 15000/30000/45000. Do not replace sampling with a boolean.

---

## 3. Rotation curves — squad, not per-component hermite (landed in wc3-formats)

`Quat` Hermite/Bezier interpolation is now `squad`
(`crates/wc3-formats/src/mdx.rs`, `AnimValue for Quat`, matching
`donors/mdx-m3-viewer/src/viewer/handlers/mdx/sd.ts:314` `quat.sqlerp`).
Per-component hermite + normalize was wrong: over 14 757 sampled keys of six
models the two differ by a median 1.9°, p90 9.45°, and 26.3% of samples differ by
more than 5° — that is a visibly wrong arm/bow pose on fast attacks.

Same commit fixes a real argument-order bug: Bezier was evaluated as
`bezier(p0, out_tan, value1, in_tan)` instead of
`bezier(p0, out_tan, in_tan, value1)`. Regression test
`mdx::tests::bezier_uses_the_in_tangent_as_the_second_control_point` pins it.

**Runtime action:** rebuild (`cargo build`), no API change.

---

## 4. Bone matrix contract (already correct — do not "fix")

- Local matrix `T(pivot) * T(translation) * R(rotation) * S(scaling) * T(-pivot)`
  (`wc3_anim::node_local_matrix`, port of `skeletalnode.ts:182`
  `mat4.fromRotationTranslationScaleOrigin`).
- Pivot = `pivots[node.object_id]` (`genericobject.ts:33`). Verified on all 542
  models: `PIVT` length equals the number of node objects (bones + helpers +
  attachments + events + lights + particles + ribbons + collision shapes;
  **cameras are not nodes and own no pivot** — `TownHall` 81 = 47+9+2+1+21+1).
- Parents resolve by `object_id`, not vector order; `objectId == parentId` means
  "no parent"; dangling parents (one model: `SacrificialPit` bones `Plane01`,
  `Plane02` → missing id 16) are treated as roots, which is what the engine does.
- Bind pose = every local matrix identity ⇒ every world matrix identity ⇒ skinned
  vertices equal the stored vertices (asserted for all four hero models).
- Skinning = equal-weight average of `matrix_groups`/`matrix_indices`
  (`transforms.glsl.ts:24-46`). Weights are normalised by `GeosetSkin`;
  `Archer` needs 5 bones/vertex, corpus maximum is 8 (`BansheeGhost`,
  `WaterElemental`), so the runtime must keep summing all influences — it does
  (`render.rs:737-751` loops over the whole list, no 4-bone cap).

---

## 5. Things the renderer does not implement (known gaps, not regressions)

1. **Billboarding** (flag `0x8`): 155 / 542 models have billboarded nodes. The
   audit flags them `BILLBOARD`; their extents and root yaw are camera dependent,
   so the extent-fit and bounds checks are downgraded to warnings for them.
   `render.rs` currently renders those planes facing +X. This is the remaining
   visual-quality gap for dragons/banners/trees, unrelated to skinning.
2. **`dontInherit*` flags** (`0x1/0x2/0x4`): not modelled; no model in the corpus
   relies on them for skinning (they appear on effect nodes).
3. **Attachments / particle emitters / lights / ribbons**: parsed but not rendered.

## 6. Yaw offsets

No model outside `Units/Critters/SammyCube/SammyCube.mdx` (walk root yaw 178.5°)
has a ~180° root yaw in Walk after the sampling fix. The Archer's 197° was the
Death-key leak described in §1, **not** an authoring choice — a per-model
`yaw_offset` for the Archer would hide the bug and rotate the model twice. Keep
`ModelAsset::yaw_offset` at 0 for Footman/Archer/Peasant/TownHall.

## 7. Reproduce

```bash
bash analysis/agent-anim/run_evidence.sh          # tests, hero verify, 542-model sweep
analysis/agent-anim/mdxaudit/target/debug/mdxaudit dump <model.mdx>
analysis/agent-anim/mdxaudit/target/debug/mdxaudit audit analysis/war3_full/Units analysis/war3_full/Buildings
python3 analysis/agent-anim/runtime_rules_check.py analysis/war3_full/Units analysis/war3_full/Buildings
```

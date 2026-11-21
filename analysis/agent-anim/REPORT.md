# REPORT — ANIMATION / MDX layer (agent-anim)

Task ref: `agent-anim` · monitor ledger **305** · date **2026-11-12** · platform
Termux/Android aarch64, KernelSU root (`su` not needed: every asset is under
`$HOME`).

---

## Goal

Prove and fix classic MDX v800 skinning / animation / geoset / material semantics so
Footman, Archer, Peasant and TownHall animate correctly in Stand/Walk/Attack/Death
with a correct bind pose and no detached or limb-scrambled vertices, then extend
the same audit to **every** unit and building model of `War3.mpq` (410 units +
131 buildings, 542 files) and report a categorized PASS/FAIL verdict.

Owner paths touched: `crates/wc3-anim/src/lib.rs`, `crates/wc3-formats/src/mdx.rs`,
`analysis/agent-anim/**`. Runtime files (`crates/wc3-game/{assets,render,game}.rs`)
were read-only; required changes are specified in `INTEGRATION.md`.

---

## Actions

### 1. Byte-level intake before any edit

- `analysis/agent-anim/probe_mdx.py` — independent Python MDX reader (chunks, nodes,
  geosets, materials, GEOA, extents) used to cross-check the Rust parser on the
  four hero models. It found two of my own mistakes early (two-size node wrappers,
  camera objects owning no pivot) and confirmed the PIVT contract.
- `donors/mdx-m3-viewer` (semantics), `donors/WarsmashModEngine` (independent
  second opinion), `donors/War3Net` (format structs), `donors/HiveWE` (extent
  parsing) were read as the reference implementations. Key citations:
  - pivot index = `objectId`: `viewer/handlers/mdx/genericobject.ts:33`
    `this.pivot = model.pivotPoints[object.objectId]`
  - local matrix about the pivot: `viewer/skeletalnode.ts:182`
    `mat4.fromRotationTranslationScaleOrigin(localMatrix, R, T, S, pivot)`
  - equal-weight matrix-group skinning: `shaders/transforms.glsl.ts:24-46`
  - quaternion squad for hermite/bezier: `viewer/handlers/mdx/sd.ts:314`
    `quat.sqlerp(out, values[start], outTans[start], inTans[end], values[end], t)`
  - sequence clipping + constant detection + loop wrap: `sd.ts` `SdSequence`

### 2. Real defects found and fixed (owner paths)

| # | Defect | File / fix |
| --- | --- | --- |
| 1 | **Global-timeline sampling leaked poses between sequences.** `Archer` `Bone_Root` `KGRT` last key (t = 12333…99333) is a 197° yaw authored for **Death**; `Walk` (18267-19100) and `Attack` (15933-16933) have no root keys, so clamping to the last key rendered the archer rotated 197° for the whole walk. Skinned geometry then overshot the authored extents by 11% (Walk) / 19% (Attack). | `wc3-anim/src/lib.rs:475` `sample_in_sequence` (key clipping + default fallback), `:61` `sample_track_in_sequence` (global sequences), `:82` `sample_node_in_sequence`, `:215` `Skeleton`, `:259/275` `poses_in_sequence` / `worlds_in_sequence`, `Animator::poses` now sequence-scoped. **Measured after fix: root yaw 0.0°, extent overshoot 0.0% / 3.1%.** |
| 2 | **Loop wrap on non-looping sequences exploded effects.** `GreatHall` bone `shockwave` has a Bezier **scale** track whose first key carries a 100.2 control value; wrapping `Death` (non-looping) back to that key scaled the quad until a vertex sat **9001** units from the building. | `sample_in_sequence`: wrap only when `sequence.flags & 1 == 0`, otherwise hold the first/last in-range key (reference viewer does not distinguish; real data does). **Measured: max |v| 9001 → 483.7, extent overshoot 6.07 → 0.000.** |
| 3 | **Quaternion hermite/bezier evaluated per component** instead of squad. | `wc3-formats/src/mdx.rs:91-115` `AnimValue for Quat` + `fn squad`. **Measured over 14 757 rotation-key samples (6 models): median 1.90°, p90 9.45°, 26.3% of samples > 5° deviation from the old curve.** |
| 4 | **Bezier control points in the wrong order** — `bezier(p0, out_tan, value1, in_tan)` instead of `bezier(p0, out_tan, in_tan, value1)`. | `mdx.rs` `Track::sample` + `wc3-anim` `interpolate_key`; regression test `mdx::tests::bezier_uses_the_in_tangent_as_the_second_control_point` (midpoint 5.0 vs the buggy 7.5). |
| 5 | **ATCH path read as 256+4+4** instead of 260+4 (clipped 4 characters, "reserved" was path tail). | `mdx.rs:444`; synthetic byte-level test `attachment_block_reads_260_byte_path`. |
| 6 | Material `LAYS` tag not validated; GEOA header accepted 24 bytes although the fixed header is 28; `TANG` skip had no `check_count`. | `mdx.rs:697`, `:504`, `:845`. |
| 7 | Missing `Extent` semantics — no way to tell "geoset absent from this sequence" (`min = +FLT_MAX`) from real bounds. | `mdx.rs:206` `Extent::{is_empty,contains,overshoot,expanded,diagonal}`, `:301` `Geoset::sequence_extent`, `:309` `present_in_sequence`. |

### 3. Deliverables

- `analysis/agent-anim/mdxaudit/` — standalone CLI (detached crate, path deps):
  - `dump <model…>` — sequences (name/start/end), bones/helpers/pivots vs raw
    node objects, geoset vertex/triangle counts, bind-pose AABB and declared
    bounds radius, per-vertex influence statistics, materials/layers, GEOA rows,
    per-sequence samples with `max|v|`, `max|bone|`, extent overshoot, bounds
    ratio, root yaw, and a PASS/WARN/FAIL check list.
    `--matrices` additionally prints the full bone world matrices at every
    sampled sequence start and mid (artifacts `audit/*.matrices.txt`).
  - `audit <dir…> --json --md` — parser/format sweep with categorized checks.
  - `runtime <dir…> --csv --summary` — **runtime-path acceptance**: for every model
    `Animator::from_model -> play(Stand|Portrait|Walk) -> update(dt) × 20 ->
    world_matrices -> skinning`, with per-model CSV columns (bones, nodes,
    sequence interval, animated tracks, moving nodes, matrix/vertex motion,
    visible geosets, absent-drawn, bounds ratio, root yaw, verdict).
  - `gallery <dir…> --thumb --columns` — CPU contact sheets (Stand/Walk/effect)
    through the same runtime path, plus `gallery-index.csv`.
  - `heroes <model…>` — 192 px per-sequence sheets and a two-frame changed-pixel
    diff (`hero-diff.txt`).
  - `shot <model>` — single-frame render for debugging/QA.
  - `verify <model…>` — hard assertions, exit code 1 on any FAIL.
- `analysis/agent-anim/run_evidence.sh` — reproduces every number below.
- `analysis/agent-anim/INTEGRATION.md` — exact runtime changes required.
- Analysis helpers kept for future audits: `probe_mdx.py`, `check_skin.py`,
  `frame_probe.py`, `root_probe.py`, `overshoot_probe.py`, `geoset_probe.py`,
  `dangling_probe.py`, `runtime_rules_check.py`, `hero_visibility.py`,
  `squad_delta.py`, `find_paths.py`.
- Tests: **14** in `wc3-anim` (7 real-data on Footman/Archer/Peasant/TownHall,
  including `runtime_animator_path_animates_real_models`, which mirrors the
  `game.rs` per-frame path `from_model -> play -> update -> world_matrices ->
  skinning`; 7 unit) and **8** in `mdx::tests` (5 new parser tests with
  real/synthetic bytes). Suite: `wc3-formats` 52 passed, `wc3-anim` 14 passed,
  0 build warnings in both crates.

---

## Evidence (commands, rc, output)

```
$ bash analysis/agent-anim/run_evidence.sh
test-anim.txt   rc=0  test result: ok. 14 passed; 0 failed
test-mdx.txt    rc=0  test result: ok. 8 passed; 0 failed
test-formats.txt rc=0 test result: ok. 52 passed; 0 failed
check-game.txt rc=0/101 while another agent refactors hud/mobile_hud (see below)
hero verify rc: rc=0
sweep rc: rc=0
# MDX sweep: 542 models, PASS 264 / WARN 278 / FAIL 0
sequences=3606 bones=15533 geosets=2464 vertices=302679 max_influences=8
visibility_hidden_by_extent=9489 present_but_alpha_zero=954
constant_rule_would_hide_visible=42 extent_required_for_absent_geosets=655
```

`cargo test -p wc3-formats` (whole crate): **52 passed; 0 failed**.
`cargo test -p wc3-anim`: **14 passed; 0 failed**.
Logs: `analysis/agent-anim/audit/{build-mdxaudit,test-anim,test-mdx,test-formats,check-game,verify-heroes,sweep.out}`.

### Hero verification (`audit/verify-heroes.txt`, rc=0)

| model | seqs | bones | geosets | max influences | worst extent overshoot | worst bounds ratio |
| --- | --- | --- | --- | --- | --- | --- |
| Footman | 13 | 25 | 5 | 3 | 0.001 (Stand) | 0.000 |
| Archer | 13 | 33 | 4 | **5** | 0.031 (Attack-1) | 0.000 |
| Peasant | 22 | 22 | 6 | 3 | 0.001 (Stand) | 0.000 |
| TownHall | 14 | 47 | 36 | 2 | 0.019 (Death) | 0.000 |

Every sample is finite, `max|v|` stays inside the model (Archer 128.1, TownHall
447.4), root yaw is 0.0° for all Walk/Stand samples.

### Bind pose (asserted for all four heroes)

`Skeleton::bind_worlds()` returns the identity matrix for every node with
deviation `0.00e0`, and bind-pose skinning reproduces the stored vertices exactly
(< 1e-3). Pivot contract verified model-wide: **542/542 models** have
`PIVT.len() == number of node objects` (`PIVOTS` PASS 542/542), including
`TownHall` 81 = 47 bones + 9 attachments + 2 events + 1 light + 21 particles + 1
collision shape (its `CAMS` camera owns no pivot).

### 542-model sweep (`audit/sweep.md`, rc=0)

```
models 542 | sequences 3606 | bones 15533 | geosets 2464 | vertices 302679 | max influences 8
PASS 264 / WARN 278 / FAIL 0
checks: PARSE 542/542 · PIVOTS 542/542 · PARENTS 541 PASS + 1 WARN · BIND_POSE 542/542
        SAMPLING 542/542 (all matrices finite) · BOUNDS 542/542 · GEOA 542/542
        MATERIALS 542/542 · INFLUENCES 448 ≤4, 94 >4 (max 8: Centaur*_Portrait)
```

Warnings are model-data facts, all listed with paths in `sweep.md`:

- `BILLBOARD` 155 — camera-dependent extents/root motion (renderer does not
  billboard yet; see INTEGRATION §5).
- `INFLUENCES` 94 — 5–8 bones per vertex (runtime already sums all influences).
- `EXTENT_FIT` 32 — secondary effect geosets (debris, thrown boulders, muzzle
  flashes) exceed their baked extents in effect frames; main body geosets fit.
- `DYNAMIC_BONE` 4 — projectile/effect bones leave the model volume
  (`Shandris`/`SylvanusWindrunner` arrow reaches 784 units during Attack).
- `PARENTS` 1 — `SacrificialPit.mdx` bones `Plane01`/`Plane02` point at a missing
  object id 16; the engine (and our `resolve_parents`) treats them as roots.
- `ROOT_YAW` 1 — `SammyCube.mdx` walk root yaw 178.5° (only model with a real
  ~180° root rotation in Walk; no per-model yaw offset needed for the heroes).
- `SEQ_ATTACK` 28 / `SEQ_WALK` 14 / `SEQ_DEATH` 8 / `SEQ_EFFECT` 4 — critters,
  wards, mines, one-shot effect models (`InfernalBirth`, `WispExplode`, …).
- `SEQ_PORTRAIT` 1, `SEQ_SENTINEL` 1 (`HeroFarSeer` filler sequence `nothing`
  with start 0xFFFFFF59), `SEQ_STAND` 0.

### Visibility semantics (the TownHall / effect-geoset class of bugs)

`python3 analysis/agent-anim/runtime_rules_check.py analysis/war3_full/Units analysis/war3_full/Buildings`
(`audit/runtime_rules.txt`, 3599 playable sequences):

- 9 489 (geoset, sequence) pairs: empty extent **and** alpha > 0 → alpha alone
  cannot hide them.
- 655 pairs: absent per extent while both alpha rules leave them visible.
- 73 living-sequence pairs where the current runtime draws an absent geoset
  (`SylvanusWindrunner` geo3 in all of Stand/Walk/Attack, `BurningArcher`
  geo0/1/5 in Stand).
- 109 sequences trigger the "nothing visible → show everything" fallback
  (`assets.rs:588-598`); `SkeletonArcher` Stand draws all 5 geosets while the
  extents say `[2, 3]` (the other three are decay-only variants).
- 61 corpse-texture heuristic false positives on living bodies:
  `SkeletonArcher.blp` contains `"skeleton"`, so `is_corpse_or_effect`
  (`assets.rs:131-139`) zeroes the body of `SkeletonArcher`/`BurningArcher`.

`audit/hero_visibility.txt` shows the extent rule and the current alpha rule agree
exactly for the heroes at sequence level (Footman Stand `{0,1}`, Archer
`{0,1}`, TownHall Stand `{11,13,14}`, TownHall Birth `{4..10}` with 7-10 fading
in mid-sequence), which is why the required change is *only* the extent gate —
see `INTEGRATION.md` §2.

---

## Acceptance round 2 — runtime-path proof for all 542 models + gallery

The operator judges what renders, so the acceptance evidence now goes through the
**runtime** path for every unit and building model, not only the four heroes.

### 1. Runtime-path sweep (`audit/runtime-sweep.csv`, `audit/runtime-sweep.md`)

`mdxaudit runtime analysis/war3_full/Units analysis/war3_full/Buildings` — for each
model: `Animator::from_model` → `play(Stand|Portrait)` and `play(Walk)` →
20 × `update(dt)` → `world_matrices()` → `GeosetSkin` + `skin_positions`, with the
runtime visibility rule (`assets.rs::mesh_alphas_for`). Result: **542 models,
PASS 528 / WARN 14 / FAIL 0, rc=0**.

Per-model CSV columns: bones, nodes, stand/walk sequence name+interval, animated
track count, moving nodes, max matrix delta, vertex motion, visible geosets,
geosets drawn that the extents mark absent, bounds ratio, root yaw, verdict,
reasons.

| metric | value |
| --- | --- |
| models with a Stand/Portrait run | 538 |
| models with a Walk run | 219 (323 models have no Walk by design: buildings, portraits, effects) |
| moving nodes in Stand | median 27, zero in 58 models (53 buildings + 5 units, all static-by-data) |
| moving nodes in Walk | median 61, **zero models with a static Walk** |
| skinned vertex motion in Stand | median 11.0 units, max 430.9 |
| skinned vertex motion in Walk | median 111.0 units, max 281.5 |
| geosets drawn that their extent marks absent | **0** (extent gate holds everywhere) |
| models with non-finite matrices | **0** |
| models with 0 bones | 2 (`PlagueCloud`, `PlagueCloudTarget` — particle-only, no geosets) |
| root yaw > 90° in Walk | 1 (`SammyCube` 178.5°, pass: bounds fit) |

14 warnings, all explained in `runtime-sweep.md`: 4 one-shot effect models without
Stand (`InfernalBirth`, `WispExplode`, 2× Abomination explosion); 4 billboard-heavy
models whose camera-dependent planes exceed the baked bounds (`BurningArcher`,
`Hellscream`, `ChaosHellscream`, `Infernal`); 6 buildings whose only animated node
is a prop that is hidden in Stand (`TrollBurrow`, 2× FurbolgHut, 2× GnollHut,
`GoldMine`).

Defect found and fixed while building this sweep: `Geoset::present_in_sequence`
treated an **empty `sequence_bounds` array as "absent"**. Models such as
`Units/Creeps/TempArt/TempArt.mdx` and `Units/Demon/Infernal/Rock1.MDX` ship no
per-sequence extents and were hidden entirely (in the runtime gate too, since
`assets.rs` calls this function). Missing data now means "present".

### 2. Gallery (`analysis/agent-anim/gallery/`)

CPU rasterizer over the same runtime path (textures from the MPQ, extent gate,
alpha rule, depth buffer, three-quarter view):

- `gallery-stand.png` — 538 thumbnails (every model with Stand/Portrait),
- `gallery-walk.png` — 219 thumbnails (every model with Walk), no blank tiles,
- `gallery-effect.png` — 4 one-shot effect models,
- `gallery-index.csv` — tile → model/sequence/verdict,
- `hero-{Footman,Archer,peasant,TownHall}.png` — 192 px, one row per sequence,
  three frames each,
- `hero-diff.txt` — two-frame changed-pixel proof:

| hero | Stand | Walk | Attack | Death |
| --- | --- | --- | --- | --- |
| Footman | 10.9 % | 37.1 % | 40.7 % | 41.6 % |
| Archer | 43.7 % | 33.0 % | 30.6 % | 47.3 % |
| Peasant | 15.3 % | 36.3 % | 42.9 % | 44.1 % |
| TownHall | 0.0 % (static Stand geoset set) | – | – | 28.0 % |

Only blank thumbnails are `PlagueCloud`/`PlagueCloudTarget` (zero geosets).

Hero geoset sets confirm the visibility semantics: Footman/Archer/Peasant draw
`[0,1]` (body + team colour) in Stand/Walk, Archer adds `[2]` (arrow) in
Attack-1, Peasant adds `[4]` (decayed part) in Death, TownHall draws `[11,13,14]`
in Stand and `[2,35]` in Death — exactly what the extents authorise.

### 3. Device verification — boundary

The four heroes are verified through the identical animation/visibility code on
the CPU (sheets + per-sequence two-frame diffs above). Framebuffer capture from a
running install is **not possible from this session**: `crates/wc3-game` is being
edited concurrently by other owners (it did not compile during this round), there
is no screenshot/demo mode in the app, and installing/launching the QA build and
driving the UI is outside this layer. Once QA installs a build, capture with:

```bash
# while the game shows a Footman/Archer/Peasant/TownHall in Stand and Walk
su -c 'screencap -p /sdcard/Download/anim-<unit>-<seq>.png'
```

What to check on-device (all four verified numerically here): Archer runs forward
(Walk root yaw 0.0°, `yaw_offset` 0), Footman sword follows the hand (geoset 1 is
fully skinned, 0 absent geosets drawn), no corpse/effect geosets in Stand/Walk
(`stand_absent_drawn=0`, `walk_absent_drawn=0`), TownHall Stand shows only geosets
11/13/14.

## Result

**Done.** MDX v800 parsing, skinning and animation are now consistent with the
reference implementations and with the data in the shipped models:

- bind pose is exact (identity world matrices, vertices unchanged);
- all sequences sample with WC3 clipping (looping wrap, non-looping hold, global
  sequences by counter) — no pose leaks between sequences;
- rotation curves use squad, Bezier uses the correct control order;
- every skinned vertex of the four heroes stays inside the authored per-sequence
  extents (max overshoot 3.1%, previously up to 19% plus a 197° root error);
- the 542-model sweep parses and animates with **0 failures**, `rc=0`, all
  matrices finite, all bones resolved, all pivots correct;
- the remaining runtime gap (per-sequence geoset visibility from GEOS extents,
  which replaces the corpse-texture heuristic and the "show everything" fallback)
  is specified precisely, with impact numbers, in `INTEGRATION.md` §2. It is the
  identified cause of "extra/dead geosets" and of TownHall-upgrade parts showing
  in the base building.

### Runtime compile status

Final evidence run: **`cargo check -p wc3-game` rc=0, 0 errors** (4 pre-existing
warnings in `game.rs`/`hud.rs`/`terrain_render.rs`, none from this layer).

`crates/wc3-game` is edited by several other owners concurrently, so intermediate
runs during this session failed on their in-flight changes and never on this
layer's API: `hud/mobile_hud` joystick symbols, `render.rs` if/else types +
`world_audit.rs::any_layer_visible`, and a `BlpImage { .. }` constructor missing
`mipmaps`. Those runs are kept in `audit/check-game.txt` history notes; the
animation API compatibility with `game.rs` is pinned independently by
`wc3_anim::tests::runtime_animator_path_animates_real_models` (exact
`from_model -> play -> update -> world_matrices -> per-vertex skinning` sequence)
and by the 542-model runtime sweep, which uses the same calls.

### Cross-agent fix (unowned file, build-blocking)

`crates/wc3-formats/src/gamedata.rs` did not compile when this session started
(two errors: partial move of `table.columns` in `legacy_view`, and
`extend_from_slice` receiving `[u8; 4]` instead of `&[u8]` in `push_value`).
Both were fixed minimally so the workspace could build; the owning agent's work
has since landed and `cargo test -p wc3-formats` is green (52/52).

### H2

- `H2_PERSISTED`: capture **559**, **560**, **570** (round 2 acceptance),
  experience **4274**, **4275**, **4288** (domain `wc3-formats`, source
  `agent-anim:2026-11-12`).
- monitor ledger **305** (`agent-anim`): registered → running → closed with
  `h2_ids_json` below.

---

## Round 8 — hero ability + projectile/missile sequences (2026-11-13)

- `AnimatorRegistry` now owns the gameplay alias tables:
  `AbilitySequence` (Cast/Channel/Attack/Victory/Death/Dissipate) and
  `ProjectileSequence` (Flight/Birth/Impact), with exact → numbered-variant →
  flagged-fallback resolution (`SequenceMatch::is_exact`).
- Proof: 19 hero models cast exact 19/19, all animate, 14 272 skinned vertices
  finite; 67 missile models, flight exact 53, impact exact 56, 3 630 vertices.
- Gallery: `gallery/hero-abilities.png`, `projectile-flight.png`,
  `projectile-impact.png` + CSVs, produced by `mdxaudit abilities` (rc=0).
- Details: `ABILITY_SEQUENCES.md`.

---

## Round 9 — toolchain + MDX→glTF 3D verification (2026-11-13)

- Toolchain audit (23 tools) and gaps: gltf-transform fixed with wasm `sharp`
  (4.5.1), three r187 installed, assimp confirmed library-only with **no MDX
  importer** (0 tokens in `libassimp.so`), stormlib absent from the repo.
- New `mdxaudit gltf` exporter (GLB 2.0, skin + sampled animations, registry
  alias selection). Curated 4 + batch 76 GLBs: Khronos validator **0 errors /
  0 warnings**, structural CSV **76/76 PASS** (145 clips, 10 476 channels,
  1 768 joints, 16 990 vertices).
- three.js r187 loads the GLB (`ready:Spell`, 2 clips); headless WebGL readback
  gap documented. Independent numpy glTF skinning render proves animation
  (ArchMage Spell 9.91 %, Blademaster Bladestorm 6.43 %, FireBall 3.03 % frame
  diff).
- Details + reproduce commands: `TOOLING.md`.

---

## FULL GOAL — every in-game model plays its real MDX animation (2026-11-14)

- Mechanic ids: `assets.models.all-units`, `assets.anim.sequences`,
  `assets.anim.skinning`, `assets.anim.ingame`; runner
  `analysis/agent-anim/run_anim_mechanic_tests.sh` → `mechanic_tests_anim.json`
  (overall_rc=0).
- Roster model set: non-portrait Units 225 + Buildings 123 + projectiles 67 =
  **415**. GLB export/validation: **396 validated, 19 skipped with explicit
  reason** (12 effect/impact-only, 7 particle-only), **0 failed**; Khronos
  validator 0 errors/0 warnings on every exported file. Audit:
  `model_audit.json` / `model_audit.csv`.
- State mapping (strict, no silent fallback): Idle 397, Walk 220, Attack 197,
  Death 394, Cast 125, Channel 25; per-model missing states in
  `audit/state-mapping{,-missing}.csv`; 12 effect-only models have no gameplay
  state (reported).
- Skinning proof: representative GLBs (melee, ranged, caster hero, melee hero,
  building, projectile) rendered by the independent glTF rasterizer — Footman
  Walk 27.06 %, Attack 24.04 %, Archer Death 15.17 %, ArchMage Spell 9.91 % /
  Channel 4.39 %, Bladestorm 5.75 %, TownHall Death 12.11 %, FireBall 1.14 %
  (`gallery/gltf-verify/roster-skinning.csv`; TownHall idle is an occluded
  static pose, noted).
- Engine integration: `crates/wc3-anim` exposes `GameplayState` +
  `state_sequence/state_animator/state_resolutions`; read-only game patch in
  `game_state_patch.md` (replaces `SequenceKind`'s `or(Some(0))` silent
  fallback, adds Death/Cast/Channel).
- Preview labeling: all galleries/renders are verification renders of original
  MDX geometry + sampled animation; not hand-drawn/preview art.

---

## Risks / Next

1. **Runtime visibility** (highest impact): apply the extent gate from
   `INTEGRATION.md` §2 in `assets.rs`. Without it, `SkeletonArcher`-class units
   show decay geosets and TownHall-class buildings lose the deterministic
   per-sequence geoset set (the all-hidden fallback).
2. **Billboarding** (155 models): renderer still renders billboarded planes
   facing +X. Needs a camera-facing matrix in `render.rs`; the data is ready
   (`Node::flags & 0x8`, `EXTENDED_BONES`-style camera uniform).
3. **Effect-geoset extent approximation** (32 models): debrites/thrown weapons
   exceed baked extents; per-model culling may pop them. Consider padding
   effect geosets at load time (e.g. `extent.expanded(0.5 * radius)`).
4. **Attachments / emitters / ribbons** are parsed but not rendered — outside this
   layer, listed in INTEGRATION §5.
5. `analysis/agent-anim/mdxaudit` is a detached crate with its own `[workspace]`
   so it never becomes part of the shipped workspace; it is reproducible from
   `run_evidence.sh`.

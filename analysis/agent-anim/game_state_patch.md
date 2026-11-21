# Patch note: game sequence selection → canonical `GameplayState` (read-only in this round)

Owner of the receiving change: **world** (`crates/wc3-game/src/assets.rs`,
`crates/wc3-game/src/game.rs`). Not applied here: `game.rs` is READ_ONLY in the
anim packet. The anim layer now provides the strict runtime API and the proof
tests; this note gives the exact integration.

## Why

`crates/wc3-game/src/assets.rs:171-183` resolves three states
(`SequenceKind::{Idle, Walk, Attack}`) with silent fallbacks:

```rust
SequenceKind::Idle => self.idle.or_else(|| pick("stand")).or(Some(0)),
SequenceKind::Walk => self.walk.or_else(|| pick("walk")),
SequenceKind::Attack => self.attack.or_else(|| pick("attack")),
```

* `or(Some(0))` silently plays MDX sequence index 0, which is `Birth`/`Death`
  on many models.
* `contains(...)` picks the first matching sequence regardless of what the
  state means.
* Death, cast and channel are not represented at all; a dying or casting unit
  keeps its idle/walk sequence.

The anim layer already has the canonical rule set (see
`crates/wc3-anim/src/registry.rs` `GameplayState`, `state_sequence`,
`state_resolutions`) and the mechanic tests
`assets_anim_sequences`, `assets_anim_ingame`.

## Patch 1 — `crates/wc3-game/src/assets.rs`

Replace the local enum with the canonical one:

```rust
// line 187
pub use wc3_anim::GameplayState as SequenceKind;
```

and make `ModelAsset::sequence` strict (no `or(Some(0))`):

```rust
pub fn sequence(&self, wanted: GameplayState) -> Option<usize> {
    // `ModelAsset` keeps the parsed `MdxModel`/registry key; delegation is the
    // single implementation.
    self.registry_handle.state_sequence_index(wanted)
}
```

If `ModelAsset` does not keep a registry handle yet (world owns the
`AnimatorRegistry` at `model_render.rs:17/104/118`), pass the resolved index in
from the world update instead of re-implementing the alias table in assets.

Missing-state policy: keep the previous visual sequence and log once per
`(model, state)` (`dev_log!("anim_missing state={} model={}", state.label(),
key)`), never substitute another sequence.

## Patch 2 — `crates/wc3-game/src/game.rs` (currently lines 1820-1838)

Extend the activity mapping from three states to six:

```rust
let wanted = if unit.hp <= 0 || matches!(order, wc3_sim::Order::Death) {
    GameplayState::Death
} else if unit.channeling.is_some() {          // sim field name in tree
    GameplayState::Channel
} else if unit.casting.is_some() {             // pending cast / spell order
    GameplayState::Cast
} else if matches!(order, wc3_sim::Order::Attack(_)) || (*cooldown > 0 && !path_empty) {
    GameplayState::Attack
} else if !path_empty || matches!(order, wc3_sim::Order::Move(_) | wc3_sim::Order::MoveAttack(_)) {
    GameplayState::Walk
} else {
    GameplayState::Idle
};
```

`visual.sequence` becomes `Option<GameplayState>`; the replay branch
(`game.rs:1840-1846`) plays the resolved sequence only when
`registry.state_sequence(&key, state)` returns `Some`, otherwise it leaves the
animator on its current sequence (bind pose on spawn) and records the missing
state.

Death is one-shot: play it once and freeze on the last frame (do not loop),
matching the MDX `flags & 1` non-looping rule already implemented in
`wc3_anim::sample_in_sequence`.

## Evidence the receiving layer can run

```bash
cargo test -p wc3-anim --test assets_anim_ingame        # runtime state selection
cargo test -p wc3-anim --test assets_anim_sequences     # 415-model state map
cargo test -p wc3-game --lib models_per_def             # registry integration
```

Per-model mapping (415 in-game models, missing states explicit):
`analysis/agent-anim/audit/state-mapping.csv` and
`analysis/agent-anim/audit/state-mapping-missing.csv`.

---

## Update 2026-11-14 — in-game parity round (verified against current game.rs)

`sync_visuals` at `crates/wc3-game/src/game.rs:2065-2125` now builds the
animator from the per-def MDX (fixed), but still:

1. resolves the sequence through `self.assets.model(*kind)` (:2101) instead of
   `assets.model_for_def(def_id)` — change that one line;
2. forces `SequenceKind::Idle` for every building (:2085-2087) — replace with
   `select_state(&activity)` using `constructing = construct.is_some()`
   (Birth), `dead`, `casting`, `channeling`, `order`, `path_empty`, `hover`;
3. has no death window: `visuals.retain(|id, _| live.contains(id))` (:2123)
   removes the visual immediately. Keep a `dying: Option<(GameplayState, f32)>`
   per visual; on the first frame the unit is not alive, play `Death` once
   (`Animator::update` now holds non-looping sequences), count `dt` down from
   1.2 s, then retain-remove. Buildings use the same window with their `Death`;
4. still selects `Attack` while walking when `cooldown > 0 && !path_empty` —
   `select_state` already orders Walk before a standing-only cooldown attack.

```rust
use wc3_anim::{ActivityOrder, GameplayState, UnitActivity, select_state};

let activity = UnitActivity {
    building: *building,
    constructing: self.sim.unit(*id).map(|unit| unit.construct.is_some()).unwrap_or(false),
    dead: self.sim.unit(*id).map(|unit| !unit.alive).unwrap_or(false),
    casting: self.sim.unit(*id).map(|unit| unit.casting.is_some()).unwrap_or(false),
    channeling: self.sim.unit(*id).map(|unit| unit.channeling.is_some()).unwrap_or(false),
    order: match order { Attack(_) => ActivityOrder::Attack, Move(_) | MoveAttack(_) => ActivityOrder::Move,
                         Build(_) | Repair(_) => ActivityOrder::Build, _ => ActivityOrder::None },
    path_empty: *path_empty,
    cooldown: *cooldown > 0,
    hover: def_flags.move_type_is_hover, // Wisp: no Walk authored
};
let wanted = select_state(&activity);
let model = self.assets.model_for_def(def_id);   // per-def, not model(*kind)
if let Some(index) = model.sequence(wanted) { visual.animator.play(&model.sequences[index]); }
// missing state: keep the previous sequence and log once (no silent fallback).
```

`assets.rs` already has `SequenceKind::Build` (Stand Work) and no index-0
fallback; extend it with `Birth` or map `GameplayState` directly as described in
the first section of this note.

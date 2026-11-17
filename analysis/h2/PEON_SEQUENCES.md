# Worker carry sequences — measured contract (2026-11-15)

Measured from the shipped model (`examples/model_probe`, `units\orc\Peon\Peon.mdx`),
22 sequences, verbatim list:

```
Stand, Walk, Attack, Stand Work, attack gold, stand work gold,
Attack Lumber, Stand Work Lumber, Attack - 2, Stand Ready,
Stand Ready Gold, Stand Ready Lumber, Stand - itch head, Stand - Talk gesture,
Stand - Stretch, Death, Walk Lumber, Walk Gold, Stand Gold, Stand Lumber,
Decay Flesh, Decay Bone
```

**Two defects followed from ignoring this list:**

1. **"Peon chops a tree while walking to the mine."** The visual state chain had
   `cooldown > 0 && path.is_empty() -> Attack`. A worker ordered to harvest has a
   non-zero spell cooldown during the trip, and its `path` is transiently empty
   between re-paths, so it fell into the Attack sequence — the chop swing — while
   it was moving. The chain now resolves harvest orders by **phase**:
   `ToMine` → Walk (or Idle when stopped), `Gathering` → Work, `ToHall` → Walk,
   and "moving" means `!path.is_empty() || goal.is_some()`.

2. **"No gold sack."** The sack is not an attachment: it is the **carry geoset**
   of the Gold/Lumber sequence variants (`Walk Gold`, `Stand Gold`,
   `attack gold`, `stand work gold`, `Walk Lumber`, …). Selecting plain `Walk`
   made the sack geoset alpha 0 for the whole trip. `Assets::sequence_with(kind,
   carry)` now picks the carrier variant first and falls back to the plain one,
   so soldiers/buildings are unaffected while workers show the load.

`Carry` derives from the sim state: `carrying > 0` → Gold, `carrying_lumber > 0`
→ Lumber (both already tracked by `wc3-sim`, no new gameplay state).

## State machine (2026-11-15, after the third regression)

`sync_visuals` now decides the clip from **measured motion**, not from goal/path:

```rust
let moving = (pos - visual.last_pos).length() > 0.6;   // units per frame at 30 Hz
visual.last_pos = pos;
```

Why: an attacker keeps `goal = Some(target)` while standing in range, so
`moving = !path.is_empty() || goal.is_some()` was always true and the `Attack` clip
never played ("батраки машут без анимации"). The chain is:

| condition | clip |
|---|---|
| building + construct | Birth |
| building | Idle |
| harvest `ToMine` | Walk (Idle when stopped) |
| harvest `Gathering` | Work (+ carry variant) |
| harvest `ToHall` | Walk (+ carry variant → sack/log visible) |
| order Build/Repair | Build |
| moving | Walk |
| standing and (`order == Attack` or `cooldown > 0`) | Attack |
| otherwise | Idle |

## Team colour

`UnitDraw.tint` was hardcoded `[0.9, 0.9, 0.9, 1.0]`. The team-colour layer is a
**mask** (white chevrons in the model), so it rendered black-and-white. The
twelve retail player colours are now `PLAYER_COLORS` + `player_tint(owner)` in
`game.rs` (player 0 red, player 1 blue), applied to units and to corpses
(`DeathVisual.owner`).

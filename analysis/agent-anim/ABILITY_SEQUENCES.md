# Hero ability + projectile/missile sequences through `AnimatorRegistry`

Round 8 (2026-11-13). Owner: `crates/wc3-anim/**`, `analysis/agent-anim/**`.
Parity reference: `PARITY_MATRIX.md` §7 increment 1 (per-definition models) —
heroes and missiles are addressed by definition/model path, not by `UnitKind`.

## API (`crates/wc3-anim/src/registry.rs`)

| item | purpose |
| --- | --- |
| `AbilitySequence` | `Cast`, `Channel`, `Attack`, `Victory`, `Death`, `Dissipate` |
| `ProjectileSequence` | `Flight`, `Birth`, `Impact` |
| `SequenceMatch` | resolved `Sequence` + `matched_alias` + `fallback` flag (`is_exact()`) |
| `AnimatorRegistry::ability_sequence(key, kind)` | alias-table lookup, exact then numbered variant, then fallback |
| `AnimatorRegistry::ability_animator(key, kind)` | `(SequenceMatch, Animator)` — independent animator already playing |
| `AnimatorRegistry::projectile_sequence(key, kind)` | flight/birth/impact lookup with the same rule |
| `AnimatorRegistry::projectile_animator(key, kind)` | `(SequenceMatch, Animator)` |
| `AnimatorRegistry::sequence_names(key)` | playable sequence names in file order (diagnostics) |

Matching rule: exact name first (case-insensitive), then numbered/variant prefix
(`Spell` also matches `Spell Throw`, `Spell Slam`, `Spell - 1`; the character
after the alias must be a separator or digit). Fallback aliases are only tried
when no canonical alias matches, and the result is flagged `fallback = true` so
callers never mistake a substitute for the authored animation.

### Alias tables (shipped-data calibrated)

| kind | aliases | fallback |
| --- | --- | --- |
| Cast | `Spell`, `Attack Walk Stand Spin`, `Cast` | `Attack Slam`, `Attack`, `Stand Ready`, `Stand` |
| Channel | `Stand Channel`, `Channel` | `Spell`, `Stand Ready`, `Stand` |
| Attack | `Attack - 1`, `Attack -1`, `Attack` | `Attack Slam`, `Stand Ready`, `Stand` |
| Victory | `Stand Victory`, `Victory` | `Spell`, `Attack Slam`, `Stand` |
| Death | `Death` | `Dissipate`, `Stand` |
| Dissipate | `Dissipate` | `Death`, `Stand` |
| Flight | `Fly`, `Stand` | `Birth`, `Death` |
| Birth | `Birth` | `Stand` |
| Impact | `Death`, `Explode`, `Impact` | `Birth`, `Stand` |

`Attack Walk Stand Spin` is the Bladestorm cast for both Blademasters;
`Spell`/`Spell Throw`/`Spell Slam`/`Spell - 1` cover every other hero;
missiles author only `Stand`, `Birth`, `Death` variants.

## Tests (`crates/wc3-anim/tests/ability_sequences.rs`)

| test | evidence |
| --- | --- |
| `heroes_expose_cast_and_channel_through_registry` | 19 hero models; cast exact **19/19** (0 fallbacks); channel resolves for all (9 use a flagged fallback); death + dissipate resolve for all |
| `hero_cast_animates_and_skins_finite` | 19/19 casts move nodes over their own time range; **14 272** skinned vertices finite and inside model space |
| `projectiles_expose_flight_and_impact_and_animate` | **67** missile models; flight exact **53** (14 Birth fallbacks = particle missiles), impact exact **56** (11 fallbacks = impact-only effects); 3 630 skinned vertices checked |

Registry unit tests additionally pin the alias matcher (`Spell`/`Spell - 1`
match, `Spellbound` does not) and the explicit fallback semantics
(Blademaster channel fallback vs Demon Hunter exact `Stand Channel`).

## Gallery (`mdxaudit abilities`, rc=0)

```bash
analysis/agent-anim/mdxaudit/target/debug/mdxaudit abilities \
  --assets analysis/war3_full --gallery analysis/agent-anim/gallery \
  --thumb 128 --columns 10
```

Output (all rendered through the registry path; `Animator` + `ModelRuntime`
visibility; frame = most-visible geosets, closest to mid-sequence):

| artifact | content |
| --- | --- |
| `gallery/hero-abilities.png` (1320×264) | 19 hero cast tiles (sequence name in tile label) |
| `gallery/projectile-flight.png` (1320×924) | 67 missile flight tiles |
| `gallery/projectile-impact.png` (1320×924) | 67 missile impact tiles |
| `gallery/hero-abilities.csv` | `model,sequence,matched,fallback,moving_nodes,visible_geosets,coverage,verdict,note` |
| `gallery/projectiles.csv` | same columns + `kind` (flight/impact) |

Notes policy (same as the 542-model runtime sweep): `particle-only` (no
geosets), `hidden-in-sequence` (extent gate hides the geoset in that sequence —
e.g. `ArrowMissile` Death), `zero-coverage` (visible geoset projects to zero
pixels — flat billboard seen edge-on, e.g. `ZigguratMissile`). All are data
facts, not failures; a missing sequence is the failure.

Counts from the last run: heroes 19 (`cast_exact=19`, failures 0), missiles 67
(`flight_exact=53`, `impact_exact=56`, failures 0); 29 hidden-in-sequence,
18 particle-only, 2 zero-coverage notes.

## Integration notes

* The game already holds `AnimatorRegistry` (`crates/wc3-game/src/model_render.rs:17/104/118`).
  Ability playback becomes
  `registry.ability_animator(&def.model, AbilitySequence::Cast)` — no second
  animation path, no per-kind sequence table in the game.
* Visibility/skinning are unchanged: `Animator::world_matrices()` feeding
  `wc3_anim::skin_positions`/`runtime::skin_vertex`, with the extent gate from
  `runtime::mesh_alphas`.
* Object-id indexing invariant still holds (bone `object_id` == BONE index in
  all 542 shipped models), so a palette built from `world_matrices()` resolves
  skin weights correctly.

# HARD MERGE — consume `wc3_re::gameplay::GameplayConfig` in `wc3-sim`

Ledger **355** (`r2-sim`), 2026-11-13. Dependency: **RE ledger 350** typed
gameplay constants (landed mid-session; consumed without forking the type).

## Contract consumed (wc3-re, RE ledger 350)

| Item | Location |
|---|---|
| `GameplayConfig` (typed GameplayConstants) | `crates/wc3-re/src/gameplay.rs:68` |
| `GameplayConfig::{from_misc, from_constants, load}` | `gameplay.rs:181`, `:192`, `:206` |
| `GameplayReport::config()` | `gameplay.rs:570` |

Fields used by the sim: `max_hero_level`, `need_hero_xp`, `grant_hero_xp`,
`hero_exp_range`, `str_attack_bonus`, `str_hit_point_bonus`, `int_mana_bonus`,
`agi_defense_bonus`, `min/max_unit_speed`, `min/max_bldg_speed`.

## Sim consumption (owner path)

| Wiring | Location |
|---|---|
| `Sim.gameplay: GameplayConfig` field (defaults = shipped table) | `crates/wc3-sim/src/lib.rs:589` |
| `Sim::apply_gameplay` (installs config + clamps every unit speed) | `lib.rs:2898` |
| Speed clamp helper (max-below-min safe) used at spawn and re-apply | `lib.rs:603`, spawn `lib.rs:802` |
| Hero level cap / XP table from config (`need_hero_xp`, `hero_xp_for_level`) | `lib.rs:2919`, `:2931` |
| Attribute growth from config (`str_hit_point_bonus`, `int_mana_bonus`, `agi_defense_bonus`, `str_attack_bonus`) | `apply_hero_level` |
| Kill XP + shared range from config (`grant_hero_xp`, `hero_exp_range`) | `lib.rs:2229` |

The hand-written `NEED_HERO_XP` / `GRANT_HERO_XP` arrays remain as the documented
default fallback and are equal to `GameplayConfig::default()`.

## Caller (file:line)

| Caller | Location |
|---|---|
| `load_and_apply_gameplay(sim, root)` — loads via `GameplayConfig::load` and applies | `crates/wc3-game/src/sim_bridge.rs:76` |
| `gameplay_root_from_assets(assets)` — install root from `mpq=`/`map=` debug lines | `crates/wc3-game/src/sim_bridge.rs:47` |
| Game scenario reset hook (dry-run-clean patch) | `analysis/agent-sim/patches/game-gameplay-constants.patch:9` → inserted after `apply_assets_pathing` in `game.rs` |

## Evidence (rc + counts)

| Command | rc | Counts |
|---|---:|---|
| `cargo test -p wc3-sim --test gameplay_config --offline` | **0** | **2 passed** (typed load; custom constants drive leveling, speed clamp, kill XP) |
| `cargo test -p wc3-game --lib typed_gameplay_constants --offline` | **0** | 1 passed (caller loads MiscGame and applies) |
| `cargo test -p wc3-sim --test physics_1v1 --offline` | **0** | 4 passed |
| `cargo check -p wc3-game --lib --offline` | **0** | — |
| QA parity harness (24 gates) | **0** | **24/24 PASS** (`gameplay-merge-parity.{log,json}`) |
| `patch --dry-run` gameplay caller patch | **0** | clean |

Custom-config test proves the type is actually consumed: `need_hero_xp[0]=1`
levels the hero with 1 XP; `str_hit_point_bonus=50` adds +150 max HP at level 2
(2.7 STR gain rounds to 3); `max_unit_speed=100` clamps footman/paladin speed;
`grant_hero_xp=777` pays 777 XP for a level-2 kill.

## Coordination notes (ledger 350)

- The typed API appeared at 22:20 while sim integration was being written;
  `wc3-sim` now re-exports `pub use wc3_re::gameplay::GameplayConfig`
  (`lib.rs:40`) so the game layer uses the same type (no duplicate struct).
- If ledger 350 renames/removes fields, `wc3-sim` compile is the early-warning:
  field list used is exactly the one above. No local fork of the parser exists.
- Shared defaults keep behavior stable: `GameplayConfig::default()` mirrors the
  previous `MiscData.txt` constants, so parity gates stay green.

## Risks / Next

- `GameplayReport` still parses full tables; sim only needs `GameplayConfig`, so
  caller uses `GameplayConfig::load` (cheap text parse) rather than building the
  whole report.
- Speed clamp uses unit/building min-max only; frost modifiers from the config
  (`frost_move_speed_decrease`) are not yet applied by spell effects.
- Device not launched (`H2_DEVICE_SLOT` unset); no screenshots claimed.

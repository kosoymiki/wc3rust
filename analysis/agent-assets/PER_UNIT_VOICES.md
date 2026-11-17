# PER-UNIT VOICES — own `UnitUI.unitSound` set per unit (ledger 351)

**Date:** 2026-11-14
**Symptom:** the Alliance peasant sounded like an Orc (wrong/shared unit voice set).
**Scope:** `crates/wc3-data`, `crates/wc3-audio`, `crates/wc3-game/src/{assets.rs,game.rs}`.

## 1. Verified lookup (current tree)

Data layer (`crates/wc3-data/src/units.rs:129` `sound_set()` → `cell(TableKind::UnitUi, "unitSound")`)
and audio layer (`crates/wc3-audio/src/soundinfo.rs:239` `UnitNameMap::parse`) both resolve
RoC and TFT `unitUI.slk` (different column order: RoC `unitSound`=X3, TFT X5; the
parsers read the header, not hardcoded positions).

`cargo run -p wc3-data --example unit_sound_sets -- analysis/war3_full3` (rc=0):

| def | unitSound | first resolved ack | attack | death |
|---|---|---|---|---|
| hpea | **Peasant** | `PeasantWhat` | `PeasantYesAttack` | `PeasantDeath` |
| opeo | **Peon** | `PeonWhat` | `PeonYesAttack` | `PeonDeath` |
| uaco | **Acolyte** | `AcolyteWhat` | `AcolyteYesAttack` | `AcolyteDeath` |
| ewsp | **Wisp** | `WispWhat` | `WispWhat` (no YesAttack row) | `WispDeath` |
| hfoo | **Footman** | `FootmanWhat` | `FootmanYesAttack` | `FootmanDeath` |
| ogru | **Grunt** | `GruntWhat` | `GruntYesAttack` | `GruntDeath` |
| ugho | **Ghoul** | `GhoulWhat` | `GhoulYesAttack` | `GhoulDeath` |
| earc | **Archer** | `ArcherWhat` | `ArcherYesAttack` | `ArcherDeath` |

8/8 distinct sets; the four workers (hpea, opeo, uaco, ewsp) resolve to four different
sets — no race shares a set. TFT `unitUI.slk` (`analysis/war3x_data/Units/unitUI.slk`)
carries the same values (hpea→Peasant … uaco→Acolyte).

## 2. Root cause (fixed)

`Game::play_unit_ack` (`crates/wc3-game/src/game.rs:1242`) built **file names** as
definitions: `{set}Yes1..3`, `{set}What1..2`. In `UnitAckSounds.slk` the definitions are
`{set}What` and `{set}Yes`; the numbered entries (`PeasantWhat1.wav`…) are *files inside*
those rows. `play_first` therefore found nothing — selection acks were silent, and any
code that fell back to a shared/generic name could voice the wrong unit.

Fix:
- `crates/wc3-audio/src/events.rs` — new set-based hooks using retail definition names:
  `unit_ack_set` (`{set}What` → `{set}Yes`), `unit_attack_set`
  (`{set}YesAttack` → `{set}What`), `unit_death_set` (`{set}Death` → `{set}What`);
  `play_first_named` returns the definition that started (for device logs).
- `crates/wc3-game/src/game.rs` — `play_unit_ack` (selection) now uses the unit's own
  `assets.unit_sound_set(def_id)` + `unit_ack_set`; new `play_unit_attack` (used by
  `issue()` for `Command::Attack`) and `play_unit_death` (used by `Event::UnitDied`)
  use the same set; each logs `GAME_AUDIO_ACK/ATTACK/DEATH unit=<def> set=<set>
  def=<definition> voice=<id>` through `dev_log` (logcat).

## 3. Test — mechanic `audio.unit-voices`

`crates/wc3-audio/tests/audio.rs`:
`audio_unit_voices_each_unit_uses_its_own_sound_set`
- prints `UNIT_VOICE def=<id> set=<set> first_ack=<def>` for the 8 reference defs;
- asserts the exact set per def and that the ack name starts with the set;
- asserts the 4 workers produce 4 distinct sets;
- asserts ack/attack/death **start voices** through the set-based hooks and every
  definition name belongs to the unit's own set.

`cargo test -p wc3-audio --test audio audio_unit_voices -- --nocapture` → **rc=0**
(prints `UNIT_VOICE_PLAY def=hpea set=Peasant ack=PeasantWhat attack=PeasantYesAttack death=PeasantDeath` etc.).

## 4. Device proof (H2_DEVICE_SLOT=1, real SDL3)

`crates/wc3-assetdb/src/bin/unit_voice_probe.rs` loads the install layers
(`War3xlocal > War3x > War3`), resolves each unit's own set and plays ack/attack/death
on the real device. Logcat (`analysis/agent-assets/device_unit_voices.log`, 25 lines):

```
GAME_AUDIO_ACK unit=hpea set=Peasant def=PeasantWhat voice=1
GAME_AUDIO_ATTACK unit=hpea set=Peasant def=PeasantYesAttack voice=2
GAME_AUDIO_DEATH unit=hpea set=Peasant def=PeasantDeath voice=3
GAME_AUDIO_ACK unit=opeo set=Peon def=PeonWhat voice=4
GAME_AUDIO_ACK unit=uaco set=Acolyte def=AcolyteWhat voice=7
GAME_AUDIO_ACK unit=ewsp set=Wisp def=WispWhat voice=10
GAME_AUDIO_ACK unit=hfoo set=Footman def=FootmanWhat voice=13
GAME_AUDIO_ACK unit=ogru set=Grunt def=GruntWhat voice=16
GAME_AUDIO_ACK unit=ugho set=Ghoul def=GhoulWhat voice=19
GAME_AUDIO_ACK unit=earc set=Archer def=ArcherWhat voice=22
GAME_AUDIO_SUMMARY voices_started=24 underruns=0 backend_errors=0
```

24/24 voices started from their own sets, 0 underruns. The in-game selection/order/death
paths emit the same `GAME_AUDIO_*` lines (dev_log → logcat) once the APK is rebuilt from
this tree (not rebuilt this round; the probe is the granted device proof).

## 5. rc

| Check | rc |
|---|---:|
| `cargo test -p wc3-audio` (all suites) | **0** (5 suites / 36 passed) |
| `cargo test -p wc3-audio --test audio audio_unit_voices -- --nocapture` | **0** |
| `cargo check -p wc3-game --lib` | **0** |
| `cargo run -p wc3-data --example unit_sound_sets` | **0** |
| `unit_voice_probe` on device (H2_DEVICE_SLOT=1) | **0** |

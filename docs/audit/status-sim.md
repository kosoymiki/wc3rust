# Simulation layer (`crates/wc3-sim`) — audit status (LEDGER 487)

Registry: `/storage/emulated/0/Download/WC3RUST_CODE_AUDIT_TZ_P0-P4.md` §0/0.1/0.2.
Repo: `herofolends-rs`, branch `main`, base revision `319588cc` (2026-11-17).
Every cargo invocation went through `scripts/h2-cargo` (single-slot wrapper:
one cargo at a time, memory gate, 2 test threads) because other audit lanes were
building on the same device.

| ID | Priority | Status | Where | Regression tests |
|----|----------|--------|-------|------------------|
| AUD-002 | P1 | **FIXED** | `crates/wc3-sim/src/save.rs` (`MAX_ID_SPACE`, `check_id_counter`, `check_id_space`, `decode_payload`, `rebuild_derived`, `read_script_section`) | `save::tests::id_counters_above_the_id_space_are_rejected_before_allocating`, `save::tests::every_entity_counter_is_bounded_by_the_id_space`, `save::tests::unit_id_at_or_above_next_id_is_rejected`, `save::tests::duplicate_unit_ids_are_rejected`, `save::tests::id_space_counter_bound_holds_over_sampled_values` (all in `src/save.rs`), `save_load_minimal_sim_round_trip_keeps_counters_and_state` (`tests/save_load.rs`) |
| AUD-005 | P2 | **FIXED** | `crates/wc3-sim/src/jass/natives.rs` (`spell_rawcode`), `crates/wc3-sim/src/jass/scheduler.rs` (spell event context, `EventIds`) | `jass_natives_spell_rawcode_covers_every_variant`, `jass_natives_spell_cast_event_context_carries_ability_id`, `jass_natives_event_id_defaults_match_the_shipped_header` (`tests/jass_natives.rs`) |
| AUD-006 | P2 | **FIXED** | `crates/wc3-sim/src/jass/host.rs` (`RegEvent::GameStateEvent`/`GameEvent`, `Host::raise_game_event`, `game_event_support`), `jass/natives/world.rs`, `jass/natives/core.rs` (`game_state_value`, `SetIntegerGameState`), `jass/natives/trigger.rs` (`GetEventGameState`), `jass/vm.rs` (`EventContext.game_state`), `jass/scheduler.rs` (`run_game_events`) | `jass_natives_game_state_trigger_fires_across_a_threshold`, `jass_natives_game_event_registration_fires_with_the_engine_event`, `jass_natives_game_event_without_an_engine_emitter_is_registered_and_partial` (`tests/jass_natives.rs`), `save_load_game_state_and_game_event_registrations_survive` (`tests/save_load.rs`) |
| AUD-007 | P2 | **FIXED** | `crates/wc3-sim/src/jass/host.rs` (`Object::Dialog`/`Button`/`TimerDialog`), `jass/natives/world.rs` (dialog/timer-dialog natives), `save.rs` (format v6) | `jass_natives_dialog_state_round_trip`, `jass_natives_timer_dialog_state_round_trip`, `jass_natives_dialog_wrong_handle_is_a_diagnostic_not_a_silent_success` (`tests/jass_natives.rs`), `save::tests::version_5_file_loads_with_neutral_ui_state`, `save_load_dialog_state_survives_the_round_trip` |

Two defects met in the same code path were fixed with the same evidence standard
and are reported here rather than left as tails:

| Found while fixing | Status | What was wrong | Test |
|--------------------|--------|----------------|------|
| `EventIds::default()` | **FIXED** | Headerless defaults for `EVENT_UNIT_SPELL_EFFECT` (77), `EVENT_PLAYER_UNIT_SPELL_EFFECT` (44), `EVENT_UNIT_CHANGE_OWNER` (80), `EVENT_PLAYER_UNIT_CHANGE_OWNER` (43) did not match the shipped `common.j` (291 / 274 / 287 / 270). Live behaviour was saved by `EventIds::from_vm` overriding from the loaded header; a map without a header registered the wrong constant. | `jass_natives_event_id_defaults_match_the_shipped_header` parses the header and pins all 22 defaults |
| `save_load_every_entity_class_roundtrip` fixture | **FIXED** | The test pushed items/camps/destructables/projectiles with explicit ids but left the counters at 1, i.e. it saved a world whose next spawn would collide. The new counter validation rejected it, which is the correct answer; the fixture now advances the counters. | same test |

Determinism: `Sim::state_hash` is unchanged by this work (no new hashed field),
and `save.load` round trips still assert hash equality every 60 ticks for 600
ticks. New state that had to survive a save (dialog/button/timer-dialog, the two
trigger registrations) is in the versioned save, not in derived memory.

---

## Reproduction before the fix

Each defect was reproduced on the unpatched tree first, with a temporary probe
test that used only the pre-fix public API (`crates/wc3-sim/tests/zz_probe_*.rs`,
removed after the patch; copies kept outside the repo). The probe reproduced the
defect by *observing* it, so a green probe run meant "bug present".

### AUD-002 — external `next_id` sizes a dense allocation

```text
$ git stash push -- crates/wc3-sim/src/save.rs crates/wc3-sim/src/jass/{host,scheduler,natives}.rs ...
$ scripts/h2-cargo test -p wc3-sim --offline --test zz_probe_repro -- --test-threads=1 --nocapture
running 2 tests
test probe_pre_fix_next_id_is_unbounded ... PREFI X next_id=2_000_000 LOADED ok, next_id=2000000
thread 'probe_pre_fix_next_id_is_unbounded' panicked at crates/wc3-sim/tests/zz_probe_repro.rs:54:5:
AUD-002 reproduced: a crafted next_id above any documented ID-space bound loads
test probe_pre_fix_zero_next_id_is_accepted ... next_id=0 rejected: OutOfRange { section: "units.id", index: 1, limit: 0 }
test result: FAILED. 1 passed; 1 failed
rc=101
```

The crafted file was a real save with four payload bytes rewritten and the FNV-1a
checksum recomputed (the checksum is a corruption check, not authentication, and
the format documents it), so the parser accepted it as valid and
`rebuild_derived()` then ran `vec![u32::MAX; self.next_id.max(1) as usize]`
(`save.rs:4964`): the external scalar chose the allocation size. `next_id =
u32::MAX` asks for 16 GiB and aborts the process on this 11 GB device; that case
was **not** executed against the unpatched code on purpose (it is the OOM the
item describes), and it is covered post-fix by a test that returns in
milliseconds.

### AUD-005 — `spell_rawcode` returns 0 for every variant

```text
test probe_pre_fix_spell_rawcode_is_zero ... PREFI X spell=HolyLight rawcode=0x00000000
PREFI X spell=DivineShield rawcode=0x00000000
... (all 11 Spell::ALL variants) ...
test result: ok. 1 passed
```

### AUD-006 — game-state/game-event registrations are fake

```text
test probe_pre_fix_game_state_trigger_never_fires ... PREFI X game-state trigger fires=0, errors=[]
test probe_pre_fix_game_event_handle_is_synthetic ... PREFI X game event handle id=0 kind=GameEvent object_is_none=true returned_trigger_registrations=0
test result: ok. 2 passed
```

The state trigger never fires and reports no error (silent), and
`TriggerRegisterGameEvent` returns a handle that no host object backs and that is
stored on no trigger.

### AUD-007 — dialog/timer-dialog state is dropped

```text
test probe_pre_fix_dialog_state_is_dropped ... PREFI X host objects after dialog setup:
  {..., 1073741831: Dialog { buttons: [1073741832] }, 1073741832: Button { text: "Archmage", hotkey: 0 }}
test result: ok. 1 passed
```

`DialogSetMessage(d, "Choose your hero")` and `DialogDisplay(Player(0), d, true)`
leave no trace: the object holds only the button list, and `TimerDialogDisplay`
returns `Null` without storing anything.

---

## AUD-002 — save load lets an external counter size a dense allocation

**Status: FIXED**

### Why

`decode_payload()` read five ID counters as plain `u32`, checked only
`unit.id < next_id`, discarded the computed `max_id` (`let _ = max_id;`) and then
`rebuild_derived()` allocated `vec![u32::MAX; self.next_id.max(1) as usize]`. The
item/product/destructable/camp/projectile counters had no check at all, and
duplicate ids were accepted.

### Fix

* One ID space, justified by the format and the entity counts the simulation
  supports:

  ```rust
  // decode_payload rejects any grid larger than 1024x1024 = 2^20 cells, and every
  // saved entity stands on at least one cell, so no legitimate save can carry a
  // counter above 2^20. The only save-derived dense structure is the unit id index
  // (4 bytes per slot) => worst case 4 MiB, two orders of magnitude above the
  // entity counts the sim runs (thousands of units, tens of thousands of trees).
  pub const MAX_ID_SPACE: u32 = 1 << 20;
  ```

* `check_id_counter(field, value)` — `1..=MAX_ID_SPACE`, else
  `SaveError::InvalidIdSpace { field, value, detail }`.
* `check_id_space(field, section, next, ids)` — counter range **and** every id
  below the counter **and** no duplicate, returning the maximum real id.
* All five counters are validated before `Sim` reconstruction and before any
  allocation. The script handle counter is bounded in its own space
  (`OBJECT_HANDLE_BASE..=OBJECT_HANDLE_BASE + MAX_ID_SPACE`), because script
  handles start at `0x4000_0000`.
* The dense unit index is sized from the real ids
  (`max_unit_id + 1`), never from the counter; `Sim::add_unit` still grows it on
  demand, so a save with three units and a counter at the cap costs three slots.
* Old files keep loading: the format is version-gated, `SAVE_FORMAT_VERSION` is
  now 6 with a `decode_v6` arm, and v1–v5 still decode through their own arms
  (`save_load_version_5_file_still_loads_with_neutral_ui_state` writes a real v5
  payload).

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --lib save::
running 11 tests
test save::tests::id_counters_above_the_id_space_are_rejected_before_allocating ... ok
test save::tests::every_entity_counter_is_bounded_by_the_id_space ... ok
test save::tests::unit_id_at_or_above_next_id_is_rejected ... ok
test save::tests::duplicate_unit_ids_are_rejected ... ok
test save::tests::id_space_counter_bound_holds_over_sampled_values ... ok
test save::tests::version_5_file_loads_with_neutral_ui_state ... ok
...
test result: ok. 11 passed; 0 failed

$ scripts/h2-cargo test -p wc3-sim --offline --test save_load
test save_load_minimal_sim_round_trip_keeps_counters_and_state ... ok
test save_load_dialog_state_survives_the_round_trip ... ok
test save_load_full_state_roundtrip_600_ticks ... ok
test save_load_every_entity_class_roundtrip ... ok
test save_versioning_unknown_version_rejected ... ok
test result: ok. 13 passed; 0 failed
```

`next_id = 0`, `next_id = 2_000_000`, `next_id = u32::MAX` and
`MAX_ID_SPACE + 1` all return `SaveError::InvalidIdSpace` and the `u32::MAX` case
returns immediately (no 16 GiB allocation); the crafted files carry a recomputed
checksum, so the checksum never masks the defect.

---

## AUD-005 — `GetSpellAbilityId` lost the ability id

**Status: FIXED**

### Why

`spell_rawcode()` formatted `Debug` for the variant, discarded it and returned
`0`. Every `Event::SpellCast` therefore reached the script with
`EventContext.spell_id == 0`.

### Fix

```rust
pub fn spell_rawcode(spell: crate::Spell) -> i32 {
    unit_rawcode(spell.ability_id())
}
```

`Spell::ability_id()` is the existing explicit table: the typed spells' `SpellDef`
row id from the ability data, or the data-backed variant's own `id`. It is the
same value `spell_for_ability` resolves, so the event id and the cast path cannot
disagree. Rewriting the `Debug` name would have been the wrong source.

While wiring the test, `EventIds::default()` was found to disagree with the
shipped `common.j` for the two spell events and the two change-owner events
(77/44/80/43 vs 291/274/287/270). `EventIds::from_vm` masked it whenever a header
was loaded; the defaults are now the header values and a test parses
`common.j` to pin all 22.

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_spell_rawcode_covers_every_variant ... ok
test jass_natives_spell_cast_event_context_carries_ability_id ... ok
test jass_natives_event_id_defaults_match_the_shipped_header ... ok
test result: ok. 14 passed; 0 failed
```

The scheduler test registers a unit spell event on the header's
`EVENT_UNIT_SPELL_EFFECT`, pushes `Event::SpellCast { spell: HolyLight }` into the
simulation and asserts the script read `0x41486862` (`'AHhb'`) from
`GetSpellAbilityId()`.

---

## AUD-006 — game-state / game-event registrations were fake

**Status: FIXED** (engine-raised state limits; host-raised game events, explicitly
`Partial` for the ids the engine has no subsystem for)

### Why

`TriggerRegisterGameStateEvent` destructured its arguments into `_`, stored
nothing and returned `Handle::new(GameEvent, 0)` — a handle no object backed.
`TriggerRegisterGameEvent` ignored the trigger, stored nothing and returned
`Handle::new(GameEvent, event_id)`. Both looked successful to the script.

### Fix

* `RegEvent::GameStateEvent { state, op, value }` and
  `RegEvent::GameEvent { event }`, saved in the trigger state (tags 13/14 in the
  script section, version 6).
* `Host::game_state_watch` records the last comparison result per
  `(trigger, state, op, value)`; the trigger fires on the false → true crossing —
  the rule `TriggerRegisterPlayerStateEvent` already uses.
* `game_state_value(host, state)` is the single definition shared by
  `GetFloatGameState`, `GetIntegerGameState` **and** the event sampler, so a
  script cannot read one number and have a trigger fire on another. It reads the
  script's own state: `SetFloatGameState` (`fgamestate`, e.g.
  `GAME_STATE_TIME_OF_DAY = ConvertFGameState(2)`) and the now-implemented
  `SetIntegerGameState` (`igamestate`); an unset state reads 0, exactly like the
  getters did before.
* `Host::raise_game_event(id)` +
  `JassVm::run_game_events()` deliver `EVENT_GAME_*` events. The engine itself
  raises `EVENT_GAME_STATE_LIMIT` (`ConvertGameEvent(3)` in the shipped header)
  on every game-state crossing, which also fires
  `TriggerRegisterGameEvent(t, EVENT_GAME_STATE_LIMIT)` watches.
* `game_event_support(id)` states honestly which ids the engine raises and which
  are raise-only; a raise-only registration is recorded in
  `Host::raise_only_game_events` so tests/tools see the `Partial` verdict.
* `EventContext.game_state` carries the `gamestate` that fired, and the
  previously unimplemented `GetEventGameState` (the header documents it directly
  under `EVENT_GAME_STATE_LIMIT`) now answers with it, so a condition/action can
  tell which state crossed the limit.
* Both natives validate their handle, push a diagnostic to `vm.errors` and return
  `null` for a non-trigger or an unknown `limitop` instead of a fake success.
  Registrations return the engine's per-id `Object::GameEvent` handle (one object
  per `gameevent` id, like the engine's global events).

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_game_state_trigger_fires_across_a_threshold ... ok
test jass_natives_game_event_registration_fires_with_the_engine_event ... ok
test jass_natives_game_event_without_an_engine_emitter_is_registered_and_partial ... ok
test result: ok. 14 passed; 0 failed
```

The threshold test covers: no fire below the limit, one fire on the crossing, no
re-fire while held, no fire on the downward crossing, silence while disabled,
fire after enable, silence after destroy, `GetTriggerEventId() == 3` and
`GetEventGameState() == 2` (the state that fired). A separate round-trip test
(`save_load_game_state_and_game_event_registrations_survive`) proves the restored
world keeps both registrations, one event object per `gameevent` id, and fires
again on later crossings and host-raised events.

---

## AUD-007 — dialog / timer-dialog natives were no-ops

**Status: FIXED** (headless UI state; the renderer/HUD reads it from the host)

### Why

`Dialog` held only a button list, `dialog_set_message` dropped the text,
`DialogDisplay` did nothing and `TimerDialogDisplay` returned `Null`. A map that
showed a dialog produced no state a renderer or a test could read.

### Fix

* `Object::Dialog { message, shown_to, asynchronous, buttons }`,
  `Object::Button { text, hotkey, quit }`,
  `Object::TimerDialog { timer, title, shown, speed, remaining_override,
  time_color, title_color }` — the headless UI model, readable through the public
  `Host::object(handle)`.
* Natives: `DialogCreate`, `DialogAddButton`, `DialogAddQuitButton`,
  `DialogClear`, `DialogSetMessage`, `DialogDisplay` (records the player, `false`
  hides), `DialogSetAsync`, `DialogDestroy` (frees the dialog and the buttons it
  owns), `CreateTimerDialog`, `TimerDialogSetTitle`, `TimerDialogSetSpeed`,
  `TimerDialogSetRealTimeRemaining`, `TimerDialogSetTimeColor`,
  `TimerDialogSetTitleColor`, `TimerDialogDisplay`, `DestroyTimerDialog`.
* A call against a wrong handle is a diagnostic in `vm.errors` plus `null` —
  never a silent success.
* Save format v6 stores the new fields; a v5 file restores the neutral values
  (empty message, hidden, no async flag, default colours) and is proven with a
  genuinely v5 payload.

### Evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline --test jass_natives
test jass_natives_dialog_state_round_trip ... ok
test jass_natives_timer_dialog_state_round_trip ... ok
test jass_natives_dialog_wrong_handle_is_a_diagnostic_not_a_silent_success ... ok

$ scripts/h2-cargo test -p wc3-sim --offline --lib save::
test save::tests::version_5_file_loads_with_neutral_ui_state ... ok

$ scripts/h2-cargo test -p wc3-sim --offline --test save_load
test save_load_dialog_state_survives_the_round_trip ... ok
```

**Not implemented, stated instead of faked:** a dialog button *click* still has no
input source in the headless run — `TriggerRegisterDialogEvent` /
`TriggerRegisterDialogButtonEvent` keep their registration (`RegEvent::DialogEvent`)
and the engine has no code path that raises a click, exactly as before this item.
The native is not touched by AUD-007 and its registration is not a fake: it is a
real record that a driver can raise once an input source exists. Genuinely
engine-unsupported game events are reported through
`Host::raise_only_game_events` (AUD-006) rather than silently succeeding.

---

## Format change (declared)

`SAVE_FORMAT_VERSION` 5 → 6, append-only per the module's forward-compatibility
rule:

* `Object::Dialog` / `Object::Button` / `Object::TimerDialog` append their headless
  UI fields (gated on `version >= 6`, so v5 objects decode to neutral values).
* `RegEvent` tags 13 (`GameStateEvent`) and 14 (`GameEvent`).
* `EventContext.game_state` appended to the saved thread/event context (gated on
  `version >= 6`), so a thread parked on `TriggerSleepAction` keeps the state that
  fired its trigger; a v5 context restores `None`.

No gameplay field, network protocol, ABI or render pipeline changed. Determinism
is preserved: no new `state_hash` input, and `save.load` still produces an
identical hash and identical following ticks.

## Full-suite evidence

```text
$ scripts/h2-cargo test -p wc3-sim --offline
60 test targets, 390 tests:  390 passed; 0 failed; 0 ignored        (SIM_EXIT=0)
```

```text
$ scripts/h2-cargo test -p wc3-game --offline --test menu_save_load
test game_menu_save_load_roundtrip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored                     (GAME_EXIT=0)
```

The `wc3-game` run is the cross-crate check that the v6 save still restores a
live battle through the game layer's own save/load path (`Game::save_slot_path`,
`Sim::from_save_path`, digest equality).

Targeted loops while iterating (all green, `scripts/h2-cargo`):

```text
--lib                    61 passed
--test save_load         14 passed
--test jass_natives      14 passed
```

Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`, host
`aarch64-linux-android` (Termux), device RMX3709. Every run used
`scripts/h2-cargo` (flock + memory gate); `CARGO_INCREMENTAL=0`.

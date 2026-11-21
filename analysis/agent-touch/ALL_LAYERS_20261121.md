# ALL LAYERS 2026-11-21 — input / touch (Agent J)

Ledger 606 (`alllayer-input-20261121`), parent all-layers push.
Owner paths written: `crates/wc3-input/src/{lib,session,remap}.rs`,
`crates/wc3-input/tests/ui_hotkeys_retail.rs`, `analysis/agent-touch/**`.
Nothing outside the owner paths was edited.

## Goal

Touch/router parity with the device: hit-testing (minimap / command card /
selection), gestures (tap, drag box, long-press, pan, pinch, two-finger tap),
hotkeys, and the multi-touch/drag-off edge cases — each fixed at the root cause
with a named test, and every trace under `analysis/agent-touch/traces/`
replaying deterministically with `touch-trace --check` rc=0.

## Baseline (observed, before any edit)

| Check | Result |
|---|---|
| `scripts/h2-cargo test -p wc3-input --offline` | rc 0, 68 lib + integration |
| `touch-trace --check` on 15 fixtures | 15/15 rc 0 |
| `git status crates/wc3-input` | `M src/lib.rs` (mod gate/menu), untracked `gate.rs`, `menu.rs`, `tests/input_idle_gate.rs` — earlier uncommitted work, preserved |

A green suite did not mean parity: the defects below are invisible to
`input tap` (ADB injects exact coordinates with zero jitter), so the ROUND 8
device round could not have exposed them.

## Root causes fixed

### 1. `moved` meant "a motion event arrived", not "the finger left the slop"

`pointer_move` set `pointer.moved = true` unconditionally (`lib.rs:393`
before this change). Consequences on a real panel, which reports 1–5 px of
jitter on a still finger:

* a world tap with jitter released through `DragMode::Pending` + `moved` →
  `TouchEnd` `None` — **no selection at all**;
* a placement tap with jitter → `up=none` — **the building was never placed**
  (`Zone::Place` cancels on `moved`);
* `long_press()` → `None` — **tooltips never appeared**.

Fix (`lib.rs:402`): `moved` latches only when
`distance(down, point) > layout.slop(TAP_SLOP)` (12 dp × HUD scale), the same
Android `ViewConfiguration.getScaledTouchSlop` rule the pan/box path already
used. One flag now serves tap-vs-drag, placement release, long-press cancel and
the minimap.

### 2. Minimap jitter scrubbed the camera; a long scrub issued an order

`Zone::Minimap` returned `TouchMove::Minimap(point)` for **any** motion, and
`Game::pointer_move` centres the camera on every such point — a tap drifted the
view. `TouchEnd::Minimap.hold` was pure duration, so panning the minimap for
0.45 s and lifting fired an attack-move for the whole army.

Fix (`lib.rs:407`, `lib.rs:482`): the scrub follows the finger only after the
slop is crossed, and `hold` is a *stationary* hold (`!pointer.moved`).
A jittered 0.5 s minimap tap still orders (it stood still); a real scrub never
does.

### 3. `last_tap` survived a drag → spurious double-tap

A tap, then a pan/box, then a tap within 0.36 s and 36 dp of the first one was
reported as `double=true` (Android's `GestureDetector` drops the pending double
tap as soon as a scroll starts). Fix: the Pan/Box transition clears `last_tap`
(`lib.rs:429`, applied at `lib.rs:437`).

### 4. A resting second finger destroyed a command-card press

`pinch_begin` unconditionally dropped the live pointer. On a 2414×1080 phone
the other thumb rests on the glass, so the platform forwards a second
`FINGER_DOWN`, the session starts a pinch, and the card press was silently
replaced by a pinch/`resume_pan` — **command buttons became unreliable while
holding the device**.

Fix (`lib.rs:533`, `lib.rs:555`, `lib.rs:570`): a pointer that owns a HUD
gesture (`Button`, `Blocked`, `Place`) is not converted into a pinch and is not
replaced by a promoted pan; `hud_gesture()` exposes the state to the host and
to diagnostics. World/minimap gestures pinch exactly as before.

### 5. Lifting a finger from a 3-finger pinch jumped the camera

With three fingers down, the pinch is driven by the first two. When one of
them lifted, the surviving pair's centroid/distance were used immediately
against the old baseline: the next `pinch_update` reported the discontinuity as
a pan (up to ~100 px here) and a zoom step. Fix (`session.rs:189`,
`session.rs:287`): the pinch rebases on the surviving pair; `moved` is
preserved so a travelled pinch can never come back as a two-finger tap.

### 6. Retail `CommandStrings.txt` was parsed wrong (oracle)

`analysis/war3_full3/Units/CommandStrings.txt` (observed 2026-11-21) has 19
sections; the port's parser got three of them wrong:

| Retail data | Old parse | Fixed |
|---|---|---|
| `[CmdCancel] Hotkey=512` (Escape, ×4 sections) | bound `'5'` → pressing control group 5 could fire Cancel | recorded as Escape (`is_escape`/`escape_actions`), no digit binding |
| `[CmdHoldPos] Hotkey=H` | never overrode `CmdHold` (orphan section) | canonicalised to `CmdHold` |
| `[CmdBuildHuman|Orc|NightElf|Undead] Hotkey=B` | four orphan sections | canonicalised to `CmdBuild` |
| `S` on `CmdStop` **and** `CmdPurchase` | alphabetical map order resolved `'s'` → `CmdPurchase` (a shop command not on the card) | card actions win; `actions_for('s')` keeps both in resolve order |

Fix: `remap.rs:25` (`HOTKEY_ESCAPE_CODE`), `remap.rs:33` (`canonical_action`),
`remap.rs:149`, `remap.rs:267` (`actions_for`), `remap.rs:287` (`is_escape`).
`apply_object_data` uses the same value parser (a virtual-key code is not a
letter there either).

### 7. The marquee selected in world-AABB space, not the drawn rectangle

`Game::box_select` projects the two screen corners to the ground and hands
`ui::selection::box_select` a world AABB (`game.rs:6389-6401`). Under the
perspective camera the ground quad is a **trapezoid**: its AABB is wider than
the drawn marquee at the far edge, so units standing beside the box were swept
in. The game already fills `UnitSnapshot.screen` for every unit
(`ui.rs:3321-3339`), so the projection is available.

Added `box_selection_screen` (`selection.rs`, after `box_selection`): the
marquee rule tests the projected position against the drawn rect; buildings,
enemies, dead and off-camera units stay out, Shift keeps and dedupes.
`box_selection` (world) is kept for callers without a projection. The call-site
switch is one line in the game layer (owner B, see Risks).

## Tests added (named by mechanic id, TEST_POLICY)

| Test | Proves |
|---|---|
| `ui_trace_sub_slop_jitter_is_still_a_tap_and_a_placement` (lib.rs) | jittered world tap selects; jittered placement tap places |
| `ui_trace_drag_past_slop_clears_the_pending_double_tap` (lib.rs) | a pan between taps kills the double tap |
| `ui_trace_hud_press_survives_a_resting_second_finger` (lib.rs) | no pinch from a HUD pointer, press resolves; world still pinches |
| `ui_trace_minimap_scrub_never_orders_and_jitter_never_scrubs` (lib.rs) | no scrub under 12 dp; scrub → `hold:false` |
| `ui_trace_lifting_one_finger_of_three_rebases_the_pinch_without_a_jump` (session.rs) | rebase values + next update measured from the surviving pair |
| `ui_hotkeys_remap_retail_sections_canonicalise_to_the_card` | `CmdHoldPos`, 4 race build sections, card letters |
| `ui_hotkeys_remap_retail_escape_code_is_not_the_five_key` | `512` ≠ `'5'`, escape set, rebind clears it |
| `ui_hotkeys_remap_retail_duplicate_letters_resolve_to_the_visible_command` | `s` → Stop, `actions_for` order |
| `ui_hotkeys_remap_matches_the_shipped_command_strings_file` | re-reads the operator's file when present; prints `SKIP … absent` on a fresh clone (no silent pass) |
| `ui_selection_box_screen_uses_the_drawn_rectangle_not_the_world_hull` | screen marquee picks [1,3]; the world hull also takes the unit beside the box |
| `ui_selection_box_screen_additive_keeps_and_dedupes_the_group` | Shift keeps order, dedupes, empty drag clears |

`stationary_long_press_reports_the_down_point` (legacy name referenced by
`PARITY_TOUCH.md`) was corrected: the old body moved 5 px and expected the
tooltip to die, i.e. it pinned defect #1. It now proves sub-slop jitter keeps
the tooltip and that leaving the disc cancels it.

No trace fixture was added or edited: none of these behaviours was captured
from a device, and a trace must encode recorded device behaviour.

## Evidence (rc / output)

```text
scripts/h2-cargo check -p wc3-input --offline --all-targets   -> rc 0
scripts/h2-cargo test  -p wc3-input --offline                 -> rc 0
  lib: 73 passed; 0 failed          (68 before, +5 new)
  integration: 13 binaries, all ok  (ui_hotkeys_retail 4, ui_selection_box_screen 2)
  doc-tests: 1 passed
touch-trace --check <each of 15 fixtures>                     -> 15/15 rc 0
  binary target/debug/examples/touch-trace  15:43:28  >  lib.rs 15:35:38
```

Trace fixtures covered: `box-select`, `box-additive`, `group-row`,
`key-repeat`, `minimap-orders`, `multitouch-cancel`, `multitouch-third-finger`,
`orders-targeting`, `patrol-two-taps`, `pinch-pan`, `placement`,
`rotation-focus`, `touch-orders`, `two-finger-tap`, `type-select`.

## Result

Done for the crate: six root causes fixed with named tests, `cargo test`
rc=0, 15/15 traces replay with `--check` rc=0, retail hotkey data parsed
against the shipped oracle.

Honest labels: the fixes are proven on the host replay path and against the
shipped data file (`observed`); their device-level effect (jitter tolerance,
resting-thumb press, 3-finger rebase) is `inferred` from Android touch-slop
semantics and the SDL finger stream — **no device run was made in this
session** (the device slot `H2_DEVICE_SLOT=1` is serial and owned by agent G).
A device re-run of ROUND 8 would make them `observed` on hardware.

## Risks / Next

1. **Residual (owner B, `crates/wc3-game/src/game.rs`).** With fix #4 the common
   stray-finger case resolves, but if the *card press's own finger* lifts
   before the resting finger, the session emits `pinch_end` +
   `pointer_resume_pan` (which the router refuses) and no `pointer_up`, so that
   one press is dropped. Closing it needs the session to know the veto:
   - `wc3-input`: `InputSink` gains `fn pinch_allowed(&self) -> bool { true }`
     and `TouchRouter` consults it instead of always entering `PinchProgress`
     (ready to add if the owner wants it);
   - `game.rs`: `impl InputSink for …Game… { fn pinch_allowed(&self) -> bool { !self.0.touch.hud_gesture() } }`.
   Exact symptom, file and next step recorded here and in capture 1258.
2. **Marquee call site (owner B, `crates/wc3-game/src/game.rs:6389` + `ui.rs:3368`).**
   `wc3-input::selection::box_selection_screen` is implemented and tested; the
   game still calls the world-AABB `box_selection`. Switch:
   `let min = a.min(b); let max = a.max(b);` (screen) and
   `policy::box_selection_screen(&units, game.player, min, max, additive, &current)`.
   Until then the over-select at the far edge of the box remains on device.
3. **Minimap long scrub** keeps moving the camera and no longer orders; if the
   product wants "scrub release = camera only", that is now the behaviour — no
   further change needed.
4. **Test runner registration (owner O/N).** `scripts/**` is outside this
   agent's paths; the new tests are discovered by `cargo test -p wc3-input`
   but are not yet listed in `analysis/tech-lead/run_mechanic_tests.sh`.
5. `analysis/war3_full3` is untracked, so the oracle test skips on a fresh
   clone; the transcribed 19-section table covers the same shape in-repo.

## H2

- preflight `ev-078b21c32349c2a2`
- capture 1258, experience 4920
- ledger 606 checkpoints, closed with `h2_ids_json` (see final report)

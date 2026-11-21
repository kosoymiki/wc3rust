# BLOCKED-touch — cross-layer compile regression in HUD-owned mobile_hud.rs (test build)

Task ref: r2-touch, ledger 356, owner: agent-touch, session: ses_e30595423ffe7cc9wy7YBbXx9q  
Date: 2026-11-13 (~18:18 MSK first block resolved; ~18:33 MSK new block). State: **BLOCKED** (not fully green — `wc3-game` lib test build fails outside owner paths).

## Prior block (resolved)
FDF-owned `crates/wc3-game/src/text.rs:81` `Eq` derive on `f32` caused `E0277` at ~18:15 MSK. Verified fixed now: `text.rs:81` is `#[derive(Debug, Clone, PartialEq)]` (mtime 18:15:47; current content has no `Eq`).

## Current symptom (exact errors, `r6-touch-fresh.log`)
Command:
`ulimit -v 4500000; CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 cargo check -p wc3-input -p wc3-game --lib; cargo test -p wc3-input; cargo test -p wc3-game --lib game::tests:: -- --test-threads=1`

Observed:
- `cargo check -p wc3-input -p wc3-game --lib` → **CHECK_RC=0**
- `cargo test -p wc3-input` → **INPUT_RC=0** (`test result: ok. 22 passed; 0 failed; 0 ignored`)
- `cargo test -p wc3-game --lib game::tests:: -- --test-threads=1` → **GAME_TESTS_RC=101** (lib test compile)

Errors (HUD-owned, outside touch owner paths):
```
error[E0428]: the name `luminance` is defined multiple times
    --> crates/wc3-game/src/mobile_hud.rs:2220:5
     |
2202 |     fn luminance(color: [f32; 4]) -> f32 { ... }  (previous)
2220 |     fn luminance(c: [f32; 4]) -> f32 { ... }      (redefined)

error[E0425]: cannot find function `rasterize` in this scope
    --> crates/wc3-game/src/mobile_hud.rs:2042:22 (+ :2076)
error[E0425]: cannot find function `write_png` in this scope
    --> crates/wc3-game/src/mobile_hud.rs:2051:9 (+ :2079, :2085)
error[E0425]: cannot find function `castbar_fill_rect` in this scope
    --> crates/wc3-game/src/mobile_hud.rs:2110:13
error[E0433]: cannot find type `AtlasGridUv` in this scope
    --> crates/wc3-game/src/mobile_hud.rs:2270:17 (+ :2272)
error[E0433]: cannot find type `BackdropInsets` in this scope
    --> crates/wc3-game/src/mobile_hud.rs:2278:17 (+ :2280)
error: could not compile `wc3-game` (lib test) due to 11 previous errors
```

## Root cause / scope
`mobile_hud.rs` is HUD-owned and **READ_ONLY** for this agent (packet: "READ_ONLY: crates/wc3-game/src/mobile_hud.rs (HUD-owned -> patch only)"). I made no edits to `mobile_hud.rs`. `wc3-game --lib` compiles (CHECK_RC=0); the break is in lib-test compilation triggered by HUD/test helpers in `mobile_hud.rs`.

## Next step (HUD owner)
In `crates/wc3-game/src/mobile_hud.rs`: remove duplicate `luminance` definition (~2220), restore/import `rasterize`, `write_png`, `castbar_fill_rect`, `AtlasGridUv`, `BackdropInsets` (or gate the affected tests/helpers appropriately). Re-run `cargo test -p wc3-game --lib game::tests:: -- --test-threads=1`.

## Evidence achieved this round (owner paths)
- **p1:** Control groups extended to WC3 1..9,0 (10 groups). `crates/wc3-input/src/lib.rs`: `ControlGroups { groups: [Vec<u32>;10] }` slot 0↔index 9, `next_free/next_non_empty` over 1..9,0, `Hotkey::parse` accepts `'0'`; tests updated (`hotkey_parse_and_control_group_cycle` covers slot 0/wrap). `crates/wc3-game/src/game.rs`: `group_row_slots() -> [Rect;10]`, `draw_group_row` labels 1..9,0, `hit_group_slot` maps index 9→slot 0, `control_group_row_is_on_screen_and_tappable` asserts 10 slots and slot 0 save/recall.
- **p2:** Box-select physical path documented (down→drag>threshold→up, `TouchEnd::Box`, marquee `box_marquee_rect`/`draw_box_marquee`); harness boundary for `input motionevent`/300 ms `DragMode::Box` documented, not faked (`analysis/agent-touch/BOX_SELECT_EDGE_PAN.md`, `PARITY_TOUCH.md`).
- **p3:** `analysis/agent-touch/PARITY_TOUCH.md` §1c extended with DragonUI micromenu grid/quickslot donor intake (`modules/micromenu.lua` `GetMicroLayoutMetrics`/`LayoutMicroButtons` grid steps, `modules/buttons.lua` quickslot art `ui-quickslot-depress.blp`/`ui-quickslot2.blp`, `ATLAS_FRAME`/`FitSlotArt`) and mapping to Rust row; read-only donor `~/DragonUI-20261113/**`.
- **Gates:** `CHECK_RC=0`, `INPUT_RC=0` (22 passed). `GAME_TESTS_RC=101` due to HUD-owned `mobile_hud.rs` test-compile errors (external). Prior green: `wc3-game` `game::tests` 31 passed/1 ignored rc 0 at 12:20 MSK before external regressions. No device run.

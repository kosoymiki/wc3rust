# ROUND 7/8 — HUD layer verification + device artifact (2026-11-13)

task_ref `r2-hud` · ledger **347** · owner paths `crates/wc3-game/src/{mobile_hud,game,hud,ui}.rs`, `analysis/agent-hud/**`

## Goal
Verify ROUND6 deliveries on the current tree, run the layer gates, and produce the device/product artifact
`analysis/agent-hud/console-v5.png` (one device cycle). No re-implementation of completed work.

## ROUND6 deliveries verified on the current tree (observed)

| Increment | Evidence (file:line) |
|---|---|
| Real lumber in HUD | `game.rs:2238` `let lumber_text = self.sim.lumber[self.player as usize].max(0).to_string();` → `:2244 ResourcePill::new(&lumber_text, "LUM", green)` |
| SLK build menu (farm/tower/producer) | `hud.rs:829` `let build_menu = crate::sim_bridge::build_menu(&game.sim, game.player);` → slots 4/5 (`:844`) and 8 (`:888`) |
| Hall pinned at card slot 9 | `hud.rs:955-983` reasserts SLK entries and `BUILD` TownHall at slot 9 (`:976`, action `UiAction::Build(TownHall)`) |
| DragonUI UiScale (uiscale.lua) | `mobile_hud.rs:20-56` `DRAGONUI_DESIGN_HEIGHT=768`, `UI_SCALE_MIN=0.40`, `UI_SCALE_MAX=1.15`, `EPSILON=0.001`, `auto=768/screen_height` |
| DragonUI hotkey font (buttons.lua:207) | `mobile_hud.rs:61` `scaled_hotkey_font_size(size, reference_scale, button_scale)` |
| DragonUI castbar (castbar.lua:885) | `mobile_hud.rs:1819` `castbar_fill_rect(parent, progress)` → fill height = parent height |
| DragonUI micromenu atlas UV | `mobile_hud.rs:1780` `AtlasGridUv::{MICRO_MENU 4x4, MINIMAP_QUICKSLOT 4x1}::uv(col,row)` |
| DragonUI button insets | `mobile_hud.rs:1801` `BackdropInsets::{uniform,new}` |
| DragonUI font registry (fonts.lua) | `ui.rs:52` `pub type DragonUiFontRegistry = FontRegistry;` |

## Layer gates (current tree, exact commands + rc)

| Gate | Command | rc | Evidence |
|---|---|---:|---|
| Compile | `cargo check -p wc3-game --lib` | **0** | 0 errors, 46 warnings (`round7` check output) |
| Full game lib | `cargo test -p wc3-game --lib -- --test-threads=1` | **0** | **102 passed, 0 failed, 19 ignored** — `analysis/agent-hud/round7-full-lib.log` |
| HUD/mobile | `cargo test -p wc3-game --lib hud::` | **0** | **13 passed, 0 failed** (8 baseline + 5 DragonUI) — `analysis/agent-hud/round7-hud-tests.log` |
| DragonUI unit tests | in `mobile_hud.rs` tests | ok | `dragonui_uiscale_auto_clamp_epsilon`, `dragonui_hotkey_font_scales_with_button_ratio`, `dragonui_castbar_fills_parent_height`, `dragonui_micromenu_atlas_grid_uv`, `dragonui_button_backdrop_insets` |

## Device cycle (one cycle; granted by packet)

Device: RMX3709, Android 16, display override 1080x2414 → game surface 2414x1080.

Preconditions fixed during the cycle (observed): device was in `DeviceIdle mState=IDLE` and keyguard was stuck
(`mKeyguardDrawComplete=false`); screen off. Fixes: `dumpsys deviceidle disable` (debug, resets on reboot),
`settings put system screen_off_timeout 2147483647`, `locksettings set-disabled true`, SystemUI restart
(`kill <pidof com.systemui>`, auto-respawn), then `cmd window dismiss-keyguard` → `deviceLocked=0`.

Launch: `monkey -p org.herofolends.p0 -c android.intent.category.LAUNCHER 1` (foreground launch; plain `am start`
left focus on Termux).

First-frame evidence (`logcat -d | grep GAME_FIRST_FRAME`):
```
GAME_FIRST_FRAME surface=2414x1080 fullscreen=true safe=SafeArea { left: 210.0, top: 120.0, right: 90.0, bottom: 96.0 } units=16 phase=Playing
```

APK: `build/android-p0/herofolends-p0.apk` sha256 `70c15e91e4f5f8d4d33cb510f371c7a4929aaee5b67b8714dc32ce06a1cc245e`,
versionName `0.1-p0`, installed `lastUpdateTime=2026-11-13 19:44:47`.

### Artifacts

| Artifact | sha256 | Content |
|---|---|---|
| `analysis/agent-hud/console-v5.png` | `7182acbb83ddd571ed0cd925776ccff760a28d990a33d56067a690568cb047e7` | 2414x1080 device capture: map `Альянс04`, console with minimap, empty portrait, **GOLD 1001 / LUM 1000** (real sim lumber), SUP 17/…, NO UPKEEP, control-group row `1..9,0`, command card `S/A/B/G` + `M/H/-/+` |
| `analysis/agent-hud/console-v5-build-menu.png` | `8d72ff93435f755273e568b8b76a4ee951373d86d05f77cc1961e36a985d3659` | Worker selected (`РАБОТНИК`, HP 220/220, DMG 6, RNG…): **three SLK build entries + `BUILD` hall at slot 9** |
| `analysis/agent-hud/console-v5-device-a..c.png`, `console-v5-selected.png`, `console-v5-input-test.png`, `console-v5-select2.png` | see files | cycle probes/intermediate frames |

## Result
- ROUND6 deliveries confirmed present and compiling on the current tree.
- Layer gates green: check rc 0, full lib 102/0/19 rc 0, HUD 13/13 rc 0.
- Device artifact produced: `console-v5.png` (no selection) and `console-v5-build-menu.png` (worker card),
  both from a real 2414x1080 device cycle with `GAME_FIRST_FRAME` evidence.

## Risks / Next
- **Supply pill truncation**: device shot shows `SUP 17/…` (cap clipped by pill width). Fix: compact supply
  format or a slightly wider supply pill in `mobile_hud.rs`, then re-capture next device cycle.
- Console art quads on device read as brown/red gradients (HumanUITile textures); fidelity pass for tile
  aspect/UV can be a separate polish item.
- Device was in DeviceIdle + stuck keyguard; the cycle needed `dumpsys deviceidle disable` and a SystemUI
  restart. Device settings restored after capture (timeout/brightness/lockscreen/deviceidle/stayon).
- `render.rs` remains world-owned; MDL 3-D portrait request stays in `analysis/tech-lead/merge-hud.md` §9.

## Round 8 addition — supply pill polish + post-fix gates

- **Root cause (device-visible):** `mobile_hud.rs` sized the resource pills from the command-card
  width (`card.w`, 268 px at scale 1) instead of the resource shelf (`resources.w`, 720 px landscape),
  so the supply value clipped: device shot shows `SUP 17/…`.
- **Fix:** `pill_w = (resources.w - pill_gap * 2.0) / 3.0` in `MobileHud::layout` (owner path).
- **Post-fix gates:**
  - `cargo check -p wc3-game --lib` rc **0** (0 errors, 44 warnings).
  - `cargo test -p wc3-game --lib hud::` rc **0** — **13 passed, 0 failed** (`round7-hud-tests2.log`);
    mock regenerated with the full cap: `analysis/agent-hud/mock-resource-crop.png` shows `7/12`.
  - `cargo test -p wc3-game --lib -- --test-threads=1` — **105 passed, 1 failed, 20 ignored**
    (`round7-full-lib2.log`). The single failure is **not in HUD owner paths**:
    `sim_bridge::tests::map_units_spawn_from_human04_scene` at `sim_bridge.rs:838`
    `assertion left == right failed: Human04 known placements left: 59 right: 90`
    (sim-owned file; uses `analysis/war3_map/Maps/Campaign/Human04.w3m`).
    HUD/game acceptance tests all pass: `game::tests::map_units_spawn` ok,
    `game::tests::build_button_slot7_enters_placement` ok,
    `game::tests::control_group_row_is_on_screen_and_tappable` ok.
- **Artifact note:** `console-v5.png` is the device capture taken *before* this fix (it documents the
  truncated `SUP 17/…`); the post-fix state is proven by the regenerated headless mock. A re-capture
  in a future device cycle will show the cap.

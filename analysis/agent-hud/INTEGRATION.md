# Integration guide — MobileHud in the game layer

Status 2026-11-12: `crates/wc3-game/src/game.rs` already imports and drives the new HUD (the tech lead/another workstream wired it while this module was being built). This file records the exact contract, the remaining gaps, and copy-paste patches. No edit to `game.rs` is required for the HUD to compile; the gaps below are data-completeness fixes.

## 1. Already wired (observed in game.rs)

- Fields: `pub mobile_hud: crate::hud::mobile_hud::MobileHud`, `pub mobile_layout: Option<MobileHudLayout>`.
- `Game::new` constructs `MobileHud::new()` (line ~247) and `mobile_layout: None`.
- Selection/full-frame rebuild calls `self.mobile_hud.layout(&dummy_view)` and copies `minimap_map`, `command`, `selection`, `primary`, `context`, `chip_slots`, `joystick_centre/radius` into `HudLayout` for hit testing (lines ~480-540). This keeps rendering and touch hit-testing on one geometry source.
- `build_hud` builds `ResourcePill`, `CommandButton`, `ChipView`, `SelectionView`, `MinimapView`, `PhaseView`, `ToastView`, `HintView`, `JoystickView`, then calls:
  `let _ = self.mobile_hud.draw(&view, ui, &self.font, &self.assets.chrome);`
- Safe area is passed as `SafeArea::new(self.safe_insets.left, self.safe_insets.top, self.safe_insets.right, self.safe_insets.bottom)`.

## 2. Required Android/inset wiring

The HUD reserves `safe.top` before drawing. The app must supply real window insets and hide the bars; otherwise the status bar still paints over the reserved band (the band is empty, but the system chrome remains visible over the 3-D scene, not over HUD controls).

Android (SDLActivity / Kotlin):

```kotlin
WindowCompat.setDecorFitsSystemWindows(window, false)
WindowInsetsControllerCompat(window, window.decorView).apply {
    hide(WindowInsetsCompat.Type.systemBars())
    systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
}
window.decorView.setOnApplyWindowInsetsListener { view, insets ->
    val cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
    val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
    nativeSetSafeInsets(
        maxOf(cutout.left, bars.left),
        maxOf(cutout.top, bars.top),
        maxOf(cutout.right, bars.right),
        maxOf(cutout.bottom, bars.bottom),
    )
    insets
}
```

SDL3 alternative: `SDL_GetWindowSafeArea` / `SDL_EVENT_WINDOW_DISPLAY_CHANGED` and SDL's display cutout APIs; pass the four sides in physical pixels.

Landscape 2414x1080 fallback: `SafeArea::landscape_1080()` = 80 top / 48 bottom / 48 sides. Portrait 1080x2414 fallback: `96 / 64 / 24 / 24`. If the system bars are still visible because immersive mode failed, report the actual inset; do not reduce the 80 px top to zero.

## 3. Two data gaps in the current game.rs call (exact patches)

The HUD supports armor/damage and 4 ability slots, but the current `SelectionView` builder does not pass them, so the card legitimately shows `ARMOR 0 DAMAGE 0` and no ability sockets. Fix inside the `SelectionView::new(...)` chain in `build_hud` (read-only for this workstream, owner must apply):

```rust
                .portrait(self.assets.ui.unit_icon(unit.kind))
                .stats(unit.armor, unit.damage)
                .abilities(&self.selection_ability_icons)
                .production(production)
```

`selection_ability_icons` can be a `Vec<Option<usize>>` built each frame from available actions, e.g.:

```rust
let ability_icons: Vec<Option<usize>> = [4, 5, 0, 3]
    .iter()
    .filter_map(|index| self.buttons.get(*index))
    .map(|button| self.assets.ui.command_icon(&button.action))
    .collect();
```

Pass `abilities(&ability_icons)` while `ability_icons` is alive in `build_hud`. This is presentation only; the action semantics stay in `buttons`/`UiAction`.

For buildings, the same call works (`unit.armor`, `unit.damage` are in `wc3_sim::Unit`); training progress is already passed through `ProductionView`.

## 4. Icon atlas / renderer contract

- `analysis/agent-hud/icons/commandbuttons_atlas.png`: 1024x2816, 16 columns x 44 rows, 64x64 cells, all 691 files from `ReplaceableTextures/CommandButtons`.
- `catalog.tsv`: stem → cell index (row-major). Add a loader in `assets.rs`/renderer (owner): read the PNG once, upload as `Rgba8UnormSrgb`, and store `stem -> (u0,v0,u1,v1)`.
- Render policy: **64 px at 1x, 72 px max**; never stretch a 64x64 source to 96 px (the current HUD already draws 64 px icons and a 64 px portrait inside the 96 px card). Use linear filtering for the 1.125x case, nearest for exact integer scales; keep mipmaps off for the atlas (icons are sampled 1:1).
- The source BLP1 command buttons use `content_type=0` (JPEG) and `alpha_bits=0`: they are opaque full-bleed art, so alpha 255..255 is correct and there are no fringes. If future BLP2/DISBTN sources carry alpha, upload the same RGBA and preserve the alpha channel; the validator (`analysis/agent-hud/icons/validation.json`) reports min/max alpha per run.
- Do not generate placeholder icons. A missing name is a hard error in the catalog test.

## 5. Hit-testing and touch actions

`MobileHudLayout` exposes `primary[4]` (up, right, down, left), `context_slots`, `chip_slots`, `minimap`, `selection`, `joystick`, and `hit(point) -> Option<HudHit>`. The current game copies these into `HudLayout` so the existing `UiAction` mapping keeps working:

| slot | existing button index | action |
|---|---|---|
| primary[0] (up) | 4 | AttackMove |
| primary[1] (right) | 5 | Hold |
| primary[2] (down) | 0 | Stop |
| primary[3] (left) | 3 | Gather |
| context[0..3] | 6..9 | Train Footman/Archer/Peasant, Build |
| chips[0..3] | 1, 2, 10, 11 | Army, Base, Zoom-, Zoom+ |

`MobileHudLayout::hit` is available for a future single-source input path; it returns `HudHit::{Minimap, Primary, Context, Chip, Selection, Joystick, Phase}`. The current game's pointer code already uses the copied rects.

## 6. Joystick

The HUD draws the base, ticks, knob and `CAMERA` label from `JoystickView { active, label }`. `game.rs` passes `self.joystick`; `VirtualJoystick` remains the input owner (drag-to vector, dead zone, release). The HUD does not consume touches itself.

## 7. Build / verify

```bash
ulimit -v 4000000
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo check -p wc3-game
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p wc3-game --lib mobile_hud -- --nocapture --test-threads=1
```

Expected evidence:

- `analysis/agent-hud/mock-2414x1080.png`, `mock-1080x2414.png`
- `analysis/agent-hud/acceptance.json` (touch sizes, overlap pairs = 0, layout issues = 0, WCAG ratios)
- `analysis/agent-hud/icons/{commandbuttons_atlas.png,contact_sheet.png,catalog.tsv,validation.json}`
- Test names: `layout_is_safe_and_clear_at_reference_resolutions`, `labels_fit_their_boxes`, `hit_test_covers_the_thumb_zones`, `contrast_of_tokens_meets_wcag_aa`, `headless_mock_render_writes_pngs`, `commandbuttons_catalog_validates_all_691_icons`.

## 8. Coordination note (2026-11-12 16:00)

A concurrent edit in `crates/wc3-game/src/render.rs` (read-only for this workstream) currently fails type inference:

```text
error[E0282]: type annotations needed for `Vec<_>`
  --> crates/wc3-game/src/render.rs:321:13
```

The line is `textures.push(textures[0].clone())` before any element is pushed; annotate the vector, e.g. `let mut textures: Vec<wgpu::BindGroup> = Vec::with_capacity(...)`. This is outside this workstream's owner paths; the render owner must fix it before a full `cargo test`/APK build. The HUD module itself compiles cleanly up to that point.

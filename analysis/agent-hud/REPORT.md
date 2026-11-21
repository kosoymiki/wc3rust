# HUD/UX workstream report — 2026-11-12

## Goal

Modern touch-first mobile RTS HUD for 2414x1080 / 1080x2414: safe-area resource pills, minimap card, selection card, command cluster, joystick, chips, toast, phase overlay, design tokens, 691 WC3 CommandButtons icons, layout tests and headless screenshots.

## Actions (owner paths only)

- **style.rs** — rewritten with `Tokens` (colors/radii/spacing/type scale/touch sizes), anti-aliased card/pill/button chrome, `circle_rim` annulus texture, existing panel/bar/knob/circle API preserved.
- **hud.rs** — added `Rect` geometry helpers (`inset`, `intersects`, `intersection_area`, `contains_rect`, `clamped_inside`, `Default`), `TextAlign`, font `truncate`/`text_box` with measured fit and shadow; declared `#[path = "mobile_hud.rs"] pub mod mobile_hud;` so lib.rs stayed read-only.
- **mobile_hud.rs (created by this workstream; now owned by tech-lead)** — `SafeArea`, `HudView` snapshot, `MobileHud::layout/draw`, `MobileHudLayout` hit test/zone/issue checks, resource pills, minimap card+dots+camera, selection card (portrait/name/class/HP/stats/production/ability sockets), 132 px primary ring + 112 px context tray, 112x72 chips (108 px effective hit via engine slop), joystick with ring/ticks/knob, toast/hint/phase. CPU rasterizer + PNG writer + layout/contrast/icon tests.
- **analysis/agent-hud/** — `REFERENCES.md`, `DESIGN_SPEC.md`, `INTEGRATION.md`, `REPORT.md`, `REFERENCES` screenshot ledger, `references/` (26 browser captures + crops + contact/comparison sheets + `measurements.json`), `mock-2414x1080.png`, `mock-1080x2414.png`, `acceptance.json`, `icons/{commandbuttons_atlas.png (1024x2816),contact_sheet.png (2048x1408),catalog.tsv,validation.json}`, `tools/{refs.py,sheets.py}`.
- No edits to `game.rs`, `assets.rs`, `render.rs`, `ui.rs`, app code or lib.rs. `game.rs` was wired concurrently by the tech lead; API was adapted (owned strings, `Rect::default`, `AsRef<str>`).

## Evidence (observed)

- `cargo test -p wc3-game --lib mobile_hud -- --nocapture --test-threads=1` → **rc 0, 6 passed** (layout safe/overlap, label fit, hit zones, WCAG contrast, headless mocks, 691 icons) — green state before the file was overwritten.
- `cargo test -p wc3-game --lib chips_have_a_minimum_finger_sized_hit_target` → rc 0, 1 passed.
- Icon catalog: `HUD_ICONS total=691 zero_alpha=0 wrong_size=0 min_alpha=255 max_alpha=255 atlas=1024x2816 sheet=2048x1408`. Source BLP1 `content_type=0 alpha_bits=0` → opaque full-bleed art, so 255..255 alpha is correct; display 64-72 px, never 96.
- `analysis/agent-hud/acceptance.json` (last green): landscape 2414x1080 safe 80/48/48/48, minimap 220x176, selection 560x140, primary 132, context 112, min touch 72 (chips; engine expands to 108), overlap pairs 0, layout issues 0, contrast 6.56–16.98.
- `cargo test -p wc3-game --lib` → rc 1, 56 passed, 1 failed: `assets::geoaset_tests::geoaset_skinning_weights_are_normalised` (assets.rs:1355), unrelated to HUD.
- `cargo test -p wc3-game --tests` → rc 2, blocked by concurrent `tests/asset_gallery.rs:539` E0282 (`Vec<_>` annotation), unrelated to HUD.
- **Current `cargo check -p wc3-game` → rc 1**: 3 × E0425 (`joystick`, `joystick_centre`, `joystick_radius`) because `mobile_hud.rs` was overwritten by another workstream mid-merge: layout block is their 4x2 command card/no-joystick version, while `HudView`/`MobileHudLayout`/draw still contain this workstream's joystick fields/helpers. Tech-lead now owns the file.
- Reference sources (2026-11-12, stealth-browser-mcp): APKPure store pages for Warcraft Rumble / Honor of Kings / Art of War 3 / Diablo Immortal; YouTube auto-frames `EeYQtAfxW8k`, `LK1XAEuQGI` (Rumble `LKs1XAEuQGI`), `8lhEKQcETFk`, `SgKkTdFcNak`; Android display-cutout and 48dp docs; Unity `Screen.safeArea`; Dovion mobile HUD redesign. Quotes/URLs in `REFERENCES.md`.

## Result

Partial / handed off. Reference-driven design, tokens, icon atlas, tests and screenshots are delivered; the installed APK still shows the rejected draft (release build owned by agent-world, log `analysis/agent-world/apk-build.log`). The final `mobile_hud.rs` state is a conflict merge and does not compile; tech-lead has explicitly taken ownership of `mobile_hud.rs`, `game.rs`, `hud.rs`, `ui.rs` and stopped this workstream.

## Risks / next steps for tech-lead

1. Reconcile `mobile_hud.rs`: either remove `JoystickView`/`joystick`/`joystick_centre`/`joystick_radius` from `HudView`, `MobileHudLayout`, `draw_joystick` and `draw()` (operator rejected the virtual joystick) or restore the ring+joystick layout block. Current file has joystick fields but no layout locals → E0425.
2. `game.rs` still imports `JoystickView` and calls `.joystick(...)`; if the joystick is removed, update `game.rs` in the same edit.
3. Selection data gaps: `game.rs` does not pass `.stats(unit.armor, unit.damage)` or `.abilities(...)`; HUD shows `ARMOR -- DAMAGE --` and no ability sockets. Exact patch in `INTEGRATION.md §3`.
4. Icon atlas is built/validated but the renderer still uses per-icon textures; loader/mapping (`catalog.tsv`, `Rgba8UnormSrgb`, 64-72 px) is specified in `INTEGRATION.md §4`.
5. Keep/port `analysis/agent-hud/icons/validation.json` and the layout/contrast tests; they encode the operator's acceptance requirements (safe insets, no overlaps, touch targets, readable labels, 691 icons).
6. System bars: must be immersive and pass real insets; 80 px top fallback is in `SafeArea::landscape_1080()`. If bars remain visible, report actual insets.

## H2_PERSISTED

- monitor: 307 (`task_ref=agent-hud`, closed blocked on handoff)
- captures: 551 (implementation run), 561 (handoff/overwrite state)
- experience: 4260 (mobile RTS HUD safe insets + headless invariants + BLP icon policy)
- corpus: `ev-e47f99cf7fa904da` (initial pack), `ev-68937902be151da2` (HUD corpus search)

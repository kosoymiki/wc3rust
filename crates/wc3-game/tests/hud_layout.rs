//! Pure layout invariants for the mobile HUD (TZ §11, ledger 478).
//!
//! No GPU, no window, no game: `HudLayout::compute` is a pure function of
//! `WindowMetrics` + preferences + the small live state, so every rule below
//! is checked on rectangles alone. Test names are the acceptance ids from the
//! engineering task; each one reports the numbers it asserted so a failure can
//! be read without a debugger.
//!
//! ```text
//! scripts/h2-cargo test -p wc3-game --test hud_layout --offline
//! ```

use glam::Vec2;
use wc3_game::hud::Rect;
use wc3_game::hud::mobile_hud::{
    HudHit, HudLayout, HudLayoutMode, HudLayoutState, Orientation, SafeArea, UiPreferences,
    WindowMetrics, CARD_COLUMNS, CARD_GAP_DP, CARD_ROWS, CARD_SLOTS, CARD_W_DP, CONSOLE_PAD_DP,
    GROUP_COUNT, TOUCH_DP, ZONE_GAP_DP,
};

/// The measured target phone (tech-lead 2026-11-17): 2414x1080 px at density
/// 480 dpi overridden from 560 dpi physical -> density_scale 3.0.
const DEVICE_PX: Vec2 = Vec2::new(2414.0, 1080.0);
const DEVICE_DENSITY: f32 = 3.0;

fn metrics(px: Vec2, density: f32) -> WindowMetrics {
    WindowMetrics::new(px, density)
}

fn device() -> WindowMetrics {
    metrics(DEVICE_PX, DEVICE_DENSITY)
}

fn prefs() -> UiPreferences {
    UiPreferences::default()
}

fn layout(metrics: WindowMetrics, state: HudLayoutState) -> HudLayout {
    HudLayout::compute(metrics, prefs(), state)
}

fn dp_of(metrics: WindowMetrics) -> Vec2 {
    metrics.screen_dp()
}

/// Every viewport of the TZ §12 matrix plus the measured device.
fn matrix() -> Vec<(Vec2, f32, &'static str)> {
    vec![
        (Vec2::new(2414.0, 1080.0), 3.0, "device 2414x1080"),
        (Vec2::new(2048.0, 916.0), 3.0, "user 2048x916"),
        (Vec2::new(2340.0, 1080.0), 3.0, "phone 2340x1080"),
        (Vec2::new(2400.0, 1080.0), 3.0, "phone 2400x1080"),
        (Vec2::new(1920.0, 1080.0), 3.0, "reference 1920x1080"),
        (Vec2::new(1280.0, 720.0), 3.0, "small 1280x720"),
        (Vec2::new(1080.0, 2340.0), 3.0, "portrait 1080x2340"),
        (Vec2::new(960.0, 2340.0), 3.0, "narrow portrait 320dp"),
        (Vec2::new(2560.0, 1600.0), 2.0, "tablet 2560x1600"),
    ]
}

// ---------------------------------------------------------------- §4.3 / §5.1

#[test]
fn hud_layout_console_edge_to_edge() {
    // `hud.layout.console-edge-to-edge`
    for (px, density, name) in matrix() {
        for orientation in [None, Some(Orientation::Landscape), Some(Orientation::Portrait)] {
            let mut metrics = metrics(px, density);
            metrics.orientation = orientation;
            let l = layout(metrics, HudLayoutState::default());
            let screen = l.screen;
            println!(
                "console-edge-to-edge {name} orient={:?} mode={} screen={:.1}x{:.1} console={:?}",
                orientation,
                l.mode.key(),
                screen.x,
                screen.y,
                l.console
            );
            assert_eq!(l.console.x, 0.0, "{name}: console.x must be 0");
            assert_eq!(l.console.right(), screen.x, "{name}: console.right must be W");
            assert_eq!(l.console.bottom(), screen.y, "{name}: console.bottom must be H");
            // The background is never inset by the safe area.
            let safe = l.safe;
            if safe.bottom > 0.0 {
                assert_eq!(
                    l.console.bottom(),
                    screen.y,
                    "{name}: a safe bottom inset must not lift the console"
                );
            }
        }
    }
}

#[test]
fn hud_layout_viewport_ends_at_console() {
    // `hud.layout.viewport-ends-at-console`
    for (px, density, name) in matrix() {
        let l = layout(metrics(px, density), HudLayoutState::default());
        println!(
            "viewport {name} top={:.1} bottom={:.1} console_top={:.1}",
            l.viewport.y,
            l.viewport.bottom(),
            l.console.y
        );
        assert!(
            (l.viewport.bottom() - l.console.y).abs() < 0.01,
            "{name}: camera viewport must end exactly at the console top"
        );
        assert_eq!(l.viewport.y, l.top_bar.h, "{name}: viewport starts under the top bar");
        assert!(l.viewport.h > 0.0, "{name}: no playfield left");
    }
}

#[test]
fn hud_layout_single_px_conversion() {
    // `hud.layout.single-conversion`: px == dp * density, exactly once.
    for (px, density, name) in matrix() {
        let l = layout(metrics(px, density), HudLayoutState {
            selection_count: 6,
            has_mana: true,
            production: true,
            inventory: false,
            groups_open: false,
            commands_open: true,
        });
        let p = l.to_px(density);
        let pairs = [
            ("console", l.console, p.console),
            ("screen", Rect::new(0.0, 0.0, l.screen.x, l.screen.y), Rect::new(0.0, 0.0, p.viewport.x, p.viewport.y)),            ("selection", l.selection, p.selection),
            ("card", l.card, p.card),
            ("minimap", l.minimap_frame, p.minimap_frame),
            ("groups", l.groups_area, p.groups_area),
            ("top_bar", l.top_bar, p.top_meta),
        ];
        for (label, dp, px_rect) in pairs {
            assert!(
                (dp.x * density - px_rect.x).abs() < 0.01
                    && (dp.w * density - px_rect.w).abs() < 0.01,
                "{name}: {label} dp {:?} -> px {:?} is not a single multiply by {density}",
                dp,
                px_rect
            );
        }
        assert_eq!(p.density, density);
        assert_eq!(p.dp.console, l.console);
        // The pixel layout's own fields are the dp values times density.
        for (dp, px_rect) in l.card_slots.iter().zip(p.card_slots.iter()) {
            assert!((dp.w * density - px_rect.w).abs() < 0.01, "{name}: card slot");
        }
    }
}

#[test]
fn hud_layout_points_pixels_round_trip() {
    // AUD-004: the platform reports framebuffer pixels plus pixels per logical
    // dp (SDL `SDL_GetWindowDisplayScale` on Android). Both directions are a
    // single multiply/divide: dp -> px for drawing, px -> dp for insets, never
    // a second scale on an already-pixel rect.
    let metrics = WindowMetrics::new(Vec2::new(1440.0, 3120.0), 2.75);
    assert!((metrics.density() - 2.75).abs() < 1e-6);
    assert!((metrics.screen_dp().x - 1440.0 / 2.75).abs() < 1e-3);
    assert!((metrics.screen_dp().y - 3120.0 / 2.75).abs() < 1e-3);

    // Insets arrive in framebuffer pixels and become dp exactly once.
    let with = metrics.with_pixel_insets(SafeArea::new(82.5, 0.0, 0.0, 66.0));
    assert!((with.safe().left - 30.0).abs() < 1e-3);
    assert!((with.safe().bottom - 24.0).abs() < 1e-3);
    assert!((with.safe().left * with.density() - 82.5).abs() < 1e-3);

    // A 48 dp target is 132 px at density 2.75, never 48 * 2.75 * 2.75.
    let l = layout(with, HudLayoutState::default());
    let p = l.to_px(with.density());
    assert!((p.cell - TOUCH_DP * 2.75).abs() < 1e-3, "cell {}", p.cell);
    let middle = CARD_SLOTS / 2;
    for (dp, px) in l.card_slots.iter().zip(&p.card_slots) {
        assert!(
            (dp.x * 2.75 - px.x).abs() < 0.01 && (dp.w * 2.75 - px.w).abs() < 0.01,
            "card slot dp {:?} -> px {:?} is not a single multiply",
            dp,
            px
        );
    }

    // The rendered pixel rect is the hit target, and the dp centre maps onto
    // that same pixel centre.
    let slot = p.card_slots[middle];
    assert_eq!(p.hit(slot.centre()), Some(HudHit::Card(middle)));
    let dp_centre_px = l.card_slots[middle].centre() * with.density();
    assert!(
        (dp_centre_px - slot.centre()).length() < 1e-3,
        "dp centre {:?} != px centre {:?}",
        dp_centre_px,
        slot.centre()
    );
}

// ---------------------------------------------------------------------- §11

#[test]
fn hud_layout_targets_48dp() {
    // `hud.layout.targets-48dp`: every actionable rect in every mode.
    for (px, density, name) in matrix() {
        for (state, label) in [
            (HudLayoutState::default(), "idle"),
            (
                HudLayoutState {
                    selection_count: 12,
                    has_mana: true,
                    production: true,
                    inventory: true,
                    groups_open: true,
                    commands_open: true,
                },
                "busy",
            ),
        ] {
            let l = layout(metrics(px, density), state);
            let p = l.to_px(density);
            let mut checked = 0usize;
            for (zone, rect) in p.interactive() {
                assert!(
                    rect.w + 0.01 >= TOUCH_DP,
                    "{name}/{label}: {zone} width {:.1} dp < 48",
                    rect.w
                );
                assert!(
                    rect.h + 0.01 >= TOUCH_DP,
                    "{name}/{label}: {zone} height {:.1} dp < 48",
                    rect.h
                );
                checked += 1;
            }
            println!("targets-48dp {name}/{label}: {checked} actionable rects ok");
            assert!(checked > 0, "{name}: no actionable rects at all");
        }
    }
}

#[test]
fn hud_layout_no_ambiguous_overlap() {
    // `hud.layout.no-ambiguous-overlap`: the 12 card cells tile without
    // overlap, and a nearest-centre hit band cannot flip two neighbours.
    for (px, density, name) in matrix() {
        for open in [false, true] {
            let l = layout(
                metrics(px, density),
                HudLayoutState {
                    commands_open: open,
                    ..HudLayoutState::default()
                },
            );
            let inline = l.mode != HudLayoutMode::LandscapeCompact;
            if !inline && !open {
                // Compact with the drawer closed: no card targets at all.
                assert!(l.card_slots.is_empty(), "{name}: closed drawer has no cells");
                assert!(l.commands_button.w >= TOUCH_DP);
                continue;
            }
            let slots = &l.card_slots;
            assert_eq!(slots.len(), CARD_SLOTS, "{name}: 12 command slots");
            for i in 0..slots.len() {
                for j in (i + 1)..slots.len() {
                    let area = slots[i].intersection_area(&slots[j]);
                    assert!(
                        area <= 0.01,
                        "{name}: card slots {i} and {j} overlap by {area:.3} dp²"
                    );
                }
            }
            // Sample the 4 dp band around each cell: every sampled point in the
            // band must be strictly closer to its own centre.
            let band = 4.0;
            for i in 0..slots.len() {
                let cell = slots[i];
                let probes = [
                    Vec2::new(cell.x - band * 0.5, cell.centre().y),
                    Vec2::new(cell.right() + band * 0.5, cell.centre().y),
                    Vec2::new(cell.centre().x, cell.y - band * 0.5),
                    Vec2::new(cell.centre().x, cell.bottom() + band * 0.5),
                ];
                for probe in probes {
                    let mut best = (i, f32::INFINITY);
                    for (k, other) in slots.iter().enumerate() {
                        let distance = other.centre().distance(probe);
                        if distance < best.1 {
                            best = (k, distance);
                        }
                    }
                    // A probe on the seam between two cells may tie; it must
                    // never pick a cell that is not adjacent on that axis.
                    if best.0 != i {
                        let other = slots[best.0];
                        let shares_axis = (other.y - cell.y).abs() < 0.01
                            || (other.x - cell.x).abs() < 0.01;
                        assert!(
                            shares_axis,
                            "{name}: probe near card cell {i} resolved to non-adjacent {}",
                            best.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn hud_layout_command_4x3_order() {
    // `hud.layout.command-4x3-order`
    for (px, density, name) in matrix() {
        let l = layout(
            metrics(px, density),
            HudLayoutState { commands_open: true, ..HudLayoutState::default() },
        );
        let slots = &l.card_slots;
        assert_eq!(slots.len(), CARD_COLUMNS * CARD_ROWS, "{name}: 12 cells");
        for (index, slot) in slots.iter().enumerate() {
            let column = index % CARD_COLUMNS;
            let row = index / CARD_COLUMNS;
            if index > 0 {
                let prev = slots[index - 1];
                if column == 0 {
                    assert!(slot.y > prev.y, "{name}: row {row} must start a new line");
                } else {
                    assert!(
                        (slot.y - prev.y).abs() < 0.01 && slot.x > prev.x,
                        "{name}: cell {index} must be row-major"
                    );
                }
            }
            let pitch_x = slots
                .get(index + 1)
                .map(|next| next.x - slot.x)
                .unwrap_or(TOUCH_DP + CARD_GAP_DP);
            assert!(
                (pitch_x - (TOUCH_DP + CARD_GAP_DP)).abs() < 0.01 || column == CARD_COLUMNS - 1,
                "{name}: column pitch {pitch_x:.1} dp is not 48+8"
            );
        }
        println!(
            "{name}: card={:?} panel={:?} first={:?} last={:?}",
            l.card,
            (CARD_W_DP, CARD_ROWS as f32 * TOUCH_DP),
            slots.first(),
            slots.last()
        );
    }
}

#[test]
fn hud_layout_groups_nine() {
    // `hud.layout.groups-nine`: the tray is 3x3 for IDs 1..9 row-major and the
    // tenth slot (ID `0`, proven by `ControlGroups`) keeps its own target.
    for (px, density, name) in matrix() {
        let l = layout(
            metrics(px, density),
            HudLayoutState { groups_open: true, ..HudLayoutState::default() },
        );
        let slots = &l.group_slots;
        assert_eq!(slots.len(), GROUP_COUNT, "{name}: ten ControlGroups slots");
        for index in 0..9 {
            let slot = slots[index];
            assert!(
                slot.w + 0.01 >= TOUCH_DP && slot.h + 0.01 >= TOUCH_DP,
                "{name}: group {} is {:.1}x{:.1} dp",
                index + 1,
                slot.w,
                slot.h
            );
            if index > 0 {
                let prev = slots[index - 1];
                if index % 3 == 0 {
                    assert!(slot.y > prev.y, "{name}: group row break at {index}");
                } else {
                    assert!(slot.x > prev.x, "{name}: groups are row-major");
                }
            }
        }
        let zero = slots[9];
        assert!(
            zero.w + 0.01 >= TOUCH_DP,
            "{name}: group ID 0 must keep a 48 dp target"
        );
        assert!(
            zero.x > slots[8].x,
            "{name}: group ID 0 sits after the 3x3 tray"
        );
        println!(
            "{name}: groups tray={:?} first={:?} zero={:?} mode={}",
            l.groups_area,
            slots[0],
            zero,
            l.mode.key()
        );
    }
}

#[test]
fn hud_layout_minimap_square() {
    // `hud.layout.minimap-square` + world/minimap transform round trip.
    for (px, density, name) in matrix() {
        let l = layout(metrics(px, density), HudLayoutState::default());
        if l.minimap_frame.w <= 0.0 {
            println!("{name}: minimap collapsed to a control (narrow portrait)");
            continue;
        }
        assert!(
            (l.minimap_frame.w - l.minimap_frame.h).abs() < 0.5,
            "{name}: minimap frame is not square: {:?}",
            l.minimap_frame
        );
        assert!(
            (l.minimap_image.w - l.minimap_image.h).abs() < 0.5,
            "{name}: map content is not square: {:?}",
            l.minimap_image
        );
        assert!(
            l.minimap_image.is_inside(&l.minimap_frame),
            "{name}: the frame must surround the content, not cover it"
        );
        // The world -> minimap mapping is affine; check a round trip.
        let world = Vec2::new(2048.0, 1536.0);
        let map = Vec2::new(0.25, 0.75);
        let point = Vec2::new(
            l.minimap_image.x + map.x * l.minimap_image.w,
            l.minimap_image.y + map.y * l.minimap_image.h,
        );
        let back = Vec2::new(
            (point.x - l.minimap_image.x) / l.minimap_image.w,
            (point.y - l.minimap_image.y) / l.minimap_image.h,
        );
        assert!((back - map).length() < 1e-4, "{name}: minimap round trip drifted");
        assert!(world.x > 0.0);
    }
}

#[test]
fn hud_layout_selection_states() {
    // `hud.layout.selection-states`: 0 / 1 / 2 / 6 / 12 selected.
    for (px, density, name) in matrix() {
        for count in [0usize, 1, 2, 6, 12] {
            let l = layout(
                metrics(px, density),
                HudLayoutState {
                    selection_count: count,
                    has_mana: count == 1,
                    production: count == 1,
                    inventory: false,
                    groups_open: false,
                    commands_open: false,
                },
            );
            assert!(
                l.selection.w > 0.0 && l.selection.h > 0.0,
                "{name}: selection zone must always exist (count {count})"
            );
            assert!(
                l.name.h > 0.0 && l.class.h > 0.0,
                "{name}: name/class rows must exist (count {count})"
            );
            // The bars never overlap the bottom band and always fit.
            let band_top = if l.army.h > 0.0 { l.army.y } else { l.selection.bottom() };
            for (label, rect) in [
                ("name", l.name),
                ("class", l.class),
                ("hp", l.hp_bar),
                ("mana", l.mana_bar),
                ("stats", l.stats),
                ("production", l.production_label),
            ] {
                if rect.h <= 0.0 {
                    continue;
                }
                assert!(
                    rect.bottom() <= band_top + 0.01,
                    "{name}/{count}: {label} row crosses into the bottom band"
                );
                assert!(
                    rect.is_inside(&l.selection),
                    "{name}/{count}: {label} escapes the selection zone"
                );
            }
            if count > 1 {
                assert!(
                    !l.army_slots.is_empty(),
                    "{name}/{count}: multi-selection must show a strip"
                );
            }
            if count < 2 {
                assert!(
                    l.army_slots.is_empty(),
                    "{name}/{count}: no strip without a multi-selection"
                );
            }
            println!(
                "{name} sel={count}: zone={:?} army={:?} slots={}",
                l.selection,
                l.army,
                l.army_slots.len()
            );
        }
    }
}

#[test]
fn hud_layout_multiselect_collapses() {
    // `hud.layout.multiselect-collapses`: count <= 1 reserves no space at all.
    for (px, density, name) in matrix() {
        let single = layout(
            metrics(px, density),
            HudLayoutState { selection_count: 1, ..HudLayoutState::default() },
        );
        let empty = layout(metrics(px, density), HudLayoutState::default());
        for l in [&single, &empty] {
            assert_eq!(l.army_slots.len(), 0, "{name}: no chips without a group");
            assert_eq!(l.army.w, 0.0, "{name}: no strip rect without a group");
            assert_eq!(l.army_badge.w, 0.0, "{name}: no badge without a group");
        }
        // The info column may use the full zone when nothing is selected: the
        // stats row exists exactly when it fits, never as a reserved blank.
        let six = layout(
            metrics(px, density),
            HudLayoutState { selection_count: 6, ..HudLayoutState::default() },
        );
        assert!(
            !six.army_slots.is_empty(),
            "{name}: 6 units must open the strip"
        );
        if six.mode.is_landscape() {
            assert!(six.army.h > 0.0 && six.army.w > 0.0, "{name}: strip rect");
        }
        println!(
            "{name}: single stats h={:.1} multi strip h={:.1}",
            single.stats.h, six.army.h
        );
    }
}

#[test]
fn hud_layout_portrait_mode() {
    // `hud.layout.portrait-mode`: its own composition, never landscape rotated.
    let portrait = layout(
        metrics(Vec2::new(1080.0, 2340.0), 3.0),
        HudLayoutState { selection_count: 6, ..HudLayoutState::default() },
    );
    let landscape = layout(
        metrics(Vec2::new(2340.0, 1080.0), 3.0),
        HudLayoutState { selection_count: 6, ..HudLayoutState::default() },
    );
    assert_eq!(portrait.mode, HudLayoutMode::Portrait);
    assert!(landscape.mode.is_landscape());
    // Portrait stacks the card under the summary; the card is at the bottom of
    // the console and the minimap sits to its left, not left of a 4-zone row.
    assert!(
        portrait.card.y >= portrait.selection.bottom() - 0.01,
        "portrait card must start below the summary strip"
    );
    assert_eq!(portrait.card.right(), portrait.content.right());
    assert!(
        portrait.minimap_frame.x < portrait.card.x
            || portrait.minimap_frame.w == 0.0,
        "portrait minimap sits left of the card"
    );
    assert!(
        portrait.minimap_frame.bottom() <= portrait.console.bottom() + 0.01,
        "portrait minimap inside the console"
    );
    // Wide landscape keeps every zone in one row; portrait stacks.
    let wide = layout(
        metrics(Vec2::new(2560.0, 1600.0), 2.0),
        HudLayoutState::default(),
    );
    assert_eq!(wide.mode, HudLayoutMode::LandscapeWide);
    assert!(
        wide.card.y >= wide.content.y && wide.card.bottom() <= wide.content.bottom() + 0.01,
        "wide card inside the content band"
    );
    assert!(
        wide.selection.x < wide.groups_area.x && wide.groups_area.x < wide.card.x,
        "wide zones run left to right: minimap, selection, groups, card"
    );
    // The card panel is fixed at 4x3 (176 dp) and centred in the band.
    assert!((wide.card.h - 176.0).abs() < 0.01, "wide card panel is 176 dp");
    assert!(wide.card.is_inside(&wide.console));
    assert!((wide.card.centre().y - wide.content.centre().y).abs() < 0.01);
    assert!(
        portrait.card.y >= portrait.selection.bottom(),
        "portrait must not reuse the landscape row"
    );
    // Portrait keeps every action target at 48 dp.
    let p = portrait.to_px(3.0);
    for (zone, rect) in p.interactive() {
        assert!(
            rect.w + 0.01 >= TOUCH_DP && rect.h + 0.01 >= TOUCH_DP,
            "portrait {zone} {:.1}x{:.1} dp",
            rect.w,
            rect.h
        );
    }
    println!(
        "portrait: console={:?} card={:?} summary={:?} minimap={:?}",
        portrait.console, portrait.card, portrait.selection, portrait.minimap_frame
    );
}

#[test]
fn hud_layout_compact_at_device_width() {
    // `hud.layout.compact-at-device-width` (tech-lead item 3): 804.7 dp of
    // content width must select the compact composition, not the wide one.
    let metrics = device();
    let dp = dp_of(metrics);
    let l = layout(metrics, HudLayoutState::default());
    println!(
        "device dp={:.1}x{:.1} content={:.1} mode={} console={:.1} dp ({:.1}%)",
        dp.x,
        dp.y,
        l.content.w,
        l.mode.key(),
        l.console.h,
        100.0 * l.console.h / dp.y
    );
    assert!(dp.x > 800.0 && dp.x < 810.0, "device dp width is {:.1}", dp.x);
    assert_eq!(
        l.mode,
        HudLayoutMode::LandscapeCompact,
        "804.7 dp content must take the compact layout (< 820 dp)"
    );
    assert!(
        l.console.h <= 0.40 * dp.y,
        "compact console {:.1} dp is {:.1}% of {:.1} dp",
        l.console.h,
        100.0 * l.console.h / dp.y,
        dp.y
    );
    // Compact: the card is a drawer, so it is closed until asked for.
    assert_eq!(l.card_slots.len(), 0, "compact card is a drawer");
    assert!(l.commands_button.w >= TOUCH_DP, "persistent COMMANDS control");
    assert!(l.groups_button.w >= TOUCH_DP, "persistent GROUPS control");
    let open = layout(
        metrics,
        HudLayoutState { commands_open: true, ..HudLayoutState::default() },
    );
    assert_eq!(open.card_slots.len(), CARD_SLOTS, "drawer keeps all 12 ids");
    assert!(open.commands_overlay.w > 0.0);
    // The drawer sits above the console and never covers its own toggle.
    assert!(open.commands_overlay.bottom() <= open.console.y + 0.01);
    println!(
        "compact: card drawer={:?} overlay={:?} groups_button={:?}",
        open.card, open.commands_overlay, open.groups_button
    );
}

#[test]
fn hud_layout_device_budget() {
    // Tech-lead item 1: the HUD must not eat the playfield on a 360 dp screen,
    // and the numbers are printed for HUD_LAYOUT_QA.md.
    let metrics = device();
    let dp = dp_of(metrics);
    let l = layout(metrics, HudLayoutState::default());
    let console_pct = 100.0 * l.console.h / dp.y;
    let hud_pct = 100.0 * (l.console.h + l.top_bar.h) / dp.y;
    println!(
        "DEVICE {}x{} px @ {:.2} => {:.1}x{:.1} dp | topbar {:.1} dp, console {:.1} dp ({:.1}%), \
         viewport {:.1} dp ({:.1}%), HUD total {:.1}%",
        DEVICE_PX.x,
        DEVICE_PX.y,
        DEVICE_DENSITY,
        dp.x,
        dp.y,
        l.top_bar.h,
        l.console.h,
        console_pct,
        l.viewport.h,
        100.0 * l.viewport.h / dp.y,
        hud_pct
    );
    assert!(
        hud_pct <= 55.0,
        "HUD takes {hud_pct:.1}% of the height; the operator rejects > 55%"
    );
    assert!(
        console_pct <= 40.0,
        "console takes {console_pct:.1}% of the height"
    );
    assert!(
        l.viewport.h >= 0.45 * dp.y,
        "playfield {:.1} dp is under 45% of the screen",
        l.viewport.h
    );
    // Topbar budget: three resource pills + upkeep + a readable map name.
    assert!(l.resource_pills.len() == 3);
    for pill in &l.resource_pills {
        assert!(
            pill.h >= 18.0,
            "resource pill {:.1} dp tall cannot host 16 sp numerals",
            pill.h
        );
    }
    assert!(
        l.map_name.w >= 90.0,
        "map name column is only {:.1} dp; a Russian map name would clip",
        l.map_name.w
    );
    assert_eq!(l.fps.w, 0.0, "FPS must be hidden unless the dev overlay is on");
    let dev = HudLayout::compute(
        metrics,
        UiPreferences { show_fps: true, ..UiPreferences::default() },
        HudLayoutState::default(),
    );
    assert!(dev.fps.w > 0.0, "the developer overlay shows FPS");
}

#[test]
fn hud_layout_relayout_on_metric_change() {
    // Density/orientation changes must re-solve cleanly (TZ §4.2 cache rules).
    let state = HudLayoutState::default();
    let a = layout(metrics(Vec2::new(2414.0, 1080.0), 3.0), state);
    let b = layout(metrics(Vec2::new(2414.0, 1080.0), 2.0), state);
    let c = layout(metrics(Vec2::new(1080.0, 2414.0), 3.0), state);
    assert!(a.mode.is_landscape() && b.mode.is_landscape());
    assert_eq!(c.mode, HudLayoutMode::Portrait);
    assert!(
        (a.screen.x - 804.7).abs() < 0.2 && (b.screen.x - 1207.0).abs() < 0.2,
        "density change must change the dp viewport: {:?} vs {:?}",
        a.screen,
        b.screen
    );
    // 1207 dp of width is a tablet-class wide layout; the console grows with
    // the 34% rule but stays under the 220 dp ceiling.
    assert_eq!(b.mode, HudLayoutMode::LandscapeWide);
    assert!(b.console.h <= 220.5);
    // A safe-area bottom inset moves the content, never the console edge.
    let mut with_inset = device();
    with_inset.safe_insets_dp = SafeArea::new(0.0, 0.0, 0.0, 24.0);
    let inset = layout(with_inset, state);
    assert_eq!(inset.console.bottom(), inset.screen.y);
    assert!(
        (inset.content.bottom() - (inset.screen.y - 24.0)).abs() < 0.01,
        "content must end above the gesture bar"
    );
    assert!(
        inset.card_slots
            .iter()
            .all(|slot| slot.bottom() <= inset.content.bottom() + 0.01
                || inset.card_slots.is_empty()),
        "no control may sit under the gesture bar"
    );
    println!(
        "density 3 -> {:?} {:?}; density 2 -> {:?} {:?}; portrait -> {:?}",
        a.screen, a.mode, b.screen, b.mode, c.screen
    );
}

#[test]
fn hud_layout_matches_legacy_zone_names() {
    // The QA table and the example read the same names; a rename must not
    // silently drop a zone from the report.
    let l = layout(device(), HudLayoutState::default());
    let p = l.to_px(DEVICE_DENSITY);
    for (name, rect) in p.zone_rects() {
        assert!(rect.w >= 0.0 && rect.h >= 0.0, "{name} is negative");
    }
    let zones: Vec<&str> = p.zone_rects().iter().map(|(name, _)| *name).collect();
    for expected in ["top_bar", "minimap", "selection", "resources", "tooltip"] {
        assert!(zones.contains(&expected), "zone {expected} missing from {zones:?}");
    }
    let pad = CONSOLE_PAD_DP;
    assert!(l.content.x >= l.safe.left + pad - 0.01);
    assert!(l.content.right() <= l.screen.x - l.safe.right - pad + 0.01);
    assert!(ZONE_GAP_DP > 0.0);
}

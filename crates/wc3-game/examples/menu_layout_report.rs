//! Deterministic main-menu layout dump for the port-parity loop (`menu-layout`).
//!
//! Runs the **real** `MobileHud::layout_menu` path for the device viewports and
//! prints one line per viewport plus one line per button rect. The output is the
//! reference recording for the rust-anything parity gate: it must be byte-stable
//! run to run (fixed precision, fixed order, no timings), so a layout change that
//! moves a button off-screen fails `scripts/port-parity.sh diff menu-layout`.
//!
//! Regression it pins (device 2026-11-21): `layout_menu` counted `use_buttons` /
//! `use_nav` booleans as `usize` slots, so the second `row_h` solve saw one slot,
//! reset the height to the 150 px cap and the four/six glue entries overflowed a
//! 2414x1080 viewport (QUIT at y=1021..1171).
//!
//! Usage:
//!   cargo run -q -p wc3-game --example menu_layout_report
//!   cargo run -q -p wc3-game --example menu_layout_report -- --viewport 2414x1080:210,120,90,96

use wc3_game::hud::mobile_hud::{MenuScreen, MenuView, MobileHud};
use wc3_game::hud::Rect;

/// (label, framebuffer px, safe insets left/top/right/bottom px)
const MATRIX: &[(&str, (f32, f32), (f32, f32, f32, f32))] = &[
    ("device-landscape", (2414.0, 1080.0), (210.0, 120.0, 90.0, 96.0)),
    ("device-portrait", (1080.0, 2414.0), (24.0, 96.0, 24.0, 64.0)),
    ("desktop-720p", (1280.0, 720.0), (0.0, 0.0, 0.0, 0.0)),
    ("wide-2340", (2340.0, 1080.0), (120.0, 80.0, 48.0, 48.0)),
    ("compact-320x240", (320.0, 240.0), (0.0, 0.0, 0.0, 0.0)),
];

fn parse_viewport(arg: &str) -> Option<((f32, f32), (f32, f32, f32, f32))> {
    let (size, insets) = arg.split_once(':')?;
    let (w, h) = size.split_once('x')?;
    let mut parts = insets.split(',');
    let l = parts.next()?.trim().parse().ok()?;
    let t = parts.next()?.trim().parse().ok()?;
    let r = parts.next()?.trim().parse().ok()?;
    let b = parts.next()?.trim().parse().ok()?;
    Some((
        (w.trim().parse().ok()?, h.trim().parse().ok()?),
        (l, t, r, b),
    ))
}

fn inside(rect: &Rect, vp: (f32, f32)) -> bool {
    rect.x >= -0.5
        && rect.y >= -0.5
        && rect.right() <= vp.0 + 0.5
        && rect.bottom() <= vp.1 + 0.5
}

fn main() {
    let mut selected: Option<((f32, f32), (f32, f32, f32, f32))> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--viewport" => {
                if let Some(value) = args.next() {
                    selected = parse_viewport(&value);
                    if selected.is_none() {
                        eprintln!("bad --viewport {value:?} (want WxH:L,T,R,B)");
                        std::process::exit(2);
                    }
                }
            }
            other => eprintln!("ignoring unknown argument {other:?}"),
        }
    }

    let hud = MobileHud::new();
    let rows: Vec<(&str, (f32, f32), (f32, f32, f32, f32))> = match selected {
        Some((size, insets)) => vec![("custom", size, insets)],
        None => MATRIX.to_vec(),
    };

    let mut all_ok = true;
    for (label, viewport, insets) in rows {
        let vp = glam::Vec2::new(viewport.0, viewport.1);
        let mut view = MenuView::for_screen(MenuScreen::Main, vp, "");
        view.safe = Some(wc3_game::hud::mobile_hud::SafeArea::new(
            insets.0, insets.1, insets.2, insets.3,
        ));
        let layout = hud.layout_menu(&view);

        let panel_ok = inside(&layout.panel, viewport);
        let title_ok = inside(&layout.title_band, viewport);
        let buttons_ok = layout.buttons.iter().all(|b| inside(b, viewport));
        let fit = panel_ok && title_ok && buttons_ok;
        all_ok &= fit;

        println!(
            "MENU_LAYOUT name={label} vp={:.0}x{:.0} safe={:.0},{:.0},{:.0},{:.0} panel={:.1},{:.1},{:.1},{:.1} title_band={:.1},{:.1},{:.1},{:.1} buttons={}",
            viewport.0,
            viewport.1,
            insets.0,
            insets.1,
            insets.2,
            insets.3,
            layout.panel.x,
            layout.panel.y,
            layout.panel.w,
            layout.panel.h,
            layout.title_band.x,
            layout.title_band.y,
            layout.title_band.w,
            layout.title_band.h,
            layout.buttons.len()
        );
        for (index, rect) in layout.buttons.iter().enumerate() {
            println!(
                "MENU_BUTTON name={label} index={index} rect={:.1},{:.1},{:.1},{:.1} inside={}",
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                if inside(rect, viewport) { "yes" } else { "no" }
            );
        }
        println!(
            "MENU_FIT name={label} panel={} title={} buttons={} verdict={}",
            if panel_ok { "ok" } else { "overflow" },
            if title_ok { "ok" } else { "overflow" },
            if buttons_ok { "ok" } else { "overflow" },
            if fit { "PASS" } else { "FAIL" }
        );
    }
    if !all_ok {
        std::process::exit(1);
    }
}

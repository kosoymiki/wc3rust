//! Screen classes and breakpoints (hud.md §7.1 / §7.3).
//!
//! The class is descriptive (what kind of panel is this?), the breakpoint is
//! operative (which composition do we solve?). Both are derived from the dp
//! viewport that is actually available, never from a device name, so a resized
//! desktop window and a phone with a split-screen bar land in the same code path.

use crate::ui::core::geometry::Size;

/// Supported screen classes (hud.md §7.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScreenClass {
    /// 16:9 desktop, 1024 dp and wider.
    Desktop169,
    /// 16:10 desktop.
    Desktop1610,
    /// 21:9 and wider.
    Ultrawide,
    /// 4:3 or squarer, still wide.
    Classic43,
    /// A small window: under 900 dp wide.
    SmallWindow,
    /// Tablet: 600 dp and up on the short side, landscape.
    Tablet,
    /// Phone in landscape.
    PhoneLandscape,
    /// Phone in portrait.
    PhonePortrait,
}

impl ScreenClass {
    /// Classify a dp viewport.
    ///
    /// Aspect ratios are the retail ones (16:9, 16:10, 21:9, 4:3); the phone
    /// landscape band is 16:9..20:9 with a short side at or under 420 dp, which
    /// separates a 2400x1080 phone from a 2560x1080 ultrawide.
    pub fn classify(dp: Size) -> Self {
        let aspect = if dp.h > 0.0 { dp.w / dp.h } else { 1.0 };
        let short = dp.w.min(dp.h);
        if aspect >= 2.30 {
            return ScreenClass::Ultrawide;
        }
        if aspect < 1.02 {
            return if short >= 600.0 {
                ScreenClass::Tablet
            } else {
                ScreenClass::PhonePortrait
            };
        }
        if short <= 420.0 || (aspect >= 1.90 && dp.w < 1600.0) {
            return ScreenClass::PhoneLandscape;
        }
        if (aspect - 4.0 / 3.0).abs() < 0.10 {
            return ScreenClass::Classic43;
        }
        if (aspect - 16.0 / 10.0).abs() < 0.06 {
            return ScreenClass::Desktop1610;
        }
        if dp.w < 900.0 {
            return ScreenClass::SmallWindow;
        }
        ScreenClass::Desktop169
    }

    pub fn key(self) -> &'static str {
        match self {
            ScreenClass::Desktop169 => "desktop-16:9",
            ScreenClass::Desktop1610 => "desktop-16:10",
            ScreenClass::Ultrawide => "ultrawide-21:9",
            ScreenClass::Classic43 => "classic-4:3",
            ScreenClass::SmallWindow => "small-window",
            ScreenClass::Tablet => "tablet",
            ScreenClass::PhoneLandscape => "phone-landscape",
            ScreenClass::PhonePortrait => "phone-portrait",
        }
    }

    /// Touch-first classes keep the 48 dp floor for every action.
    pub fn is_touch_first(self) -> bool {
        matches!(
            self,
            ScreenClass::PhoneLandscape | ScreenClass::PhonePortrait | ScreenClass::Tablet
        )
    }
}

/// The operative composition size (hud.md §7.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Breakpoint {
    /// Full composition: every zone inline.
    Wide,
    /// Standard: same zones, tighter band.
    Standard,
    /// Compact: the command card and control groups move into drawers.
    Compact,
    /// Minimal: only the playfield, the resource strip and the primary controls.
    Minimal,
}

impl Breakpoint {
    pub fn key(self) -> &'static str {
        match self {
            Breakpoint::Wide => "wide",
            Breakpoint::Standard => "standard",
            Breakpoint::Compact => "compact",
            Breakpoint::Minimal => "minimal",
        }
    }

    /// Fallback order: each step gives the playfield more room.
    pub fn next(self) -> Option<Breakpoint> {
        match self {
            Breakpoint::Wide => Some(Breakpoint::Standard),
            Breakpoint::Standard => Some(Breakpoint::Compact),
            Breakpoint::Compact => Some(Breakpoint::Minimal),
            Breakpoint::Minimal => None,
        }
    }
}

/// Width thresholds in dp, measured on the safe content band.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Thresholds {
    /// Content at or above this width keeps the full inline console.
    pub wide: f32,
    /// Below this width the card and the group tray become drawers.
    pub standard: f32,
    /// Below this width only the primary controls survive.
    pub compact: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            wide: 820.0,
            standard: 620.0,
            compact: 420.0,
        }
    }
}

impl Thresholds {
    /// The widest composition this content width can host.
    pub fn classify(&self, content_w: f32) -> Breakpoint {
        if content_w >= self.wide {
            Breakpoint::Wide
        } else if content_w >= self.standard {
            Breakpoint::Standard
        } else if content_w >= self.compact {
            Breakpoint::Compact
        } else {
            Breakpoint::Minimal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_cover_the_supported_matrix() {
        let cases = [
            (Size::new(1280.0, 720.0), ScreenClass::Desktop169),
            (Size::new(1920.0, 1080.0), ScreenClass::Desktop169),
            (Size::new(1440.0, 900.0), ScreenClass::Desktop1610),
            (Size::new(2560.0, 1080.0), ScreenClass::Ultrawide),
            (Size::new(1024.0, 768.0), ScreenClass::Classic43),
            (Size::new(800.0, 600.0), ScreenClass::Classic43),
            (Size::new(800.0, 360.0), ScreenClass::PhoneLandscape),
            (Size::new(800.0, 360.0), ScreenClass::PhoneLandscape),
            (Size::new(360.0, 800.0), ScreenClass::PhonePortrait),
            (Size::new(1280.0, 800.0), ScreenClass::Desktop1610),
        ];
        for (dp, expected) in cases {
            assert_eq!(ScreenClass::classify(dp), expected, "{dp:?}");
        }
    }

    #[test]
    fn phone_classes_are_touch_first() {
        assert!(ScreenClass::classify(Size::new(800.0, 360.0)).is_touch_first());
        assert!(!ScreenClass::classify(Size::new(1920.0, 1080.0)).is_touch_first());
    }

    #[test]
    fn breakpoints_step_down_to_minimal() {
        let t = Thresholds::default();
        assert_eq!(t.classify(900.0), Breakpoint::Wide);
        assert_eq!(t.classify(804.7), Breakpoint::Standard);
        assert_eq!(t.classify(500.0), Breakpoint::Compact);
        assert_eq!(t.classify(300.0), Breakpoint::Minimal);
        assert_eq!(Breakpoint::Wide.next(), Some(Breakpoint::Standard));
        assert_eq!(Breakpoint::Minimal.next(), None);
    }
}

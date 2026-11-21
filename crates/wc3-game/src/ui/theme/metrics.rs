//! Spatial metrics in dp: the single source of the HUD's gaps and floors.
//!
//! These are the numbers the layout used to keep as `pub const`s inside
//! `mobile_hud.rs`. They live here as data so the menu, the solver and the
//! audit all measure the same interface, and so a screen class can scale them
//! without a second layout transform.

/// Minimum actionable side (hud.md §7.5).
pub const TOUCH_DP: f32 = 48.0;
/// Gap between command slots.
pub const CARD_GAP_DP: f32 = 8.0;
/// Padding inside the console plate.
pub const CONSOLE_PAD_DP: f32 = 8.0;
/// Gap between console zones.
pub const ZONE_GAP_DP: f32 = 8.0;
/// Minimap target side (1:1 frame).
pub const MINIMAP_DP: f32 = 144.0;
/// Minimap floor before it collapses to a control.
pub const MINIMAP_MIN_DP: f32 = 128.0;
/// Compact-band minimap.
pub const MINIMAP_COMPACT_DP: f32 = 112.0;
/// 3 x 3 control-group tray.
pub const GROUPS_TRAY_DP: f32 = 3.0 * TOUCH_DP + 2.0 * CARD_GAP_DP;
/// 4 x 3 command card panel width.
pub const CARD_W_DP: f32 = 4.0 * TOUCH_DP + 3.0 * CARD_GAP_DP + 2.0 * CARD_GAP_DP;
/// 4 x 3 command card panel height.
pub const CARD_H_DP: f32 = 3.0 * TOUCH_DP + 2.0 * CARD_GAP_DP + 2.0 * CARD_GAP_DP;
/// Content width below which the console compacts.
pub const WIDE_MIN_CONTENT_DP: f32 = 820.0;
/// Resource strip height.
pub const TOP_BAR_DP: f32 = 30.0;
/// Resource strip height in portrait.
pub const TOP_BAR_PORTRAIT_DP: f32 = 44.0;
/// Console band floor.
pub const CONSOLE_MIN_DP: f32 = 176.0;
/// Console band ceiling.
pub const CONSOLE_MAX_DP: f32 = 220.0;
/// Compact console band floor.
pub const CONSOLE_COMPACT_MIN_DP: f32 = 96.0;
/// Compact console band ceiling.
pub const CONSOLE_COMPACT_MAX_DP: f32 = 128.0;
/// Portrait selection summary strip.
pub const PORTRAIT_SUMMARY_DP: f32 = 48.0;
/// Ten control groups (1..9 plus `0`).
pub const GROUP_COUNT: usize = 10;
/// Column count of the command card.
pub const CARD_COLUMNS: usize = 4;
/// Row count of the command card.
pub const CARD_ROWS: usize = 3;
/// Every slot of the command card.
pub const CARD_SLOTS: usize = CARD_COLUMNS * CARD_ROWS;
/// Playfield floor: the HUD must leave this share of the height.
pub const PLAYFIELD_MIN_FRACTION: f32 = 0.45;
/// The HUD may not occupy more than this share of the height.
pub const HUD_MAX_FRACTION: f32 = 0.55;

/// The metric set the solver receives.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Metrics {
    pub touch: f32,
    pub gap: f32,
    pub pad: f32,
    pub minimap: f32,
    pub minimap_min: f32,
    pub minimap_compact: f32,
    pub groups_tray: f32,
    pub card_w: f32,
    pub card_h: f32,
    pub top_bar: f32,
    pub top_bar_portrait: f32,
    pub console_min: f32,
    pub console_max: f32,
    pub console_compact_min: f32,
    pub console_compact_max: f32,
    pub portrait_summary: f32,
    pub wide_min_content: f32,
    pub card_columns: usize,
    pub card_rows: usize,
    pub group_count: usize,
    pub playfield_min_fraction: f32,
    pub hud_max_fraction: f32,
}

impl Metrics {
    pub const fn direction() -> Self {
        Metrics {
            touch: TOUCH_DP,
            gap: ZONE_GAP_DP,
            pad: CONSOLE_PAD_DP,
            minimap: MINIMAP_DP,
            minimap_min: MINIMAP_MIN_DP,
            minimap_compact: MINIMAP_COMPACT_DP,
            groups_tray: GROUPS_TRAY_DP,
            card_w: CARD_W_DP,
            card_h: CARD_H_DP,
            top_bar: TOP_BAR_DP,
            top_bar_portrait: TOP_BAR_PORTRAIT_DP,
            console_min: CONSOLE_MIN_DP,
            console_max: CONSOLE_MAX_DP,
            console_compact_min: CONSOLE_COMPACT_MIN_DP,
            console_compact_max: CONSOLE_COMPACT_MAX_DP,
            portrait_summary: PORTRAIT_SUMMARY_DP,
            wide_min_content: WIDE_MIN_CONTENT_DP,
            card_columns: CARD_COLUMNS,
            card_rows: CARD_ROWS,
            group_count: GROUP_COUNT,
            playfield_min_fraction: PLAYFIELD_MIN_FRACTION,
            hud_max_fraction: HUD_MAX_FRACTION,
        }
    }

    /// Command card slot count.
    pub fn card_slots(&self) -> usize {
        self.card_columns * self.card_rows
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics::direction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_panel_is_four_by_three_of_48_dp() {
        assert_eq!(CARD_W_DP, 232.0);
        assert_eq!(CARD_H_DP, 176.0);
        assert_eq!(CARD_SLOTS, 12);
        let inner_w = CARD_W_DP - 2.0 * CARD_GAP_DP;
        let cell = (inner_w - 3.0 * CARD_GAP_DP) / 4.0;
        assert!((cell - TOUCH_DP).abs() < 0.01, "cell {cell}");
    }

    #[test]
    fn group_tray_holds_nine_plus_the_zero_slot() {
        assert_eq!(GROUPS_TRAY_DP, 160.0);
        assert_eq!(GROUP_COUNT, 10);
    }

    #[test]
    fn playfield_floor_and_hud_ceiling_do_not_contradict() {
        assert!(PLAYFIELD_MIN_FRACTION + HUD_MAX_FRACTION >= 1.0);
        assert!(Metrics::direction().playfield_min_fraction >= 0.45);
    }
}

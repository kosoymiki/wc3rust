//! Text metrics for the pure layout pass (hud.md §5.3, §9.5).
//!
//! The solver has no font file. It estimates the advance of a label with the
//! same conservative constant the geometry auditor uses (`0.55 em` for a
//! condensed game face), which is enough to answer the only question layout
//! cares about: *does this string fit the rect it was given?* The renderer
//! still shapes the real face; when the estimate says "does not fit", the
//! composition makes the box bigger, wraps the line or drops the row — it never
//! lets the text be clipped.

/// Conservative average advance for a condensed UI face, in em.
///
/// Matches `CHAR_WIDTH_RATIO` in `layout_audit.py`, so a layout that passes the
/// in-engine check also passes the auditor.
pub const CONDENSED_ADVANCE_EM: f32 = 0.55;

/// Cyrillic and uppercase text is wider than the conservative average; the
/// estimate adds this fraction when the string contains non-ASCII letters.
pub const WIDE_SCRIPT_BOOST: f32 = 1.12;

/// Estimated advance width of `text` at `size_dp`.
pub fn advance(text: &str, size_dp: f32) -> f32 {
    if text.is_empty() || size_dp <= 0.0 {
        return 0.0;
    }
    let chars = text.chars().count() as f32;
    let boost = if text.is_ascii() { 1.0 } else { WIDE_SCRIPT_BOOST };
    chars * size_dp * CONDENSED_ADVANCE_EM * boost
}

/// Result of fitting one label into a box.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextFit {
    /// Estimated width of the full string.
    pub needed_w: f32,
    /// Width actually available.
    pub avail_w: f32,
    /// Number of lines the string needs at `avail_w`.
    pub lines: u16,
    /// Height the lines need.
    pub needed_h: f32,
    /// Available height (0 when the caller only checked width).
    pub avail_h: f32,
    /// The whole string fits on one line.
    pub fits_one_line: bool,
    /// The string fits in the box, honouring `wrap`.
    pub fits: bool,
    /// Characters that fit on one line, ellipsis included.
    pub keep_chars: u16,
}

impl TextFit {
    /// How much of the line is used (`needed / avail`), for diagnostics.
    pub fn ratio(&self) -> f32 {
        if self.avail_w <= 0.0 {
            f32::INFINITY
        } else {
            self.needed_w / self.avail_w
        }
    }
}

/// Fit `text` into a box of `avail_w` × `avail_h` at `size_dp`.
///
/// `wrap` allows multiple lines; without it the string must fit on one line.
pub fn fit(text: &str, size_dp: f32, avail_w: f32, avail_h: f32, wrap: bool) -> TextFit {
    let needed_w = advance(text, size_dp);
    let line_h = line_height(size_dp);
    let fits_one_line = needed_w <= avail_w + 0.01;
    let lines = if fits_one_line || avail_w <= 0.0 {
        1
    } else if wrap {
        let per_line = ((avail_w / (size_dp * CONDENSED_ADVANCE_EM)).floor() as u16).max(1);
        let chars = text.chars().count() as u16;
        chars.div_ceil(per_line).max(1)
    } else {
        1
    };
    let needed_h = line_h * lines as f32;
    let fits = fits_one_line || (wrap && needed_h <= avail_h + 0.01);
    let char_w = (size_dp * CONDENSED_ADVANCE_EM).max(0.01);
    let keep_chars = ((avail_w / char_w).floor() as u16).max(0);
    TextFit {
        needed_w,
        avail_w,
        lines,
        needed_h,
        avail_h,
        fits_one_line,
        fits,
        keep_chars,
    }
}

/// Line height for a type size (1.25 is the console's authored ratio).
pub fn line_height(size_dp: f32) -> f32 {
    (size_dp * 1.25).max(size_dp + 1.0)
}

/// Shorten `text` with a single `…` so it fits `avail_w` at `size_dp`.
///
/// This is the *last* resort: the composition first tries to give the label
/// more room, wrap it, or drop the optional row. Ellipsis exists so a dynamic
/// value (a player name, a map title) can never paint outside its box.
pub fn ellipsize(text: &str, size_dp: f32, avail_w: f32) -> String {
    if advance(text, size_dp) <= avail_w + 0.01 {
        return text.to_string();
    }
    let ell = '…';
    let char_w = (size_dp * CONDENSED_ADVANCE_EM).max(0.01);
    // The ellipsis is a wide glyph and a non-ASCII string pays the script
    // boost, so the budget is shrunk until the result really fits.
    let mut keep = ((avail_w / char_w).floor() as usize).max(1);
    loop {
        let mut candidate: String = text.chars().take(keep).collect();
        candidate.push(ell);
        if advance(&candidate, size_dp) <= avail_w + 0.01 || keep == 0 {
            return candidate;
        }
        keep -= 1;
    }
}

/// Digits reserved for a numeric value, so a changing count never shifts the
/// layout (hud.md §3.2 / §5.3 "стабильное выравнивание").
pub fn numeric_slot(width_digits: u8, size_dp: f32) -> f32 {
    // A digit is narrower than the condensed average; 0.62 em is the measured
    // worst case for the retail numeral face.
    width_digits as f32 * size_dp * 0.62
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_grows_with_length_and_size() {
        assert!(advance("STOP", 14.0) > advance("STO", 14.0));
        assert!(advance("STOP", 20.0) > advance("STOP", 14.0));
        assert_eq!(advance("", 14.0), 0.0);
        // Cyrillic is estimated wider than ASCII of the same length.
        assert!(advance("Орда", 14.0) > advance("Orda", 14.0));
    }

    #[test]
    fn fit_reports_wrap_and_overflow() {
        let wide = fit("EXODUS OF THE HORDE", 14.0, 400.0, 20.0, false);
        assert!(wide.fits_one_line && wide.fits);
        let narrow = fit("EXODUS OF THE HORDE", 14.0, 60.0, 20.0, false);
        assert!(!narrow.fits, "one line cannot hold it");
        let wrapped = fit("EXODUS OF THE HORDE", 14.0, 60.0, 60.0, true);
        assert!(wrapped.lines > 1);
        assert!(wrapped.fits, "two lines at 60 dp are enough");
    }

    #[test]
    fn ellipsize_keeps_the_string_inside_the_box() {
        let text = "THE PRODIGAL";
        let out = ellipsize(text, 14.0, 40.0);
        assert!(out.ends_with('…'));
        assert!(advance(&out, 14.0) <= 40.0 + 0.01, "{out} is still too wide");
        assert_eq!(ellipsize("OK", 14.0, 40.0), "OK");
    }

    #[test]
    fn numeric_slot_is_stable() {
        let slot = numeric_slot(6, 14.0);
        assert!(slot > 0.0);
        assert!(numeric_slot(7, 14.0) > slot);
    }
}

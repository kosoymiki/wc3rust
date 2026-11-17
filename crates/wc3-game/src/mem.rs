//! Process memory readings for the asset/render layer.
//!
//! The app's budget is a number the operator measures with `dumpsys meminfo`,
//! but a probe has to attribute the bytes to a layer before anyone can fix
//! them. These helpers read the kernel counters directly so the asset loader
//! can stamp an RSS checkpoint into its own diagnostics (`mem_stage=…`) and
//! `examples/mem_probe` can print the same stages the device logs.

/// Resident set size in KiB (`/proc/self/statm` field 2, in pages).
pub fn rss_kb() -> u64 {
    let Ok(text) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let pages: u64 = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    pages * (page_kb())
}

/// `VmHWM` (peak RSS) in KiB, or 0 when `/proc/self/status` is unavailable.
pub fn hwm_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.trim().trim_end_matches(" kB").parse().ok())
        .unwrap_or(0)
}

/// Current RSS in mebibytes.
pub fn rss_mb() -> f64 {
    rss_kb() as f64 / 1024.0
}

fn page_kb() -> u64 {
    // Android aarch64 uses 4 KiB pages, 16 KiB on the newer 16k kernels; ask
    // the kernel instead of hardcoding a device assumption.
    static PAGE_KB: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *PAGE_KB.get_or_init(|| {
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if size > 0 { (size as u64) / 1024 } else { 4 }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn rss_is_positive_and_reasonable() {
        let kb = super::rss_kb();
        // A running test process holds at least a few hundred KiB and never
        // more than the device has.
        assert!(kb > 256, "rss_kb={kb}");
        assert!(kb < 64 * 1024 * 1024, "rss_kb={kb} is not a real reading");
    }

    #[test]
    fn hwm_is_at_least_current_rss() {
        let hwm = super::hwm_kb();
        let rss = super::rss_kb();
        assert!(hwm >= rss, "hwm={hwm}kB < rss={rss}kB");
    }
}

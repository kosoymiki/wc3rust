//! Bounded data-diagnostics channel for the format parsers.
//!
//! The parsers never panic on malformed operator data: a bad element is
//! skipped and reported here. Two guarantees make the channel safe to use on a
//! device log:
//!
//! * **Bounded volume** — only the first [`MAX_LINES_PER_REASON`] lines of a
//!   given reason are emitted; every occurrence still increments the counter.
//! * **Bounded work** — the detail string is built by a closure, so a
//!   suppressed line costs no allocation.
//!
//! `wc3-game` installs a sink that forwards the lines to logcat through
//! `dev_log!`; without a sink the lines go to stderr. Both carry the
//! `GAME_DATA_SKIP reason=…` prefix so a report can be grepped from a device
//! log. The counters are exposed through [`snapshot`] and [`summary`] for the
//! load reports and for tests.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

/// Forward one diagnostic line (no trailing newline).
pub type Sink = fn(&str);

/// Lines emitted per reason before the channel goes quiet.
pub const MAX_LINES_PER_REASON: u64 = 8;

static SINK: OnceLock<Sink> = OnceLock::new();
static COUNTS: OnceLock<Mutex<BTreeMap<&'static str, u64>>> = OnceLock::new();
static TOTAL: AtomicU64 = AtomicU64::new(0);

fn counts() -> MutexGuard<'static, BTreeMap<&'static str, u64>> {
    COUNTS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Install the process-wide line sink. The first installation wins; returns
/// whether this call installed it.
pub fn set_sink(sink: Sink) -> bool {
    SINK.set(sink).is_ok()
}

/// Record one skipped/patched data element.
///
/// `reason` is the stable machine key (`missing_slk_column`, `bad_cell`, …);
/// `detail` is only invoked when the line will actually be emitted. Returns the
/// running count for `reason`.
pub fn note(reason: &'static str, detail: impl FnOnce() -> String) -> u64 {
    let count = {
        let mut counts = counts();
        let entry = counts.entry(reason).or_insert(0);
        *entry += 1;
        *entry
    };
    TOTAL.fetch_add(1, Ordering::Relaxed);
    if count <= MAX_LINES_PER_REASON {
        let line = format!("GAME_DATA_SKIP reason={reason} {}", detail());
        match SINK.get() {
            Some(sink) => sink(&line),
            None => eprintln!("{line}"),
        }
    }
    count
}

/// Occurrences recorded for `reason`.
pub fn count(reason: &str) -> u64 {
    counts().get(reason).copied().unwrap_or(0)
}

/// Every reason with its count, sorted by reason.
pub fn snapshot() -> Vec<(&'static str, u64)> {
    counts().iter().map(|(k, v)| (*k, *v)).collect()
}

/// Total skipped/patched elements across all reasons.
pub fn total() -> u64 {
    TOTAL.load(Ordering::Relaxed)
}

/// One-line aggregate for the load report (`GAME_DATA_SKIP total=… …`); empty
/// when nothing was skipped.
pub fn summary() -> String {
    if total() == 0 {
        return String::new();
    }
    let mut line = format!("GAME_DATA_SKIP total={}", total());
    for (reason, count) in snapshot() {
        line.push(' ');
        line.push_str(reason);
        line.push('=');
        line.push_str(&count.to_string());
    }
    line
}

/// Clear counters and the sink. Test-only: process state must not leak between
/// malformed-input cases.
#[doc(hidden)]
pub fn reset() {
    counts().clear();
    TOTAL.store(0, Ordering::Relaxed);
}

/// The `GAME_DATA_SKIP` macro: `data_skip!("reason", "sheet={} col={}", s, c)`.
#[macro_export]
macro_rules! data_skip {
    ($reason:literal $(, $($arg:tt)*)?) => {
        $crate::diag::note($reason, || format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_and_logging_stays_bounded() {
        reset();
        for i in 0..(MAX_LINES_PER_REASON + 5) {
            note("unit_test_reason", || format!("i={i}"));
        }
        assert_eq!(count("unit_test_reason"), MAX_LINES_PER_REASON + 5);
        assert!(total() >= MAX_LINES_PER_REASON + 5);
        assert!(summary().contains("unit_test_reason="));
        reset();
        assert_eq!(total(), 0);
        assert!(summary().is_empty());
    }
}

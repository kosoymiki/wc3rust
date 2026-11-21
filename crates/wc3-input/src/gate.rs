//! Idle-gating and deadline contract for the Android input loop.
//!
//! Ported from the `controls_needs_tick()` / `controls_next_deadline_ms()` /
//! `controls_visual_dirty()` contract in
//! `runtime/platform/android/controls/controls.h` of NIK2703/WC3_AndroidRecomp
//! (MIT, commit `c753ab89…`).
//!
//! The donor's Android host must not poll: an active finger alone is **not** a
//! reason to wake up. Only a due timer (auto-repeat, held action, pending
//! movement) is, and the host asks for an exact absolute deadline instead of
//! inventing a tick rate.
//!
//! This module is that contract, independent of the touch engine that produces
//! the gestures ([`crate`] handles the gestures themselves):
//!
//! ```
//! use wc3_input::gate::InputGate;
//!
//! let mut gate = InputGate::new();
//! gate.finger_down();                 // a finger is on the glass
//! assert!(!gate.needs_tick(0));       // …which is not a wake-up reason
//! assert_eq!(gate.next_deadline_ms(), None);
//!
//! gate.schedule(120);                 // auto-repeat in 120 ms
//! assert!(!gate.needs_tick(119));
//! assert!(gate.needs_tick(120));
//! assert!(gate.take_due(120));        // the host runs exactly one tick
//! assert_eq!(gate.next_deadline_ms(), None);
//! ```

/// Tracks whether the host has anything to do, and when.
#[derive(Clone, Debug, Default)]
pub struct InputGate {
    fingers: u32,
    deadline_ms: Option<u64>,
    visual_dirty: bool,
}

impl InputGate {
    /// A gate with no fingers, no deadline and nothing to publish.
    pub fn new() -> Self {
        Self::default()
    }

    /// A finger went down.
    pub fn finger_down(&mut self) {
        self.fingers = self.fingers.saturating_add(1);
    }

    /// A finger lifted or was cancelled.
    pub fn finger_up(&mut self) {
        self.fingers = self.fingers.saturating_sub(1);
    }

    /// Number of fingers currently down.
    pub fn fingers(&self) -> u32 {
        self.fingers
    }

    /// Schedule work at an absolute `CLOCK_MONOTONIC` millisecond deadline.
    ///
    /// The earliest pending deadline wins: scheduling a later time must not
    /// push an already-armed one back.
    pub fn schedule(&mut self, at_ms: u64) {
        self.deadline_ms = Some(match self.deadline_ms {
            Some(current) => current.min(at_ms),
            None => at_ms,
        });
    }

    /// The exact absolute deadline the host should wait for, or `None` when
    /// there is nothing to wait for.
    ///
    /// `None` means "block until the next input event", **not** "due now".
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// True when a deadline is due at `now_ms`. An active finger alone never
    /// makes this true.
    pub fn needs_tick(&self, now_ms: u64) -> bool {
        matches!(self.deadline_ms, Some(deadline) if deadline <= now_ms)
    }

    /// Consume a due deadline. Returns true when the caller should run one
    /// tick now; a not-yet-due or absent deadline returns false and is left
    /// armed.
    pub fn take_due(&mut self, now_ms: u64) -> bool {
        if self.needs_tick(now_ms) {
            self.deadline_ms = None;
            true
        } else {
            false
        }
    }

    /// Drop every pending deadline (for example on focus loss).
    pub fn cancel(&mut self) {
        self.deadline_ms = None;
        self.fingers = 0;
    }

    /// Mark that overlay visuals changed and need one publish.
    pub fn mark_visual_dirty(&mut self) {
        self.visual_dirty = true;
    }

    /// Consume the dirty flag. True at most once per change.
    ///
    /// Kept separate from the publish policy on purpose: change detection and
    /// publishing are two decisions, and merging them makes the overlay publish
    /// twice or not at all.
    pub fn take_visual_dirty(&mut self) -> bool {
        core::mem::take(&mut self.visual_dirty)
    }
}

//! Multi-finger input routing for the Android platform layer.
//!
//! SDL finger IDs are stable for the lifetime of a touch, but fingers arrive
//! and leave in any order and Android delivers `ACTION_CANCEL` as
//! `SDL_EVENT_FINGER_CANCELED` for every pointer. The platform layer keeps a
//! [`FingerTable`] of up to [`MAX_FINGERS`] ids instead of assuming exactly two
//! slots, routes the first two fingers into the pinch gesture, promotes the
//! next pair when one of them lifts, and treats a cancel as a discard — never
//! as the finger-up that completes a tap, box or order.
//!
//! The routing policy lives here (not in `apps/android-native`) so the host
//! tests replay the same code the device runs; [`RecordingSink`] produces the
//! event strings the trace fixtures pin.

use glam::Vec2;

use crate::sdl::{self, event};
use crate::{Hotkey, scancode_char};

/// The most simultaneous fingers the table tracks. SDL does not impose a small
/// limit; ten covers a full two-hand gesture with palms.
pub const MAX_FINGERS: usize = 10;

/// Stable finger-ID table with compaction on removal.
#[derive(Clone, Debug, Default)]
pub struct FingerTable {
    ids: [i64; MAX_FINGERS],
    pos: [Vec2; MAX_FINGERS],
    count: usize,
}

impl FingerTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn contains(&self, id: i64) -> bool {
        self.index_of(id).is_some()
    }

    pub fn index_of(&self, id: i64) -> Option<usize> {
        (0..self.count).find(|&index| self.ids[index] == id)
    }

    pub fn id(&self, index: usize) -> Option<i64> {
        (index < self.count).then_some(self.ids[index])
    }

    pub fn position(&self, index: usize) -> Option<Vec2> {
        (index < self.count).then_some(self.pos[index])
    }

    /// Insert a new finger (or update a duplicate id). Returns the count after
    /// the operation. A full table ignores further fingers; the tracked pair
    /// keeps driving the gesture.
    pub fn add(&mut self, id: i64, pos: Vec2) -> usize {
        if let Some(index) = self.index_of(id) {
            self.pos[index] = pos;
            return self.count;
        }
        if self.count < MAX_FINGERS {
            self.ids[self.count] = id;
            self.pos[self.count] = pos;
            self.count += 1;
        }
        self.count
    }

    pub fn update(&mut self, id: i64, pos: Vec2) -> Option<usize> {
        let index = self.index_of(id)?;
        self.pos[index] = pos;
        Some(index)
    }

    /// Remove a finger and keep insertion order for the survivors.
    pub fn remove(&mut self, id: i64) -> usize {
        let Some(index) = self.index_of(id) else {
            return self.count;
        };
        for read in index + 1..self.count {
            self.ids[read - 1] = self.ids[read];
            self.pos[read - 1] = self.pos[read];
        }
        self.count -= 1;
        self.count
    }

    pub fn clear(&mut self) {
        self.count = 0;
    }

    /// Distance between the first two fingers (0 with fewer than two).
    pub fn distance(&self) -> f32 {
        if self.count >= 2 {
            self.pos[0].distance(self.pos[1])
        } else {
            0.0
        }
    }

    /// Mean of all tracked fingers.
    pub fn centroid(&self) -> Vec2 {
        if self.count == 0 {
            return Vec2::ZERO;
        }
        let mut sum = Vec2::ZERO;
        for index in 0..self.count {
            sum += self.pos[index];
        }
        sum / self.count as f32
    }

    /// Mean of the first two fingers (the pinch centre), or the only finger.
    pub fn pair_centroid(&self) -> Vec2 {
        match self.count {
            0 => Vec2::ZERO,
            1 => self.pos[0],
            _ => (self.pos[0] + self.pos[1]) * 0.5,
        }
    }
}

/// What the platform layer applies for a routed input event.
///
/// The app implements this for its `Game`; the host tests implement it with
/// [`RecordingSink`].
pub trait InputSink {
    fn pointer_down(&mut self, at: Vec2);
    fn pointer_move(&mut self, at: Vec2);
    fn pointer_up(&mut self, at: Vec2);
    fn pointer_cancel(&mut self);
    fn pointer_resume_pan(&mut self, at: Vec2);
    fn pinch_begin(&mut self, distance: f32, centre: Vec2);
    fn pinch_update(&mut self, distance: f32, centre: Vec2);
    /// `two_finger_tap` is the centroid of a pinch that never moved (the touch
    /// right-click), `None` after a real pan/zoom or after a cancel.
    fn pinch_end(&mut self, two_finger_tap: Option<Vec2>);
    fn set_additive(&mut self, additive: bool);
    fn key_char(&mut self, key: char, ctrl: bool);
    fn key_escape(&mut self, ctrl: bool);
    fn cycle_subgroups(&mut self, reverse: bool);
}

/// Pinch progress: the tap detector mirrors `TouchInput::pinch_end` so the
/// platform layer can surface the two-finger right-click without touching the
/// router's internals.
#[derive(Clone, Copy, Debug)]
struct PinchProgress {
    start_distance: f32,
    last_centre: Vec2,
    moved: bool,
}

impl PinchProgress {
    fn begin(distance: f32, centre: Vec2) -> Self {
        Self {
            start_distance: distance.max(1.0),
            last_centre: centre,
            moved: false,
        }
    }

    fn update(&mut self, distance: f32, centre: Vec2) {
        if self.last_centre.distance(centre) > 2.0
            || (self.start_distance - distance.max(1.0)).abs() > 8.0
        {
            self.moved = true;
        }
        self.last_centre = centre;
    }

    fn finish(self) -> Option<Vec2> {
        (!self.moved).then_some(self.last_centre)
    }
}

/// The gesture state machine the platform layer keeps across frames.
#[derive(Clone, Debug)]
pub struct TouchRouter {
    fingers: FingerTable,
    pinch: Option<PinchProgress>,
}

impl Default for TouchRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl TouchRouter {
    pub fn new() -> Self {
        Self {
            fingers: FingerTable::new(),
            pinch: None,
        }
    }

    pub fn fingers(&self) -> &FingerTable {
        &self.fingers
    }

    pub fn pinching(&self) -> bool {
        self.pinch.is_some()
    }

    /// New finger down. The first finger starts the pointer; the second starts
    /// the pinch. Further fingers only join the table (the first two keep
    /// driving the gesture) and are promoted when an earlier finger lifts.
    pub fn finger_down(&mut self, sink: &mut impl InputSink, id: i64, pos: Vec2) {
        if self.fingers.contains(id) {
            self.fingers.add(id, pos);
            return;
        }
        let count = self.fingers.add(id, pos);
        if count == 1 {
            sink.pointer_down(pos);
        } else if count >= 2 && self.pinch.is_none() {
            let distance = self.fingers.distance();
            let centre = self.fingers.pair_centroid();
            self.pinch = Some(PinchProgress::begin(distance, centre));
            sink.pinch_begin(distance, centre);
        }
    }

    /// Finger motion. Returns true when a sink event was emitted.
    pub fn finger_move(&mut self, sink: &mut impl InputSink, id: i64, pos: Vec2) -> bool {
        let Some(index) = self.fingers.update(id, pos) else {
            return false;
        };
        if self.fingers.count() >= 2 {
            if self.pinch.is_none() {
                let distance = self.fingers.distance();
                let centre = self.fingers.pair_centroid();
                self.pinch = Some(PinchProgress::begin(distance, centre));
                sink.pinch_begin(distance, centre);
                return true;
            }
            let distance = self.fingers.distance();
            let centre = self.fingers.pair_centroid();
            if let Some(pinch) = self.pinch.as_mut() {
                pinch.update(distance, centre);
            }
            sink.pinch_update(distance, centre);
            true
        } else if index == 0 {
            sink.pointer_move(pos);
            true
        } else {
            false
        }
    }

    /// Finger up. A finished pinch promotes the remaining finger to a camera
    /// pan; the last finger ending a plain pointer completes the gesture.
    pub fn finger_up(&mut self, sink: &mut impl InputSink, id: i64, pos: Vec2) {
        if !self.fingers.contains(id) {
            return;
        }
        let was_pinching = self.pinch.is_some();
        let remaining = self.fingers.remove(id);
        if was_pinching {
            if remaining < 2 {
                let tap = self.pinch.take().and_then(PinchProgress::finish);
                sink.pinch_end(tap);
            }
            if remaining == 1 {
                sink.pointer_resume_pan(self.fingers.pair_centroid());
            }
        } else if remaining == 0 {
            sink.pointer_up(pos);
        }
    }

    /// Finger canceled (Android `ACTION_CANCEL`). Android cancels the whole
    /// gesture and SDL emits one `FINGER_CANCELED` per tracked pointer, so the
    /// first cancel discards every finger: no tap, box, order or two-finger
    /// tap may fire, and no zombie pinch may survive into the next gesture.
    pub fn finger_cancel(&mut self, sink: &mut impl InputSink, id: i64) {
        if !self.fingers.contains(id) {
            return;
        }
        let was_pinching = self.pinch.take().is_some();
        self.fingers.clear();
        if was_pinching {
            sink.pinch_end(None);
        }
        sink.pointer_cancel();
    }

    /// Drop every gesture: app background, focus loss or activity restart.
    /// The keyboard modifier is owned by [`KeyState`] and released through
    /// [`InputState::reset`].
    pub fn reset(&mut self, sink: &mut impl InputSink) {
        self.cancel_gesture(sink);
    }

    /// Rotation/resize: pixel coordinates changed, so the gesture is dead but
    /// the keyboard modifiers stay as they are.
    pub fn rotate(&mut self, sink: &mut impl InputSink) {
        self.cancel_gesture(sink);
    }

    fn cancel_gesture(&mut self, sink: &mut impl InputSink) {
        if !self.fingers.is_empty() || self.pinch.is_some() {
            sink.pointer_cancel();
        }
        self.fingers.clear();
        self.pinch = None;
    }
}

/// Keyboard state shared by the SDL key events: physical scancode -> WC3 key,
/// repeat suppression and the Shift add-to-selection modifier.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyState {
    additive: bool,
}

impl KeyState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn additive(&self) -> bool {
        self.additive
    }

    /// Forget a held modifier after focus/background loss; emits one
    /// `set_additive(false)` only when Shift was actually held.
    pub fn reset(&mut self, sink: &mut impl InputSink) {
        if self.additive {
            self.additive = false;
            sink.set_additive(false);
        }
    }

    /// Handle one SDL key event. Returns true when the event was a key event
    /// (even if it only updated the modifier state).
    pub fn handle(
        &mut self,
        sink: &mut impl InputSink,
        event_type: u32,
        scancode: u32,
        modifiers: u16,
        repeat: bool,
    ) -> bool {
        let down = match event_type {
            event::KEY_DOWN => true,
            event::KEY_UP => false,
            _ => return false,
        };
        let additive = sdl::shift_down(modifiers);
        if additive != self.additive {
            self.additive = additive;
            sink.set_additive(additive);
        }
        if !down || repeat {
            return true;
        }
        if scancode == sdl::scancode::TAB {
            sink.cycle_subgroups(additive);
        } else if let Some(key) = scancode_char(scancode) {
            sink.key_char(key, sdl::ctrl_down(modifiers));
        } else if scancode == sdl::scancode::ESCAPE {
            sink.key_escape(sdl::ctrl_down(modifiers));
        }
        true
    }
}

/// One SDL input event after decoding, ready for [`InputState::apply`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    FingerDown {
        id: i64,
        pos: Vec2,
    },
    FingerMove {
        id: i64,
        pos: Vec2,
    },
    FingerUp {
        id: i64,
        pos: Vec2,
    },
    FingerCancel {
        id: i64,
        pos: Vec2,
    },
    MouseDown {
        pos: Vec2,
    },
    MouseUp {
        pos: Vec2,
    },
    MouseMove {
        pos: Vec2,
    },
    KeyDown {
        scancode: u32,
        modifiers: u16,
        repeat: bool,
    },
    KeyUp {
        scancode: u32,
        modifiers: u16,
    },
}

/// The whole app input state: touch router, keyboard and the desktop mouse
/// button latch.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    pub router: TouchRouter,
    pub keys: KeyState,
    mouse_down: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mouse_down(&self) -> bool {
        self.mouse_down
    }

    /// Apply one decoded event. Returns true when the event was consumed.
    pub fn apply(&mut self, sink: &mut impl InputSink, event: InputEvent) -> bool {
        match event {
            InputEvent::FingerDown { id, pos } => {
                self.router.finger_down(sink, id, pos);
                true
            }
            InputEvent::FingerMove { id, pos } => self.router.finger_move(sink, id, pos),
            InputEvent::FingerUp { id, pos } => {
                self.router.finger_up(sink, id, pos);
                true
            }
            InputEvent::FingerCancel { id, .. } => {
                self.router.finger_cancel(sink, id);
                true
            }
            InputEvent::MouseDown { pos } => {
                self.mouse_down = true;
                sink.pointer_down(pos);
                true
            }
            InputEvent::MouseUp { pos } => {
                let was_down = self.mouse_down;
                self.mouse_down = false;
                if was_down {
                    sink.pointer_up(pos);
                }
                was_down
            }
            InputEvent::MouseMove { pos } => {
                if self.mouse_down {
                    sink.pointer_move(pos);
                }
                self.mouse_down
            }
            InputEvent::KeyDown {
                scancode,
                modifiers,
                repeat,
            } => self
                .keys
                .handle(sink, event::KEY_DOWN, scancode, modifiers, repeat),
            InputEvent::KeyUp {
                scancode,
                modifiers,
            } => self
                .keys
                .handle(sink, event::KEY_UP, scancode, modifiers, false),
        }
    }

    /// Background/focus loss: discard every gesture and release Shift.
    pub fn reset(&mut self, sink: &mut impl InputSink) {
        self.router.reset(sink);
        self.keys.reset(sink);
        self.mouse_down = false;
    }

    /// Rotation/resize: discard the gesture, keep keyboard modifiers.
    pub fn rotate(&mut self, sink: &mut impl InputSink) {
        self.router.rotate(sink);
        self.mouse_down = false;
    }
}

/// Deterministic sink used by the trace fixtures and unit tests. Its strings
/// are the trace vocabulary (`touch=...`).
#[derive(Clone, Debug, Default)]
pub struct RecordingSink {
    events: Vec<String>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[String] {
        &self.events
    }

    pub fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl InputSink for RecordingSink {
    fn pointer_down(&mut self, at: Vec2) {
        self.events
            .push(format!("touch=pointer_down:{:.1}:{:.1}", at.x, at.y));
    }

    fn pointer_move(&mut self, at: Vec2) {
        self.events
            .push(format!("touch=pointer_move:{:.1}:{:.1}", at.x, at.y));
    }

    fn pointer_up(&mut self, at: Vec2) {
        self.events
            .push(format!("touch=pointer_up:{:.1}:{:.1}", at.x, at.y));
    }

    fn pointer_cancel(&mut self) {
        self.events.push("touch=pointer_cancel".to_string());
    }

    fn pointer_resume_pan(&mut self, at: Vec2) {
        self.events
            .push(format!("touch=pointer_resume_pan:{:.1}:{:.1}", at.x, at.y));
    }

    fn pinch_begin(&mut self, distance: f32, centre: Vec2) {
        self.events.push(format!(
            "touch=pinch_begin:{:.1}:{:.1}:{:.1}",
            distance, centre.x, centre.y
        ));
    }

    fn pinch_update(&mut self, distance: f32, centre: Vec2) {
        self.events.push(format!(
            "touch=pinch_update:{:.1}:{:.1}:{:.1}",
            distance, centre.x, centre.y
        ));
    }

    fn pinch_end(&mut self, two_finger_tap: Option<Vec2>) {
        match two_finger_tap {
            Some(point) => self.events.push(format!(
                "touch=pinch_end:two_finger_tap:{:.1}:{:.1}",
                point.x, point.y
            )),
            None => self.events.push("touch=pinch_end".to_string()),
        }
    }

    fn set_additive(&mut self, additive: bool) {
        self.events.push(format!(
            "touch=shift:{}",
            if additive { "on" } else { "off" }
        ));
    }

    fn key_char(&mut self, key: char, ctrl: bool) {
        self.events.push(format!("touch=key:{key}:ctrl:{ctrl}"));
    }

    fn key_escape(&mut self, ctrl: bool) {
        self.events.push(format!("touch=key:escape:ctrl:{ctrl}"));
    }

    fn cycle_subgroups(&mut self, reverse: bool) {
        self.events
            .push(format!("touch=cycle_subgroups:reverse:{reverse}"));
    }
}

/// The WC3 hotkey for an SDL scancode (thin wrapper over [`scancode_char`]).
pub fn scancode_hotkey(scancode: u32) -> Option<Hotkey> {
    scancode_char(scancode).and_then(Hotkey::parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl::scancode;

    fn router() -> (TouchRouter, RecordingSink) {
        (TouchRouter::new(), RecordingSink::new())
    }

    #[test]
    fn single_finger_cancel_never_becomes_a_tap() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 7, Vec2::new(100.0, 200.0));
        router.finger_cancel(&mut sink, 7);
        assert_eq!(
            sink.events(),
            ["touch=pointer_down:100.0:200.0", "touch=pointer_cancel"]
        );
        assert!(router.fingers().is_empty());
    }

    #[test]
    fn cancel_of_an_untracked_finger_is_ignored() {
        let (mut router, mut sink) = router();
        router.finger_cancel(&mut sink, 99);
        assert!(sink.events().is_empty());
    }

    #[test]
    fn two_finger_cancel_ends_the_pinch_without_a_tap() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 1, Vec2::new(100.0, 300.0));
        router.finger_down(&mut sink, 2, Vec2::new(300.0, 300.0));
        router.finger_move(&mut sink, 2, Vec2::new(320.0, 300.0));
        router.finger_cancel(&mut sink, 1);
        router.finger_cancel(&mut sink, 2);
        let events = sink.events();
        assert_eq!(events[0], "touch=pointer_down:100.0:300.0");
        assert_eq!(events[1], "touch=pinch_begin:200.0:200.0:300.0");
        assert_eq!(events[2], "touch=pinch_update:220.0:210.0:300.0");
        assert_eq!(events[3], "touch=pinch_end");
        assert_eq!(events[4], "touch=pointer_cancel");
        assert!(
            !events.iter().any(|event| event.contains("two_finger_tap")),
            "a cancel must never complete a right-click: {events:?}"
        );
        assert!(router.fingers().is_empty());
    }

    #[test]
    fn cancel_with_three_fingers_discards_all_of_them() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 1, Vec2::new(100.0, 300.0));
        router.finger_down(&mut sink, 2, Vec2::new(300.0, 300.0));
        router.finger_down(&mut sink, 3, Vec2::new(200.0, 100.0));
        router.finger_cancel(&mut sink, 2);
        assert!(router.fingers().is_empty());
        assert!(!router.pinching(), "no zombie pinch after a cancel");
        assert_eq!(sink.events().last().unwrap(), "touch=pointer_cancel");
        let before = sink.events().len();
        router.finger_move(&mut sink, 3, Vec2::new(250.0, 120.0));
        assert_eq!(sink.events().len(), before, "a stale finger emits nothing");
    }

    #[test]
    fn lifting_one_pinch_finger_promotes_the_other_to_a_pan() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 1, Vec2::new(100.0, 300.0));
        router.finger_down(&mut sink, 2, Vec2::new(300.0, 300.0));
        router.finger_up(&mut sink, 1, Vec2::new(100.0, 300.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pointer_resume_pan:300.0:300.0"
        );
        router.finger_move(&mut sink, 2, Vec2::new(320.0, 300.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pointer_move:320.0:300.0"
        );
        router.finger_up(&mut sink, 2, Vec2::new(320.0, 300.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pointer_up:320.0:300.0"
        );
    }

    #[test]
    fn third_finger_is_tracked_and_promoted() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 1, Vec2::new(100.0, 300.0));
        router.finger_down(&mut sink, 2, Vec2::new(300.0, 300.0));
        router.finger_down(&mut sink, 3, Vec2::new(200.0, 100.0));
        assert_eq!(
            sink.events()
                .iter()
                .filter(|e| e.starts_with("touch=pinch_begin"))
                .count(),
            1,
            "a third finger must not restart the pinch"
        );
        assert_eq!(router.fingers().count(), 3);
        router.finger_up(&mut sink, 1, Vec2::new(100.0, 300.0));
        assert!(
            !sink
                .events()
                .iter()
                .any(|e| e.starts_with("touch=pinch_end")),
            "two fingers still down: the pinch continues"
        );
        router.finger_move(&mut sink, 3, Vec2::new(320.0, 100.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pinch_update:201.0:310.0:200.0"
        );
        router.finger_up(&mut sink, 2, Vec2::new(300.0, 300.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pointer_resume_pan:320.0:100.0"
        );
        router.finger_up(&mut sink, 3, Vec2::new(320.0, 100.0));
        assert_eq!(
            sink.events().last().unwrap(),
            "touch=pointer_up:320.0:100.0"
        );
    }

    #[test]
    fn stationary_two_finger_tap_is_reported() {
        let (mut router, mut sink) = router();
        router.finger_down(&mut sink, 1, Vec2::new(100.0, 300.0));
        router.finger_down(&mut sink, 2, Vec2::new(300.0, 300.0));
        router.finger_up(&mut sink, 1, Vec2::new(100.0, 300.0));
        let events = sink.events();
        assert_eq!(
            events[events.len() - 2],
            "touch=pinch_end:two_finger_tap:200.0:300.0"
        );
        assert_eq!(
            events.last().unwrap(),
            "touch=pointer_resume_pan:300.0:300.0"
        );
    }

    #[test]
    fn focus_loss_cancels_everything_and_releases_shift() {
        let mut state = InputState::new();
        let mut sink = RecordingSink::new();
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: sdl::scancode::ESCAPE,
                modifiers: sdl::keymod::LSHIFT,
                repeat: false,
            },
        );
        assert!(
            sink.events().iter().any(|e| e == "touch=shift:on"),
            "{:?}",
            sink.events()
        );
        state.apply(
            &mut sink,
            InputEvent::FingerDown {
                id: 4,
                pos: Vec2::new(10.0, 20.0),
            },
        );
        state.reset(&mut sink);
        assert_eq!(sink.events().last().unwrap(), "touch=shift:off");
        assert!(
            sink.events().iter().any(|e| e == "touch=pointer_cancel"),
            "{:?}",
            sink.events()
        );
        let before = sink.events().len();
        state.apply(
            &mut sink,
            InputEvent::FingerMove {
                id: 4,
                pos: Vec2::new(50.0, 60.0),
            },
        );
        assert_eq!(sink.events().len(), before, "a stale finger is ignored");
    }

    #[test]
    fn rotation_cancels_the_gesture_but_keeps_shift() {
        let mut state = InputState::new();
        let mut sink = RecordingSink::new();
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: sdl::scancode::ESCAPE,
                modifiers: sdl::keymod::LSHIFT,
                repeat: false,
            },
        );
        state.apply(
            &mut sink,
            InputEvent::FingerDown {
                id: 1,
                pos: Vec2::new(1.0, 2.0),
            },
        );
        state.rotate(&mut sink);
        let events = sink.events();
        assert_eq!(events.iter().filter(|e| *e == "touch=shift:off").count(), 0);
        assert!(events.iter().any(|e| e == "touch=pointer_cancel"));
    }

    #[test]
    fn key_repeat_never_refires_a_hotkey_and_shift_release_clears_it() {
        let mut state = InputState::new();
        let mut sink = RecordingSink::new();
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: scancode::A,
                modifiers: sdl::keymod::NONE,
                repeat: false,
            },
        );
        assert_eq!(sink.events().last().unwrap(), "touch=key:a:ctrl:false");
        let before = sink.events().len();
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: scancode::A,
                modifiers: sdl::keymod::NONE,
                repeat: true,
            },
        );
        assert_eq!(sink.events().len(), before, "repeat is swallowed");
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: scancode::ESCAPE,
                modifiers: sdl::keymod::LSHIFT,
                repeat: false,
            },
        );
        assert_eq!(sink.events().last().unwrap(), "touch=key:escape:ctrl:false");
        assert!(
            sink.events().iter().any(|e| e == "touch=shift:on"),
            "{:?}",
            sink.events()
        );
        state.apply(
            &mut sink,
            InputEvent::KeyUp {
                scancode: sdl::scancode::ESCAPE,
                modifiers: sdl::keymod::NONE,
            },
        );
        assert_eq!(sink.events().last().unwrap(), "touch=shift:off");
        assert!(!state.keys.additive());
    }

    #[test]
    fn tab_escape_and_digits_route_to_the_card() {
        let mut state = InputState::new();
        let mut sink = RecordingSink::new();
        for (scancode, modifiers, expected) in [
            (
                sdl::scancode::TAB,
                sdl::keymod::LSHIFT,
                "touch=cycle_subgroups:reverse:true",
            ),
            (
                sdl::scancode::TAB,
                sdl::keymod::NONE,
                "touch=cycle_subgroups:reverse:false",
            ),
            (
                sdl::scancode::DIGIT_1,
                sdl::keymod::CTRL,
                "touch=key:1:ctrl:true",
            ),
        ] {
            state.apply(
                &mut sink,
                InputEvent::KeyDown {
                    scancode,
                    modifiers,
                    repeat: false,
                },
            );
            assert_eq!(sink.events().last().unwrap(), expected);
        }
        state.apply(
            &mut sink,
            InputEvent::KeyUp {
                scancode: sdl::scancode::DIGIT_1,
                modifiers: sdl::keymod::NONE,
            },
        );
        state.apply(
            &mut sink,
            InputEvent::KeyDown {
                scancode: sdl::scancode::ESCAPE,
                modifiers: sdl::keymod::CTRL,
                repeat: false,
            },
        );
        assert_eq!(sink.events().last().unwrap(), "touch=key:escape:ctrl:true");
    }

    #[test]
    fn mouse_drag_only_fires_while_the_button_is_down() {
        let mut state = InputState::new();
        let mut sink = RecordingSink::new();
        state.apply(
            &mut sink,
            InputEvent::MouseMove {
                pos: Vec2::new(5.0, 5.0),
            },
        );
        assert!(sink.events().is_empty());
        state.apply(
            &mut sink,
            InputEvent::MouseDown {
                pos: Vec2::new(5.0, 5.0),
            },
        );
        state.apply(
            &mut sink,
            InputEvent::MouseMove {
                pos: Vec2::new(9.0, 5.0),
            },
        );
        state.apply(
            &mut sink,
            InputEvent::MouseUp {
                pos: Vec2::new(9.0, 5.0),
            },
        );
        state.apply(
            &mut sink,
            InputEvent::MouseUp {
                pos: Vec2::new(9.0, 5.0),
            },
        );
        assert_eq!(
            sink.events(),
            [
                "touch=pointer_down:5.0:5.0",
                "touch=pointer_move:9.0:5.0",
                "touch=pointer_up:9.0:5.0"
            ]
        );
    }

    #[test]
    fn finger_table_compacts_and_is_bounded() {
        let mut table = FingerTable::new();
        for id in 0..MAX_FINGERS as i64 {
            table.add(id, Vec2::splat(id as f32));
        }
        assert_eq!(table.count(), MAX_FINGERS);
        table.add(999, Vec2::ZERO);
        assert_eq!(table.count(), MAX_FINGERS, "the table never overflows");
        table.remove(4);
        assert_eq!(table.count(), MAX_FINGERS - 1);
        assert_eq!(table.id(4), Some(5));
        assert_eq!(
            table.centroid().x,
            (0..MAX_FINGERS as i32).filter(|id| *id != 4).sum::<i32>() as f32 / 9.0
        );
    }

    #[test]
    fn scancode_hotkey_maps_letters_and_digits() {
        assert_eq!(scancode_hotkey(scancode::A), Some(Hotkey::Letter('a')));
        assert_eq!(scancode_hotkey(scancode::DIGIT_1), Some(Hotkey::Digit(1)));
        assert_eq!(scancode_hotkey(scancode::DIGIT_0), Some(Hotkey::Digit(0)));
        assert_eq!(scancode_hotkey(scancode::TAB), None);
    }
}

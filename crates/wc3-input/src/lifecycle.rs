//! Android application lifecycle, decoupled from SDL and from the game.
//!
//! SDL 3.5.0 does **not** queue the app lifecycle events: `TERMINATING`,
//! `LOW_MEMORY`, `WILL_ENTER_BACKGROUND`, `DID_ENTER_BACKGROUND`,
//! `WILL_ENTER_FOREGROUND` and `DID_ENTER_FOREGROUND` are delivered only to
//! `SDL_AddEventWatch` callbacks (`src/events/SDL_events.c:2031-2055` in the
//! pinned donor). Window focus, display, safe-area and resize events are
//! ordinary queued events, but the same watcher sees them, so the platform
//! layer funnels every relevant type through this state machine and gets one
//! ordered list of effects per frame.
//!
//! The state machine is pure: the platform layer maps each [`Effect`] to a
//! concrete action (cancel touches, save the sim, rebuild the surface), and the
//! host tests here pin the ordering, idempotency and save-once behaviour.
//!
//! `android-activity`'s `MainEvent` donor (`src/lib.rs:456-543`) is the model
//! for the event set: `SaveState`/`Pause`/`Stop`/`Resume`/`Destroy` map to the
//! SDL background/foreground pair, `LostFocus`/`GainedFocus` to the window
//! focus events, `ConfigChanged`/`InsetsChanged` to display/scale/safe-area,
//! `LowMemory` and `Destroy` keep their names.

use crate::sdl::event;

/// Where the activity is in its Android lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Foreground,
    Background,
    Terminating,
}

/// One platform lifecycle event after SDL/Android mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    WillEnterBackground,
    DidEnterBackground,
    WillEnterForeground,
    DidEnterForeground,
    FocusLost,
    FocusGained,
    LowMemory,
    Terminating,
    /// Rotation, DPI/display change or a window resize: pixel coordinates and
    /// the swapchain are stale.
    DisplayChanged,
    WindowResized,
    SafeAreaChanged,
    SurfaceLost,
    SurfaceRecreated,
}

/// What the platform layer must do for the events handled this frame.
///
/// Every field is false unless the event asks for it; a frame that handles no
/// lifecycle event returns [`Effects::none`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub pause_updates: bool,
    pub resume_updates: bool,
    /// Cancel the active gesture and forget finger/pinch state so a touch
    /// collected before the transition cannot complete after it.
    pub reset_gestures: bool,
    /// Write the sim checkpoint (the native equivalent of
    /// `android-activity`'s `SaveState`).
    pub save_checkpoint: bool,
    /// The OS asked us to free memory.
    pub low_memory: bool,
    /// Re-read the `ANativeWindow` and rebuild/reconfigure the wgpu surface.
    pub rebuild_surface: bool,
    /// Re-read `SDL_GetWindowSafeArea` and update the HUD safe insets.
    pub reapply_insets: bool,
    /// Leave the event loop after this frame.
    pub quit: bool,
}

impl Effects {
    pub const fn none() -> Self {
        Self {
            pause_updates: false,
            resume_updates: false,
            reset_gestures: false,
            save_checkpoint: false,
            low_memory: false,
            rebuild_surface: false,
            reapply_insets: false,
            quit: false,
        }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            pause_updates: self.pause_updates || other.pause_updates,
            resume_updates: self.resume_updates || other.resume_updates,
            reset_gestures: self.reset_gestures || other.reset_gestures,
            save_checkpoint: self.save_checkpoint || other.save_checkpoint,
            low_memory: self.low_memory || other.low_memory,
            rebuild_surface: self.rebuild_surface || other.rebuild_surface,
            reapply_insets: self.reapply_insets || other.reapply_insets,
            quit: self.quit || other.quit,
        }
    }
}

/// The lifecycle state machine. One instance lives for the whole native run.
#[derive(Clone, Debug)]
pub struct Lifecycle {
    phase: Phase,
    focused: bool,
    surface_lost: bool,
    /// A checkpoint has been written since the last foreground transition.
    saved_since_foreground: bool,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    /// A fresh process starts foreground and focused; SDL delivers a focus
    /// event if that is not true (multi-window) before the first frame.
    pub const fn new() -> Self {
        Self {
            phase: Phase::Foreground,
            focused: true,
            surface_lost: false,
            saved_since_foreground: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn surface_lost(&self) -> bool {
        self.surface_lost
    }

    /// The simulation may advance: the app is foreground and not terminating.
    /// Focus loss alone does not freeze the sim (a transient system dialog must
    /// not desynchronise a lockstep RTS); it only cancels the active gesture.
    pub fn updates_allowed(&self) -> bool {
        self.phase == Phase::Foreground
    }

    pub fn handle(&mut self, event: Event) -> Effects {
        match event {
            Event::WillEnterBackground => self.enter_background(),
            Event::DidEnterBackground => {
                let mut effects = self.enter_background();
                self.surface_lost = true;
                // The surface is gone; a resume must rebuild it even if the
                // app never renders in the background.
                effects.rebuild_surface = true;
                effects
            }
            Event::WillEnterForeground => {
                // SDL's WILL event is the point where the process may still be
                // backgrounded; do not resume updates until DID.
                Effects::none()
            }
            Event::DidEnterForeground => self.enter_foreground(),
            Event::FocusLost => {
                if !self.focused {
                    return Effects::none();
                }
                self.focused = false;
                Effects {
                    reset_gestures: true,
                    ..Effects::none()
                }
            }
            Event::FocusGained => {
                self.focused = true;
                Effects::none()
            }
            Event::LowMemory => Effects {
                low_memory: true,
                ..Effects::none()
            },
            Event::Terminating => self.terminate(),
            Event::DisplayChanged => {
                if self.phase == Phase::Terminating {
                    return Effects::none();
                }
                Effects {
                    reset_gestures: true,
                    rebuild_surface: true,
                    reapply_insets: true,
                    ..Effects::none()
                }
            }
            Event::WindowResized => {
                if self.phase == Phase::Terminating {
                    return Effects::none();
                }
                Effects {
                    reset_gestures: true,
                    rebuild_surface: true,
                    ..Effects::none()
                }
            }
            Event::SafeAreaChanged => {
                if self.phase == Phase::Terminating {
                    return Effects::none();
                }
                Effects {
                    reapply_insets: true,
                    ..Effects::none()
                }
            }
            Event::SurfaceLost => {
                self.surface_lost = true;
                Effects::none()
            }
            Event::SurfaceRecreated => {
                self.surface_lost = false;
                Effects::none()
            }
        }
    }

    /// Map an SDL event type to a lifecycle event. `None` for events this
    /// state machine does not own (quit, close request, occlusion, input).
    pub fn event_from_sdl(event_type: u32) -> Option<Event> {
        Some(match event_type {
            event::TERMINATING => Event::Terminating,
            event::LOW_MEMORY => Event::LowMemory,
            event::WILL_ENTER_BACKGROUND => Event::WillEnterBackground,
            event::DID_ENTER_BACKGROUND => Event::DidEnterBackground,
            event::WILL_ENTER_FOREGROUND => Event::WillEnterForeground,
            event::DID_ENTER_FOREGROUND => Event::DidEnterForeground,
            event::WINDOW_FOCUS_LOST => Event::FocusLost,
            event::WINDOW_FOCUS_GAINED => Event::FocusGained,
            event::DISPLAY_ORIENTATION
            | event::WINDOW_DISPLAY_CHANGED
            | event::WINDOW_DISPLAY_SCALE_CHANGED => Event::DisplayChanged,
            event::WINDOW_RESIZED | event::WINDOW_PIXEL_SIZE_CHANGED => Event::WindowResized,
            event::WINDOW_SAFE_AREA_CHANGED => Event::SafeAreaChanged,
            event::WINDOW_DESTROYED => Event::Terminating,
            _ => return None,
        })
    }

    /// Convenience for the platform layer: map and handle in one call.
    pub fn handle_sdl(&mut self, event_type: u32) -> Option<Effects> {
        Self::event_from_sdl(event_type).map(|event| self.handle(event))
    }

    fn enter_background(&mut self) -> Effects {
        if self.phase == Phase::Background {
            return Effects::none();
        }
        if self.phase == Phase::Terminating {
            return Effects::none();
        }
        self.phase = Phase::Background;
        let save = !self.saved_since_foreground;
        self.saved_since_foreground = true;
        Effects {
            pause_updates: true,
            reset_gestures: true,
            save_checkpoint: save,
            ..Effects::none()
        }
    }

    fn enter_foreground(&mut self) -> Effects {
        if self.phase == Phase::Terminating {
            return Effects::none();
        }
        if self.phase == Phase::Foreground {
            return Effects::none();
        }
        self.phase = Phase::Foreground;
        self.saved_since_foreground = false;
        self.surface_lost = false;
        Effects {
            resume_updates: true,
            reset_gestures: true,
            rebuild_surface: true,
            reapply_insets: true,
            ..Effects::none()
        }
    }

    fn terminate(&mut self) -> Effects {
        if self.phase == Phase::Terminating {
            return Effects::none();
        }
        self.phase = Phase::Terminating;
        let save = !self.saved_since_foreground;
        self.saved_since_foreground = true;
        Effects {
            quit: true,
            pause_updates: true,
            reset_gestures: true,
            save_checkpoint: save,
            ..Effects::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_pauses_saves_once_and_resumes() {
        let mut life = Lifecycle::new();
        assert!(life.updates_allowed());

        let effects = life.handle(Event::WillEnterBackground);
        assert!(effects.pause_updates);
        assert!(effects.reset_gestures);
        assert!(effects.save_checkpoint, "WILL_ENTER_BACKGROUND saves");
        assert!(!life.updates_allowed());
        assert_eq!(life.phase(), Phase::Background);

        let duplicate = life.handle(Event::WillEnterBackground);
        assert_eq!(duplicate, Effects::none(), "a second WILL is idempotent");

        let did = life.handle(Event::DidEnterBackground);
        assert!(did.rebuild_surface, "background loses the surface");
        assert!(!did.pause_updates, "already paused");

        assert_eq!(life.handle(Event::WillEnterForeground), Effects::none());
        assert!(!life.updates_allowed(), "WILL alone does not resume");

        let resumed = life.handle(Event::DidEnterForeground);
        assert!(resumed.resume_updates);
        assert!(resumed.reset_gestures);
        assert!(resumed.rebuild_surface);
        assert!(resumed.reapply_insets);
        assert!(life.updates_allowed());

        // A second background+resume saves again: the checkpoint belongs to the
        // new foreground session.
        let background = life.handle(Event::WillEnterBackground);
        assert!(background.save_checkpoint);
    }

    #[test]
    fn did_enter_background_without_will_still_saves_and_pauses() {
        let mut life = Lifecycle::new();
        let effects = life.handle(Event::DidEnterBackground);
        assert!(effects.pause_updates);
        assert!(effects.save_checkpoint);
        assert!(effects.rebuild_surface);
    }

    #[test]
    fn duplicate_did_enter_foreground_is_a_noop() {
        let mut life = Lifecycle::new();
        assert_eq!(life.handle(Event::DidEnterForeground), Effects::none());
    }

    #[test]
    fn focus_loss_cancels_the_gesture_but_keeps_the_sim_running() {
        let mut life = Lifecycle::new();
        assert!(life.focused());
        let effects = life.handle(Event::FocusLost);
        assert!(effects.reset_gestures);
        assert!(!effects.pause_updates);
        assert!(
            life.updates_allowed(),
            "focus loss is not a background pause"
        );
        assert!(!life.focused());
        assert_eq!(life.handle(Event::FocusLost), Effects::none());
        assert_eq!(life.handle(Event::FocusGained), Effects::none());
        assert!(life.focused());
    }

    #[test]
    fn display_change_resets_gestures_and_rebuilds() {
        let mut life = Lifecycle::new();
        let effects = life.handle(Event::DisplayChanged);
        assert!(effects.reset_gestures);
        assert!(effects.rebuild_surface);
        assert!(effects.reapply_insets);
        assert_eq!(effects.pause_updates, false);
    }

    #[test]
    fn resize_and_safe_area_effects_are_distinct() {
        let mut life = Lifecycle::new();
        let resize = life.handle(Event::WindowResized);
        assert!(resize.reset_gestures && resize.rebuild_surface);
        assert!(!resize.reapply_insets);
        let insets = life.handle(Event::SafeAreaChanged);
        assert!(insets.reapply_insets);
        assert!(!insets.reset_gestures && !insets.rebuild_surface);
    }

    #[test]
    fn low_memory_is_reported_without_touching_the_lifecycle() {
        let mut life = Lifecycle::new();
        let effects = life.handle(Event::LowMemory);
        assert!(effects.low_memory);
        assert_eq!(life.phase(), Phase::Foreground);
        assert!(life.updates_allowed());
    }

    #[test]
    fn terminating_saves_once_and_ignores_every_later_event() {
        let mut life = Lifecycle::new();
        let effects = life.handle(Event::Terminating);
        assert!(effects.quit);
        assert!(effects.pause_updates);
        assert!(effects.reset_gestures);
        assert!(effects.save_checkpoint);
        assert_eq!(life.phase(), Phase::Terminating);
        assert!(!life.updates_allowed());
        for event in [
            Event::DidEnterForeground,
            Event::WillEnterForeground,
            Event::DisplayChanged,
            Event::WindowResized,
            Event::FocusGained,
        ] {
            assert_eq!(life.handle(event), Effects::none(), "{event:?}");
        }
        // Terminating after a background save does not write twice.
        let mut life = Lifecycle::new();
        life.handle(Event::WillEnterBackground);
        let second = life.handle(Event::Terminating);
        assert!(!second.save_checkpoint);
        assert!(second.quit);
    }

    #[test]
    fn sdl_event_mapping_covers_the_platform_events() {
        use crate::sdl::event as e;
        let table = [
            (e::TERMINATING, Event::Terminating),
            (e::LOW_MEMORY, Event::LowMemory),
            (e::WILL_ENTER_BACKGROUND, Event::WillEnterBackground),
            (e::DID_ENTER_BACKGROUND, Event::DidEnterBackground),
            (e::WILL_ENTER_FOREGROUND, Event::WillEnterForeground),
            (e::DID_ENTER_FOREGROUND, Event::DidEnterForeground),
            (e::WINDOW_FOCUS_LOST, Event::FocusLost),
            (e::WINDOW_FOCUS_GAINED, Event::FocusGained),
            (e::DISPLAY_ORIENTATION, Event::DisplayChanged),
            (e::WINDOW_DISPLAY_CHANGED, Event::DisplayChanged),
            (e::WINDOW_DISPLAY_SCALE_CHANGED, Event::DisplayChanged),
            (e::WINDOW_RESIZED, Event::WindowResized),
            (e::WINDOW_PIXEL_SIZE_CHANGED, Event::WindowResized),
            (e::WINDOW_SAFE_AREA_CHANGED, Event::SafeAreaChanged),
            (e::WINDOW_DESTROYED, Event::Terminating),
        ];
        for (sdl, expected) in table {
            assert_eq!(Lifecycle::event_from_sdl(sdl), Some(expected), "0x{sdl:x}");
        }
        for ignored in [
            e::QUIT,
            e::WINDOW_OCCLUDED,
            e::WINDOW_CLOSE_REQUESTED,
            e::KEY_DOWN,
        ] {
            assert_eq!(Lifecycle::event_from_sdl(ignored), None, "0x{ignored:x}");
        }
        let mut life = Lifecycle::new();
        assert_eq!(
            life.handle_sdl(e::LOW_MEMORY),
            Some(Effects {
                low_memory: true,
                ..Effects::none()
            })
        );
    }

    #[test]
    fn merge_combines_effects() {
        let a = Effects {
            pause_updates: true,
            ..Effects::none()
        };
        let b = Effects {
            save_checkpoint: true,
            ..Effects::none()
        };
        let merged = a.merge(b);
        assert!(merged.pause_updates && merged.save_checkpoint);
    }
}

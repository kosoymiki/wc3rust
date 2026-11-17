//! SDL3 ABI values consumed by the Android platform layer.
//!
//! The values are pinned against the SDL tree that produced the shipped
//! `third_party/sdl3-android-build/libSDL3.so` (`SDL-3.5.0-4ca1b1c`, donor
//! commit `4ca1b1c2d084b59ca72c87f36a18fc1a3898f424`), so the platform layer
//! never re-guesses an enum by hand. Every constant below cites the donor
//! header and line; [`tests`] keeps the numbers honest on the host.
//!
//! The old `sdl3-sys-rs` donor snapshot predates the SDL 3.2 event rename
//! (`SDL_APP_DIDENTERFOREGROUND` and friends) and is **not** a source of
//! truth for this runtime.

/// `SDL_EventType` app/lifecycle values (`include/SDL3/SDL_events.h:90-113`).
pub mod event {
    pub const QUIT: u32 = 0x100;
    /// `SDL_events.h:93` — called on Android in `onDestroy()`.
    pub const TERMINATING: u32 = 0x101;
    /// `SDL_events.h:97` — called on Android in `onTrimMemory()`.
    pub const LOW_MEMORY: u32 = 0x102;
    /// `SDL_events.h:101` — called on Android in `onPause()`.
    pub const WILL_ENTER_BACKGROUND: u32 = 0x103;
    /// `SDL_events.h:105` — called on Android in `onPause()`.
    pub const DID_ENTER_BACKGROUND: u32 = 0x104;
    /// `SDL_events.h:109` — called on Android in `onResume()`.
    pub const WILL_ENTER_FOREGROUND: u32 = 0x105;
    /// `SDL_events.h:113` — called on Android in `onResume()`.
    pub const DID_ENTER_FOREGROUND: u32 = 0x106;

    /// `SDL_events.h:124` — display orientation changed (Android rotation).
    pub const DISPLAY_ORIENTATION: u32 = 0x151;

    /// `SDL_events.h:139` — the window is shown and ready to draw.
    pub const WINDOW_SHOWN: u32 = 0x202;
    /// `SDL_events.h:140`.
    pub const WINDOW_HIDDEN: u32 = 0x203;
    /// `SDL_events.h:141` — Android sends this when the native surface is
    /// created; it is the only safe moment to start presenting frames.
    pub const WINDOW_EXPOSED: u32 = 0x204;
    /// `SDL_events.h:143`.
    pub const WINDOW_RESIZED: u32 = 0x206;
    /// `SDL_events.h:145`.
    pub const WINDOW_MINIMIZED: u32 = 0x209;
    /// `SDL_events.h:147`.
    pub const WINDOW_RESTORED: u32 = 0x20B;
    /// `SDL_events.h:144`.
    pub const WINDOW_PIXEL_SIZE_CHANGED: u32 = 0x207;
    /// `SDL_events.h:151`.
    pub const WINDOW_FOCUS_GAINED: u32 = 0x20E;
    /// `SDL_events.h:152`.
    pub const WINDOW_FOCUS_LOST: u32 = 0x20F;
    /// `SDL_events.h:153`.
    pub const WINDOW_CLOSE_REQUESTED: u32 = 0x210;
    /// `SDL_events.h:156`.
    pub const WINDOW_DISPLAY_CHANGED: u32 = 0x213;
    /// `SDL_events.h:157`.
    pub const WINDOW_DISPLAY_SCALE_CHANGED: u32 = 0x214;
    /// `SDL_events.h:158`.
    pub const WINDOW_SAFE_AREA_CHANGED: u32 = 0x215;
    /// `SDL_events.h:159`.
    pub const WINDOW_OCCLUDED: u32 = 0x216;
    /// `SDL_events.h:162`.
    pub const WINDOW_DESTROYED: u32 = 0x219;

    /// `SDL_events.h:172`.
    pub const KEY_DOWN: u32 = 0x300;
    /// `SDL_events.h:173`.
    pub const KEY_UP: u32 = 0x301;

    /// `SDL_events.h:187`.
    pub const MOUSE_MOTION: u32 = 0x400;
    /// `SDL_events.h:188`.
    pub const MOUSE_BUTTON_DOWN: u32 = 0x401;
    /// `SDL_events.h:189`.
    pub const MOUSE_BUTTON_UP: u32 = 0x402;
    /// `SDL_events.h:190`.
    pub const MOUSE_WHEEL: u32 = 0x403;

    /// `SDL_events.h:213`.
    pub const GAMEPAD_AXIS_MOTION: u32 = 0x650;
    /// `SDL_events.h:214`.
    pub const GAMEPAD_BUTTON_DOWN: u32 = 0x651;
    /// `SDL_events.h:215`.
    pub const GAMEPAD_BUTTON_UP: u32 = 0x652;

    /// `SDL_events.h:228`.
    pub const FINGER_DOWN: u32 = 0x700;
    /// `SDL_events.h:229`.
    pub const FINGER_UP: u32 = 0x701;
    /// `SDL_events.h:230`.
    pub const FINGER_MOTION: u32 = 0x702;
    /// `SDL_events.h:231` — Android `ACTION_CANCEL`
    /// (`src/video/android/SDL_androidtouch.c:102-103`).
    pub const FINGER_CANCELED: u32 = 0x703;
}

/// `SDL_Keymod` bits (`include/SDL3/SDL_keycode.h:329-345`).
pub mod keymod {
    pub const NONE: u16 = 0x0000;
    pub const LSHIFT: u16 = 0x0001;
    pub const RSHIFT: u16 = 0x0002;
    pub const SHIFT: u16 = 0x0003;
    pub const LCTRL: u16 = 0x0040;
    pub const RCTRL: u16 = 0x0080;
    pub const CTRL: u16 = 0x00c0;
    pub const LALT: u16 = 0x0100;
    pub const RALT: u16 = 0x0200;
    pub const ALT: u16 = 0x0300;
}

/// Physical `SDL_Scancode` values used by the WC3 hotkey table
/// (`include/SDL3/SDL_scancode.h:63-105`).
pub mod scancode {
    pub const A: u32 = 4;
    pub const Z: u32 = 29;
    pub const DIGIT_1: u32 = 30;
    pub const DIGIT_9: u32 = 38;
    pub const DIGIT_0: u32 = 39;
    pub const RETURN: u32 = 40;
    pub const ESCAPE: u32 = 41;
    pub const TAB: u32 = 43;
    pub const SPACE: u32 = 44;
}

/// Android hints (`include/SDL3/SDL_hints.h:86,101,118`).
pub mod hint {
    use core::ffi::CStr;

    /// `SDL_hints.h:86` — default "0": SDL terminates instead of recreating the
    /// activity, so the game restarts from its own checkpoint.
    pub const ANDROID_ALLOW_RECREATE_ACTIVITY: &CStr = c"SDL_ANDROID_ALLOW_RECREATE_ACTIVITY";
    /// `SDL_hints.h:101` — default "1". The platform layer sets "0" so the
    /// native loop keeps pumping while paused and can write its checkpoint.
    pub const ANDROID_BLOCK_ON_PAUSE: &CStr = c"SDL_ANDROID_BLOCK_ON_PAUSE";
    /// `SDL_hints.h:118` — default "1": low-latency audio for games.
    pub const ANDROID_LOW_LATENCY_AUDIO: &CStr = c"SDL_ANDROID_LOW_LATENCY_AUDIO";
}

/// True while `modifiers` carries either Shift key.
pub fn shift_down(modifiers: u16) -> bool {
    modifiers & keymod::SHIFT != 0
}

/// True while `modifiers` carries either Ctrl key.
pub fn ctrl_down(modifiers: u16) -> bool {
    modifiers & keymod::CTRL != 0
}

/// The WC3 hotkey character for a physical SDL scancode. Layout independent:
/// Shift never turns group `1` into `!`.
pub fn scancode_char(scancode: u32) -> Option<char> {
    match scancode {
        4..=29 => Some((b'a' + (scancode as u8 - 4)) as char),
        30..=39 => Some(if scancode == 39 {
            '0'
        } else {
            (b'1' + (scancode as u8 - 30)) as char
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_values_match_sdl_3_5_0() {
        // Donor: donors/SDL/include/SDL3/SDL_events.h (commit 4ca1b1c).
        assert_eq!(event::QUIT, 0x100);
        assert_eq!(event::TERMINATING, 0x101);
        assert_eq!(event::LOW_MEMORY, 0x102);
        assert_eq!(event::WILL_ENTER_BACKGROUND, 0x103);
        assert_eq!(event::DID_ENTER_BACKGROUND, 0x104);
        assert_eq!(event::WILL_ENTER_FOREGROUND, 0x105);
        assert_eq!(event::DID_ENTER_FOREGROUND, 0x106);
        assert_eq!(event::DISPLAY_ORIENTATION, 0x151);
        assert_eq!(event::WINDOW_RESIZED, 0x206);
        assert_eq!(event::WINDOW_PIXEL_SIZE_CHANGED, 0x207);
        assert_eq!(event::WINDOW_FOCUS_GAINED, 0x20e);
        assert_eq!(event::WINDOW_FOCUS_LOST, 0x20f);
        assert_eq!(event::WINDOW_CLOSE_REQUESTED, 0x210);
        assert_eq!(event::WINDOW_DISPLAY_CHANGED, 0x213);
        assert_eq!(event::WINDOW_DISPLAY_SCALE_CHANGED, 0x214);
        assert_eq!(event::WINDOW_SAFE_AREA_CHANGED, 0x215);
        assert_eq!(event::WINDOW_OCCLUDED, 0x216);
        assert_eq!(event::WINDOW_DESTROYED, 0x219);
        assert_eq!(event::KEY_DOWN, 0x300);
        assert_eq!(event::KEY_UP, 0x301);
        assert_eq!(event::MOUSE_MOTION, 0x400);
        assert_eq!(event::MOUSE_BUTTON_DOWN, 0x401);
        assert_eq!(event::MOUSE_BUTTON_UP, 0x402);
        assert_eq!(event::MOUSE_WHEEL, 0x403);
        assert_eq!(event::GAMEPAD_AXIS_MOTION, 0x650);
        assert_eq!(event::GAMEPAD_BUTTON_DOWN, 0x651);
        assert_eq!(event::GAMEPAD_BUTTON_UP, 0x652);
        assert_eq!(event::FINGER_DOWN, 0x700);
        assert_eq!(event::FINGER_UP, 0x701);
        assert_eq!(event::FINGER_MOTION, 0x702);
        assert_eq!(event::FINGER_CANCELED, 0x703);
    }

    #[test]
    fn modifier_masks_match_sdl_keycode_h() {
        // Donor: donors/SDL/include/SDL3/SDL_keycode.h (commit 4ca1b1c).
        assert_eq!(keymod::SHIFT, keymod::LSHIFT | keymod::RSHIFT);
        assert_eq!(keymod::CTRL, keymod::LCTRL | keymod::RCTRL);
        assert_eq!(keymod::ALT, keymod::LALT | keymod::RALT);
        assert!(shift_down(keymod::LSHIFT | keymod::RCTRL));
        assert!(!shift_down(keymod::CTRL));
        assert!(ctrl_down(keymod::RCTRL | keymod::LSHIFT));
    }

    #[test]
    fn scancode_table_is_layout_independent() {
        assert_eq!(scancode_char(scancode::A), Some('a'));
        assert_eq!(scancode_char(scancode::Z), Some('z'));
        assert_eq!(scancode_char(scancode::DIGIT_1), Some('1'));
        assert_eq!(scancode_char(scancode::DIGIT_9), Some('9'));
        assert_eq!(scancode_char(scancode::DIGIT_0), Some('0'));
        assert_eq!(scancode_char(scancode::ESCAPE), None);
        assert_eq!(scancode_char(scancode::TAB), None);
        assert_eq!(scancode_char(scancode::RETURN), None);
    }

    #[test]
    fn hint_names_match_the_donor_header() {
        assert_eq!(
            hint::ANDROID_ALLOW_RECREATE_ACTIVITY.to_bytes(),
            b"SDL_ANDROID_ALLOW_RECREATE_ACTIVITY"
        );
        assert_eq!(
            hint::ANDROID_BLOCK_ON_PAUSE.to_bytes(),
            b"SDL_ANDROID_BLOCK_ON_PAUSE"
        );
        assert_eq!(
            hint::ANDROID_LOW_LATENCY_AUDIO.to_bytes(),
            b"SDL_ANDROID_LOW_LATENCY_AUDIO"
        );
    }
}

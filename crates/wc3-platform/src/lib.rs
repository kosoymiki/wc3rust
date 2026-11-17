//! Minimal SDL3 platform bootstrap.
//!
//! P0 intentionally exposes only what the smoke test needs: initialization, version,
//! error text and shutdown. The full event/window/audio layer arrives in the next
//! P0 slice; no stub functions are declared here.

use std::ffi::{CStr, c_char};
use std::fmt;

const SDL_INIT_VIDEO: u32 = 0x0000_0020;
const SDL_INIT_EVENTS: u32 = 0x0000_4000;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdlVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

// SDL3's Android video/log path references liblog symbols. Termux is an Android
// userland without the app-process liblog implementation; provide no-op shims so
// the offscreen/host smoke can link. Real Android builds link liblog normally.
#[unsafe(no_mangle)]
pub extern "C" fn __android_log_print(_prio: i32, _tag: *const c_char, _fmt: *const c_char) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __android_log_write(
    _prio: i32,
    _tag: *const c_char,
    _text: *const c_char,
) -> i32 {
    0
}

unsafe extern "C" {
    fn SDL_Init(flags: u32) -> i32;
    fn SDL_Quit();
    fn SDL_GetVersion() -> i32;
    fn SDL_GetError() -> *const c_char;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformError(pub String);

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlatformError {}

/// Owns the SDL3 initialization. Dropping it calls `SDL_Quit`.
pub struct Sdl3 {
    version: SdlVersion,
}

impl Sdl3 {
    /// Initialize SDL3 video+events with the offscreen driver.
    ///
    /// `SDL_VIDEODRIVER` is set only if the caller has not chosen one; on a real
    /// Android/desktop app the caller should set `android`/`x11`/`wayland`.
    pub fn init() -> Result<Self, PlatformError> {
        if std::env::var_os("SDL_VIDEODRIVER").is_none() {
            // SAFETY: called during single-threaded process startup before any
            // SDL threads exist. Rust 2024 marks set_var unsafe for the general
            // case; this call is confined to initialization.
            unsafe {
                std::env::set_var("SDL_VIDEODRIVER", "offscreen");
            }
        }
        let rc = unsafe { SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS) };
        if rc != 0 {
            return Err(PlatformError(error_text()));
        }
        let packed = unsafe { SDL_GetVersion() };
        let version = SdlVersion {
            major: (packed / 1_000_000) as u8,
            minor: ((packed / 1_000) % 1_000) as u8,
            patch: (packed % 1_000) as u8,
        };
        Ok(Self { version })
    }

    pub fn version(&self) -> SdlVersion {
        self.version
    }

    pub fn version_string(&self) -> String {
        format!(
            "{}.{}.{}",
            self.version.major, self.version.minor, self.version.patch
        )
    }

    pub fn error_text() -> String {
        error_text()
    }
}

impl Drop for Sdl3 {
    fn drop(&mut self) {
        unsafe { SDL_Quit() };
    }
}

fn error_text() -> String {
    let ptr = unsafe { SDL_GetError() };
    if ptr.is_null() {
        return "SDL3 error (no message)".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

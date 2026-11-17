//! Herofolends-RS Android entry point.
//!
//! Loaded by SDL3's `SDLActivity` as `libmain.so`; SDL3 calls the exported
//! `SDL_main` on its dedicated thread. The app creates the SDL3 Vulkan window,
//! builds a wgpu surface from its `ANativeWindow`, loads the *player's*
//! `War3.mpq` and map from the app's external files directory, and runs the
//! playable RTS game loop: real MDX units, terrain, selection, orders, combat,
//! economy, AI, HUD, touch camera and command card.

#![cfg(target_os = "android")]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use glam::Vec2;
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use wc3_game::sim_bridge::{CheckpointMeta, MapStamp, ResumeReject};
use wc3_game::{Assets, Game, Renderer};
use wc3_input::lifecycle::{Effects, Lifecycle};
use wc3_input::sdl::{event as sdl_event, hint as sdl_hint};
use wc3_input::session::{InputEvent, InputSink, InputState};

const SDL_INIT_VIDEO: u32 = 0x0000_0020;
const SDL_WINDOW_FULLSCREEN: u64 = 0x0000_0001;
const SDL_WINDOW_RESIZABLE: u64 = 0x0000_0020;
const SDL_WINDOW_HIGH_PIXEL_DENSITY: u64 = 0x0000_2000;
const SDL_WINDOW_VULKAN: u64 = 0x1000_0000;

// SDL 3.5.0 ABI values live in `wc3_input::sdl`, pinned against the donor
// headers and the shipped `libSDL3.so` (`SDL-3.5.0-4ca1b1c`).
const SDL_EVENT_QUIT: u32 = sdl_event::QUIT;
const SDL_EVENT_WINDOW_SHOWN: u32 = sdl_event::WINDOW_SHOWN;
const SDL_EVENT_WINDOW_HIDDEN: u32 = sdl_event::WINDOW_HIDDEN;
const SDL_EVENT_WINDOW_EXPOSED: u32 = sdl_event::WINDOW_EXPOSED;
const SDL_EVENT_WINDOW_RESIZED: u32 = sdl_event::WINDOW_RESIZED;
const SDL_EVENT_WINDOW_MINIMIZED: u32 = sdl_event::WINDOW_MINIMIZED;
const SDL_EVENT_WINDOW_RESTORED: u32 = sdl_event::WINDOW_RESTORED;
const SDL_EVENT_WINDOW_OCCLUDED: u32 = sdl_event::WINDOW_OCCLUDED;
const SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED: u32 = sdl_event::WINDOW_PIXEL_SIZE_CHANGED;
const SDL_EVENT_WINDOW_CLOSE_REQUESTED: u32 = sdl_event::WINDOW_CLOSE_REQUESTED;
const SDL_EVENT_WINDOW_SAFE_AREA_CHANGED: u32 = sdl_event::WINDOW_SAFE_AREA_CHANGED;
const SDL_EVENT_KEY_DOWN: u32 = sdl_event::KEY_DOWN;
const SDL_EVENT_KEY_UP: u32 = sdl_event::KEY_UP;
const SDL_EVENT_MOUSE_MOTION: u32 = sdl_event::MOUSE_MOTION;
const SDL_EVENT_MOUSE_BUTTON_DOWN: u32 = sdl_event::MOUSE_BUTTON_DOWN;
const SDL_EVENT_MOUSE_BUTTON_UP: u32 = sdl_event::MOUSE_BUTTON_UP;
const SDL_EVENT_FINGER_DOWN: u32 = sdl_event::FINGER_DOWN;
const SDL_EVENT_FINGER_UP: u32 = sdl_event::FINGER_UP;
const SDL_EVENT_FINGER_MOTION: u32 = sdl_event::FINGER_MOTION;
const SDL_EVENT_FINGER_CANCELED: u32 = sdl_event::FINGER_CANCELED;

const SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER: &CStr = c"SDL.window.android.window";

#[repr(C)]
struct SdlWindow {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SdlRect {
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
}

#[repr(C, align(8))]
struct SdlEvent {
    bytes: [u8; 128],
}

impl SdlEvent {
    fn new() -> Self {
        Self { bytes: [0; 128] }
    }

    fn event_type(&self) -> u32 {
        u32_at(&self.bytes, 0)
    }

    fn finger_id(&self) -> i64 {
        i64::from_ne_bytes(self.bytes[24..32].try_into().expect("8 bytes"))
    }

    fn finger_x(&self) -> f32 {
        f32_at(&self.bytes, 32)
    }

    fn finger_y(&self) -> f32 {
        f32_at(&self.bytes, 36)
    }

    fn mouse_x(&self) -> f32 {
        f32_at(&self.bytes, 28)
    }

    fn mouse_y(&self) -> f32 {
        f32_at(&self.bytes, 32)
    }

    fn mouse_which(&self) -> u32 {
        u32_at(&self.bytes, 20)
    }

    fn mouse_down(&self) -> bool {
        self.bytes[25] != 0
    }

    /// `SDL_KeyboardEvent.scancode` (offset 24) — physical key, layout-neutral.
    fn key_scancode(&self) -> u32 {
        u32_at(&self.bytes, 24)
    }

    /// `SDL_KeyboardEvent.mod` (offset 32, Uint16).
    fn key_mod(&self) -> u16 {
        u16::from_ne_bytes(self.bytes[32..34].try_into().expect("2 bytes"))
    }

    /// `SDL_KeyboardEvent.repeat` (offset 37) — held-key repeats never re-fire
    /// a hotkey or a group assignment.
    fn key_repeat(&self) -> bool {
        self.bytes[37] != 0
    }
}

/// `SDL_EventFilter`: `bool (*)(void *userdata, SDL_Event *event)`.
type SdlEventFilter = unsafe extern "C" fn(*mut c_void, *mut SdlEvent) -> bool;

/// SDL 3.5.0 does not queue the app lifecycle events; they are delivered only
/// to `SDL_AddEventWatch` callbacks (`src/events/SDL_events.c:2031-2055` in the
/// pinned donor). The watcher runs on the SDL event thread, so it only appends
/// the event type to this small queue; the render loop drains it in order.
static LIFECYCLE_QUEUE: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// The live `Game`, registered after construction. It is only dereferenced
/// inside the event watcher while the render loop is blocked in
/// `SDL_PumpEvents`, which is exactly SDL's documented lifecycle-handling
/// point. Cleared before the game is dropped.
static GAME_FOR_CHECKPOINT: AtomicPtr<Game> = AtomicPtr::new(ptr::null_mut());
/// True when the watcher already wrote the save for the current transition.
static WATCHER_SAVED: AtomicBool = AtomicBool::new(false);
/// Tick of the last successful checkpoint write. A second save request for the
/// same tick (watcher + lifecycle effect, or two pause events in one frame)
/// produces identical bytes and is skipped instead of storming the disk.
static LAST_SAVED_TICK: AtomicU64 = AtomicU64::new(u64::MAX);

unsafe extern "C" fn on_sdl_event(_userdata: *mut c_void, event: *mut SdlEvent) -> bool {
    if event.is_null() {
        return true;
    }
    // SAFETY: SDL passes a live SDL_Event for the duration of the call.
    let event_type = unsafe { (*event).event_type() };
    if Lifecycle::event_from_sdl(event_type).is_some()
        && let Ok(mut queue) = LIFECYCLE_QUEUE.lock()
        && queue.len() < 64
    {
        queue.push(event_type);
    }
    if event_type == sdl_event::WILL_ENTER_BACKGROUND || event_type == sdl_event::TERMINATING {
        let game = GAME_FOR_CHECKPOINT.load(Ordering::Acquire);
        if !game.is_null() {
            // SAFETY: the watcher runs on the SDL main thread while the render
            // loop is inside SDL_PumpEvents, so nothing else borrows the game.
            save_checkpoint(unsafe { &*game });
            WATCHER_SAVED.store(true, Ordering::Release);
        }
    }
    true
}

fn drain_lifecycle_events() -> Vec<u32> {
    match LIFECYCLE_QUEUE.lock() {
        Ok(mut queue) => std::mem::take(&mut *queue),
        Err(_) => Vec::new(),
    }
}

// Physical SDL scancode -> hotkey character lives in the input layer
// (`wc3_input::scancode_char`) so it is unit-tested with the rest of the
// routing table.

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes"))
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_ne_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes"))
}

/// SDL fingers are normalized to the window (0..1); the router/HUD work in
/// pixel coordinates (`SDL_GetWindowSizeInPixels`).
fn finger_pos(event: &SdlEvent, config: &wgpu::SurfaceConfiguration) -> Vec2 {
    Vec2::new(
        event.finger_x() * config.width as f32,
        event.finger_y() * config.height as f32,
    )
}

unsafe extern "C" {
    fn SDL_Init(flags: u32) -> bool;
    fn SDL_Quit();
    fn SDL_CreateWindow(
        title: *const c_char,
        width: c_int,
        height: c_int,
        flags: u64,
    ) -> *mut SdlWindow;
    fn SDL_DestroyWindow(window: *mut SdlWindow);
    fn SDL_GetWindowProperties(window: *mut SdlWindow) -> u32;
    fn SDL_GetPointerProperty(
        properties: u32,
        name: *const c_char,
        default_value: *mut c_void,
    ) -> *mut c_void;
    fn SDL_GetWindowSizeInPixels(
        window: *mut SdlWindow,
        width: *mut c_int,
        height: *mut c_int,
    ) -> bool;
    fn SDL_GetWindowSize(
        window: *mut SdlWindow,
        width: *mut c_int,
        height: *mut c_int,
    ) -> bool;
    fn SDL_SetWindowFullscreen(window: *mut SdlWindow, fullscreen: bool) -> bool;
    fn SDL_SetWindowFullscreenMode(window: *mut SdlWindow, mode: *const c_void) -> bool;
    fn SDL_GetWindowFlags(window: *mut SdlWindow) -> u64;
    fn SDL_GetWindowSafeArea(window: *mut SdlWindow, rect: *mut SdlRect) -> bool;
    fn SDL_SyncWindow(window: *mut SdlWindow) -> bool;
    fn SDL_DisableScreenSaver() -> bool;
    fn SDL_GetAndroidJNIEnv() -> *mut c_void;
    fn SDL_GetAndroidActivity() -> *mut c_void;
    fn SDL_PollEvent(event: *mut SdlEvent) -> bool;
    fn SDL_PumpEvents();
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> bool;
    fn SDL_HasProperty(properties: u32, name: *const c_char) -> bool;
    fn SDL_GetError() -> *const c_char;
    fn SDL_GetCurrentVideoDriver() -> *const c_char;
    fn SDL_GetHint(name: *const c_char) -> *const c_char;
    fn SDL_LoadObject(path: *const c_char) -> *mut c_void;
    fn SDL_LoadFunction(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn SDL_UnloadObject(handle: *mut c_void);
    fn SDL_AddEventWatch(filter: SdlEventFilter, userdata: *mut c_void) -> bool;
    fn SDL_RemoveEventWatch(filter: SdlEventFilter, userdata: *mut c_void);
    fn SDL_Delay(milliseconds: u32);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VkExtensionProperties {
    extension_name: [u8; 256],
    spec_version: u32,
}

impl Default for VkExtensionProperties {
    fn default() -> Self {
        Self {
            extension_name: [0; 256],
            spec_version: 0,
        }
    }
}

impl VkExtensionProperties {
    fn name(&self) -> &str {
        let end = self
            .extension_name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.extension_name.len());
        std::str::from_utf8(&self.extension_name[..end]).unwrap_or("")
    }
}

type LogPrint = unsafe extern "C" fn(c_int, *const c_char, *const c_char, ...) -> c_int;
static LOG: OnceLock<Option<LogPrint>> = OnceLock::new();

fn log(message: &str) {
    let print = *LOG.get_or_init(|| {
        // SAFETY: liblog.so is part of every Android app namespace.
        unsafe {
            let handle = libc::dlopen(c"liblog.so".as_ptr(), libc::RTLD_NOW);
            if handle.is_null() {
                return None;
            }
            let symbol = libc::dlsym(handle, c"__android_log_print".as_ptr());
            if symbol.is_null() {
                None
            } else {
                Some(std::mem::transmute::<*mut c_void, LogPrint>(symbol))
            }
        }
    });
    if let Some(print) = print {
        let Ok(text) = CString::new(message) else {
            return;
        };
        // SAFETY: tag/fmt are static NUL-terminated strings; text lives for the call.
        unsafe { print(4, c"Herofolends".as_ptr(), c"%s".as_ptr(), text.as_ptr()) };
    }
}

fn sdl_error(context: &str) -> String {
    // SAFETY: SDL_GetError returns a NUL-terminated string (possibly empty).
    let text = unsafe { CStr::from_ptr(SDL_GetError()) }
        .to_string_lossy()
        .into_owned();
    format!("{context}: {text}")
}

/// Safe-area insets in window pixels: (left, top, right, bottom).
///
/// SDL's Android backend feeds `SDL_GetWindowSafeArea` from
/// `SDLSurface.onApplyWindowInsets` (system bars + gestures + cutout), so this
/// is the same rectangle the window manager considers safe for content.
fn window_insets(window: *mut SdlWindow) -> Option<(f32, f32, f32, f32)> {
    let mut rect = SdlRect::default();
    // SAFETY: window is live; rect is a valid out-pointer.
    if !unsafe { SDL_GetWindowSafeArea(window, &mut rect) } {
        return None;
    }
    if rect.w <= 0 || rect.h <= 0 {
        return None;
    }
    let mut window_w = 0i32;
    let mut window_h = 0i32;
    let mut pixel_w = 0i32;
    let mut pixel_h = 0i32;
    // SAFETY: window is live; all out-pointers are valid.
    unsafe {
        SDL_GetWindowSize(window, &mut window_w, &mut window_h);
        SDL_GetWindowSizeInPixels(window, &mut pixel_w, &mut pixel_h);
    }
    // Safe-area rectangles are in window coordinates; the renderer and finger
    // events are in pixels. On Android these are 1:1, but scale defensively.
    let scale_x = if window_w > 0 && pixel_w > 0 {
        pixel_w as f32 / window_w as f32
    } else {
        1.0
    };
    let scale_y = if window_h > 0 && pixel_h > 0 {
        pixel_h as f32 / window_h as f32
    } else {
        1.0
    };
    let left = rect.x as f32 * scale_x;
    let top = rect.y as f32 * scale_y;
    let right = (window_w - (rect.x + rect.w)).max(0) as f32 * scale_x;
    let bottom = (window_h - (rect.y + rect.h)).max(0) as f32 * scale_y;
    Some((left.max(0.0), top.max(0.0), right.max(0.0), bottom.max(0.0)))
}

fn apply_safe_insets(
    game: &mut Game,
    window: *mut SdlWindow,
    last: &mut Option<(f32, f32, f32, f32)>,
) {
    let Some((left, top, right, bottom)) = window_insets(window) else {
        return;
    };
    let changed = game.set_safe_insets(left, top, right, bottom);
    if changed || *last != Some((left, top, right, bottom)) {
        log(&format!(
            "GAME_SAFE_AREA left={left:.0} top={top:.0} right={right:.0} bottom={bottom:.0}"
        ));
        *last = Some((left, top, right, bottom));
    }
}

// ---------------------------------------------------------------- haptics
//
// SDL3 has no portable phone-vibration call (its haptic API targets gamepads),
// so press feedback goes through the Android `Vibrator` reached via SDL's own
// JNI bridge. Resolution happens once; a missing VIBRATE permission, a JNI
// exception, or an absent vibrator disables haptics permanently and logs why.
mod haptics {
    use super::*;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    // JNI function-table indices (JNINativeInterface order from jni.h).
    const EXCEPTION_OCCURRED: usize = 15;
    const EXCEPTION_CLEAR: usize = 17;
    const NEW_GLOBAL_REF: usize = 21;
    const DELETE_LOCAL_REF: usize = 23;
    const GET_OBJECT_CLASS: usize = 31;
    const GET_METHOD_ID: usize = 33;
    const CALL_OBJECT_METHOD: usize = 34;
    const CALL_BOOLEAN_METHOD: usize = 37;
    const CALL_VOID_METHOD: usize = 61;
    const NEW_STRING_UTF: usize = 167;

    const UNKNOWN: u8 = 0;
    const READY: u8 = 1;
    const UNAVAILABLE: u8 = 2;

    static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
    static VIBRATOR: AtomicUsize = AtomicUsize::new(0);
    static VIBRATE: AtomicUsize = AtomicUsize::new(0);

    /// A JNIEnv is a pointer to a pointer to the function table.
    unsafe fn table_fn<F: Copy>(env: *mut c_void, index: usize) -> F {
        let table = unsafe { *(env as *const *const *const c_void) };
        let function = unsafe { *table.add(index) };
        unsafe { std::mem::transmute_copy(&function) }
    }

    /// Resolve Activity.getSystemService("vibrator") and Vibrator.vibrate(J)V.
    unsafe fn resolve() -> Option<(usize, usize)> {
        let env = unsafe { SDL_GetAndroidJNIEnv() };
        if env.is_null() {
            return None;
        }
        // SDL documents this as a local reference that the caller must release.
        let activity = unsafe { SDL_GetAndroidActivity() };
        if activity.is_null() {
            return None;
        }
        unsafe {
            let get_object_class: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                table_fn(env, GET_OBJECT_CLASS);
            let get_method: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *const c_char,
                *const c_char,
            ) -> *mut c_void = table_fn(env, GET_METHOD_ID);
            let new_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void =
                table_fn(env, NEW_STRING_UTF);
            let call_object: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                ...
            ) -> *mut c_void = table_fn(env, CALL_OBJECT_METHOD);
            let call_boolean: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> u8 =
                table_fn(env, CALL_BOOLEAN_METHOD);
            let new_global: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                table_fn(env, NEW_GLOBAL_REF);
            let delete_local: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                table_fn(env, DELETE_LOCAL_REF);

            let activity_class = get_object_class(env, activity);
            let service = get_method(
                env,
                activity_class,
                c"getSystemService".as_ptr(),
                c"(Ljava/lang/String;)Ljava/lang/Object;".as_ptr(),
            );
            let mut resolved = None;
            if !service.is_null() {
                let name = new_string(env, c"vibrator".as_ptr());
                let vibrator = call_object(env, activity, service, name);
                if !name.is_null() {
                    delete_local(env, name);
                }
                if !vibrator.is_null() {
                    let vibrator_class = get_object_class(env, vibrator);
                    let has = get_method(
                        env,
                        vibrator_class,
                        c"hasVibrator".as_ptr(),
                        c"()Z".as_ptr(),
                    );
                    let vibrate = get_method(
                        env,
                        vibrator_class,
                        c"vibrate".as_ptr(),
                        c"(J)V".as_ptr(),
                    );
                    if !has.is_null() && !vibrate.is_null() && call_boolean(env, vibrator, has) != 0 {
                        resolved = Some((new_global(env, vibrator) as usize, vibrate as usize));
                    }
                    delete_local(env, vibrator_class);
                    delete_local(env, vibrator);
                }
            }
            delete_local(env, activity_class);
            delete_local(env, activity);

            let exception_occurred: unsafe extern "C" fn(*mut c_void) -> *mut c_void =
                table_fn(env, EXCEPTION_OCCURRED);
            let exception_clear: unsafe extern "C" fn(*mut c_void) = table_fn(env, EXCEPTION_CLEAR);
            let pending = exception_occurred(env);
            if !pending.is_null() {
                exception_clear(env);
                return None;
            }
            resolved
        }
    }

    /// Short press feedback. `milliseconds` is clamped so a bug cannot buzz
    /// the phone for a noticeable time.
    pub fn play(milliseconds: u64) {
        if milliseconds == 0 {
            return;
        }
        if STATE.load(Ordering::Relaxed) == UNKNOWN {
            match unsafe { resolve() } {
                Some((vibrator, vibrate)) => {
                    VIBRATOR.store(vibrator, Ordering::Relaxed);
                    VIBRATE.store(vibrate, Ordering::Relaxed);
                    STATE.store(READY, Ordering::Relaxed);
                    log("GAME_HAPTICS ready method=Vibrator.vibrate(J)V");
                }
                None => {
                    STATE.store(UNAVAILABLE, Ordering::Relaxed);
                    log("GAME_HAPTICS unavailable (no vibrator service or VIBRATE permission)");
                }
            }
        }
        if STATE.load(Ordering::Relaxed) != READY {
            return;
        }
        // SAFETY: cached global ref + method id live for the process; the SDL
        // main thread stays attached to the JVM.
        unsafe {
            let env = SDL_GetAndroidJNIEnv();
            if env.is_null() {
                return;
            }
            let call_void: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, ...) =
                table_fn(env, CALL_VOID_METHOD);
            call_void(
                env,
                VIBRATOR.load(Ordering::Relaxed) as *mut c_void,
                VIBRATE.load(Ordering::Relaxed) as *mut c_void,
                milliseconds.min(80) as i64,
            );
            let exception_occurred: unsafe extern "C" fn(*mut c_void) -> *mut c_void =
                table_fn(env, EXCEPTION_OCCURRED);
            let exception_clear: unsafe extern "C" fn(*mut c_void) = table_fn(env, EXCEPTION_CLEAR);
            let pending = exception_occurred(env);
            if !pending.is_null() {
                exception_clear(env);
                STATE.store(UNAVAILABLE, Ordering::Relaxed);
                log("GAME_HAPTICS disabled: vibrate() threw (VIBRATE permission missing?)");
            }
        }
    }
}

/// Point the Vulkan loader at a user-installed Turnip ICD when present.
/// The app ships a Khronos loader that honours this variable; the AOSP loader
/// ignores it and keeps using the vendor Adreno driver.
fn configure_turnip() {
    let json = "/data/data/org.herofolends.p0/files/turnip/freedreno_icd.aarch64.json";
    if std::path::Path::new(json).is_file() {
        // Preload our Khronos loader (SONAME libvulkan.so) before SDL opens
        // Vulkan, so `dlopen("libvulkan.so")` reuses it instead of the AOSP
        // loader, which ignores VK_ICD_FILENAMES.
        let loader = "/data/data/org.herofolends.p0/files/turnip/libvulkan.so";
        // SAFETY: called on SDL's main thread before any Vulkan object exists.
        unsafe {
            std::env::set_var("VK_ICD_FILENAMES", json);
            std::env::set_var("VK_DRIVER_FILES", json);
            // SDL loads the Vulkan loader by this hint. The system loader is
            // preloaded by Android's linker and ignores VK_ICD_FILENAMES, so
            // point SDL at the app's Khronos loader instead.
            SDL_SetHint(
                c"SDL_VULKAN_LIBRARY".as_ptr(),
                c"/data/data/org.herofolends.p0/files/turnip/libvulkan.so".as_ptr(),
            );
            // Capture stderr from the Vulkan loader for diagnostics.
            let fd = libc::open(
                c"/data/data/org.herofolends.p0/files/loader.log".as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
                0o600,
            );
            if fd >= 0 {
                libc::dup2(fd, 2);
                libc::close(fd);
            }
            std::env::set_var("VK_LOADER_DEBUG", "error,warn,info");
            let hint = CStr::from_ptr(SDL_GetHint(c"SDL_VULKAN_LIBRARY".as_ptr()))
                .to_string_lossy()
                .into_owned();
            log(&format!(
                "GAME_VULKAN turnip=1 loader={loader} icd={json} hint={hint}"
            ));
            // Direct probe with SDL's own loader calls: prove the loader this
            // process will use exposes VK_KHR_surface and the Android WSI.
            let handle = SDL_LoadObject(c"/data/data/org.herofolends.p0/files/turnip/libvulkan.so".as_ptr());
            if handle.is_null() {
                log(&format!("GAME_VULKAN probe_load_failed={}", sdl_error("SDL_LoadObject")));
            } else {
                let gipa = SDL_LoadFunction(handle, c"vkGetInstanceProcAddr".as_ptr());
                if gipa.is_null() {
                    log("GAME_VULKAN probe_no_gipa");
                } else {
                    let gipa: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void =
                        std::mem::transmute(gipa);
                    let enumerate = gipa(ptr::null_mut(), c"vkEnumerateInstanceExtensionProperties".as_ptr());
                    if enumerate.is_null() {
                        log("GAME_VULKAN probe_no_enumerate");
                    } else {
                        let enumerate: unsafe extern "C" fn(
                            *const c_char,
                            *mut u32,
                            *mut VkExtensionProperties,
                        ) -> i32 = std::mem::transmute(enumerate);
                        let mut count = 0u32;
                        let first = enumerate(ptr::null(), &mut count, ptr::null_mut());
                        let mut list = vec![VkExtensionProperties::default(); count as usize];
                        let second = if count > 0 {
                            enumerate(ptr::null(), &mut count, list.as_mut_ptr())
                        } else {
                            0
                        };
                        let mut surface = false;
                        let mut android = false;
                        for extension in &list {
                            let name = extension.name();
                            if name == "VK_KHR_surface" {
                                surface = true;
                            }
                            if name == "VK_KHR_android_surface" {
                                android = true;
                            }
                        }
                        log(&format!(
                            "GAME_VULKAN probe rc={first}/{second} count={count} surface={surface} android={android}"
                        ));
                    }
                }
                SDL_UnloadObject(handle);
            }
        }
    } else {
        log("GAME_VULKAN turnip=0 (no ICD; vendor driver)");
    }
}

fn data_dir() -> PathBuf {
    // Writes to `/sdcard/Android/data/<pkg>/files` are denied to this process
    // by scoped storage (observed: GAME_CHECKPOINT_FAILED ... Permission denied
    // os error 13), so the app's internal files dir is the writable home. The
    // external dir stays reachable for `adb push` during setup.
    let internal = PathBuf::from("/data/data/org.herofolends.p0/files");
    if internal.is_dir() {
        internal
    } else {
        PathBuf::from("/sdcard/Android/data/org.herofolends.p0/files")
    }
}

/// Checkpoint path that is actually writable on this device.
fn checkpoint_paths() -> [PathBuf; 2] {
    [
        checkpoint_path(),
        PathBuf::from("/sdcard/Android/data/org.herofolends.p0/files/herofolends-resume.sav"),
    ]
}

fn find_file(names: &[&str]) -> Option<PathBuf> {
    let bases = vec![
        PathBuf::from("/data/data/org.herofolends.p0/files"),
        data_dir(),
        PathBuf::from("/storage/emulated/0/Android/data/org.herofolends.p0/files"),
        PathBuf::from("/sdcard"),
    ];
    for base in bases {
        for name in names {
            let candidate = base.join(name);
            match std::fs::metadata(&candidate) {
                Ok(meta) if meta.is_file() => {
                    log(&format!("GAME_DATA_FOUND {}", candidate.display()));
                    return Some(candidate);
                }
                Ok(_) => log(&format!("GAME_DATA_NOT_FILE {}", candidate.display())),
                Err(error) => log(&format!(
                    "GAME_DATA_MISS {} ({})",
                    candidate.display(),
                    error.kind() as i32
                )),
            }
        }
    }
    None
}

// ------------------------------------------------------------ checkpoint
//
// `android-activity`'s `MainEvent::SaveState` has no SDL app-facing
// equivalent: SDLActivity keeps the native thread across configuration changes
// and terminates the process on activity recreation by default
// (`SDL_HINT_ANDROID_ALLOW_RECREATE_ACTIVITY` = 0). The SDL-native point to
// store state is `SDL_EVENT_WILL_ENTER_BACKGROUND`
// (`src/video/android/SDL_androidevents.c`: "The user should have handled state
// storage in SDL_EVENT_WILL_ENTER_BACKGROUND"). We write the sim there and
// resume it on the next launch; a clean quit removes the checkpoint.
//
// A save is only resumable on the same map bytes and in the same phase: the
// `Sim` payload is the sim layer's, while the map identity (path/length/FNV-1a
// digest) and the game phase travel in a `*.sav.meta` sidecar written next to
// it (`wc3_game::sim_bridge::CheckpointMeta`). Rejecting is the default for
// anything a previous build or another map wrote.

fn checkpoint_path() -> PathBuf {
    data_dir().join("herofolends-resume.sav")
}

/// `herofolends-resume.sav` -> `herofolends-resume.sav.meta`.
fn meta_path(save: &std::path::Path) -> PathBuf {
    let mut name = save.as_os_str().to_owned();
    name.push(".meta");
    PathBuf::from(name)
}

fn read_meta(save: &std::path::Path) -> Option<CheckpointMeta> {
    let text = std::fs::read_to_string(meta_path(save)).ok()?;
    CheckpointMeta::decode(&text)
}

fn write_meta(save: &std::path::Path, meta: &CheckpointMeta) -> std::io::Result<()> {
    std::fs::write(meta_path(save), meta.encode())
}

/// Identity of the map this process loaded (`Assets.debug` carries the
/// `map=<path>` line). Computed once per process: the map is fixed at launch.
fn cached_map_stamp(game: &Game) -> Option<&'static MapStamp> {
    static STAMP: OnceLock<Option<MapStamp>> = OnceLock::new();
    STAMP
        .get_or_init(|| {
            let path = game
                .assets
                .debug
                .iter()
                .find_map(|line| line.strip_prefix("map="))
                .map(PathBuf::from)?;
            MapStamp::of_file(&path)
        })
        .as_ref()
}

/// Mirrors `Game::phase` into the checkpoint phase vocabulary.
fn phase_name(phase: wc3_game::game::Phase) -> &'static str {
    match phase {
        wc3_game::game::Phase::Playing => "playing",
        wc3_game::game::Phase::Won => "won",
        wc3_game::game::Phase::Lost => "lost",
    }
}

fn save_checkpoint(game: &Game) {
    // A finished match is not resumable: drop the stale checkpoint instead of
    // writing one that would resurrect a won/lost board on the next launch.
    if game.phase != wc3_game::game::Phase::Playing {
        LAST_SAVED_TICK.store(u64::MAX, Ordering::Release);
        clear_checkpoint();
        log(&format!(
            "GAME_CHECKPOINT_SKIPPED reason=finished phase={}",
            phase_name(game.phase)
        ));
        return;
    }
    if !game.melee {
        return;
    }
    let Some(stamp) = cached_map_stamp(game) else {
        log("GAME_CHECKPOINT_SKIPPED reason=no_map");
        return;
    };
    // The watcher and the lifecycle effect can both ask for a save on one
    // background transition; an unchanged tick writes identical bytes, so the
    // second request is dropped instead of storming the disk.
    let tick = game.sim.tick;
    if LAST_SAVED_TICK.swap(tick, Ordering::AcqRel) == tick {
        log(&format!(
            "GAME_CHECKPOINT_SKIP reason=same_tick tick={tick}"
        ));
        return;
    }
    let grid = (game.sim.grid.width, game.sim.grid.height);
    let meta = CheckpointMeta::new(
        stamp.clone(),
        grid,
        phase_name(game.phase),
        tick,
        game.sim.save_digest(),
    );
    let mut last: Option<(PathBuf, String)> = None;
    for path in checkpoint_paths() {
        match game.sim.save_to_path(&path) {
            Ok(()) => {
                if let Err(error) = write_meta(&path, &meta) {
                    last = Some((meta_path(&path), format!("{error}")));
                    continue;
                }
                log(&format!(
                    "GAME_CHECKPOINT path={} digest={:016x} units={} tick={} map={} phase={}",
                    path.display(),
                    game.sim.save_digest(),
                    game.sim.units.iter().filter(|unit| unit.alive).count(),
                    tick,
                    meta.map.path,
                    meta.phase
                ));
                return;
            }
            Err(error) => last = Some((path, format!("{error:?}"))),
        }
    }
    LAST_SAVED_TICK.store(u64::MAX, Ordering::Release);
    if let Some((path, error)) = last {
        log(&format!(
            "GAME_CHECKPOINT_FAILED path={} err={error}",
            path.display()
        ));
    }
}

fn restore_checkpoint(game: &mut Game) {
    if !game.melee {
        return;
    }
    // The first candidate that carries readable metadata wins; a save without
    // metadata (written by an older build) is reported, never guessed at.
    let mut found: Option<(PathBuf, CheckpointMeta)> = None;
    let mut legacy: Option<PathBuf> = None;
    for candidate in checkpoint_paths() {
        if !candidate.is_file() {
            continue;
        }
        match read_meta(&candidate) {
            Some(meta) => {
                found = Some((candidate, meta));
                break;
            }
            None => {
                legacy.get_or_insert(candidate);
            }
        }
    }
    let Some((path, meta)) = found else {
        if let Some(path) = legacy {
            log(&format!(
                "GAME_RESUME_REJECT path={} reason=missing_meta",
                path.display()
            ));
        }
        return;
    };
    let stamp = cached_map_stamp(game).cloned();
    if let Err(reject) = meta.verdict(stamp.as_ref(), (game.sim.grid.width, game.sim.grid.height)) {
        if reject == ResumeReject::Finished {
            // Never resurrect a finished match: remove the stale pair.
            clear_checkpoint();
        }
        log(&format!(
            "GAME_RESUME_REJECT path={} reason={} saved_phase={} saved_grid={}x{} current_grid={}x{} saved_map={}",
            path.display(),
            reject.reason(),
            meta.phase,
            meta.grid.0,
            meta.grid.1,
            game.sim.grid.width,
            game.sim.grid.height,
            meta.map.path
        ));
        return;
    }
    match wc3_sim::Sim::from_save_path(&path) {
        Ok(sim) => {
            if !meta.matches(sim.tick, sim.save_digest()) {
                log(&format!(
                    "GAME_RESUME_REJECT path={} reason=digest saved_tick={} loaded_tick={} saved_digest={:016x} loaded_digest={:016x}",
                    path.display(),
                    meta.tick,
                    sim.tick,
                    meta.digest,
                    sim.save_digest()
                ));
                return;
            }
            game.sim = sim;
            // The phase is restored as `Playing` explicitly: `reset_scenario`
            // already produced it, and a rejected phase cannot reach here.
            game.phase = wc3_game::game::Phase::Playing;
            LAST_SAVED_TICK.store(game.sim.tick, Ordering::Release);
            log(&format!(
                "GAME_RESUME path={} units={} tick={} digest={:016x} map={} phase={}",
                path.display(),
                game.sim.units.iter().filter(|unit| unit.alive).count(),
                game.sim.tick,
                game.sim.save_digest(),
                meta.map.path,
                phase_name(game.phase)
            ));
        }
        Err(error) => log(&format!(
            "GAME_RESUME_FAILED path={} err={error:?}",
            path.display()
        )),
    }
}

fn clear_checkpoint() {
    for path in checkpoint_paths() {
        for file in [path.clone(), meta_path(&path)] {
            match std::fs::remove_file(&file) {
                Ok(()) => log(&format!("GAME_CHECKPOINT_CLEARED path={}", file.display())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => log(&format!(
                    "GAME_CHECKPOINT_CLEAR_FAILED path={} err={error}",
                    file.display()
                )),
            }
        }
    }
}

// ------------------------------------------------------------ input sink

/// The platform side of [`wc3_input::session::InputSink`]: every routed event
/// becomes the corresponding `Game` call. Keeping the mapping explicit means
/// the host-replayed input traces cover the exact sequence the device runs.
struct GameSink<'a> {
    game: &'a mut Game,
}

impl InputSink for GameSink<'_> {
    fn pointer_down(&mut self, at: Vec2) {
        self.game.pointer_down(at);
    }

    fn pointer_move(&mut self, at: Vec2) {
        self.game.pointer_move(at);
    }

    fn pointer_up(&mut self, at: Vec2) {
        self.game.pointer_up(at);
    }

    fn pointer_cancel(&mut self) {
        self.game.pointer_cancel();
    }

    fn pointer_resume_pan(&mut self, at: Vec2) {
        self.game.pointer_resume_pan(at);
    }

    fn pinch_begin(&mut self, distance: f32, centre: Vec2) {
        self.game.pinch_begin(distance, centre);
    }

    fn pinch_update(&mut self, distance: f32, centre: Vec2) {
        self.game.pinch_update(distance, centre);
    }

    fn pinch_end(&mut self, two_finger_tap: Option<Vec2>) {
        // `Game::pinch_end` consumes the router result; the two-finger tap is
        // logged here because the read-only game layer drops it (finding in
        // analysis/agent-touch/REPORT.md; the ready patch turns it into a
        // right-click once the game layer exposes one).
        self.game.pinch_end();
        if let Some(at) = two_finger_tap {
            log(&format!("GAME_TWO_FINGER_TAP {:.0},{:.0}", at.x, at.y));
        }
    }

    fn set_additive(&mut self, additive: bool) {
        self.game.set_additive(additive);
        log(&format!("GAME_SHIFT add={additive}"));
    }

    fn key_char(&mut self, key: char, ctrl: bool) {
        log(&format!("GAME_HOTKEY key={key} ctrl={ctrl}"));
        self.game.key_char(key, ctrl);
    }

    fn key_escape(&mut self, ctrl: bool) {
        log("GAME_HOTKEY key=escape");
        self.game.hotkey(wc3_input::Hotkey::Escape, ctrl);
    }

    fn cycle_subgroups(&mut self, reverse: bool) {
        log(&format!("GAME_TAB reverse={reverse}"));
        self.game.cycle_subgroups(reverse);
    }
}




/// Read the current `ANativeWindow` pointer from SDL.
///
/// Android recreates (or detaches) the window on orientation changes, activity
/// resume and fullscreen transitions; the pointer can change while the app runs,
/// so it must be re-read before any surface recreate/configure.
fn current_native_window(window: *mut SdlWindow) -> *mut c_void {
    let properties = unsafe { SDL_GetWindowProperties(window) };
    for _ in 0..20 {
        // SAFETY: SDL is initialised; pumping events lets the Android backend
        // publish the window property if it is still pending.
        unsafe { SDL_PumpEvents() };
        let native = unsafe {
            SDL_GetPointerProperty(
                properties,
                SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER.as_ptr(),
                ptr::null_mut(),
            )
        };
        if !native.is_null() {
            return native;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ptr::null_mut()
}

/// Build a wgpu surface over the given `ANativeWindow`.
fn create_android_surface(
    instance: &wgpu::Instance,
    native_window: *mut c_void,
) -> Result<wgpu::Surface<'static>, String> {
    let handle = AndroidNdkWindowHandle::new(
        NonNull::new(native_window).ok_or_else(|| "ANativeWindow is NULL".to_string())?,
    );
    // SAFETY: the ANativeWindow outlives the returned surface (SDL owns it until
    // the window is destroyed). Recreating it from the current pointer is the
    // supported way to follow an Android window recreation.
    unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::AndroidNdk(handle),
        })
    }
    .map_err(|error| format!("create_surface: {error}"))
}

/// Recover a surface whose configure failed (stale `ANativeWindow`).
///
/// Order: re-read the window pointer → guarded reconfigure (fast path) → on
/// failure recreate the wgpu surface from the *current* window and configure it.
/// Every configure is error-scoped, so the app can never `SIGABRT` here.
#[allow(clippy::too_many_arguments)]
fn recover_surface(
    instance: &wgpu::Instance,
    window: *mut SdlWindow,
    native_window: &mut *mut c_void,
    surface: &mut Option<wgpu::Surface<'static>>,
    device: &wgpu::Device,
    config: &mut wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    reason: &str,
) -> Result<(), String> {
    let current = current_native_window(window);
    let pointer_changed = current != *native_window;
    if pointer_changed {
        *native_window = current;
    }
    config.width = width.max(1);
    config.height = height.max(1);
    // A lifecycle event means Android detached and possibly rebuilt the SDL
    // window: the wgpu surface still wraps the *old* ANativeWindow, and the
    // replacement buffer queue can land at the same address (measured on the
    // device: identical pointer, then `dequeueBuffer` blocks forever and the
    // loop never presents again — the screen freezes on the last frame while
    // the sim stays at tick 0). Reconfigure-in-place is only valid for a pure
    // resize or a lost swapchain on a window that was never detached.
    let queue_rebuilt = reason == "lifecycle" || reason == "watchdog";
    if !pointer_changed && !queue_rebuilt {
        let live = surface.as_ref().ok_or("no live surface")?;
        match wc3_render::surface::reconfigure(device, live, config) {
            Ok(()) => {
                log(&format!(
                    "P0_SURFACE_RECONFIGURED reason={reason} {}x{}",
                    config.width, config.height
                ));
                return Ok(());
            }
            Err(error) => log(&format!("P0_SURFACE_STALE reason={reason} err={error}")),
        }
    }
    // Vulkan aborts the process with ERROR_NATIVE_WINDOW_IN_USE_KHR when a
    // second VkSurfaceKHR is created over a window that still owns one (device
    // capture 2026-11-17 10:25:32: `Abort message: 'AndroidSurface failed:
    // ERROR_NATIVE_WINDOW_IN_USE_KHR'`). Drop the old surface first, then build
    // the replacement over the live window.
    *surface = None;
    let fresh = create_android_surface(instance, *native_window)?;
    wc3_render::surface::configure_guarded(device, &fresh, config)
        .map_err(|error| format!("surface recover ({reason}) failed: {error}"))?;
    *surface = Some(fresh);
    log(&format!(
        "P0_SURFACE_RECOVERED reason={reason} {}x{} ptr=0x{:x}",
        config.width, config.height, *native_window as usize
    ));
    Ok(())
}

fn run() -> Result<(), String> {
    // Hints that must be set before SDL_Init (SDL reads them when the Android
    // backend initialises):
    //  - BLOCK_ON_PAUSE=1 (default, set explicitly): SDL blocks native pumping
    //    while paused and pauses the audio devices. The WILL_ENTER_BACKGROUND
    //    event is still delivered to the watcher *before* the block, and the
    //    watcher writes the checkpoint through `GAME_FOR_CHECKPOINT` — the
    //    documented SDL pattern ("do any life cycle handling in an event
    //    filter while the event was being queued").
    //  - LOW_LATENCY_AUDIO=1: the shipped default for games (AAudio low-latency
    //    path); set explicitly so the log proves the mode.
    //  - ALLOW_RECREATE_ACTIVITY=0: the SDL default (terminate, start fresh);
    //    the game restores its own checkpoint instead of relying on an opaque
    //    Android instance-state bundle.
    // SAFETY: both strings are static NUL-terminated C strings.
    unsafe {
        SDL_SetHint(sdl_hint::ANDROID_BLOCK_ON_PAUSE.as_ptr(), c"1".as_ptr());
        SDL_SetHint(sdl_hint::ANDROID_LOW_LATENCY_AUDIO.as_ptr(), c"1".as_ptr());
        SDL_SetHint(
            sdl_hint::ANDROID_ALLOW_RECREATE_ACTIVITY.as_ptr(),
            c"0".as_ptr(),
        );
    }
    // SAFETY: called once on SDL's main thread before any SDL object exists.
    if !unsafe { SDL_Init(SDL_INIT_VIDEO) } {
        return Err(sdl_error("SDL_Init"));
    }
    // SAFETY: SDL is initialised; the filter only touches the lifecycle queue
    // and, for the save points, the game registered after construction.
    let watch_installed = unsafe { SDL_AddEventWatch(on_sdl_event, ptr::null_mut()) };
    log(&format!(
        "GAME_LIFECYCLE watch={watch_installed} block_on_pause=1 low_latency_audio=1 recreate_activity=0"
    ));
    // SAFETY: SDL is initialised.
    let driver = unsafe { CStr::from_ptr(SDL_GetCurrentVideoDriver()) }
        .to_string_lossy()
        .into_owned();
    log(&format!("P0_SDL_INIT video_driver={driver}"));

    // SAFETY: title is static and NUL terminated.
    let window = unsafe {
        SDL_CreateWindow(
            c"Herofolends".as_ptr(),
            1280,
            720,
            SDL_WINDOW_VULKAN | SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIGH_PIXEL_DENSITY,
        )
    };
    if window.is_null() {
        return Err(sdl_error("SDL_CreateWindow"));
    }
    // Immersive fullscreen. This is the call that reaches
    // SDLActivity.setWindowStyle(true) -> COMMAND_CHANGE_WINDOW_STYLE ->
    // WindowInsetsController.hide(systemBars()) with sticky-immersive behaviour
    // on API 30+. Pick desktop fullscreen first, then enter it, then let the
    // window manager settle the style before the surface is sized.
    // SAFETY: window is live; the fullscreen mode argument is NULL = desktop.
    unsafe {
        SDL_SetWindowFullscreenMode(window, ptr::null());
        let entered = SDL_SetWindowFullscreen(window, true);
        let _ = SDL_SyncWindow(window);
        let _ = SDL_DisableScreenSaver();
        let flags = SDL_GetWindowFlags(window);
        log(&format!(
            "GAME_FULLSCREEN entered={entered} flag={} flags=0x{flags:x}",
            flags & SDL_WINDOW_FULLSCREEN != 0
        ));
    }
    // SAFETY: window is live.
    let properties = unsafe { SDL_GetWindowProperties(window) };
    let has_window_prop =
        unsafe { SDL_HasProperty(properties, SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER.as_ptr()) };
    log(&format!(
        "P0_WINDOW_PROP window=0x{:x} properties={properties} has_android_window={has_window_prop}",
        window as usize
    ));
    let mut native_window = current_native_window(window);
    if native_window.is_null() {
        // SAFETY: window is live and owned here.
        unsafe { SDL_DestroyWindow(window) };
        return Err("SDL.window.android.window is NULL".to_string());
    }
    log(&format!(
        "P0_ANATIVE_WINDOW ptr=0x{:x}",
        native_window as usize
    ));

    let mut width = 0i32;
    let mut height = 0i32;
    // SAFETY: window is live; both out-pointers are valid.
    unsafe { SDL_GetWindowSizeInPixels(window, &mut width, &mut height) };
    let width = width.max(1) as u32;
    let height = height.max(1) as u32;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        // No indirect draws in this renderer. wgpu's default release flags still
        // build a validation compute pipeline at device creation, which Turnip
        // 26.3 fails to compile (ralloc corruption in ir3_optimize_loop).
        flags: wgpu::InstanceFlags::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let mut surface: Option<wgpu::Surface<'static>> =
        Some(create_android_surface(&instance, native_window)?);
    let setup = wc3_render::surface::configure_surface(
        &instance,
        surface.as_ref().expect("surface"),
        width,
        height,
    )
        .map_err(|error| {
            log(&format!("P0_SURFACE_SETUP_FAILED {error}"));
            error
        })?;
    for line in &setup.diagnostics {
        log(&format!("P0_SURFACE {line}"));
    }
    let wc3_render::SurfaceSetup {
        adapter,
        device,
        queue,
        mut config,
        ..
    } = setup;
    let info = adapter.get_info();
    log(&format!(
        "P0_GPU_ADAPTER name={:?} backend={:?} driver={:?}",
        info.name, info.backend, info.driver
    ));
    // The fullscreen transition can recreate the ANativeWindow between the
    // surface creation and this point; follow the current pointer once before
    // the first frame, with a guarded configure.
    let current = current_native_window(window);
    if current != native_window {
        native_window = current;
        surface = None;
        surface = Some(create_android_surface(&instance, native_window)?);
        wc3_render::surface::configure_guarded(
            &device,
            surface.as_ref().expect("surface"),
            &config,
        )?;
        log(&format!(
            "P0_SURFACE_FOLLOWED_WINDOW ptr=0x{:x}",
            native_window as usize
        ));
    }
    let format = config.format;
    let present_mode = config.present_mode;
    log(&format!(
        "GAME_SURFACE {}x{} format={format:?} present={present_mode:?}",
        config.width, config.height
    ));

    // ------------------------------------------------------------ game data
    // Library diagnostics (GAME_SKIN / GAME_TERRAIN_TILES / GAME_DOODADS) go
    // through the app's android logger; SDL does not mirror library stdio.
    wc3_game::set_log_sink(log);
    let mpq = find_file(&["War3.mpq", "war3.mpq"]);
    // P0 world: the scenario runs the Orc-vs-Human melee arena on a real map.
    // Ready 1v1 melee map from the library first; campaign maps only as a
    // fallback when the operator has not installed a melee map.
    let map = find_file(&[
        "Melee.w3x",
        "Maps/Melee.w3x",
        "(2)Ponds_v1.4.w3x",
        "Orc01.w3m",
        "Maps/Campaign/Orc01.w3m",
        "Human04.w3m",
        "Maps/Campaign/Human04.w3m",
    ]);
    log(&format!(
        "GAME_DATA mpq={:?} map={:?}",
        mpq.as_ref().map(|p| p.display().to_string()),
        map.as_ref().map(|p| p.display().to_string())
    ));
    let assets = Assets::load(mpq.as_deref(), map.as_deref());
    for line in assets.debug.iter().take(12) {
        log(&format!("GAME_ASSET {line}"));
    }
    log(&format!(
        "GAME_READY source={} map={:?} models={} roster={} scope={} textures={} world={:.0}x{:.0} terrain_atlas={} minimap={}",
        assets.source,
        assets.map_name,
        assets.loaded_model_count(),
        assets.model_roster,
        assets.model_scope.mode.label(),
        assets.textures.len(),
        assets.world_size.x,
        assets.world_size.y,
        assets.terrain_atlas.is_some(),
        assets.minimap.is_some()
    ));

    let mut game = Game::new(assets, 0xC0FFEE);
    game.resize(config.width as f32, config.height as f32);
    restore_checkpoint(&mut game);
    // The pointer is cleared before the game is dropped and is only read by
    // the event watcher while the loop is blocked in SDL_PumpEvents.
    GAME_FOR_CHECKPOINT.store(&mut game as *mut Game, Ordering::Release);
    let mut last_insets: Option<(f32, f32, f32, f32)> = None;
    apply_safe_insets(&mut game, window, &mut last_insets);
    let mut renderer = Renderer::new(
        &device,
        &queue,
        format,
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
    )?;
    // Adreno's vendor Vulkan driver renders the vertex-stage storage palette
    // as bind pose (TownHalls become flat slabs, units disappear), while the
    // same scene skins correctly on the GPU path headless. The device uses the
    // verified CPU skinning path; desktop/headless keeps GPU skinning.
    renderer.set_skin_mode(wc3_game::SkinMode::Cpu);
    renderer.resize(&device, config.width, config.height);
    // The renderer owns the GPU copies now: drop the decoded sheets so the
    // process does not carry 200 MB of dead RGBA next to the textures it draws.
    let released = game.assets.release_cpu_textures();
    log(&format!(
        "GAME_MEM textures_cpu_freed_mb={:.1} surfaces={} asset_cache_freed_mb={:.1} cpu_textures_left={}",
        released.bytes as f64 / (1024.0 * 1024.0),
        released.surfaces,
        released.cache_bytes as f64 / (1024.0 * 1024.0),
        game.assets.cpu_texture_bytes()
    ));
    for line in game.take_log() {
        log(&line);
    }
    // Audio verification line: one `GAME_AUDIO_HOOKS …` summary built by the
    // audio layer from this scenario's hooks (every unit sound set, music,
    // ambience and the engine queue counters). Logged after the scenario has
    // spawned its units, so `unresolved=` reflects real lookups, not just the
    // install state. `AUDIO_*` event lines are emitted by the audio layer
    // itself and are not filtered here.
    if let Some(audio) = game.assets.audio.as_ref() {
        log(&audio.stats_line());
    }

    // Multi-finger/keyboard routing lives in `wc3-input` so the trace fixtures
    // replay the same policy the device runs (cancel semantics, third finger,
    // key repeat, focus/rotation resets).
    let mut input = InputState::new();
    let mut lifecycle = Lifecycle::new();
    let mut suspended = false;
    let mut clean_exit = false;
    let mut needs_surface = false;
    let mut exposed = false;
    let mut input_events: Vec<InputEvent> = Vec::with_capacity(32);
    let mut frames = 0u64;
    let mut last = Instant::now();
    let mut last_frame_at = Instant::now();
    let mut status_at = Instant::now();
    let mut fullscreen_attempts = 0u32;
    let mut fullscreen_at = Instant::now() + Duration::from_millis(700);

    'outer: loop {
        let mut event = SdlEvent::new();
        let mut needs_resize = false;
        let mut needs_safe_area = false;
        // SAFETY: event is a correctly sized SDL_Event buffer.
        while unsafe { SDL_PollEvent(&mut event) } {
            match event.event_type() {
                SDL_EVENT_QUIT | SDL_EVENT_WINDOW_CLOSE_REQUESTED => {
                    clean_exit = true;
                    break 'outer;
                }
                SDL_EVENT_WINDOW_RESIZED | SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                    needs_resize = true;
                    needs_safe_area = true;
                }
                // Android's `AcquireNextImageKHR` has no timeout (wgpu-hal
                // forces `u64::MAX` before Android 11), so presenting into a
                // window whose queue is not consumed yet blocks the render
                // thread forever — measured cold-start freeze: tick 0, fps 0,
                // `dequeueBuffer` wait in debuggerd. Only draw once SDL says the
                // native surface is exposed, and rebuild the surface then
                // because the window can be recreated between the lifecycle
                // event and this one.
                SDL_EVENT_WINDOW_SHOWN | SDL_EVENT_WINDOW_EXPOSED | SDL_EVENT_WINDOW_RESTORED => {
                    if !exposed {
                        exposed = true;
                        needs_surface = true;
                        log("P0_WINDOW exposed=1");
                    }
                }
                SDL_EVENT_WINDOW_HIDDEN | SDL_EVENT_WINDOW_MINIMIZED | SDL_EVENT_WINDOW_OCCLUDED => {
                    if exposed {
                        exposed = false;
                        log("P0_WINDOW exposed=0");
                    }
                }
                SDL_EVENT_WINDOW_SAFE_AREA_CHANGED => {
                    needs_safe_area = true;
                }
                SDL_EVENT_FINGER_DOWN => {
                    input_events.push(InputEvent::FingerDown {
                        id: event.finger_id(),
                        pos: finger_pos(&event, &config),
                    });
                }
                SDL_EVENT_FINGER_MOTION => {
                    input_events.push(InputEvent::FingerMove {
                        id: event.finger_id(),
                        pos: finger_pos(&event, &config),
                    });
                }
                SDL_EVENT_FINGER_UP => {
                    input_events.push(InputEvent::FingerUp {
                        id: event.finger_id(),
                        pos: finger_pos(&event, &config),
                    });
                }
                SDL_EVENT_FINGER_CANCELED => {
                    // Android ACTION_CANCEL -> SDL_EVENT_FINGER_CANCELED
                    // (donor src/video/android/SDL_androidtouch.c:102-103).
                    // A canceled finger must never complete a tap/order.
                    input_events.push(InputEvent::FingerCancel {
                        id: event.finger_id(),
                        pos: finger_pos(&event, &config),
                    });
                }
                SDL_EVENT_MOUSE_BUTTON_DOWN => {
                    if event.mouse_which() != u32::MAX && event.mouse_down() {
                        input_events.push(InputEvent::MouseDown {
                            pos: Vec2::new(event.mouse_x(), event.mouse_y()),
                        });
                    }
                }
                SDL_EVENT_MOUSE_BUTTON_UP => {
                    if event.mouse_which() != u32::MAX {
                        input_events.push(InputEvent::MouseUp {
                            pos: Vec2::new(event.mouse_x(), event.mouse_y()),
                        });
                    }
                }
                SDL_EVENT_MOUSE_MOTION => {
                    if event.mouse_which() != u32::MAX {
                        input_events.push(InputEvent::MouseMove {
                            pos: Vec2::new(event.mouse_x(), event.mouse_y()),
                        });
                    }
                }
                SDL_EVENT_KEY_DOWN => {
                    // Hardware/virtual keyboard parity: digits assign or recall
                    // control groups (Ctrl+digit assigns), letters activate the
                    // visible card hotkey, Escape cancels placement/targeting,
                    // Tab cycles subgroups. Held-key repeats are swallowed by
                    // `KeyState`; Shift is tracked on down and up.
                    input_events.push(InputEvent::KeyDown {
                        scancode: event.key_scancode(),
                        modifiers: event.key_mod(),
                        repeat: event.key_repeat(),
                    });
                }
                SDL_EVENT_KEY_UP => {
                    input_events.push(InputEvent::KeyUp {
                        scancode: event.key_scancode(),
                        modifiers: event.key_mod(),
                    });
                }
                _ => {}
            }
        }
        // ------------------------------------------------------ lifecycle
        // Drain the watcher queue in order, then apply the effects before any
        // input from this frame: a gesture that started before a background,
        // focus or rotation transition must not complete after it.
        let mut effects = Effects::none();
        for event_type in drain_lifecycle_events() {
            if let Some(event_effects) = lifecycle.handle_sdl(event_type) {
                effects = effects.merge(event_effects);
            }
        }
        {
            let mut sink = GameSink { game: &mut game };
            if effects.reset_gestures {
                input.reset(&mut sink);
                // Events collected before the transition are stale.
                input_events.clear();
            }
            if suspended {
                input_events.clear();
            }
            for input_event in input_events.drain(..) {
                input.apply(&mut sink, input_event);
            }
        }
        if effects.save_checkpoint {
            // The watcher writes the checkpoint synchronously on the pause
            // event (before SDL blocks); only save here if it could not (e.g.
            // a lifecycle event before the game was registered).
            let watcher_saved = WATCHER_SAVED.swap(false, Ordering::AcqRel);
            if !watcher_saved {
                save_checkpoint(&game);
            }
        } else {
            WATCHER_SAVED.store(false, Ordering::Release);
        }
        if effects.low_memory {
            // The renderer/audio layers expose no releasable cache; log the
            // demand and the measured footprint instead of pretending to free
            // memory that is not held.
            log(&format!(
                "GAME_LOW_MEMORY units={} textures={} models={} placeholders={}",
                game.sim.units.iter().filter(|unit| unit.alive).count(),
                game.assets.textures.len(),
                game.assets.loaded_model_count(),
                game.assets.placeholder_count()
            ));
        }
        if effects.rebuild_surface {
            needs_surface = true;
        }
        if effects.reapply_insets {
            needs_safe_area = true;
        }
        if effects.quit {
            log("GAME_LIFECYCLE terminating");
            break 'outer;
        }
        if effects.pause_updates {
            suspended = true;
            log(&format!(
                "GAME_LIFECYCLE background phase={:?} focused={}",
                lifecycle.phase(),
                lifecycle.focused()
            ));
        }
        if effects.resume_updates {
            suspended = false;
            needs_surface = true;
            last = Instant::now();
            log(&format!(
                "GAME_LIFECYCLE foreground phase={:?} focused={}",
                lifecycle.phase(),
                lifecycle.focused()
            ));
        }

        // The first fullscreen request can race the activity's surface; retry a
        // bounded number of times if SDL still does not report the flag.
        if fullscreen_attempts < 3 && Instant::now() >= fullscreen_at {
            let flags = unsafe { SDL_GetWindowFlags(window) };
            if flags & SDL_WINDOW_FULLSCREEN == 0 {
                let entered = unsafe { SDL_SetWindowFullscreen(window, true) };
                fullscreen_attempts += 1;
                let flags = unsafe { SDL_GetWindowFlags(window) };
                log(&format!(
                    "GAME_FULLSCREEN retry={fullscreen_attempts} entered={entered} flag={} flags=0x{flags:x}",
                    flags & SDL_WINDOW_FULLSCREEN != 0
                ));
                fullscreen_at = Instant::now() + Duration::from_millis(900);
            } else {
                fullscreen_attempts = 3;
            }
        }
        if !suspended && (needs_resize || needs_surface) {
            let mut width = 0i32;
            let mut height = 0i32;
            // SAFETY: window is live; out-pointers are valid.
            unsafe { SDL_GetWindowSizeInPixels(window, &mut width, &mut height) };
            let width = (width.max(1) as u32).max(1);
            let height = (height.max(1) as u32).max(1);
            let reason = if needs_surface { "lifecycle" } else { "resize" };
            if needs_surface || width != config.width || height != config.height {
                recover_surface(
                    &instance,
                    window,
                    &mut native_window,
                    &mut surface,
                    &device,
                    &mut config,
                    width,
                    height,
                    reason,
                )
                .map_err(|error| {
                    log(&format!("P0_SURFACE_FAILED reason={reason} err={error}"));
                    error
                })?;
                renderer.resize(&device, config.width, config.height);
                game.resize(config.width as f32, config.height as f32);
                apply_safe_insets(&mut game, window, &mut last_insets);
                log(&format!(
                    "GAME_SURFACE_RESIZED {}x{} reason={reason} selection={} phase={:?} tick={}",
                    config.width,
                    config.height,
                    game.selection.len(),
                    game.phase,
                    game.sim.tick
                ));
            }
            needs_surface = false;
        }
        if needs_safe_area || frames % 30 == 0 {
            apply_safe_insets(&mut game, window, &mut last_insets);
            if last_insets.is_none() && frames == 90 {
                log("GAME_SAFE_AREA unavailable (SDL_GetWindowSafeArea failed)");
            }
        }
        if suspended || !exposed {
            // Hidden/background/minimised, or the native surface is not exposed
            // yet: never touch the swapchain, but keep pumping SDL so the
            // lifecycle watcher can see terminations and low memory.
            unsafe { SDL_Delay(16) };
            continue;
        }

        let live = surface.as_ref().ok_or("surface missing")?;
        let frame = match live.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                last_frame_at = Instant::now();
                frame
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                last_frame_at = Instant::now();
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                // A starved buffer queue (all swapchain images held by a
                // detached consumer) never recovers on its own: `dequeueBuffer`
                // just times out forever and the loop spins while the sim
                // stays frozen (observed: `tick=0 fps=0` for minutes). Rebuild
                // the surface from the live window instead of waiting.
                if last_frame_at.elapsed() >= Duration::from_millis(1500) {
                    let mut width = 0i32;
                    let mut height = 0i32;
                    // SAFETY: window is live; out-pointers are valid.
                    unsafe { SDL_GetWindowSizeInPixels(window, &mut width, &mut height) };
                    let width = (width.max(1) as u32).max(1);
                    let height = (height.max(1) as u32).max(1);
                    recover_surface(
                        &instance,
                        window,
                        &mut native_window,
                        &mut surface,
                        &device,
                        &mut config,
                        width,
                        height,
                        "watchdog",
                    )
                    .map_err(|error| {
                        log(&format!("P0_SURFACE_FAILED reason=watchdog err={error}"));
                        error
                    })?;
                    renderer.resize(&device, config.width, config.height);
                    game.resize(config.width as f32, config.height as f32);
                    apply_safe_insets(&mut game, window, &mut last_insets);
                    last_frame_at = Instant::now();
                    log("P0_SURFACE_WATCHDOG recreated after 1.5 s without a frame");
                }
                continue;
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                let mut width = 0i32;
                let mut height = 0i32;
                // SAFETY: window is live; out-pointers are valid.
                unsafe { SDL_GetWindowSizeInPixels(window, &mut width, &mut height) };
                let width = (width.max(1) as u32).max(1);
                let height = (height.max(1) as u32).max(1);
                recover_surface(
                    &instance,
                    window,
                    &mut native_window,
                    &mut surface,
                    &device,
                    &mut config,
                    width,
                    height,
                    "lost",
                )
                .map_err(|error| {
                    log(&format!("P0_SURFACE_FAILED reason=lost err={error}"));
                    error
                })?;
                renderer.resize(&device, config.width, config.height);
                game.resize(config.width as f32, config.height as f32);
                apply_safe_insets(&mut game, window, &mut last_insets);
                continue;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface validation error".to_string());
            }
        };

        // `Mailbox`/`Immediate` never wait for the compositor, so without an
        // explicit budget the loop free-runs at 115-166 fps on the Adreno 730
        // (device log 2026-11-17) and heats the phone for no visible gain. Pace
        // to the 60 Hz the HUD counters advertise; `Fifo` already blocks.
        let frame_pacing = !matches!(
            config.present_mode,
            wgpu::PresentMode::Fifo | wgpu::PresentMode::AutoVsync
        );
        let frame_started = Instant::now();
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        game.update(dt);
        for cue in game.take_haptics() {
            haptics::play(cue.duration_ms());
        }
        for line in game.take_log() {
            log(&line);
        }
        let scene = game.scene();
        renderer.set_fog(&game.sim, game.player);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        renderer.render(&device, &queue, &view, &scene);
        queue.present(frame);

        if frame_pacing {
            const FRAME_BUDGET: Duration = Duration::from_micros(16_667);
            if let Some(rest) = FRAME_BUDGET.checked_sub(frame_started.elapsed()) {
                // SAFETY: SDL_Delay is a plain sleep; the value is > 0.
                unsafe { SDL_Delay(rest.as_millis().max(1) as u32) };
            }
        }

        frames += 1;
        if frames == 1 {
            let native = scene.viewport.x.round() as u32 == config.width
                && scene.viewport.y.round() as u32 == config.height;
            log(&format!(
                "GAME_RENDER_SCALE swapchain={}x{} scene={:.0}x{:.0} native={}",
                config.width, config.height, scene.viewport.x, scene.viewport.y, native
            ));
            let flags = unsafe { SDL_GetWindowFlags(window) };
            log(&format!(
                "GAME_FIRST_FRAME surface={}x{} fullscreen={} safe={:?} units={} phase={:?}",
                config.width,
                config.height,
                flags & SDL_WINDOW_FULLSCREEN != 0,
                game.safe_insets,
                scene.units.len(),
                game.phase
            ));
        }
        if status_at.elapsed() >= Duration::from_secs(2) {
            status_at = Instant::now();
            log(&format!("GAME_STATE {}", game.status_line()));
            if let Some(audio) = game.assets.audio.as_ref() {
                let stats = audio.audio_stats();
                log(&format!(
                    "GAME_AUDIO_STATS frames={} blocks={} underruns={} backend_errors={} buffered_ms={:?} active={} max_active={} dropped={} occluded={} ducked_blocks={}",
                    stats.frames_rendered,
                    stats.blocks_rendered,
                    stats.buffer_underruns,
                    stats.backend_errors,
                    audio.buffered_ms(),
                    stats.active_voices,
                    stats.max_active_voices,
                    stats.commands_dropped,
                    stats.occluded_voices,
                    stats.ducked_blocks,
                ));
            }
        }
    }

    // SAFETY: the filter is the one installed above; SDL is still initialised.
    unsafe { SDL_RemoveEventWatch(on_sdl_event, ptr::null_mut()) };
    GAME_FOR_CHECKPOINT.store(ptr::null_mut(), Ordering::Release);
    if clean_exit {
        clear_checkpoint();
        LAST_SAVED_TICK.store(u64::MAX, Ordering::Release);
    }
    log(&format!(
        "GAME_CLOSED frames={frames} clean={clean_exit} phase={:?} suspended={suspended}",
        lifecycle.phase()
    ));
    drop(renderer);
    drop(surface);
    // SAFETY: window is live and owned here.
    unsafe { SDL_DestroyWindow(window) };
    Ok(())
}

/// SDL3 entry point, called by `SDL_android.c:nativeRunMain` via `SDL_RunApp`.
#[unsafe(no_mangle)]
pub extern "C" fn SDL_main(_argc: c_int, _argv: *mut *mut c_char) -> c_int {
    // Keep the activity in landscape; Android 12+ may otherwise lose an
    // `am start`-launched activity after SDL requests a new orientation.
    // SAFETY: both strings are static and NUL terminated.
    unsafe {
        SDL_SetHint(
            c"SDL_ORIENTATIONS".as_ptr(),
            c"LandscapeLeft LandscapeRight".as_ptr(),
        );
    }
    configure_turnip();
    match run() {
        Ok(()) => 0,
        Err(error) => {
            log(&format!("GAME_FATAL {error}"));
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn herofolends_quit_sdl() {
    // SAFETY: paired with SDL_Init in run(); safe to call once.
    unsafe { SDL_Quit() };
}

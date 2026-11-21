//! `libvulkan.so` shim: the AOSP Vulkan loader with a user Turnip ICD.
//!
//! The loader shipped in Termux is built without surface WSI (only
//! `VK_EXT_headless_surface`), so SDL rejects it. The AOSP loader has
//! `VK_KHR_surface`/`VK_KHR_android_surface`, but ignores
//! `VK_ICD_FILENAMES`. This shim uses the Rust libadrenotools port to open a
//! private copy of the AOSP loader whose `android_dlopen_ext` is redirected to
//! the Turnip ICD, then forwards the loader entry points to it. When the Turnip
//! files are absent it forwards to the system loader (vendor Adreno), so the app
//! still starts.
//!
//! Diagnostics: the delegate choice is written once per process
//! (`VK_SHIM delegate=turnip|vendor|none dir=… system_mode=…`) to stderr and to
//! `<app files>/vulkan_shim.log`. A missing delegate answers
//! `VK_ERROR_INCOMPATIBLE_DRIVER` from the instance enumerations instead of
//! "zero extensions, success", so SDL's failure names the driver instead of the
//! shim being invisible (WC3-VK-001).
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::OnceLock;

type Gipa = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

const DRIVER_NAME: &CStr = c"libvulkan_freedreno.so";
/// `VK_ERROR_INCOMPATIBLE_DRIVER`: the loader exists but cannot provide the
/// requested functionality. Returned when no delegate loader could be opened.
const VK_ERROR_INCOMPATIBLE_DRIVER: i32 = -9;
/// Fallback application id, used only when `/proc/self/cmdline` cannot be read
/// (host tests). The running app's package is the source of truth, so a debug
/// suffix or a rebrand cannot silently select the vendor driver.
const DEFAULT_APP_ID: &str = "org.wc3rust.p0";

static GIPA: OnceLock<Option<Gipa>> = OnceLock::new();
static TURNIP_DIR: OnceLock<CString> = OnceLock::new();

/// Package id of the running process (`<pkg>` or `<pkg>:service`), overridable
/// with `H2_APP_ID`.
fn app_id() -> String {
    if let Ok(explicit) = std::env::var("H2_APP_ID") {
        let explicit = explicit.trim().to_string();
        if !explicit.is_empty() {
            return explicit;
        }
    }
    let raw = std::fs::read("/proc/self/cmdline").unwrap_or_default();
    let first = raw.split(|byte| *byte == 0).next().unwrap_or(&[]);
    let cmd = String::from_utf8_lossy(first);
    let package = cmd.split(':').next().unwrap_or("").trim();
    if package.is_empty() {
        DEFAULT_APP_ID.to_string()
    } else {
        package.to_string()
    }
}

/// App-private Turnip directory: `H2_TURNIP_DIR` when set, otherwise
/// `<app files>/turnip` for the live package id.
fn turnip_dir() -> &'static CStr {
    TURNIP_DIR.get_or_init(|| {
        let dir = std::env::var("H2_TURNIP_DIR")
            .ok()
            .filter(|dir| !dir.trim().is_empty())
            .unwrap_or_else(|| format!("/data/data/{}/files/turnip", app_id()));
        CString::new(dir).unwrap_or_else(|_| {
            CString::new(format!("/data/data/{DEFAULT_APP_ID}/files/turnip"))
                .expect("fallback turnip path has no NUL")
        })
    })
}

/// One diagnostic line: stderr for host/headless runs plus `vulkan_shim.log` in
/// the app's files dir (the crate has no logger dependency and library stderr
/// is not mirrored to logcat; the file is readable by `adb`/root).
fn shim_log(line: &str) {
    eprintln!("{line}");
    let path = format!("/data/data/{}/files/vulkan_shim.log", app_id());
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 64 * 1024 {
            let _ = std::fs::remove_file(&path);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

unsafe fn load_via_adrenotools(dir: &CStr) -> Option<Gipa> {
    // SAFETY: all strings are NUL terminated and live for the process; the
    // feature flags match the port's C API (CUSTOM only).
    unsafe {
        let opened = wc3_adrenotools::driver::open_libvulkan(
            libc::RTLD_NOW,
            wc3_adrenotools::FeatureFlags::CUSTOM,
            std::ptr::null(),
            dir.as_ptr(),
            dir.as_ptr(),
            DRIVER_NAME.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
        )
        .ok()?;
        let symbol = libc::dlsym(opened.vulkan, c"vkGetInstanceProcAddr".as_ptr());
        if symbol.is_null() {
            return None;
        }
        // Keep the loader handle open for the process lifetime.
        std::mem::forget(opened);
        Some(std::mem::transmute::<*mut c_void, Gipa>(symbol))
    }
}

unsafe fn load_vendor() -> Option<Gipa> {
    // SAFETY: the system loader path is stable on 64-bit Android.
    unsafe {
        let handle = libc::dlopen(c"/system/lib64/libvulkan.so".as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            return None;
        }
        let symbol = libc::dlsym(handle, c"vkGetInstanceProcAddr".as_ptr());
        if symbol.is_null() {
            return None;
        }
        Some(std::mem::transmute::<*mut c_void, Gipa>(symbol))
    }
}

fn gipa() -> Option<Gipa> {
    *GIPA.get_or_init(|| {
        let dir = turnip_dir();
        let dir_text = dir.to_string_lossy();
        // A root install can replace the vendor ICD in the app's mount
        // namespace; then the stock AOSP loader already enumerates Turnip and
        // no adrenotools redirection is needed.
        let system_mode = std::path::Path::new(dir_text.as_ref())
            .join("system_loader")
            .exists();
        // SAFETY: both loaders are safe to attempt once per process.
        let loaded = unsafe {
            if system_mode {
                match load_vendor() {
                    Some(gipa) => (Some(gipa), "vendor"),
                    None => match load_via_adrenotools(dir) {
                        Some(gipa) => (Some(gipa), "turnip"),
                        None => (None, "none"),
                    },
                }
            } else {
                match load_via_adrenotools(dir) {
                    Some(gipa) => (Some(gipa), "turnip"),
                    None => match load_vendor() {
                        Some(gipa) => (Some(gipa), "vendor"),
                        None => (None, "none"),
                    },
                }
            }
        };
        // One line per process: which delegate answered, from where, and why.
        shim_log(&format!(
            "VK_SHIM delegate={} dir={} system_mode={}",
            loaded.1, dir_text, system_mode
        ));
        loaded.0
    })
}

fn proc(name: &CStr) -> *mut c_void {
    let Some(gipa) = gipa() else {
        return std::ptr::null_mut();
    };
    // SAFETY: gipa follows the Vulkan ABI; a NULL instance requests a global
    // entry point.
    unsafe { gipa(std::ptr::null_mut(), name.as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkGetInstanceProcAddr(
    instance: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    let Some(gipa) = gipa() else {
        return std::ptr::null_mut();
    };
    if name.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded verbatim under the Vulkan ABI.
    unsafe { gipa(instance, name) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkCreateInstance(
    create_info: *const c_void,
    allocator: *const c_void,
    instance: *mut *mut c_void,
) -> i32 {
    let function = proc(c"vkCreateInstance");
    if function.is_null() {
        return VK_ERROR_INCOMPATIBLE_DRIVER;
    }
    // SAFETY: exact `PFN_vkCreateInstance` ABI.
    let function: unsafe extern "C" fn(*const c_void, *const c_void, *mut *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    unsafe { function(create_info, allocator, instance) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkEnumerateInstanceExtensionProperties(
    layer_name: *const c_char,
    count: *mut u32,
    properties: *mut c_void,
) -> i32 {
    let function = proc(c"vkEnumerateInstanceExtensionProperties");
    if function.is_null() {
        // "Zero extensions, VK_SUCCESS" made a missing Turnip delegate look
        // like a device without any instance extensions: SDL reported its own
        // generic failure and nothing named the driver. Fail loudly instead;
        // the Vulkan spec allows an error from the enum on a broken install.
        if !count.is_null() {
            // SAFETY: caller supplied a writable count pointer.
            unsafe { *count = 0 };
        }
        shim_log("VK_SHIM enumerate_instance_extensions=unavailable rc=-9");
        return VK_ERROR_INCOMPATIBLE_DRIVER;
    }
    // SAFETY: exact `PFN_vkEnumerateInstanceExtensionProperties` ABI.
    let function: unsafe extern "C" fn(*const c_char, *mut u32, *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    unsafe { function(layer_name, count, properties) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkEnumerateInstanceVersion(version: *mut u32) -> i32 {
    let function = proc(c"vkEnumerateInstanceVersion");
    if function.is_null() {
        if !version.is_null() {
            // SAFETY: caller supplied a writable version pointer.
            unsafe { *version = (1 << 22) | (1 << 12) }; // 1.1.0
        }
        return 0;
    }
    // SAFETY: exact `PFN_vkEnumerateInstanceVersion` ABI.
    let function: unsafe extern "C" fn(*mut u32) -> i32 = unsafe { std::mem::transmute(function) };
    unsafe { function(version) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkEnumerateInstanceLayerProperties(
    count: *mut u32,
    properties: *mut c_void,
) -> i32 {
    let function = proc(c"vkEnumerateInstanceLayerProperties");
    if function.is_null() {
        if !count.is_null() {
            // SAFETY: caller supplied a writable count pointer.
            unsafe { *count = 0 };
        }
        shim_log("VK_SHIM enumerate_instance_layers=unavailable rc=-9");
        return VK_ERROR_INCOMPATIBLE_DRIVER;
    }
    // SAFETY: exact `PFN_vkEnumerateInstanceLayerProperties` ABI.
    let function: unsafe extern "C" fn(*mut u32, *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    unsafe { function(count, properties) }
}

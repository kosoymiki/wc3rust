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
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

type Gipa = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

const TURNIP_DIR: &CStr = c"/data/data/org.herofolends.p0/files/turnip";
const DRIVER_NAME: &CStr = c"libvulkan_freedreno.so";

static GIPA: OnceLock<Option<Gipa>> = OnceLock::new();

unsafe fn load_via_adrenotools() -> Option<Gipa> {
    // SAFETY: all strings are NUL terminated and live for the process; the
    // feature flags match the port's C API (CUSTOM only).
    unsafe {
        let opened = wc3_adrenotools::driver::open_libvulkan(
            libc::RTLD_NOW,
            wc3_adrenotools::FeatureFlags::CUSTOM,
            std::ptr::null(),
            TURNIP_DIR.as_ptr(),
            TURNIP_DIR.as_ptr(),
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
        // A root install can replace the vendor ICD in the app's mount
        // namespace; then the stock AOSP loader already enumerates Turnip and
        // no adrenotools redirection is needed.
        let system_mode = std::path::Path::new(
            "/data/data/org.herofolends.p0/files/turnip/system_loader",
        )
        .exists();
        // SAFETY: both loaders are safe to attempt once per process.
        unsafe {
            if system_mode {
                load_vendor().or_else(|| load_via_adrenotools())
            } else {
                load_via_adrenotools().or_else(|| load_vendor())
            }
        }
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
        return -9; // VK_ERROR_INCOMPATIBLE_DRIVER
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
        if !count.is_null() {
            // SAFETY: caller supplied a writable count pointer.
            unsafe { *count = 0 };
        }
        return 0; // VK_SUCCESS with no extensions
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
        return 0;
    }
    // SAFETY: exact `PFN_vkEnumerateInstanceLayerProperties` ABI.
    let function: unsafe extern "C" fn(*mut u32, *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    unsafe { function(count, properties) }
}

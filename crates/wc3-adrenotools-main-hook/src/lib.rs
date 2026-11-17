//! `libmain_hook.so` — Rust interposer for `android_dlopen_ext` and
//! `android_load_sphal_library`.
//!
//! Built with `-z global` (see `build.rs`): when the driver loads this library
//! with `RTLD_GLOBAL`, the linker puts it on the namespace's preload list, so the
//! Vulkan loader's `android_dlopen_ext` call resolves here. The body forwards to
//! [`wc3_adrenotools::hooks::hook_dlopen_ext`], which decides whether the call is
//! a Vulkan driver open that must be redirected.

use std::ffi::c_char;
use std::ffi::c_int;
use std::ffi::c_void;

use wc3_adrenotools::hooks;
use wc3_adrenotools::linker::{self, DlextInfo};

/// `android_dlopen_ext` interposer.
///
/// # Safety
/// Same contract as bionic's `android_dlopen_ext`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn android_dlopen_ext(
    filename: *const c_char,
    flags: c_int,
    extinfo: *const DlextInfo,
) -> *mut c_void {
    // SAFETY: forwarded unchanged.
    unsafe { hooks::hook_dlopen_ext(filename, flags, extinfo) }
}

/// `android_load_sphal_library` interposer.
///
/// # Safety
/// `filename` must be NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn android_load_sphal_library(
    filename: *const c_char,
    flags: c_int,
) -> *mut c_void {
    // SAFETY: forwarded unchanged.
    unsafe { hooks::hook_load_sphal_library(filename, flags) }
}

/// Install the parameter block. Called once by `wc3-adrenotools` after this
/// library is loaded into the hook namespace.
#[unsafe(no_mangle)]
pub extern "C" fn adrenotools_main_hook_init(params: *const hooks::HookParams) {
    hooks::set_params(params);
}

/// Probe helper: exercise the hook exactly as the Vulkan loader would, using
/// `parent_namespace` as the caller's namespace.
///
/// # Safety
/// `parent_namespace` must be a live linker namespace and `filename` NUL
/// terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_hook_load(
    filename: *const c_char,
    parent_namespace: *mut c_void,
) -> *mut c_void {
    let info = DlextInfo {
        flags: linker::ANDROID_DLEXT_USE_NAMESPACE,
        library_namespace: parent_namespace,
        ..Default::default()
    };
    // SAFETY: info is a valid dlextinfo for this call.
    unsafe { hooks::hook_dlopen_ext(filename, libc::RTLD_NOW, &info) }
}

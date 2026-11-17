//! `libfile_redirect_hook.so` — Rust interposer for `fopen`.
//!
//! Loaded with `RTLD_GLOBAL` into the driver namespace when
//! `ADRENOTOOLS_DRIVER_FILE_REDIRECT` is enabled. Every path the driver opens
//! (except `/proc` and `/sys`) is prefixed with `fileRedirectDir`, which lets an
//! app redirect driver data files without modifying the driver.

use std::ffi::c_char;
use std::ffi::c_void;

use wc3_adrenotools::hooks;

/// `fopen` interposer.
///
/// # Safety
/// Same contract as libc's `fopen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(filename: *const c_char, mode: *const c_char) -> *mut c_void {
    // SAFETY: forwarded unchanged.
    unsafe { hooks::hook_fopen(filename, mode) }
}

/// Install the parameter block for this library instance.
#[unsafe(no_mangle)]
pub extern "C" fn adrenotools_fopen_init(params: *const hooks::HookParams) {
    hooks::set_params(params);
}

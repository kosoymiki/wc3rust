//! `libgsl_alloc_hook.so` — Rust interposer for the Adreno GSL allocator.
//!
//! Loaded with `RTLD_GLOBAL` into the driver namespace when
//! `ADRENOTOOLS_DRIVER_GPU_MAPPING_IMPORT` is enabled. When the driver allocates a
//! memory object whose size/flags match the app's imported mapping, the hook fills
//! the descriptor with the imported GPU address instead of calling the vendor
//! allocator; `vkAllocateMemory` then returns a view over the app's memory.

use std::ffi::c_void;

use wc3_adrenotools::hooks;

/// 32-bit variant (donor `gsl_alloc_hook.c:3-5`).
///
/// # Safety
/// `mem_desc` must point at a `GslMemDesc` the driver is about to fill.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gsl_memory_alloc_pure(
    size: u32,
    flags: u32,
    mem_desc: *mut c_void,
) -> i32 {
    // SAFETY: forwarded to the 64-bit implementation like the donor shim.
    unsafe { hooks::hook_gsl_alloc_64(u64::from(size), flags, mem_desc) }
}

/// 64-bit variant (`hook_impl.cpp:181-200`).
///
/// # Safety
/// `mem_desc` must point at a `GslMemDesc` the driver is about to fill.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gsl_memory_alloc_pure_64(
    size: u64,
    flags: u32,
    mem_desc: *mut c_void,
) -> i32 {
    // SAFETY: forwarded unchanged.
    unsafe { hooks::hook_gsl_alloc_64(size, flags, mem_desc) }
}

/// Free variant (`hook_impl.cpp:202-228`).
///
/// # Safety
/// `mem_desc` must point at a `GslMemDesc` previously passed to the allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gsl_memory_free_pure(mem_desc: *mut c_void) -> i32 {
    // SAFETY: forwarded unchanged.
    unsafe { hooks::hook_gsl_free(mem_desc) }
}

/// Install the parameter block for this library instance.
#[unsafe(no_mangle)]
pub extern "C" fn adrenotools_gsl_init_params(params: *const hooks::HookParams) {
    hooks::set_params(params);
}

/// Hand over the vendor GSL entries resolved by the main hook.
///
/// # Safety
/// The three pointers must be valid GSL function addresses or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_gsl_init(
    alloc: *mut c_void,
    alloc64: *mut c_void,
    free: *mut c_void,
) {
    hooks::init_gsl(alloc, alloc64, free);
}

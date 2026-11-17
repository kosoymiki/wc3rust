//! Interposition layer — Rust port of `src/hook/hook_impl.cpp` and the three
//! `src/hook/*_hook.c` shims.
//!
//! The donor compiles four shared objects:
//!
//! | donor | exports | role |
//! |---|---|---|
//! | `libmain_hook.so` | `android_dlopen_ext`, `android_load_sphal_library` | intercepts the Vulkan loader's driver open |
//! | `libhook_impl.so`  | `init_hook_param`, hook bodies | shared state |
//! | `libfile_redirect_hook.so` | `fopen` | redirects driver file access |
//! | `libgsl_alloc_hook.so` | `gsl_memory_alloc_pure*` | imports user/GPU memory |
//!
//! The Rust port keeps the same split (three `cdylib` crates plus this shared
//! module), but removes the donor's cross-`.so` symbol sharing: each hook library
//! receives the parameter block through its own uniquely named init entry point
//! (`adrenotools_fopen_init`, `adrenotools_gsl_init`). The donor relies on
//! `libhook_impl.so` being preloaded first so all hooks resolve to one copy; that
//! is an implicit link-order contract this port does not need.
//!
//! Delta: every call to `android_dlopen_ext` inside the hooks goes through
//! [`crate::linker::real_dlopen_ext`] (resolved from `libdl.so`), so an interposed
//! symbol can never recurse into itself. The donor depends on dependency scope
//! ordering for the same guarantee.

use std::ffi::CStr;
use std::fmt;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::kgsl;
use crate::linker::{self, DlextInfo, Namespace};

pub const FEATURE_CUSTOM: libc::c_int = 1 << 0;
pub const FEATURE_FILE_REDIRECT: libc::c_int = 1 << 1;
pub const FEATURE_GPU_MAPPING: libc::c_int = 1 << 2;

/// `ADRENOTOOLS_GPU_MAPPING_SUCCEEDED_MAGIC` (`include/adrenotools/priv.h:17`).
pub const GPU_MAPPING_SUCCEEDED_MAGIC: u64 = 0xDEAD_BEEF;
/// `GslMemDescImportedPrivMagic` (`hook_impl.cpp:172`).
pub const GSL_MEMDESC_IMPORTED_MAGIC: usize = 0xdead_b33f;

/// Mirror of `adrenotools_gpu_mapping` (`include/adrenotools/priv.h:21-26`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GpuMapping {
    pub host_ptr: *mut libc::c_void,
    pub gpu_addr: u64,
    pub size: u64,
    pub flags: u64,
}

impl Default for GpuMapping {
    fn default() -> Self {
        Self {
            host_ptr: ptr::null_mut(),
            gpu_addr: 0,
            size: 0,
            flags: 0,
        }
    }
}

/// Parameters shared by all hooks.
///
/// The donor passes a C++ `HookImplParams` with `std::string` members. Rust cannot
/// share that layout safely across namespaces, so this port stores NUL-terminated
/// C strings. The struct is internal to the port; nothing outside this crate reads
/// it, and the owner leaks it for the process lifetime exactly like the donor's
/// `new HookImplParams(...)`.
#[repr(C)]
pub struct HookParams {
    pub feature_flags: libc::c_int,
    pub tmp_lib_dir: *const libc::c_char,
    pub hook_lib_dir: *const libc::c_char,
    pub custom_driver_dir: *const libc::c_char,
    pub custom_driver_name: *const libc::c_char,
    pub file_redirect_dir: *const libc::c_char,
    pub next_gpu_mapping: *mut GpuMapping,
}

static PARAMS: AtomicPtr<HookParams> = AtomicPtr::new(ptr::null_mut());
static REAL_DLOPEN_EXT: OnceLock<Option<linker::RealDlopenExt>> = OnceLock::new();
static REAL_FOPEN: OnceLock<Option<RealFopen>> = OnceLock::new();
static GSL_ALLOC: AtomicPtr<libc::c_void> = AtomicPtr::new(ptr::null_mut());
static GSL_ALLOC_64: AtomicPtr<libc::c_void> = AtomicPtr::new(ptr::null_mut());
static GSL_FREE: AtomicPtr<libc::c_void> = AtomicPtr::new(ptr::null_mut());
static TRACE: OnceLock<bool> = OnceLock::new();

type RealFopen =
    unsafe extern "C" fn(*const libc::c_char, *const libc::c_char) -> *mut libc::c_void;
type GslAlloc = unsafe extern "C" fn(u32, u32, *mut libc::c_void) -> libc::c_int;
type GslAlloc64 = unsafe extern "C" fn(u64, u32, *mut libc::c_void) -> libc::c_int;
type GslFree = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int;

/// Hook log, disabled unless `ADRENOTOOLS_TRACE` is set.
pub fn trace(args: fmt::Arguments<'_>) {
    let enabled = *TRACE.get_or_init(|| std::env::var_os("ADRENOTOOLS_TRACE").is_some());
    if enabled {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "[adrenotools] {args}");
    }
}

/// Install the parameter block (called by each hook library's init entry).
pub fn set_params(params: *const HookParams) {
    PARAMS.store(params as *mut HookParams, Ordering::Release);
}

pub fn params() -> Option<&'static HookParams> {
    let ptr = PARAMS.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the pointer is a leaked HookParams owned by the driver for the
        // process lifetime; set_params only ever receives such a pointer.
        Some(unsafe { &*ptr })
    }
}

pub fn real_dlopen_ext_cached() -> Option<linker::RealDlopenExt> {
    *REAL_DLOPEN_EXT.get_or_init(linker::real_dlopen_ext)
}

fn dlsym(handle: *mut libc::c_void, name: &CStr) -> *mut libc::c_void {
    if handle.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: handle is a live dlopen handle; name is NUL terminated.
    unsafe { libc::dlsym(handle, name.as_ptr()) }
}

/// Resolve `fopen` from libc, bypassing this library's own interposer.
fn real_fopen_cached() -> Option<RealFopen> {
    *REAL_FOPEN.get_or_init(|| {
        // SAFETY: libc.so is always present; dlsym returns the real implementation.
        let handle =
            unsafe { libc::dlopen(c"libc.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return None;
        }
        let sym = dlsym(handle, c"fopen");
        if sym.is_null() {
            None
        } else {
            // SAFETY: symbol has the documented fopen signature.
            Some(unsafe { std::mem::transmute::<*mut libc::c_void, RealFopen>(sym) })
        }
    })
}

/// `strstr(filename, "vulkan.")` from `hook_impl.cpp:47`.
pub fn contains_vulkan_dot(name: &[u8]) -> bool {
    name.windows(7).any(|window| window == b"vulkan.")
}

fn fallback(
    real: linker::RealDlopenExt,
    filename: *const libc::c_char,
    flags: libc::c_int,
    extinfo: *const DlextInfo,
) -> *mut libc::c_void {
    trace(format_args!("hook: falling back"));
    // SAFETY: forwarded arguments come from the caller unchanged.
    unsafe { real(filename, flags, extinfo) }
}

fn dlerror_text() -> String {
    linker::last_dl_error()
}

/// Port of `hook_android_dlopen_ext` (`hook_impl.cpp:38-140`).
///
/// # Safety
/// Same contract as `android_dlopen_ext`.
pub unsafe fn hook_dlopen_ext(
    filename: *const libc::c_char,
    flags: libc::c_int,
    extinfo: *const DlextInfo,
) -> *mut libc::c_void {
    let Some(real) = real_dlopen_ext_cached() else {
        return ptr::null_mut();
    };
    if filename.is_null() || extinfo.is_null() {
        // SAFETY: forwarding the caller's pointers.
        return unsafe { real(filename, flags, extinfo) };
    }
    // SAFETY: filename is non-NULL and NUL terminated per the dlopen contract.
    let name = unsafe { CStr::from_ptr(filename) };
    trace(format_args!("android_dlopen_ext({:?})", name));

    if !contains_vulkan_dot(name.to_bytes()) {
        // SAFETY: pass-through for every non-Vulkan library.
        return unsafe { real(filename, flags, extinfo) };
    }

    // SAFETY: extinfo is non-NULL and points to a valid struct for this call.
    let ext = unsafe { &*extinfo };
    if ext.library_namespace.is_null() || ext.flags & linker::ANDROID_DLEXT_USE_NAMESPACE == 0 {
        trace(format_args!("hook: no namespace supplied, falling back"));
        return fallback(real, filename, flags, extinfo);
    }
    let Some(p) = params() else {
        return fallback(real, filename, flags, extinfo);
    };

    // A child namespace whose default search path is the custom driver directory;
    // its parent is the namespace the loader wanted to use.
    // SAFETY: all string pointers come from the leaked parameter block.
    let driver_ns = unsafe {
        linker::create_namespace(
            filename,
            p.custom_driver_dir,
            p.hook_lib_dir,
            linker::ANDROID_NAMESPACE_TYPE_SHARED,
            ptr::null(),
            ext.library_namespace,
        )
    };
    if driver_ns.is_null() {
        trace(format_args!("hook: create_namespace failed"));
        return fallback(real, filename, flags, extinfo);
    }

    // The driver needs libandroid (property lookups, etc.); its own namespace
    // normally does not have it.
    // SAFETY: driver_ns came from create_namespace above.
    if !unsafe { linker::link_namespaces(driver_ns, ptr::null_mut(), c"libandroid.so") } {
        trace(format_args!(
            "hook: linking libandroid into driver namespace failed"
        ));
    }

    if p.feature_flags & FEATURE_FILE_REDIRECT != 0 {
        // SAFETY: driver_ns is valid; the library lives in hook_lib_dir.
        let handle = unsafe {
            linker::namespace_dlopen(
                c"libfile_redirect_hook.so",
                libc::RTLD_NOW | libc::RTLD_GLOBAL,
                driver_ns,
            )
        };
        if handle.is_null() {
            trace(format_args!(
                "hook: failed to load libfile_redirect_hook.so"
            ));
            return fallback(real, filename, flags, extinfo);
        }
        let init = dlsym(handle, c"adrenotools_fopen_init");
        if init.is_null() {
            return fallback(real, filename, flags, extinfo);
        }
        // SAFETY: symbol was exported by our own hook crate with this signature.
        let init: unsafe extern "C" fn(*const HookParams) = unsafe { std::mem::transmute(init) };
        // SAFETY: params live for the process lifetime.
        unsafe { init(p as *const HookParams) };
        trace(format_args!("hook: applied libfile_redirect_hook"));
    }

    let new_ext = DlextInfo {
        library_namespace: driver_ns,
        ..*ext
    };

    if p.feature_flags & FEATURE_GPU_MAPPING != 0 {
        // SAFETY: as above.
        let hook_handle = unsafe {
            linker::namespace_dlopen(
                c"libgsl_alloc_hook.so",
                libc::RTLD_NOW | libc::RTLD_GLOBAL,
                driver_ns,
            )
        };
        if hook_handle.is_null() {
            trace(format_args!("hook: failed to load libgsl_alloc_hook.so"));
            return fallback(real, filename, flags, extinfo);
        }
        let init_params = dlsym(hook_handle, c"adrenotools_gsl_init_params");
        if init_params.is_null() {
            return fallback(real, filename, flags, extinfo);
        }
        // SAFETY: exported by our gsl hook crate.
        let init_params: unsafe extern "C" fn(*const HookParams) =
            unsafe { std::mem::transmute(init_params) };
        // SAFETY: params live for the process lifetime.
        unsafe { init_params(p as *const HookParams) };

        // The GSL allocator library is vendor-specific; the donor tries three
        // names in this order (`hook_impl.cpp:99-104`).
        // SAFETY: new_ext is valid; the names are static.
        let mut gsl = unsafe { real(c"vkbgsl.so".as_ptr(), libc::RTLD_NOW, &new_ext) };
        if gsl.is_null() {
            // SAFETY: as above.
            gsl = unsafe { real(c"notgsl.so".as_ptr(), libc::RTLD_NOW, &new_ext) };
        }
        if gsl.is_null() {
            // SAFETY: as above.
            gsl = unsafe { real(c"libgsl.so".as_ptr(), libc::RTLD_NOW, &new_ext) };
        }

        if !gsl.is_null() {
            let alloc = dlsym(gsl, c"gsl_memory_alloc_pure");
            let alloc64 = dlsym(gsl, c"gsl_memory_alloc_pure_64");
            let free = dlsym(gsl, c"gsl_memory_free_pure");
            if (!alloc.is_null() || !alloc64.is_null()) && !free.is_null() {
                let init_gsl = dlsym(hook_handle, c"adrenotools_gsl_init");
                if init_gsl.is_null() {
                    return fallback(real, filename, flags, extinfo);
                }
                // SAFETY: exported by our gsl hook crate.
                let init_gsl: unsafe extern "C" fn(
                    *mut libc::c_void,
                    *mut libc::c_void,
                    *mut libc::c_void,
                ) = unsafe { std::mem::transmute(init_gsl) };
                // SAFETY: the three pointers are freshly resolved GSL symbols.
                unsafe { init_gsl(alloc, alloc64, free) };
                if !p.next_gpu_mapping.is_null() {
                    // SAFETY: the driver allocated this mapping for the first
                    // allocation after open; single-threaded by contract.
                    unsafe { (*p.next_gpu_mapping).gpu_addr = GPU_MAPPING_SUCCEEDED_MAGIC };
                }
                trace(format_args!("hook: applied libgsl_alloc_hook"));
            } else {
                trace(format_args!(
                    "hook: libgsl symbols missing, allocation hook inactive"
                ));
            }
        } else {
            trace(format_args!(
                "hook: no vendor GSL library loaded, allocation hook inactive"
            ));
        }
    }

    if p.feature_flags & FEATURE_CUSTOM != 0 {
        trace(format_args!("hook: loading custom driver by name"));
        // SAFETY: custom_driver_name points into the leaked parameter block.
        let handle = unsafe { real(p.custom_driver_name, flags, &new_ext) };
        if handle.is_null() {
            trace(format_args!(
                "hook: custom driver load failed: {}",
                dlerror_text()
            ));
            return fallback(real, filename, flags, extinfo);
        }
        return handle;
    }

    trace(format_args!("hook: loading default driver"));
    // SAFETY: filename is the original ICD path, now opened in the driver namespace.
    unsafe { real(filename, flags, &new_ext) }
}

/// Port of `hook_android_load_sphal_library` (`hook_impl.cpp:142-158`).
///
/// # Safety
/// `filename` must be NUL terminated.
pub unsafe fn hook_load_sphal_library(
    filename: *const libc::c_char,
    flags: libc::c_int,
) -> *mut libc::c_void {
    for name in [c"sphal", c"vendor", c"default"] {
        let namespace = linker::get_exported_namespace(name);
        if !namespace.is_null() {
            let info = DlextInfo {
                flags: linker::ANDROID_DLEXT_USE_NAMESPACE,
                library_namespace: namespace,
                ..Default::default()
            };
            // SAFETY: info is a valid dlextinfo; forwarding the caller's filename.
            return unsafe { hook_dlopen_ext(filename, flags, &info) };
        }
    }
    ptr::null_mut()
}

/// Port of `hook_fopen` (`hook_impl.cpp:160-170`).
///
/// The donor concatenates the redirect directory and the requested path verbatim,
/// which requires the directory to end in `/`. That contract is preserved.
///
/// # Safety
/// Both arguments must be NUL-terminated C strings.
pub unsafe fn hook_fopen(
    filename: *const libc::c_char,
    mode: *const libc::c_char,
) -> *mut libc::c_void {
    let Some(real) = real_fopen_cached() else {
        return ptr::null_mut();
    };
    if filename.is_null() {
        // SAFETY: the real fopen accepts NULL and returns NULL/EINVAL.
        return unsafe { real(filename, mode) };
    }
    // SAFETY: filename is non-NULL and NUL terminated.
    let name = unsafe { CStr::from_ptr(filename) };
    let bytes = name.to_bytes();
    if bytes.starts_with(b"/proc") || bytes.starts_with(b"/sys") {
        // SAFETY: pass-through for kernel pseudo files.
        return unsafe { real(filename, mode) };
    }
    let Some(p) = params() else {
        // SAFETY: no redirect configured (or init not called): behave like libc.
        return unsafe { real(filename, mode) };
    };
    let dir = if p.file_redirect_dir.is_null() {
        &[][..]
    } else {
        // SAFETY: the parameter block stores NUL-terminated strings.
        unsafe { CStr::from_ptr(p.file_redirect_dir) }.to_bytes()
    };
    let mut path = Vec::with_capacity(dir.len() + bytes.len() + 1);
    path.extend_from_slice(dir);
    path.extend_from_slice(bytes);
    path.push(0);
    // SAFETY: path is NUL terminated and lives for the call.
    unsafe { real(path.as_ptr().cast::<libc::c_char>(), mode) }
}

/// Mirror of the donor's private `GslMemDesc` (`hook_impl.cpp:173-179`).
#[repr(C)]
pub struct GslMemDesc {
    pub hostptr: *mut libc::c_void,
    pub gpuaddr: u64,
    pub size: u64,
    pub flags: u64,
    pub priv_: usize,
}

pub fn init_gsl(alloc: *mut libc::c_void, alloc64: *mut libc::c_void, free: *mut libc::c_void) {
    GSL_ALLOC.store(alloc, Ordering::Release);
    GSL_ALLOC_64.store(alloc64, Ordering::Release);
    GSL_FREE.store(free, Ordering::Release);
}

/// Port of `hook_gsl_memory_alloc_pure_64` (`hook_impl.cpp:181-200`).
///
/// # Safety
/// `mem_desc` must point at a `GslMemDesc` the driver is about to fill.
pub unsafe fn hook_gsl_alloc_64(size: u64, flags: u32, mem_desc: *mut libc::c_void) -> libc::c_int {
    if let Some(p) = params() {
        let mapping = p.next_gpu_mapping;
        if !mapping.is_null() {
            // SAFETY: the mapping is owned by the driver and valid while hooks run.
            let mapping = unsafe { &mut *mapping };
            if mapping.size == size && (mapping.flags & u64::from(flags)) == mapping.flags {
                if mem_desc.is_null() {
                    return -1;
                }
                // SAFETY: caller guarantees the GslMemDesc layout.
                let desc = unsafe { &mut *(mem_desc as *mut GslMemDesc) };
                desc.hostptr = mapping.host_ptr;
                desc.gpuaddr = mapping.gpu_addr;
                desc.size = mapping.size;
                desc.flags = mapping.flags;
                desc.priv_ = GSL_MEMDESC_IMPORTED_MAGIC;
                mapping.size = 0;
                mapping.gpu_addr = GPU_MAPPING_SUCCEEDED_MAGIC;
                return 0;
            }
        }
    }

    let alloc64 = GSL_ALLOC_64.load(Ordering::Acquire);
    if !alloc64.is_null() {
        // SAFETY: pointer was resolved from the vendor GSL library.
        let alloc64: GslAlloc64 = unsafe { std::mem::transmute(alloc64) };
        // SAFETY: forwarding the caller's arguments unchanged.
        return unsafe { alloc64(size, flags, mem_desc) };
    }
    let alloc = GSL_ALLOC.load(Ordering::Acquire);
    if !alloc.is_null() {
        // SAFETY: pointer was resolved from the vendor GSL library.
        let alloc: GslAlloc = unsafe { std::mem::transmute(alloc) };
        // SAFETY: forwarding the caller's arguments (size truncated like the donor).
        return unsafe { alloc(size as u32, flags, mem_desc) };
    }
    -1
}

/// Port of `hook_gsl_memory_free_pure` (`hook_impl.cpp:202-228`).
///
/// # Safety
/// `mem_desc` must point at a `GslMemDesc`.
pub unsafe fn hook_gsl_free(mem_desc: *mut libc::c_void) -> libc::c_int {
    if mem_desc.is_null() {
        return -1;
    }
    // SAFETY: caller guarantees the GslMemDesc layout.
    let desc = unsafe { &*(mem_desc as *const GslMemDesc) };
    if desc.priv_ == GSL_MEMDESC_IMPORTED_MAGIC {
        if let Err(error) = kgsl::free_gpuobj_by_addr(desc.gpuaddr) {
            trace(format_args!(
                "hook: freeing imported GPU object failed: {error}"
            ));
        }
        return 0;
    }
    let free = GSL_FREE.load(Ordering::Acquire);
    if free.is_null() {
        return -1;
    }
    // SAFETY: pointer was resolved from the vendor GSL library.
    let free: GslFree = unsafe { std::mem::transmute(free) };
    // SAFETY: forwarding the caller's argument unchanged.
    unsafe { free(mem_desc) }
}

/// A namespace handle for diagnostics/probes.
pub fn namespace_is_valid(namespace: Namespace) -> bool {
    !namespace.is_null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The parameter block is process-global by design; serialise tests that mutate it.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn vulkan_matcher_matches_donor_strstr() {
        assert!(contains_vulkan_dot(b"/vendor/lib64/hw/vulkan.adreno.so"));
        assert!(contains_vulkan_dot(b"vulkan.foo.so"));
        // `libvulkan.so` contains "vulkan." too; the donor's strstr matches it.
        assert!(contains_vulkan_dot(b"/system/lib64/libvulkan.so"));
        // A Turnip-style name does not: "vulkan_" is not "vulkan.".
        assert!(!contains_vulkan_dot(b"libvulkan_freedreno.so"));
        assert!(!contains_vulkan_dot(b""));
    }

    #[test]
    fn mapping_and_params_layouts_are_c_abi() {
        assert_eq!(std::mem::size_of::<GpuMapping>(), 32);
        assert_eq!(std::mem::size_of::<GslMemDesc>(), 40);
        assert_eq!(
            std::mem::size_of::<HookParams>(),
            8 + 5 * std::mem::size_of::<*const libc::c_char>()
                + std::mem::size_of::<*mut GpuMapping>()
        );
    }

    #[test]
    fn params_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut mapping = GpuMapping::default();
        let block = HookParams {
            feature_flags: FEATURE_CUSTOM | FEATURE_FILE_REDIRECT,
            tmp_lib_dir: ptr::null(),
            hook_lib_dir: c"/tmp/hooks".as_ptr(),
            custom_driver_dir: c"/tmp/driver/".as_ptr(),
            custom_driver_name: c"libvulkan_freedreno.so".as_ptr(),
            file_redirect_dir: c"/tmp/redirect/".as_ptr(),
            next_gpu_mapping: &mut mapping,
        };
        set_params(&block);
        let read = params().unwrap();
        assert_eq!(read.feature_flags, FEATURE_CUSTOM | FEATURE_FILE_REDIRECT);
        // SAFETY: pointers point at static C strings in this test.
        assert_eq!(
            unsafe { CStr::from_ptr(read.custom_driver_name) },
            c"libvulkan_freedreno.so"
        );
        assert_eq!(read.next_gpu_mapping, &raw mut mapping);
        set_params(ptr::null());
    }

    #[test]
    fn gsl_alloc_without_backend_returns_error() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_params(ptr::null());
        GSL_ALLOC.store(ptr::null_mut(), Ordering::Release);
        GSL_ALLOC_64.store(ptr::null_mut(), Ordering::Release);
        GSL_FREE.store(ptr::null_mut(), Ordering::Release);
        // SAFETY: null params means no interception; the call takes a NULL desc
        // only in the no-backend branch, which returns immediately.
        assert_eq!(unsafe { hook_gsl_alloc_64(16, 0, ptr::null_mut()) }, -1);
        // SAFETY: NULL is explicitly handled.
        assert_eq!(unsafe { hook_gsl_free(ptr::null_mut()) }, -1);
    }
}

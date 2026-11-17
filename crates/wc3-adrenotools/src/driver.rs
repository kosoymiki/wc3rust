//! Custom Vulkan driver loading — Rust port of `src/driver.cpp` plus the C API in
//! `include/adrenotools/driver.h`.
//!
//! Flow (`adrenotools_open_libvulkan`):
//!
//! 1. validate feature flags against the supplied parameters (the donor returns
//!    `nullptr` for every inconsistent combination; here the error is typed);
//! 2. create a shared namespace rooted at `hookLibDir` and link it to a copy of the
//!    default namespace, so the hook libraries can use libc/libdl/libandroid;
//! 3. load `libmain_hook.so` into that namespace with `RTLD_GLOBAL` — the library
//!    is built with `-z global` so the linker adds it to the namespace's preload
//!    list, which is what makes the Vulkan loader's `android_dlopen_ext` reach it;
//! 4. hand the parameter block to the hook;
//! 5. load a *unique*, soname-patched copy of `/system/lib64/libvulkan.so` inside
//!    the same namespace. The loader will then try to open an ICD
//!    (`…/vulkan.adreno.so` on stock Android), the hook intercepts that open, and
//!    loads the custom driver from `customDriverDir` instead.
//!
//! The returned pointer is the `dlopen` handle of the patched `libvulkan.so`; the
//! caller `dlsym`s `vkGetInstanceProcAddr` from it as usual.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

use crate::bcn::{self, BcnType};
use crate::hooks::{self, GpuMapping, HookParams};
use crate::kgsl;
use crate::linker::{self, Namespace};

/// Feature bits, mirroring `include/adrenotools/priv.h:10-14`.
pub struct FeatureFlags;
impl FeatureFlags {
    pub const CUSTOM: libc::c_int = hooks::FEATURE_CUSTOM;
    pub const FILE_REDIRECT: libc::c_int = hooks::FEATURE_FILE_REDIRECT;
    pub const GPU_MAPPING_IMPORT: libc::c_int = hooks::FEATURE_GPU_MAPPING;
}

const ALL_FEATURES: libc::c_int =
    FeatureFlags::CUSTOM | FeatureFlags::FILE_REDIRECT | FeatureFlags::GPU_MAPPING_IMPORT;

#[derive(Debug)]
pub enum DriverError {
    LinkerUnavailable,
    InvalidCombination(&'static str),
    MissingParameter(&'static str),
    PathMissing { what: &'static str, path: PathBuf },
    NamespaceCreation,
    NamespaceLink,
    MainHookLoad(String),
    MainHookInit,
    VulkanLoad(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LinkerUnavailable => {
                f.write_str("linkernsbypass could not resolve the hidden linker entry points")
            }
            Self::InvalidCombination(reason) => {
                write!(f, "invalid feature/parameter combination: {reason}")
            }
            Self::MissingParameter(what) => write!(f, "missing required parameter: {what}"),
            Self::PathMissing { what, path } => {
                write!(f, "{what} does not exist: {}", path.display())
            }
            Self::NamespaceCreation => f.write_str("android_create_namespace failed"),
            Self::NamespaceLink => {
                f.write_str("linking the hook namespace to the default namespace failed")
            }
            Self::MainHookLoad(error) => write!(f, "loading libmain_hook.so failed: {error}"),
            Self::MainHookInit => {
                f.write_str("libmain_hook.so does not export adrenotools_main_hook_init")
            }
            Self::VulkanLoad(error) => {
                write!(f, "loading the patched libvulkan.so failed: {error}")
            }
        }
    }
}

impl std::error::Error for DriverError {}

/// Result of opening a driver. `vulkan` is what a C caller receives; the remaining
/// fields exist for Rust callers and probes (e.g. to exercise the hook directly).
pub struct OpenedDriver {
    pub vulkan: *mut libc::c_void,
    /// `dlopen` handle of `libmain_hook.so` inside the hook namespace.
    pub hook: *mut libc::c_void,
    pub hook_namespace: Namespace,
    pub params: *const HookParams,
    pub mapping: *mut GpuMapping,
}

/// Pure flag/parameter consistency checks.
///
/// Mirrors `driver.cpp:21-64`; the donor returns `nullptr`, this returns the exact
/// reason. `has_*` describe presence, not validity (paths are checked separately).
pub fn validate_combination(
    feature_flags: libc::c_int,
    has_tmp_dir: bool,
    has_hook_dir: bool,
    has_custom_dir: bool,
    has_custom_name: bool,
    has_redirect_dir: bool,
    has_user_mapping: bool,
) -> Result<(), DriverError> {
    if feature_flags & !ALL_FEATURES != 0 {
        return Err(DriverError::InvalidCombination("unknown feature bits set"));
    }
    if !has_tmp_dir {
        // tmpLibDir is optional: memfd is used when it is absent.
    }
    if !has_hook_dir {
        return Err(DriverError::MissingParameter("hookLibDir"));
    }
    if feature_flags & FeatureFlags::FILE_REDIRECT == 0 && has_redirect_dir {
        return Err(DriverError::InvalidCombination(
            "fileRedirectDir supplied without ADRENOTOOLS_DRIVER_FILE_REDIRECT",
        ));
    }
    if feature_flags & FeatureFlags::CUSTOM == 0 && (has_custom_dir || has_custom_name) {
        return Err(DriverError::InvalidCombination(
            "customDriverDir/customDriverName supplied without ADRENOTOOLS_DRIVER_CUSTOM",
        ));
    }
    if feature_flags & FeatureFlags::GPU_MAPPING_IMPORT == 0 && has_user_mapping {
        return Err(DriverError::InvalidCombination(
            "userMappingHandle supplied without ADRENOTOOLS_DRIVER_GPU_MAPPING_IMPORT",
        ));
    }
    if feature_flags & FeatureFlags::CUSTOM != 0 && (!has_custom_dir || !has_custom_name) {
        return Err(DriverError::MissingParameter(
            "customDriverDir and customDriverName are both required with ADRENOTOOLS_DRIVER_CUSTOM",
        ));
    }
    if feature_flags & FeatureFlags::FILE_REDIRECT != 0 && !has_redirect_dir {
        return Err(DriverError::MissingParameter(
            "fileRedirectDir is required with ADRENOTOOLS_DRIVER_FILE_REDIRECT",
        ));
    }
    Ok(())
}

/// Join a driver directory and a soname, inserting `/` when the directory does not
/// already end with one.
///
/// The donor validates `customDriverDir + customDriverName` verbatim
/// (`driver.cpp:44`), which silently fails for a directory without a trailing
/// slash even though the linker's search path would accept it. The port
/// normalises; the original directory is still what gets passed to the linker.
pub fn join_driver_path(dir: &Path, name: &Path) -> PathBuf {
    let mut bytes = dir.as_os_str().as_bytes().to_vec();
    if !bytes.ends_with(b"/") {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name.as_os_str().as_bytes());
    PathBuf::from(OsString::from_vec(bytes))
}

unsafe fn cstr_to_path(ptr: *const libc::c_char) -> Option<PathBuf> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: contract of the C API; checked for NULL above.
    let c = unsafe { CStr::from_ptr(ptr) };
    Some(PathBuf::from(OsString::from_vec(c.to_bytes().to_vec())))
}

/// Duplicate a caller-owned C string into process-lifetime storage.
///
/// The donor copies the path strings into `std::string` members, so the caller may
/// free its buffers immediately. The port must do the same because hooks read the
/// strings long after `open_libvulkan` returns.
unsafe fn leak_cstr(ptr: *const libc::c_char) -> *const libc::c_char {
    if ptr.is_null() {
        return ptr::null();
    }
    // SAFETY: contract of the C API; strings are NUL terminated.
    let c = unsafe { CStr::from_ptr(ptr) };
    match CString::new(c.to_bytes()) {
        Ok(owned) => owned.into_raw() as *const libc::c_char,
        Err(_) => ptr::null(),
    }
}

fn require_exists(what: &'static str, path: PathBuf) -> Result<PathBuf, DriverError> {
    if path.exists() {
        Ok(path)
    } else {
        Err(DriverError::PathMissing { what, path })
    }
}

/// Loader used by the C API and by the donor (`driver.cpp:82`).
pub const SYSTEM_VULKAN_LOADER: &CStr = c"/system/lib64/libvulkan.so";

/// Open a hooked loader and load `customDriverName` when configured.
///
/// Uses the stock Android loader; see [`open_libvulkan_with_loader`] to bundle a
/// Khronos loader instead (required for Turnip: the AOSP shim resolves the vendor
/// HAL entry points, not the standard ICD interface).
///
/// # Safety
/// All pointers follow the C API contract in `include/adrenotools/driver.h`:
/// strings are NUL terminated or NULL; `user_mapping_handle` is writable when the
/// GPU mapping feature is enabled.
#[allow(clippy::too_many_arguments)]
pub unsafe fn open_libvulkan(
    dlopen_flags: libc::c_int,
    feature_flags: libc::c_int,
    tmp_lib_dir: *const libc::c_char,
    hook_lib_dir: *const libc::c_char,
    custom_driver_dir: *const libc::c_char,
    custom_driver_name: *const libc::c_char,
    file_redirect_dir: *const libc::c_char,
    user_mapping_handle: *mut *mut libc::c_void,
) -> Result<OpenedDriver, DriverError> {
    // SAFETY: the caller forwards the C API contract unchanged.
    unsafe {
        open_libvulkan_with_loader(
            SYSTEM_VULKAN_LOADER,
            dlopen_flags,
            feature_flags,
            tmp_lib_dir,
            hook_lib_dir,
            custom_driver_dir,
            custom_driver_name,
            file_redirect_dir,
            user_mapping_handle,
        )
    }
}

/// Same as [`open_libvulkan`] but loads `loader_path` instead of the system one.
///
/// # Safety
/// As [`open_libvulkan`]; `loader_path` must be a NUL-terminated path to a Vulkan
/// loader that calls `android_dlopen_ext` for its ICDs.
#[allow(clippy::too_many_arguments)]
pub unsafe fn open_libvulkan_with_loader(
    loader_path: &CStr,
    dlopen_flags: libc::c_int,
    feature_flags: libc::c_int,
    tmp_lib_dir: *const libc::c_char,
    hook_lib_dir: *const libc::c_char,
    custom_driver_dir: *const libc::c_char,
    custom_driver_name: *const libc::c_char,
    file_redirect_dir: *const libc::c_char,
    user_mapping_handle: *mut *mut libc::c_void,
) -> Result<OpenedDriver, DriverError> {
    let hook_path = unsafe { cstr_to_path(hook_lib_dir) };
    let custom_dir = unsafe { cstr_to_path(custom_driver_dir) };
    let custom_name = unsafe { cstr_to_path(custom_driver_name) };
    let redirect_dir = unsafe { cstr_to_path(file_redirect_dir) };
    let tmp_dir = unsafe { cstr_to_path(tmp_lib_dir) };

    validate_combination(
        feature_flags,
        tmp_dir.is_some(),
        hook_path.is_some(),
        custom_dir.is_some(),
        custom_name.is_some(),
        redirect_dir.is_some(),
        !user_mapping_handle.is_null(),
    )?;

    require_exists("hookLibDir", hook_path.expect("validated"))?;
    if feature_flags & FeatureFlags::CUSTOM != 0 {
        let dir = custom_dir.as_ref().expect("validated");
        let name = custom_name.as_ref().expect("validated");
        require_exists("custom driver", join_driver_path(dir, name))?;
        // The namespace default path must be a directory.
        require_exists("customDriverDir", dir.clone())?;
    }
    if feature_flags & FeatureFlags::FILE_REDIRECT != 0 {
        require_exists("fileRedirectDir", redirect_dir.clone().expect("validated"))?;
    }
    if let Some(tmp) = &tmp_dir {
        require_exists("tmpLibDir", tmp.clone())?;
    }

    if !linker::load_status() {
        return Err(DriverError::LinkerUnavailable);
    }

    let hook_dir_c = unsafe { leak_cstr(hook_lib_dir) };
    if hook_dir_c.is_null() {
        return Err(DriverError::MissingParameter("hookLibDir"));
    }

    // SAFETY: hook_dir_c is a leaked NUL-terminated string; the namespace name is
    // static and the parent is the root namespace.
    let hook_ns = unsafe {
        linker::create_namespace(
            c"adrenotools-libvulkan".as_ptr(),
            hook_dir_c,
            ptr::null(),
            linker::ANDROID_NAMESPACE_TYPE_SHARED,
            ptr::null(),
            ptr::null_mut(),
        )
    };
    if hook_ns.is_null() {
        return Err(DriverError::NamespaceCreation);
    }
    // SAFETY: hook_ns is a live namespace created above.
    if !unsafe { linker::link_namespace_to_default_all_libs(hook_ns) } {
        hooks::trace(format_args!(
            "open_libvulkan: linking hook namespace failed"
        ));
        return Err(DriverError::NamespaceLink);
    }

    // SAFETY: hook_ns is a live namespace; the library lives in hookLibDir.
    let main_hook = unsafe {
        linker::namespace_dlopen(
            c"libmain_hook.so",
            libc::RTLD_NOW | libc::RTLD_GLOBAL,
            hook_ns,
        )
    };
    if main_hook.is_null() {
        return Err(DriverError::MainHookLoad(linker::last_dl_error()));
    }
    // SAFETY: symbol lookup on a live handle.
    let init = unsafe { libc::dlsym(main_hook, c"adrenotools_main_hook_init".as_ptr()) };
    if init.is_null() {
        return Err(DriverError::MainHookInit);
    }

    let mapping = if feature_flags & FeatureFlags::GPU_MAPPING_IMPORT != 0 {
        Box::into_raw(Box::new(GpuMapping::default()))
    } else {
        ptr::null_mut()
    };
    if !user_mapping_handle.is_null() {
        // SAFETY: caller supplied a writable pointer per the C API.
        unsafe { *user_mapping_handle = mapping.cast::<libc::c_void>() };
    }

    let params = Box::into_raw(Box::new(HookParams {
        feature_flags,
        tmp_lib_dir: unsafe { leak_cstr(tmp_lib_dir) },
        hook_lib_dir: hook_dir_c,
        custom_driver_dir: unsafe { leak_cstr(custom_driver_dir) },
        custom_driver_name: unsafe { leak_cstr(custom_driver_name) },
        file_redirect_dir: unsafe { leak_cstr(file_redirect_dir) },
        next_gpu_mapping: mapping,
    }));

    // SAFETY: exported by libmain_hook.so with this signature; params leak for the
    // process lifetime, matching the donor's heap allocation.
    let init: unsafe extern "C" fn(*const HookParams) = unsafe { std::mem::transmute(init) };
    unsafe { init(params) };

    // On API >= 29 the donor always uses memfd and ignores tmpLibDir.
    let tmp_for_unique = if linker::device_api_level() >= 29 || tmp_lib_dir.is_null() {
        None
    } else {
        // SAFETY: validated as non-NULL and NUL terminated by the C API contract.
        Some(unsafe { CStr::from_ptr(tmp_lib_dir) })
    };

    // SAFETY: hook_ns is live; the loader path exists on every 64-bit Android.
    let vulkan = unsafe {
        linker::namespace_dlopen_unique(loader_path, tmp_for_unique, dlopen_flags, hook_ns)
    };
    if vulkan.is_null() {
        return Err(DriverError::VulkanLoad(linker::last_dl_error()));
    }

    Ok(OpenedDriver {
        vulkan,
        hook: main_hook,
        hook_namespace: hook_ns,
        params,
        mapping,
    })
}

/// Import a CPU-mapped region; the next matching allocation becomes a view of it.
///
/// # Safety
/// `handle` must come from [`open_libvulkan`] with GPU mapping enabled.
pub unsafe fn import_user_mem(
    handle: *mut GpuMapping,
    host_ptr: *mut libc::c_void,
    size: u64,
) -> bool {
    if handle.is_null() || host_ptr.is_null() || size == 0 {
        return false;
    }
    match kgsl::import_user_mem(host_ptr, size) {
        Ok(gpu_addr) => {
            // SAFETY: handle is a leaked GpuMapping owned by this API.
            let mapping = unsafe { &mut *handle };
            mapping.host_ptr = host_ptr;
            mapping.gpu_addr = gpu_addr;
            mapping.size = size;
            mapping.flags = kgsl::GPU_MAPPING_FLAGS;
            true
        }
        Err(error) => {
            hooks::trace(format_args!("import_user_mem failed: {error}"));
            false
        }
    }
}

/// Allocate GPU memory and record it as the next mapping.
///
/// # Safety
/// Same contract as [`import_user_mem`]; `size` must be a valid pointer.
pub unsafe fn mem_gpu_allocate(handle: *mut GpuMapping, size: *mut u64) -> bool {
    if handle.is_null() || size.is_null() {
        return false;
    }
    // SAFETY: caller guaranteed a writable u64 per the C API.
    let requested = unsafe { *size };
    match kgsl::alloc_gpu(requested) {
        Ok((gpu_addr, mmap_size)) => {
            // SAFETY: as above.
            unsafe { *size = mmap_size };
            // SAFETY: handle is a leaked GpuMapping owned by this API.
            let mapping = unsafe { &mut *handle };
            mapping.host_ptr = ptr::null_mut();
            mapping.gpu_addr = gpu_addr;
            mapping.size = mmap_size;
            mapping.flags = kgsl::GPU_MAPPING_FLAGS;
            true
        }
        Err(error) => {
            hooks::trace(format_args!("mem_gpu_allocate failed: {error}"));
            false
        }
    }
}

/// Map the last allocated GPU object into caller-provided host memory.
///
/// # Safety
/// `host_ptr` must be a reserved region of `size` bytes.
pub unsafe fn mem_cpu_map(handle: *mut GpuMapping, host_ptr: *mut libc::c_void, size: u64) -> bool {
    if handle.is_null() || host_ptr.is_null() {
        return false;
    }
    // SAFETY: handle is a leaked GpuMapping owned by this API.
    let mapping = unsafe { &mut *handle };
    // SAFETY: caller guarantees the reserved region.
    match unsafe { kgsl::cpu_map(host_ptr, size, mapping.gpu_addr) } {
        Ok(mapped) => {
            mapping.host_ptr = mapped;
            true
        }
        Err(error) => {
            hooks::trace(format_args!("mem_cpu_map failed: {error}"));
            false
        }
    }
}

/// True once the GSL allocation hook has claimed the mapping.
///
/// # Safety
/// `handle` must come from [`open_libvulkan`].
pub unsafe fn validate_gpu_mapping(handle: *mut GpuMapping) -> bool {
    if handle.is_null() {
        return false;
    }
    // SAFETY: handle is a leaked GpuMapping owned by this API.
    unsafe { (*handle).gpu_addr == hooks::GPU_MAPPING_SUCCEEDED_MAGIC }
}

/// Force maximum GPU clocks.
pub fn set_turbo(turbo: bool) {
    if let Err(error) = kgsl::set_turbo(turbo) {
        hooks::trace(format_args!("set_turbo failed: {error}"));
    }
}

/// Build the `-z global` hook library name list for packaging checks.
pub fn hook_libraries() -> [&'static str; 3] {
    [
        "libmain_hook.so",
        "libfile_redirect_hook.so",
        "libgsl_alloc_hook.so",
    ]
}

/* ------------------------------------------------------------------------- */
/* C ABI (include/adrenotools/driver.h + bcenabler.h)                        */
/* ------------------------------------------------------------------------- */

/// C ABI for [`open_libvulkan`].
///
/// # Safety
/// All pointers follow the C API contract in `include/adrenotools/driver.h`:
/// strings are NUL terminated or NULL; `user_mapping_handle` is writable when the
/// GPU mapping feature is enabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_open_libvulkan(
    dlopen_flags: libc::c_int,
    feature_flags: libc::c_int,
    tmp_lib_dir: *const libc::c_char,
    hook_lib_dir: *const libc::c_char,
    custom_driver_dir: *const libc::c_char,
    custom_driver_name: *const libc::c_char,
    file_redirect_dir: *const libc::c_char,
    user_mapping_handle: *mut *mut libc::c_void,
) -> *mut libc::c_void {
    // SAFETY: C contract; arguments are forwarded unchanged.
    match unsafe {
        open_libvulkan(
            dlopen_flags,
            feature_flags,
            tmp_lib_dir,
            hook_lib_dir,
            custom_driver_dir,
            custom_driver_name,
            file_redirect_dir,
            user_mapping_handle,
        )
    } {
        Ok(opened) => opened.vulkan,
        Err(error) => {
            hooks::trace(format_args!("adrenotools_open_libvulkan: {error}"));
            ptr::null_mut()
        }
    }
}

/// C ABI for [`import_user_mem`].
///
/// # Safety
/// `handle` must be the mapping pointer returned by
/// [`adrenotools_open_libvulkan`] with GPU mapping enabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_import_user_mem(
    handle: *mut libc::c_void,
    host_ptr: *mut libc::c_void,
    size: u64,
) -> bool {
    // SAFETY: handle is the user mapping pointer returned by open_libvulkan.
    unsafe { import_user_mem(handle.cast::<GpuMapping>(), host_ptr, size) }
}

/// C ABI for [`mem_gpu_allocate`].
///
/// # Safety
/// `handle` must come from [`adrenotools_open_libvulkan`]; `size` must be a
/// writable `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_mem_gpu_allocate(
    handle: *mut libc::c_void,
    size: *mut u64,
) -> bool {
    // SAFETY: handle is the user mapping pointer returned by open_libvulkan.
    unsafe { mem_gpu_allocate(handle.cast::<GpuMapping>(), size) }
}

/// C ABI for [`mem_cpu_map`].
///
/// # Safety
/// `handle` must come from [`adrenotools_open_libvulkan`]; `host_ptr` must be a
/// reserved region of `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_mem_cpu_map(
    handle: *mut libc::c_void,
    host_ptr: *mut libc::c_void,
    size: u64,
) -> bool {
    // SAFETY: handle is the user mapping pointer returned by open_libvulkan.
    unsafe { mem_cpu_map(handle.cast::<GpuMapping>(), host_ptr, size) }
}

/// C ABI for [`validate_gpu_mapping`].
///
/// # Safety
/// `handle` must be the mapping pointer returned by
/// [`adrenotools_open_libvulkan`] with GPU mapping enabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_validate_gpu_mapping(handle: *mut libc::c_void) -> bool {
    // SAFETY: handle is the user mapping pointer returned by open_libvulkan.
    unsafe { validate_gpu_mapping(handle.cast::<GpuMapping>()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn adrenotools_set_turbo(turbo: bool) {
    set_turbo(turbo);
}

#[unsafe(no_mangle)]
pub extern "C" fn adrenotools_get_bcn_type(major: u32, minor: u32, vendor_id: u32) -> u32 {
    bcn::get_bcn_type(major, minor, vendor_id) as u32
}

/// C ABI for [`bcn::patch_bcn`].
///
/// # Safety
/// `function` must be the driver's `vkGetPhysicalDeviceFormatProperties` and the
/// caller must have checked [`adrenotools_get_bcn_type`] == patchable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adrenotools_patch_bcn(function: *mut libc::c_void) -> bool {
    // SAFETY: forwarded from the caller per the C API contract.
    unsafe { bcn::patch_bcn(function) }.is_ok()
}

/// Exposed for tests/probes that want the enum as a Rust value.
pub fn bcn_type(major: u32, minor: u32, vendor_id: u32) -> BcnType {
    bcn::get_bcn_type(major, minor, vendor_id)
}

/// `Path` helper used by the probe.
pub fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsStr::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_feature_parameter_mismatches() {
        assert!(matches!(
            validate_combination(0, false, true, true, false, false, false),
            Err(DriverError::InvalidCombination(_))
        ));
        assert!(matches!(
            validate_combination(0, false, true, false, false, true, false),
            Err(DriverError::InvalidCombination(_))
        ));
        assert!(matches!(
            validate_combination(0, false, true, false, false, false, true),
            Err(DriverError::InvalidCombination(_))
        ));
        assert!(matches!(
            validate_combination(FeatureFlags::CUSTOM, false, true, true, false, false, false),
            Err(DriverError::MissingParameter(_))
        ));
        assert!(matches!(
            validate_combination(
                FeatureFlags::FILE_REDIRECT,
                false,
                true,
                false,
                false,
                false,
                false
            ),
            Err(DriverError::MissingParameter(_))
        ));
        assert!(matches!(
            validate_combination(0, false, false, false, false, false, false),
            Err(DriverError::MissingParameter(_))
        ));
        assert!(validate_combination(0, false, true, false, false, false, false).is_ok());
        assert!(
            validate_combination(
                FeatureFlags::CUSTOM
                    | FeatureFlags::FILE_REDIRECT
                    | FeatureFlags::GPU_MAPPING_IMPORT,
                true,
                true,
                true,
                true,
                true,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_unknown_feature_bits() {
        assert!(matches!(
            validate_combination(0x4000, false, true, false, false, false, false),
            Err(DriverError::InvalidCombination(_))
        ));
    }

    #[test]
    fn joins_driver_paths_without_losing_bytes() {
        assert_eq!(
            join_driver_path(
                Path::new("/data/app/files"),
                Path::new("libvulkan_freedreno.so")
            ),
            PathBuf::from("/data/app/files/libvulkan_freedreno.so")
        );
        assert_eq!(
            join_driver_path(
                Path::new("/data/app/files/"),
                Path::new("libvulkan_freedreno.so")
            ),
            PathBuf::from("/data/app/files/libvulkan_freedreno.so")
        );
    }

    #[test]
    fn hook_library_list_is_stable() {
        assert_eq!(hook_libraries()[0], "libmain_hook.so");
        assert_eq!(hook_libraries().len(), 3);
    }
}

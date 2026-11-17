//! Android linker-namespace bypass — Rust port of
//! `lib/linkernsbypass/android_linker_ns.cpp`.
//!
//! Android does not expose namespace APIs through the NDK. The donor resolves the
//! hidden linker entry points in two steps:
//!
//! 1. `dlopen()` is a thin wrapper that forwards to the linker's internal
//!    `__loader_dlopen(filename, flags, caller_addr)`. Walking its first `BL`
//!    recovers `__loader_dlopen`, and calling it with `caller_addr` pointing at
//!    `dlopen` itself makes the linker believe the caller is the linker, which
//!    grants access to the unrestricted default namespace.
//! 2. With that handle, `__loader_android_link_namespaces*` and
//!    `__loader_android_create_namespace` / `__loader_android_get_exported_namespace`
//!    are resolved from `ld-android.so` and `libdl_android.so`.
//!
//! Every load in this module goes through the *real* `android_dlopen_ext` captured
//! from `libdl.so`. That is deliberate: the hook libraries redefine
//! `android_dlopen_ext`, and the donor relies on link-order subtleties to avoid
//! recursing into them. The Rust port removes that dependency entirely.

use std::ffi::{CStr, CString};
use std::io;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};

use crate::elf;
use crate::proc_maps;

pub type Namespace = *mut libc::c_void;

pub const ANDROID_NAMESPACE_TYPE_REGULAR: u64 = 0;
pub const ANDROID_NAMESPACE_TYPE_ISOLATED: u64 = 1;
pub const ANDROID_NAMESPACE_TYPE_SHARED: u64 = 2;

pub const ANDROID_DLEXT_USE_LIBRARY_FD: u64 = 0x10;
pub const ANDROID_DLEXT_USE_NAMESPACE: u64 = 0x200;

/// Mirror of `android_dlextinfo` from `android/dlext.h`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DlextInfo {
    pub flags: u64,
    pub reserved_addr: *mut libc::c_void,
    pub reserved_size: usize,
    pub relro_fd: libc::c_int,
    pub library_fd: libc::c_int,
    pub library_fd_offset: i64,
    pub library_namespace: Namespace,
}

impl Default for DlextInfo {
    fn default() -> Self {
        Self {
            flags: 0,
            reserved_addr: ptr::null_mut(),
            reserved_size: 0,
            relro_fd: -1,
            library_fd: -1,
            library_fd_offset: 0,
            library_namespace: ptr::null_mut(),
        }
    }
}

const _: () = assert!(std::mem::size_of::<DlextInfo>() == 48);

pub type RealDlopenExt =
    unsafe extern "C" fn(*const libc::c_char, libc::c_int, *const DlextInfo) -> *mut libc::c_void;
type LoaderDlopen = unsafe extern "C" fn(
    *const libc::c_char,
    libc::c_int,
    *const libc::c_void,
) -> *mut libc::c_void;
type CreateNamespace = unsafe extern "C" fn(
    *const libc::c_char,
    *const libc::c_char,
    *const libc::c_char,
    u64,
    *const libc::c_char,
    Namespace,
    *const libc::c_void,
) -> Namespace;
type GetExportedNamespace = unsafe extern "C" fn(*const libc::c_char) -> Namespace;
type LinkNamespaces = unsafe extern "C" fn(Namespace, Namespace, *const libc::c_char) -> bool;
type LinkNamespacesAllLibs = unsafe extern "C" fn(Namespace, Namespace) -> bool;

/// Resolved entry points, stored as `usize` so the `OnceLock` stays `Send + Sync`.
#[derive(Clone, Copy)]
struct Symbols {
    create_namespace: usize,
    get_exported_namespace: usize,
    link_namespaces: usize,
    link_namespaces_all_libs: usize,
    real_dlopen_ext: usize,
    /// Address of libdl's `dlopen`, used both as the linker-caller token and as
    /// the scan origin for `__loader_dlopen`.
    dlopen_addr: usize,
}

static SYMBOLS: OnceLock<Option<Symbols>> = OnceLock::new();
static DEFAULT_NAMESPACE: OnceLock<usize> = OnceLock::new();
static TARGET_ID: AtomicU16 = AtomicU16::new(0);

unsafe extern "C" {
    fn __system_property_get(name: *const libc::c_char, value: *mut libc::c_char) -> libc::c_int;
}

/// `ro.build.version.sdk`, or 0 when the property cannot be read.
pub fn device_api_level() -> libc::c_int {
    let mut buf = [0 as libc::c_char; 92];
    // SAFETY: both pointers are valid for the duration of the call.
    let len = unsafe { __system_property_get(c"ro.build.version.sdk".as_ptr(), buf.as_mut_ptr()) };
    if len <= 0 {
        return 0;
    }
    // SAFETY: property values are NUL terminated within the buffer.
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// True when the hidden linker entry points were resolved successfully.
pub fn load_status() -> bool {
    symbols().is_some()
}

fn symbols() -> Option<&'static Symbols> {
    SYMBOLS
        .get_or_init(|| {
            // SAFETY: resolution only calls libc/linker functions with valid strings.
            unsafe { resolve() }
        })
        .as_ref()
}

fn page_size() -> usize {
    // SAFETY: sysconf has no preconditions.
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value <= 0 { 4096 } else { value as usize }
}

/// Make `[addr, addr+len)` readable, writable and executable.
fn make_rwx(addr: usize, len: usize) -> io::Result<()> {
    let ps = page_size();
    let start = addr & !(ps - 1);
    let end = addr
        .checked_add(len)
        .and_then(|v| v.checked_add(ps - 1))
        .map(|v| v & !(ps - 1))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: the range is page aligned and describes this process's memory.
    let rc = unsafe {
        libc::mprotect(
            start as *mut libc::c_void,
            end - start,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Walk `dlopen`'s body to the first `BL` and return its target
/// (`__loader_dlopen`).
///
/// # Safety
/// `dlopen_addr` must point at executable code.
unsafe fn find_loader_dlopen(dlopen_addr: usize) -> Option<*const libc::c_void> {
    let regions = proc_maps::read_self().ok()?;
    let region = proc_maps::containing(&regions, dlopen_addr)?;
    if !region.executable {
        return None;
    }
    // Some devices map executables `--x`; the donor protects one page, the port
    // protects the whole containing mapping so every scanned word is readable.
    make_rwx(region.start, region.end - region.start).ok()?;
    let mut pc = dlopen_addr;
    while pc + 4 <= region.end {
        // SAFETY: the page is readable after make_rwx, and pc+4 <= region.end.
        let word = unsafe { ptr::read_unaligned(pc as *const u32) };
        if let Some(target) = crate::aarch64::decode_bl_target(pc, word)
            && proc_maps::containing(&regions, target).is_some()
        {
            return Some(target as *const libc::c_void);
        }
        pc += 4;
    }
    None
}

/// # Safety
/// Runs at most once; mutates process-wide linker state.
unsafe fn resolve() -> Option<Symbols> {
    if device_api_level() < 28 {
        return None;
    }

    // SAFETY: libdl.so always exists on Android; both names are valid C strings.
    let libdl = unsafe { libc::dlopen(c"libdl.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if libdl.is_null() {
        return None;
    }
    // SAFETY: handle is valid; dlsym returns NULL for missing symbols.
    let dlopen_addr = unsafe { libc::dlsym(libdl, c"dlopen".as_ptr()) } as usize;
    let real_dlopen_ext = unsafe { libc::dlsym(libdl, c"android_dlopen_ext".as_ptr()) } as usize;
    if dlopen_addr == 0 || real_dlopen_ext == 0 {
        // SAFETY: handle came from dlopen.
        unsafe { libc::dlclose(libdl) };
        return None;
    }

    // SAFETY: dlopen_addr points into libdl's executable mapping.
    let loader_dlopen_ptr = unsafe { find_loader_dlopen(dlopen_addr)? };
    // SAFETY: target is a real function taking (filename, flags, caller).
    let loader_dlopen: LoaderDlopen = unsafe { std::mem::transmute(loader_dlopen_ptr) };

    // SAFETY: loader_dlopen is the linker's internal entry point; the caller token
    // is libdl's dlopen, which grants the unrestricted default namespace.
    let ld_android = unsafe {
        loader_dlopen(
            c"ld-android.so".as_ptr(),
            libc::RTLD_LAZY,
            dlopen_addr as *const libc::c_void,
        )
    };
    if ld_android.is_null() {
        return None;
    }

    // SAFETY: handle is valid; the symbols are exported by ld-android.so.
    let link_all = unsafe {
        libc::dlsym(
            ld_android,
            c"__loader_android_link_namespaces_all_libs".as_ptr(),
        )
    } as usize;
    let link_ns =
        unsafe { libc::dlsym(ld_android, c"__loader_android_link_namespaces".as_ptr()) } as usize;

    // SAFETY: as above, with libdl_android.so.
    let dl_android = unsafe {
        loader_dlopen(
            c"libdl_android.so".as_ptr(),
            libc::RTLD_LAZY,
            dlopen_addr as *const libc::c_void,
        )
    };
    if dl_android.is_null() {
        return None;
    }
    // SAFETY: handle is valid; the symbols are exported by libdl_android.so.
    let create_namespace =
        unsafe { libc::dlsym(dl_android, c"__loader_android_create_namespace".as_ptr()) } as usize;
    let get_exported = unsafe {
        libc::dlsym(
            dl_android,
            c"__loader_android_get_exported_namespace".as_ptr(),
        )
    } as usize;

    if link_all == 0 || link_ns == 0 || create_namespace == 0 || get_exported == 0 {
        return None;
    }

    Some(Symbols {
        create_namespace,
        get_exported_namespace: get_exported,
        link_namespaces: link_ns,
        link_namespaces_all_libs: link_all,
        real_dlopen_ext,
        dlopen_addr,
    })
}

/// The real `android_dlopen_ext` from libdl, bypassing any interposer.
pub fn real_dlopen_ext() -> Option<RealDlopenExt> {
    let symbols = symbols()?;
    // SAFETY: the address was stored from a successful dlsym of the documented
    // android_dlopen_ext signature.
    Some(unsafe { std::mem::transmute::<usize, RealDlopenExt>(symbols.real_dlopen_ext) })
}

/// Last `dlerror()` message, or a placeholder when none is set.
pub fn last_dl_error() -> String {
    // SAFETY: dlerror returns a NUL-terminated string or NULL; reading it is safe.
    let ptr = unsafe { libc::dlerror() };
    if ptr.is_null() {
        return "no dlerror message".to_string();
    }
    // SAFETY: as above.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Create a linker namespace. `caller` is this wrapper's return address, matching
/// `__builtin_return_address(0)` in the donor.
///
/// # Safety
/// All string pointers must be NUL-terminated or NULL.
#[inline(never)]
pub unsafe fn create_namespace(
    name: *const libc::c_char,
    ld_library_path: *const libc::c_char,
    default_library_path: *const libc::c_char,
    namespace_type: u64,
    permitted_when_isolated_path: *const libc::c_char,
    parent_namespace: Namespace,
) -> Namespace {
    let caller: usize;
    // SAFETY: reading x30 immediately on entry yields the return address into our
    // caller, which is what the linker uses to pick the caller's namespace.
    unsafe {
        core::arch::asm!("mov {}, x30", out(reg) caller, options(nostack, preserves_flags));
    }
    let Some(symbols) = symbols() else {
        return ptr::null_mut();
    };
    // SAFETY: the function pointer was resolved from __loader_android_create_namespace.
    let create: CreateNamespace = unsafe { std::mem::transmute(symbols.create_namespace) };
    // SAFETY: caller forwarded the same contract as the donor.
    unsafe {
        create(
            name,
            ld_library_path,
            default_library_path,
            namespace_type,
            permitted_when_isolated_path,
            parent_namespace,
            caller as *const libc::c_void,
        )
    }
}

/// Create a namespace with `dlopen` as the caller token, i.e. the unrestricted
/// default namespace (`android_create_namespace_escape`).
///
/// # Safety
/// Same contract as [`create_namespace`].
pub unsafe fn create_namespace_escape(
    name: *const libc::c_char,
    ld_library_path: *const libc::c_char,
    default_library_path: *const libc::c_char,
    namespace_type: u64,
    permitted_when_isolated_path: *const libc::c_char,
    parent_namespace: Namespace,
) -> Namespace {
    let Some(symbols) = symbols() else {
        return ptr::null_mut();
    };
    // SAFETY: see create_namespace.
    let create: CreateNamespace = unsafe { std::mem::transmute(symbols.create_namespace) };
    // SAFETY: the caller token is libdl's dlopen, which the linker maps to the
    // default (root) namespace.
    unsafe {
        create(
            name,
            ld_library_path,
            default_library_path,
            namespace_type,
            permitted_when_isolated_path,
            parent_namespace,
            symbols.dlopen_addr as *const libc::c_void,
        )
    }
}

/// Link every library of a copy of the default namespace into `to`.
///
/// # Safety
/// `to` must be a live linker namespace handle.
pub unsafe fn link_namespace_to_default_all_libs(to: Namespace) -> bool {
    let Some(symbols) = symbols() else {
        return false;
    };
    let default = *DEFAULT_NAMESPACE.get_or_init(|| {
        // SAFETY: building a default-copy namespace is the documented escape.
        unsafe {
            create_namespace_escape(
                c"default_copy".as_ptr(),
                ptr::null(),
                ptr::null(),
                ANDROID_NAMESPACE_TYPE_SHARED,
                ptr::null(),
                ptr::null_mut(),
            )
        }
    } as usize);
    if default == 0 {
        return false;
    }
    // SAFETY: both namespaces came from the linker.
    let link: LinkNamespacesAllLibs =
        unsafe { std::mem::transmute(symbols.link_namespaces_all_libs) };
    // SAFETY: as above.
    unsafe { link(to, default as Namespace) }
}

/// `android_link_namespaces(from, to, shared_libs)`.
///
/// # Safety
/// `from` and `to` must be live linker namespace handles.
pub unsafe fn link_namespaces(from: Namespace, to: Namespace, shared_libs: &CStr) -> bool {
    let Some(symbols) = symbols() else {
        return false;
    };
    // SAFETY: function pointer resolved from __loader_android_link_namespaces.
    let link: LinkNamespaces = unsafe { std::mem::transmute(symbols.link_namespaces) };
    // SAFETY: caller guarantees the namespace handles; the string is NUL terminated.
    unsafe { link(from, to, shared_libs.as_ptr()) }
}

/// `android_get_exported_namespace(name)`.
pub fn get_exported_namespace(name: &CStr) -> Namespace {
    let Some(symbols) = symbols() else {
        return ptr::null_mut();
    };
    // SAFETY: function pointer resolved from __loader_android_get_exported_namespace.
    let get: GetExportedNamespace = unsafe { std::mem::transmute(symbols.get_exported_namespace) };
    // SAFETY: name is NUL terminated.
    unsafe { get(name.as_ptr()) }
}

/// Open `filename` inside `namespace`.
///
/// # Safety
/// `filename` must be NUL terminated.
pub unsafe fn namespace_dlopen(
    filename: &CStr,
    flags: libc::c_int,
    namespace: Namespace,
) -> *mut libc::c_void {
    let Some(real) = real_dlopen_ext() else {
        return ptr::null_mut();
    };
    let info = DlextInfo {
        flags: ANDROID_DLEXT_USE_NAMESPACE,
        library_namespace: namespace,
        ..Default::default()
    };
    // SAFETY: info is a valid android_dlextinfo; filename is NUL terminated.
    unsafe { real(filename.as_ptr(), flags, &info) }
}

/// Force a *unique* instance of `lib_path` into `namespace`.
///
/// Copies the ELF into a memfd (API >= 29) or into `target_dir`, overwrites the
/// first two soname characters with a rotating counter, then loads it by fd. The
/// donor flow is `android_linker_ns.cpp:81-122`.
///
/// # Safety
/// `lib_path` must be NUL terminated; `target_dir`, when present, must be a
/// writable directory.
pub unsafe fn namespace_dlopen_unique(
    lib_path: &CStr,
    target_dir: Option<&CStr>,
    flags: libc::c_int,
    namespace: Namespace,
) -> *mut libc::c_void {
    let Some(real) = real_dlopen_ext() else {
        return ptr::null_mut();
    };
    let id = TARGET_ID.fetch_add(1, Ordering::Relaxed);

    let fd = match target_dir {
        Some(dir) => {
            let mut path = Vec::with_capacity(dir.to_bytes().len() + 32);
            path.extend_from_slice(dir.to_bytes());
            path.push(b'/');
            path.extend_from_slice(id.to_string().as_bytes());
            path.extend_from_slice(b"_patched.so");
            let Ok(path) = CString::new(path) else {
                return ptr::null_mut();
            };
            // SAFETY: path is NUL terminated; mode 0600.
            unsafe { libc::open(path.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) }
        }
        None => {
            // SAFETY: syscall with a NUL-terminated name and flags 0.
            unsafe { libc::syscall(SYS_MEMFD_CREATE, lib_path.as_ptr(), 0) as libc::c_int }
        }
    };
    if fd < 0 {
        return ptr::null_mut();
    }

    // The donor uses a 3-byte buffer with snprintf("%03u"), so at most two
    // characters are ever written. Reproduce that exactly.
    let mut patch = Vec::with_capacity(3);
    patch.extend_from_slice(format!("{id:03}").as_bytes());
    patch.truncate(2);

    if let Err(error) = elf::patch_soname_file(lib_path, fd, &patch) {
        crate::hooks::trace(format_args!("soname patch failed: {error}"));
        // SAFETY: fd is owned here and no longer needed.
        unsafe { libc::close(fd) };
        return ptr::null_mut();
    }

    let info = DlextInfo {
        flags: ANDROID_DLEXT_USE_NAMESPACE | ANDROID_DLEXT_USE_LIBRARY_FD,
        library_fd: fd,
        library_namespace: namespace,
        ..Default::default()
    };
    let path = CString::new(format!("/proc/self/fd/{fd}")).expect("fd path has no NUL");
    // SAFETY: info is valid; the linker duplicates the descriptor.
    unsafe { real(path.as_ptr(), flags, &info) }
}

/// aarch64 `memfd_create` syscall number.
const SYS_MEMFD_CREATE: libc::c_long = 279;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlext_layout_matches_bionic() {
        assert_eq!(std::mem::size_of::<DlextInfo>(), 48);
        assert_eq!(std::mem::align_of::<DlextInfo>(), 8);
    }

    #[test]
    fn api_level_is_readable_on_device() {
        let api = device_api_level();
        // Termux runs on Android 9+; 0 would mean the property probe is broken.
        assert!(api >= 28, "unexpected API level {api}");
    }

    #[test]
    fn linker_symbols_resolve() {
        assert!(
            load_status(),
            "hidden linker entry points were not resolved"
        );
        assert!(real_dlopen_ext().is_some());
    }
}

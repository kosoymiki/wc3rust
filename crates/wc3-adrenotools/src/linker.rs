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

/// Oldest Android release with the hidden `__loader_*` namespace entry points
/// (`__loader_dlopen`, `__loader_android_*`): Android 9 / API 28.
///
/// The scan below never trusts the version alone. It verifies the resolved
/// address against `/proc/self/maps` and rejects any layout it does not
/// recognise, so a future linker can only make the Turnip path unavailable
/// (`DriverError::LinkerUnavailable`), never call an arbitrary address.
pub const MIN_SUPPORTED_API: libc::c_int = 28;

/// Newest Android release on which the `dlopen` wrapper layout and the PLT
/// stub were verified on hardware (Android 16 / API 36, bionic
/// `libdl.so` at `/apex/com.android.runtime/lib64/bionic/libdl.so`).
///
/// Newer releases are accepted only when the same runtime fingerprint checks
/// pass; they are not gated by the version number.
pub const MAX_VERIFIED_API: libc::c_int = 36;

/// Bionic's `dlopen` wrapper is a 36-byte function today. The scan is bounded
/// so a layout change cannot walk into the next function's branches and mistake
/// an unrelated call for `__loader_dlopen`.
const DLOPEN_SCAN_BYTES: usize = 256;

/// Why a linker layout was rejected. Every error leaves the process with the
/// original page protections: reading uses [`ReadableWindow`], and the scan
/// never asks for write or execute permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoaderScanError {
    NotMapped,
    NotExecutable,
    NotReadable,
    NoLoaderCall,
    TargetNotMapped,
    TargetNotExecutable,
    TargetNotLinker,
    UnsupportedLayout(&'static str),
}

impl std::fmt::Display for LoaderScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMapped => f.write_str("address is not mapped"),
            Self::NotExecutable => f.write_str("source address is not executable"),
            Self::NotReadable => f.write_str("source address cannot be made readable"),
            Self::NoLoaderCall => f.write_str("no loader call found in the dlopen wrapper"),
            Self::TargetNotMapped => f.write_str("BL target is not mapped"),
            Self::TargetNotExecutable => f.write_str("BL target is not executable"),
            Self::TargetNotLinker => f.write_str("BL target is not a linker mapping"),
            Self::UnsupportedLayout(what) => write!(f, "unsupported linker layout: {what}"),
        }
    }
}

fn page_size() -> usize {
    // SAFETY: sysconf has no preconditions.
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value <= 0 { 4096 } else { value as usize }
}

fn region_prot(region: &proc_maps::Region) -> libc::c_int {
    let mut prot = 0;
    if region.readable {
        prot |= libc::PROT_READ;
    }
    if region.writable {
        prot |= libc::PROT_WRITE;
    }
    if region.executable {
        prot |= libc::PROT_EXEC;
    }
    prot
}

/// Change protection of `[addr, addr+len)`; `len` may be 0.
fn mprotect_range(addr: usize, len: usize, prot: libc::c_int) -> io::Result<()> {
    let ps = page_size();
    let start = addr & !(ps - 1);
    let end = addr
        .checked_add(len)
        .and_then(|value| value.checked_add(ps - 1))
        .map(|value| value & !(ps - 1))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: the range is page aligned and inside a mapped region.
    let rc = unsafe { libc::mprotect(start as *mut libc::c_void, end - start, prot) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Read permission for the minimal pages the scan touches.
///
/// Android maps the linker `r-x`, so the normal path performs no `mprotect` at
/// all and the permission map is unchanged. Only an execute-only (`--x`) text
/// mapping gets `READ` added, and [`Drop`] restores the exact original flags on
/// every exit path, including early returns from errors.
struct ReadableWindow {
    start: usize,
    len: usize,
    original: libc::c_int,
    active: bool,
}

impl ReadableWindow {
    fn new(
        region: &proc_maps::Region,
        addr: usize,
        end: usize,
    ) -> Result<Self, LoaderScanError> {
        let ps = page_size();
        let start = addr & !(ps - 1);
        let end = end.min(region.end);
        if start < region.start || end <= start {
            return Err(LoaderScanError::NotMapped);
        }
        let page_end = end
            .checked_add(ps - 1)
            .map(|value| value & !(ps - 1))
            .ok_or(LoaderScanError::NotMapped)?
            .min(region.end);
        let original = region_prot(region);
        if region.readable {
            return Ok(Self {
                start,
                len: page_end - start,
                original,
                active: false,
            });
        }
        mprotect_range(start, page_end - start, original | libc::PROT_READ)
            .map_err(|_| LoaderScanError::NotReadable)?;
        Ok(Self {
            start,
            len: page_end - start,
            original,
            active: true,
        })
    }
}

impl Drop for ReadableWindow {
    fn drop(&mut self) {
        if self.active {
            // Best effort: the process fails closed if the restore is refused,
            // but an execute-only page that accepted +READ above accepts the
            // original flags again.
            let _ = mprotect_range(self.start, self.len, self.original);
        }
    }
}

fn file_name(path: Option<&str>) -> Option<&str> {
    let path = path?;
    Some(path.rsplit('/').next().unwrap_or(path))
}

/// Mapping that contains the public `dlopen` wrapper.
fn is_dlopen_mapping(path: Option<&str>) -> bool {
    matches!(
        file_name(path),
        Some("libdl.so" | "libdl_android.so" | "linker64" | "linker")
    )
}

/// Mapping that can contain `__loader_dlopen` or a PLT stub for it.
fn is_linker_mapping(path: Option<&str>) -> bool {
    matches!(
        file_name(path),
        Some("ld-android.so" | "libdl_android.so" | "linker64" | "linker")
    )
}

/// Walk at most [`DLOPEN_SCAN_BYTES`] of the `dlopen` wrapper and return the
/// first `BL` target that is either a linker mapping or a PLT stub in the same
/// library. Other mapped targets (logging helpers, stack protector, unrelated
/// libraries) are skipped; the scan stops at `RET`.
fn scan_branch_target(
    dlopen_addr: usize,
    source: &proc_maps::Region,
    regions: &[proc_maps::Region],
    read: impl Fn(usize) -> u32,
) -> Result<usize, LoaderScanError> {
    let end = dlopen_addr
        .checked_add(DLOPEN_SCAN_BYTES)
        .ok_or(LoaderScanError::NoLoaderCall)?
        .min(source.end);
    let mut pc = dlopen_addr;
    while pc + 4 <= end {
        let word = read(pc);
        if crate::aarch64::is_ret(word) {
            break;
        }
        if let Some(target) = crate::aarch64::decode_bl_target(pc, word)
            && let Some(region) = proc_maps::containing(regions, target)
            && region.executable
        {
            if is_linker_mapping(region.path.as_deref()) {
                return Ok(target);
            }
            // `lld` routes the hidden linker import through a PLT stub in the
            // caller's own library (observed on Android 16: `libdl.so+0x1298`,
            // `imp.__loader_dlopen`). The caller verifies the stub shape.
            if region.start == source.start {
                return Ok(target);
            }
        }
        pc += 4;
    }
    Err(LoaderScanError::NoLoaderCall)
}

/// Decode the standard `lld` PLT entry:
/// `adrp x16, page; ldr x17, [x16, #off]; add x16, x16, #off; br x17`.
///
/// Returns the GOT slot address when the four instructions match exactly, or
/// `None` when the target does not look like a PLT stub.
fn decode_plt_stub(stub: usize, words: [u32; 4]) -> Option<usize> {
    let [adrp, ldr, add, br] = words;
    if adrp & 0x9F00_0000 != 0x9000_0000 {
        return None;
    }
    if ldr & 0xFFC0_0000 != 0xF940_0000 {
        return None;
    }
    if add & 0xFF00_0000 != 0x9100_0000 {
        return None;
    }
    if br & 0xFFFF_FC1F != 0xD61F_0000 {
        return None;
    }
    let pages = (((adrp >> 5) & 0x7FFFF) << 2) | ((adrp >> 29) & 0x3);
    let pages = ((pages as i64) << 43) >> 43;
    let base = ((stub as i64 & !0xFFF) + (pages << 12)) as usize;
    let offset = ((ldr >> 10) & 0xFFF) as usize * 8;
    Some(base.wrapping_add(offset))
}

/// Verify that a candidate is a live linker entry point.
///
/// A PLT stub is accepted only if it has the exact four-instruction shape and
/// its GOT slot is either still lazily unresolved or points into a linker
/// mapping. Any other mapping fails closed.
fn validate_loader_target(
    target: usize,
    source: &proc_maps::Region,
    regions: &[proc_maps::Region],
) -> Result<(), LoaderScanError> {
    let region = proc_maps::containing(regions, target).ok_or(LoaderScanError::TargetNotMapped)?;
    if !region.executable {
        return Err(LoaderScanError::TargetNotExecutable);
    }
    if is_linker_mapping(region.path.as_deref()) {
        return Ok(());
    }
    if region.start != source.start {
        return Err(LoaderScanError::TargetNotLinker);
    }
    // Read the four stub instructions through the same read-only guard.
    let window = ReadableWindow::new(source, target, target + 16)?;
    let words = {
        let read = |addr: usize| unsafe { ptr::read_unaligned(addr as *const u32) };
        [read(target), read(target + 4), read(target + 8), read(target + 12)]
    };
    drop(window);
    let slot = decode_plt_stub(target, words).ok_or(LoaderScanError::UnsupportedLayout(
        "dlopen call is neither a linker mapping nor a PLT stub",
    ))?;
    let slot_region = proc_maps::containing(regions, slot)
        .ok_or(LoaderScanError::UnsupportedLayout("PLT GOT slot is not mapped"))?;
    if !slot_region.readable {
        return Err(LoaderScanError::UnsupportedLayout("PLT GOT slot is not readable"));
    }
    // Unresolved lazy slots are zero. When the linker has already filled the
    // slot, it must point into a linker mapping; never accept an interposer.
    // SAFETY: the slot is inside a readable mapping (checked above).
    let resolved = unsafe { ptr::read_unaligned(slot as *const u64) } as usize;
    if resolved != 0 && !is_linker_mapping(proc_maps::containing(regions, resolved).and_then(|r| r.path.as_deref())) {
        return Err(LoaderScanError::TargetNotLinker);
    }
    Ok(())
}

/// Resolve `__loader_dlopen` without ever granting write or execute permission.
///
/// Order:
/// 1. `dlsym(libdl, "__loader_dlopen")` when the platform exports it (Android
///    16 does); the address must fall in a linker mapping.
/// 2. Otherwise decode the first loader `BL` of the `dlopen` wrapper (bionic
///    API 28..36 layout) and validate its target.
///
/// # Safety
/// `libdl` must be a live handle and `dlopen_addr` must point at its `dlopen`.
unsafe fn find_loader_dlopen(
    libdl: *mut libc::c_void,
    dlopen_addr: usize,
) -> Result<*const libc::c_void, LoaderScanError> {
    let regions = proc_maps::read_self().map_err(|_| LoaderScanError::NotMapped)?;
    let source = proc_maps::containing(&regions, dlopen_addr).ok_or(LoaderScanError::NotMapped)?;
    if !source.executable {
        return Err(LoaderScanError::NotExecutable);
    }
    if !is_dlopen_mapping(source.path.as_deref()) {
        return Err(LoaderScanError::UnsupportedLayout(
            "dlopen is not inside libdl.so/linker64",
        ));
    }

    // SAFETY: libdl is a live handle; the name is a NUL-terminated C string.
    let direct = unsafe { libc::dlsym(libdl, c"__loader_dlopen".as_ptr()) } as usize;
    if direct != 0 {
        validate_loader_target(direct, source, &regions)?;
        return Ok(direct as *const libc::c_void);
    }

    let end = dlopen_addr
        .checked_add(DLOPEN_SCAN_BYTES)
        .ok_or(LoaderScanError::NoLoaderCall)?
        .min(source.end);
    let window = ReadableWindow::new(source, dlopen_addr, end)?;
    let target = {
        let read = |addr: usize| unsafe { ptr::read_unaligned(addr as *const u32) };
        scan_branch_target(dlopen_addr, source, &regions, read)?
    };
    drop(window);
    validate_loader_target(target, source, &regions)?;
    Ok(target as *const libc::c_void)
}

/// # Safety
/// Runs at most once; mutates process-wide linker state.
unsafe fn resolve() -> Option<Symbols> {
    let api = device_api_level();
    if api < MIN_SUPPORTED_API {
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

    // SAFETY: dlopen_addr points into libdl's executable mapping; libdl is live.
    let loader_dlopen_ptr = match unsafe { find_loader_dlopen(libdl, dlopen_addr) } {
        Ok(target) => target,
        Err(error) => {
            crate::hooks::trace(format_args!(
                "linker layout rejected (api={api}, supported {MIN_SUPPORTED_API}..={MAX_VERIFIED_API}): {error}"
            ));
            return None;
        }
    };
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

    fn region(
        start: usize,
        end: usize,
        readable: bool,
        writable: bool,
        executable: bool,
        path: &str,
    ) -> proc_maps::Region {
        proc_maps::Region {
            start,
            end,
            readable,
            writable,
            executable,
            path: Some(path.to_string()),
        }
    }

    fn reader(base: usize, words: Vec<u32>) -> impl Fn(usize) -> u32 {
        move |pc| words[(pc - base) / 4]
    }

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

    /// AUD-015 regression: resolving the loader must leave the permission map
    /// of the `dlopen` mapping untouched (`r-x`, never `rwx`). The harness also
    /// prints the before/after table used as evidence in the status document.
    #[test]
    fn aud015_scanning_leaves_mapping_protections_unchanged() {
        let libdl = unsafe { libc::dlopen(c"libdl.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        assert!(!libdl.is_null(), "libdl.so must be loadable");
        let dlopen_addr = unsafe { libc::dlsym(libdl, c"dlopen".as_ptr()) } as usize;
        assert_ne!(dlopen_addr, 0, "dlopen must resolve");
        let before_regions = proc_maps::read_self().expect("maps");
        let before = proc_maps::containing(&before_regions, dlopen_addr)
            .expect("dlopen mapped")
            .clone();
        let found = unsafe { find_loader_dlopen(libdl, dlopen_addr) };
        let after_regions = proc_maps::read_self().expect("maps");
        let after = proc_maps::containing(&after_regions, dlopen_addr)
            .expect("dlopen mapped")
            .clone();
        eprintln!(
            "AUD015_PERMISSIONS before r={} w={} x={} after r={} w={} x={} path={:?} target={found:?}",
            before.readable,
            before.writable,
            before.executable,
            after.readable,
            after.writable,
            after.executable,
            before.path
        );
        assert!(found.is_ok(), "the linker scan must resolve a target: {found:?}");
        assert_eq!(
            (before.readable, before.writable, before.executable),
            (after.readable, after.writable, after.executable),
            "resolving __loader_dlopen must not change the dlopen mapping protections"
        );
        assert!(!after.writable, "linker text must never become writable");
    }

    #[test]
    fn aud015_scan_accepts_a_linker_branch() {
        let source = region(0x1000, 0x1100, true, false, true, "/lib/libdl.so");
        let linker = region(0x2000, 0x2100, true, false, true, "/lib/ld-android.so");
        let base = 0x1000;
        let words = vec![
            0xD503_233F,                     // paciasp
            crate::aarch64::encode_bl(0x1004, 0x2008).unwrap(),
            0xD65F_03C0,                     // ret
        ];
        let target = scan_branch_target(base, &source, &[source.clone(), linker], reader(base, words));
        assert_eq!(target, Ok(0x2008));
    }

    #[test]
    fn aud015_scan_skips_foreign_and_non_executable_targets() {
        let source = region(0x1000, 0x1100, true, false, true, "/lib/libdl.so");
        let foreign = region(0x2000, 0x2100, true, false, true, "/lib/liblog.so");
        let data = region(0x3000, 0x3100, true, true, false, "/lib/data");
        let base = 0x1000;
        let words = vec![
            crate::aarch64::encode_bl(0x1000, 0x2008).unwrap(),
            crate::aarch64::encode_bl(0x1004, 0x3008).unwrap(),
            0xD65F_03C0,
        ];
        let regions = [source.clone(), foreign, data];
        let target = scan_branch_target(base, &source, &regions, reader(base, words));
        assert_eq!(target, Err(LoaderScanError::NoLoaderCall));
    }

    #[test]
    fn aud015_scan_stops_at_ret_and_at_the_window_bound() {
        let source = region(0x1000, 0x2000, true, false, true, "/lib/libdl.so");
        let linker = region(0x3000, 0x3100, true, false, true, "/lib/ld-android.so");
        let base = 0x1000;
        // A BL after RET must not be considered.
        let words = vec![
            0xD65F_03C0,
            crate::aarch64::encode_bl(0x1004, 0x3008).unwrap(),
        ];
        let regions = [source.clone(), linker.clone()];
        assert_eq!(
            scan_branch_target(base, &source, &regions, reader(base, words.clone())),
            Err(LoaderScanError::NoLoaderCall)
        );
        // A BL outside DLOPEN_SCAN_BYTES must not be reached either.
        let far = 0x1000 + DLOPEN_SCAN_BYTES;
        let far_word = crate::aarch64::encode_bl(far, 0x3008).unwrap();
        let mut words = vec![0xD503_233F; DLOPEN_SCAN_BYTES / 4];
        words.push(far_word);
        assert_eq!(
            scan_branch_target(base, &source, &regions, reader(base, words)),
            Err(LoaderScanError::NoLoaderCall)
        );
    }

    #[test]
    fn aud015_same_mapping_target_is_returned_for_plt_validation() {
        let source = region(0x1000, 0x1200, true, false, true, "/lib/libdl.so");
        let base = 0x1000;
        let words = vec![
            crate::aarch64::encode_bl(0x1000, 0x1020).unwrap(),
            0xD65F_03C0,
        ];
        let target = scan_branch_target(base, &source, &[source.clone()], reader(base, words));
        assert_eq!(target, Ok(0x1020));
    }

    /// The exact stub observed in Android 16 bionic `libdl.so` at `+0x1298`
    /// (`imp.__loader_dlopen`): adrp x16,0x2000 / ldr x17,[x16,#0x140] /
    /// add x16,x16,#0x140 / br x17. The GOT slot is 0x2140.
    #[test]
    fn aud015_decodes_the_android16_plt_stub() {
        let words = [0xB000_0010, 0xF940_A211, 0x9105_0210, 0xD61F_0220];
        assert_eq!(decode_plt_stub(0x1298, words), Some(0x2140));
        // Same mapping but not a PLT stub: reject.
        let words = [0xD503_233F, 0xF940_A211, 0x9105_0210, 0xD61F_0220];
        assert_eq!(decode_plt_stub(0x1298, words), None);
        let words = [0xB000_0010, 0xF940_A211, 0x9105_0210, 0xD65F_03C0];
        assert_eq!(decode_plt_stub(0x1298, words), None);
    }

    #[test]
    fn aud015_linker_mapping_fingerprint() {
        assert!(is_linker_mapping(Some(
            "/apex/com.android.runtime/bin/linker64"
        )));
        assert!(is_linker_mapping(Some(
            "/apex/com.android.runtime/lib64/bionic/ld-android.so"
        )));
        assert!(is_linker_mapping(Some("/system/lib64/libdl_android.so")));
        assert!(!is_linker_mapping(Some("/system/lib64/liblog.so")));
        assert!(!is_linker_mapping(None));
        assert!(is_dlopen_mapping(Some(
            "/apex/com.android.runtime/lib64/bionic/libdl.so"
        )));
        assert!(!is_dlopen_mapping(Some("/data/app/libfoo.so")));
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

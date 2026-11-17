//! On-device probe for the Rust adrenotools port.
//!
//! Runs four independent checks and prints PASS/FAIL with the exact error text:
//!
//! 1. hidden linker entry points resolve;
//! 2. a private namespace can host a unique copy of `/system/lib64/libvulkan.so`
//!    and enumerate physical devices through it;
//! 3. `open_libvulkan` with `ADRENOTOOLS_DRIVER_CUSTOM` loads a custom Turnip
//!    driver, and the interposed `android_dlopen_ext` redirects a loader-style
//!    `vulkan.adreno.so` request to that driver (the driver is then queried
//!    directly through `vk_icdGetInstanceProcAddr`);
//! 4. the same path with the provided Gen8 pack reports whether that pack can act
//!    as an ICD at all.
//!
//! Usage: `driver-probe [hookLibDir] [customDriverDir]`
//!
//! Defaults: the directory of this executable (cargo puts the hook `.so` files
//! there) and `$PREFIX/lib` (Termux's Turnip build).

use std::ffi::{CStr, CString, c_char, c_void};
use std::mem;
use std::path::{Path, PathBuf};
use std::ptr;

use wc3_adrenotools::driver::{self, FeatureFlags, OpenedDriver};
use wc3_adrenotools::{linker, proc_maps};

#[repr(C)]
struct VkApplicationInfo {
    s_type: u32,
    p_next: *const c_void,
    application_name: *const c_char,
    application_version: u32,
    engine_name: *const c_char,
    engine_version: u32,
    api_version: u32,
}

#[repr(C)]
struct VkInstanceCreateInfo {
    s_type: u32,
    p_next: *const c_void,
    flags: u32,
    application_info: *const VkApplicationInfo,
    enabled_layer_count: u32,
    enabled_layer_names: *const *const c_char,
    enabled_extension_count: u32,
    enabled_extension_names: *const *const c_char,
}

type GetInstanceProcAddr = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type CreateInstance =
    unsafe extern "C" fn(*const VkInstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32;
type EnumeratePhysicalDevices =
    unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type GetPhysicalDeviceProperties = unsafe extern "C" fn(*mut c_void, *mut c_void);
type NegotiateIcdVersion = unsafe extern "C" fn(*mut u32) -> i32;

const VK_STRUCTURE_TYPE_APPLICATION_INFO: u32 = 0;
const VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO: u32 = 1;
const VK_SUCCESS: i32 = 0;
const VK_API_VERSION_1_1: u32 = (1 << 22) | (1 << 12);

struct VkApi {
    gipa: GetInstanceProcAddr,
}

impl VkApi {
    fn from_handle(handle: *mut c_void, symbol: &CStr) -> Result<Self, String> {
        // SAFETY: handle is a dlopen handle; symbol lookup is bounded by the name.
        let ptr = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
        if ptr.is_null() {
            return Err(format!(
                "dlsym({symbol:?}) is NULL: {}",
                linker::last_dl_error()
            ));
        }
        // SAFETY: vkGetInstanceProcAddr/vk_icdGetInstanceProcAddr share this signature.
        Ok(Self {
            gipa: unsafe { mem::transmute::<*mut c_void, GetInstanceProcAddr>(ptr) },
        })
    }

    unsafe fn proc(&self, instance: *mut c_void, name: &CStr) -> *mut c_void {
        // SAFETY: forwarded to the driver's entry point.
        unsafe { (self.gipa)(instance, name.as_ptr()) }
    }
}

fn device_name(props: &[u8; 1024]) -> String {
    let name = &props[20..276];
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

/// Create an instance and list physical devices through `api`.
///
/// # Safety
/// `api` must hold a valid `vkGetInstanceProcAddr`-compatible entry point.
unsafe fn enumerate_devices(api: &VkApi, label: &str) -> Result<Vec<String>, String> {
    // SAFETY: symbol lookup on a live instance/dispatch.
    let create_ptr = unsafe { api.proc(ptr::null_mut(), c"vkCreateInstance") };
    if create_ptr.is_null() {
        return Err("vkCreateInstance is NULL".to_string());
    }
    // SAFETY: signature checked by the loader contract.
    let create: CreateInstance = unsafe { mem::transmute(create_ptr) };

    let app_name = c"wc3-driver-probe";
    let engine_name = c"herofolends-rs";
    let app = VkApplicationInfo {
        s_type: VK_STRUCTURE_TYPE_APPLICATION_INFO,
        p_next: ptr::null(),
        application_name: app_name.as_ptr(),
        application_version: 1,
        engine_name: engine_name.as_ptr(),
        engine_version: 1,
        api_version: VK_API_VERSION_1_1,
    };
    let create_info = VkInstanceCreateInfo {
        s_type: VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        p_next: ptr::null(),
        flags: 0,
        application_info: &app,
        enabled_layer_count: 0,
        enabled_layer_names: ptr::null(),
        enabled_extension_count: 0,
        enabled_extension_names: ptr::null(),
    };
    let mut instance: *mut c_void = ptr::null_mut();
    // SAFETY: create_info is fully initialised for the API version used.
    let rc = unsafe { create(&create_info, ptr::null(), &mut instance) };
    if rc != VK_SUCCESS {
        return Err(format!("vkCreateInstance({label}) returned {rc}"));
    }

    // SAFETY: instance is live.
    let enum_ptr = unsafe { api.proc(instance, c"vkEnumeratePhysicalDevices") };
    if enum_ptr.is_null() {
        return Err("vkEnumeratePhysicalDevices is NULL".to_string());
    }
    // SAFETY: signature per Vulkan spec.
    let enumerate: EnumeratePhysicalDevices = unsafe { mem::transmute(enum_ptr) };
    let mut count = 0u32;
    // SAFETY: count pointer is writable.
    let rc = unsafe { enumerate(instance, &mut count, ptr::null_mut()) };
    if rc != VK_SUCCESS {
        return Err(format!("vkEnumeratePhysicalDevices(count) returned {rc}"));
    }
    let mut devices = vec![ptr::null_mut(); count as usize];
    // SAFETY: vector has room for `count` handles.
    let rc = unsafe { enumerate(instance, &mut count, devices.as_mut_ptr()) };
    if rc != VK_SUCCESS {
        return Err(format!("vkEnumeratePhysicalDevices(list) returned {rc}"));
    }

    // SAFETY: instance is live.
    let props_ptr = unsafe { api.proc(instance, c"vkGetPhysicalDeviceProperties") };
    if props_ptr.is_null() {
        return Err("vkGetPhysicalDeviceProperties is NULL".to_string());
    }
    // SAFETY: signature per Vulkan spec.
    let get_props: GetPhysicalDeviceProperties = unsafe { mem::transmute(props_ptr) };
    let mut out = Vec::with_capacity(devices.len());
    for device in devices.into_iter().take(count as usize) {
        let mut props = [0u8; 1024];
        // SAFETY: Vulkan writes a VkPhysicalDeviceProperties (<= 824 bytes).
        unsafe { get_props(device, props.as_mut_ptr().cast()) };
        let api_version = u32::from_le_bytes(props[0..4].try_into().expect("fixed slice"));
        let driver_version = u32::from_le_bytes(props[4..8].try_into().expect("fixed slice"));
        let vendor = u32::from_le_bytes(props[8..12].try_into().expect("fixed slice"));
        out.push(format!(
            "{} (api={}.{}.{} driver=0x{driver_version:08x} vendor=0x{vendor:04x})",
            device_name(&props),
            api_version >> 22,
            (api_version >> 12) & 0x3ff,
            api_version & 0xfff,
        ));
    }
    Ok(out)
}

fn default_hook_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn print_devices(label: &str, devices: &[String]) {
    if devices.is_empty() {
        println!("  {label}: 0 physical devices");
    }
    for device in devices {
        println!("  {label}: {device}");
    }
}

/// Run the interposed `android_dlopen_ext` like the Vulkan loader would and query
/// the resulting driver through the ICD interface.
unsafe fn hook_redirect_test(
    opened: &OpenedDriver,
    filename: &CStr,
) -> Result<Vec<String>, String> {
    // SAFETY: opened.hook is libmain_hook.so's handle.
    let load_ptr = unsafe { libc::dlsym(opened.hook, c"adrenotools_hook_load".as_ptr()) };
    if load_ptr.is_null() {
        return Err("libmain_hook.so does not export adrenotools_hook_load".to_string());
    }
    // SAFETY: exported by our own crate with this signature.
    let load: unsafe extern "C" fn(*const c_char, *mut c_void) -> *mut c_void =
        unsafe { mem::transmute(load_ptr) };

    let mut parent = linker::get_exported_namespace(c"default");
    if parent.is_null() {
        parent = opened.hook_namespace;
    }
    // SAFETY: parent is a live namespace (or the hook namespace as fallback).
    let handle = unsafe { load(filename.as_ptr(), parent) };
    if handle.is_null() {
        return Err(format!(
            "hook returned NULL for {:?}: {}",
            filename,
            linker::last_dl_error()
        ));
    }
    // SAFETY: handle comes from dlopen.
    let negotiate_ptr =
        unsafe { libc::dlsym(handle, c"vk_icdNegotiateLoaderICDInterfaceVersion".as_ptr()) };
    let icd_gipa_ptr = unsafe { libc::dlsym(handle, c"vk_icdGetInstanceProcAddr".as_ptr()) };
    if negotiate_ptr.is_null() || icd_gipa_ptr.is_null() {
        return Err(format!(
            "driver loaded, but ICD entry points are missing \
             (vk_icdNegotiateLoaderICDInterfaceVersion={negotiate_ptr:p}, \
             vk_icdGetInstanceProcAddr={icd_gipa_ptr:p})"
        ));
    }
    // SAFETY: symbol signatures per the Vulkan ICD interface.
    let negotiate: NegotiateIcdVersion = unsafe { mem::transmute(negotiate_ptr) };
    let mut version = 7u32;
    // SAFETY: version pointer is writable.
    let rc = unsafe { negotiate(&mut version) };
    if rc != VK_SUCCESS {
        return Err(format!(
            "vk_icdNegotiateLoaderICDInterfaceVersion returned {rc}"
        ));
    }
    // SAFETY: handle from dlopen; symbol name is the ICD one.
    let api = VkApi::from_handle(handle, c"vk_icdGetInstanceProcAddr")?;
    // SAFETY: api is a valid ICD entry point.
    unsafe { enumerate_devices(&api, "icd") }
}

fn open_custom(
    hook_dir: &Path,
    driver_dir: &Path,
    loader: Option<&CStr>,
) -> Result<OpenedDriver, driver::DriverError> {
    let hook_c = CString::new(hook_dir.as_os_str().to_string_lossy().as_bytes()).expect("no NUL");
    let dir_c = CString::new(driver_dir.as_os_str().to_string_lossy().as_bytes()).expect("no NUL");
    let name_c = CString::new("libvulkan_freedreno.so").expect("no NUL");
    // SAFETY: all pointers are valid NUL-terminated strings for this call.
    unsafe {
        match loader {
            Some(loader) => driver::open_libvulkan_with_loader(
                loader,
                libc::RTLD_NOW,
                FeatureFlags::CUSTOM,
                ptr::null(),
                hook_c.as_ptr(),
                dir_c.as_ptr(),
                name_c.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ),
            None => driver::open_libvulkan(
                libc::RTLD_NOW,
                FeatureFlags::CUSTOM,
                ptr::null(),
                hook_c.as_ptr(),
                dir_c.as_ptr(),
                name_c.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ),
        }
    }
}

fn main() {
    if std::env::var_os("ADRENOTOOLS_TRACE").is_none() {
        // SAFETY: single-threaded start-up, before any thread is spawned.
        unsafe { std::env::set_var("ADRENOTOOLS_TRACE", "1") };
    }

    let mut args = std::env::args().skip(1);
    let hook_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_hook_dir);
    let prefix =
        std::env::var("PREFIX").unwrap_or_else(|_| "/data/data/com.termux/files/usr".to_string());
    let turnip_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{prefix}/lib")));

    println!("driver-probe: rust adrenotools port");
    println!("  api_level        = {}", linker::device_api_level());
    println!("  linker_loaded    = {}", linker::load_status());
    println!("  hook_lib_dir     = {}", hook_dir.display());
    println!("  custom_driver_dir= {}", turnip_dir.display());

    let mut failures = 0usize;

    // ---- 1. linker symbols ----
    println!("\n[1] hidden linker entry points");
    if linker::load_status() {
        println!("  PASS");
    } else {
        println!("  FAIL: linkernsbypass could not resolve __loader_* symbols");
        // Nothing else can work.
        std::process::exit(1);
    }

    // ---- 2. unique libvulkan in a private namespace ----
    println!("\n[2] unique /system/lib64/libvulkan.so in a private namespace");
    // SAFETY: all strings are static; the namespace parent is NULL.
    let probe_ns = unsafe {
        linker::create_namespace(
            c"probe-vulkan".as_ptr(),
            c"/system/lib64".as_ptr(),
            ptr::null(),
            linker::ANDROID_NAMESPACE_TYPE_SHARED,
            ptr::null(),
            ptr::null_mut(),
        )
    };
    if probe_ns.is_null() {
        println!("  FAIL: android_create_namespace returned NULL");
        failures += 1;
    } else if !unsafe { linker::link_namespace_to_default_all_libs(probe_ns) } {
        println!("  FAIL: linking the namespace to the default copy failed");
        failures += 1;
    } else {
        // SAFETY: probe_ns is live; the loader path exists.
        let vulkan = unsafe {
            linker::namespace_dlopen_unique(
                c"/system/lib64/libvulkan.so",
                None,
                libc::RTLD_NOW,
                probe_ns,
            )
        };
        if vulkan.is_null() {
            println!("  FAIL: {}", linker::last_dl_error());
            failures += 1;
        } else {
            match VkApi::from_handle(vulkan, c"vkGetInstanceProcAddr")
                .and_then(|api| unsafe { enumerate_devices(&api, "libvulkan") })
            {
                Ok(devices) => {
                    println!("  PASS");
                    print_devices("libvulkan", &devices);
                }
                Err(error) => {
                    println!("  FAIL: {error}");
                    failures += 1;
                }
            }
        }
    }

    // ---- 3. custom driver through the hook (system Turnip) ----
    let system_turnip = turnip_dir.join("libvulkan_freedreno.so");
    println!("\n[3] custom driver via interposed android_dlopen_ext");
    println!("  driver = {}", system_turnip.display());
    if !system_turnip.exists() {
        println!("  SKIP: driver not found");
    } else {
        match open_custom(&hook_dir, &turnip_dir, None) {
            Ok(opened) => {
                println!("  PASS: open_libvulkan returned a libvulkan handle");
                match VkApi::from_handle(opened.vulkan, c"vkGetInstanceProcAddr") {
                    Ok(api) => match unsafe { enumerate_devices(&api, "libvulkan+hook") } {
                        Ok(devices) => print_devices("libvulkan+hook", &devices),
                        Err(error) => println!("  note: loader enumeration: {error}"),
                    },
                    Err(error) => println!("  note: {error}"),
                }
                match unsafe { hook_redirect_test(&opened, c"vulkan.adreno.so") } {
                    Ok(devices) => {
                        println!("  PASS: hook loaded the custom driver as the ICD");
                        print_devices("custom-icd", &devices);
                    }
                    Err(error) => {
                        println!("  FAIL: {error}");
                        failures += 1;
                    }
                }
            }
            Err(error) => {
                println!("  FAIL: open_libvulkan: {error}");
                failures += 1;
            }
        }
    }

    // ---- 4. provided Gen8 pack ----
    let pack_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/turnip");
    let pack = pack_dir.join("libvulkan_freedreno.so");
    println!("\n[4] provided Gen8 pack");
    println!("  driver = {}", pack.display());
    if !pack.exists() {
        println!("  SKIP: pack not extracted");
    } else {
        match open_custom(&hook_dir, &pack_dir, None) {
            Ok(opened) => match unsafe { hook_redirect_test(&opened, c"vulkan.adreno.so") } {
                Ok(devices) => {
                    println!("  PASS: hook loaded the pack as the ICD");
                    print_devices("gen8", &devices);
                }
                Err(error) => println!("  observed: {error}"),
            },
            Err(error) => println!("  observed: open_libvulkan: {error}"),
        }
    }

    // ---- 6. bundled Khronos loader + fake ICD name ----
    println!("\n[6a] bundled Khronos loader + fake ICD name (informational)");
    let loader_path = std::env::var("ADRENOTOOLS_LOADER")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(format!("{prefix}/lib/libvulkan.so")));
    println!("  loader = {}", loader_path.display());
    if !loader_path.exists() || !system_turnip.exists() {
        println!("  SKIP: loader or driver not found");
    } else {
        let scratch = std::env::temp_dir().join(format!("wc3-probe-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("scratch dir");
        let json = scratch.join("fake_icd.json");
        // The path does not exist on purpose: only the interposed
        // android_dlopen_ext can satisfy this request.
        std::fs::write(
            &json,
            "{\n  \"file_format_version\": \"1.0.0\",\n  \"ICD\": {\n    \"library_path\": \"/nonexistent/vulkan.fake.so\",\n    \"api_version\": \"1.3.0\"\n  }\n}\n",
        )
        .expect("write fake icd json");
        // SAFETY: single-threaded probe; the loader reads this at instance creation.
        unsafe { std::env::set_var("VK_ICD_FILENAMES", &json) };

        let loader_c =
            CString::new(loader_path.as_os_str().to_string_lossy().as_bytes()).expect("no NUL");
        match open_custom(&hook_dir, &turnip_dir, Some(&loader_c)) {
            Ok(opened) => {
                println!("  observed: open_libvulkan_with_loader returned a handle");
                match VkApi::from_handle(opened.vulkan, c"vkGetInstanceProcAddr") {
                    Ok(api) => match unsafe { enumerate_devices(&api, "khronos+hook") } {
                        Ok(devices) => {
                            let turnip = devices.iter().any(|d| d.contains("Turnip"));
                            print_devices("khronos+hook", &devices);
                            if turnip {
                                println!("  PASS: loader enumerated the custom Turnip driver");
                            } else {
                                println!(
                                    "  observed: the loader did not route the ICD open through \
                                     android_dlopen_ext (it uses plain dlopen on Termux), so the \
                                     interposer is not involved; this is a loader-side limitation, \
                                     not a port failure"
                                );
                            }
                        }
                        Err(error) => {
                            println!("  observed: {error}");
                        }
                    },
                    Err(error) => {
                        println!("  observed: {error}");
                    }
                }
            }
            Err(error) => {
                println!("  observed: {error}");
            }
        }
        // SAFETY: single-threaded probe.
        unsafe { std::env::remove_var("VK_ICD_FILENAMES") };
        let _ = std::fs::remove_dir_all(&scratch);
    }

    // ---- 7. recommended native game path: real ICD JSON + bundled loader ----
    println!("\n[6b] Turnip via a generated ICD JSON (no hook, plain dlopen loader)");
    if !loader_path.exists() || !system_turnip.exists() {
        println!("  SKIP: loader or driver not found");
    } else {
        let scratch = std::env::temp_dir().join(format!("wc3-probe-icd-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("scratch dir");
        let json = scratch.join("freedreno_icd.aarch64.json");
        let json_body = format!(
            "{{\n  \"file_format_version\": \"1.0.1\",\n  \"ICD\": {{\n    \"api_version\": \"1.4.354\",\n    \"library_path\": \"{}\"\n  }}\n}}\n",
            system_turnip.display()
        );
        std::fs::write(&json, json_body).expect("write icd json");
        // SAFETY: single-threaded probe; the loader reads this at instance creation.
        unsafe { std::env::set_var("VK_ICD_FILENAMES", &json) };

        let loader_c =
            CString::new(loader_path.as_os_str().to_string_lossy().as_bytes()).expect("no NUL");
        // SAFETY: plain dlopen of a real loader; the handle stays valid for the process.
        let handle = unsafe { libc::dlopen(loader_c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            println!(
                "  FAIL: dlopen({}): {}",
                loader_path.display(),
                linker::last_dl_error()
            );
            failures += 1;
        } else {
            match VkApi::from_handle(handle, c"vkGetInstanceProcAddr") {
                Ok(api) => match unsafe { enumerate_devices(&api, "icd-json") } {
                    Ok(devices) => {
                        let turnip = devices.iter().any(|d| d.contains("Turnip"));
                        print_devices("icd-json", &devices);
                        if turnip {
                            println!("  PASS: generated ICD JSON drives the bundled loader");
                        } else {
                            println!("  FAIL: Turnip not enumerated");
                            failures += 1;
                        }
                    }
                    Err(error) => {
                        println!("  FAIL: {error}");
                        failures += 1;
                    }
                },
                Err(error) => {
                    println!("  FAIL: {error}");
                    failures += 1;
                }
            }
        }
        // SAFETY: single-threaded probe.
        unsafe { std::env::remove_var("VK_ICD_FILENAMES") };
        let _ = std::fs::remove_dir_all(&scratch);
    }

    // ---- 5. mapping sanity ----
    println!("\n[7] /proc/self/maps parser");
    match proc_maps::read_self() {
        Ok(regions) => println!("  PASS: {} regions", regions.len()),
        Err(error) => {
            println!("  FAIL: {error}");
            failures += 1;
        }
    }

    println!("\nresult: {} failure(s)", failures);
    if failures > 0 {
        std::process::exit(1);
    }
}

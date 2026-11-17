//! A full Vulkan driver surface over the Turnip ICD.
//!
//! The AOSP loader's vendor path (`/vendor/lib64/hw/vulkan.adreno.so`) treats
//! the library as a complete driver and asks for `vkGetInstanceProcAddr`.
//! Mesa Turnip builds are ICDs (`vk_icdGetInstanceProcAddr`). This shim exports
//! the full-driver entry points and forwards them to the ICD, so the stock
//! Android loader (which has VK_KHR_surface/ANDROID_surface) can use a stable
//! Turnip build.
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

type Gipa = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;

const ICD: &CStr = c"/data/data/org.wc3rust.p0/files/turnip/libvulkan_freedreno_termux.so";
const ANDROID_SURFACE: &[u8] = b"VK_KHR_android_surface\0";

#[repr(C)]
struct VkExtensionProperties {
    name: [u8; 256],
    version: u32,
}

impl Default for VkExtensionProperties {
    fn default() -> Self {
        Self { name: [0; 256], version: 0 }
    }
}

#[repr(C)]
struct VkInstanceCreateInfo {
    s_type: u32,
    p_next: *const c_void,
    flags: u32,
    application_info: *const c_void,
    enabled_layer_count: u32,
    enabled_layer_names: *const *const c_char,
    enabled_extension_count: u32,
    enabled_extension_names: *const *const c_char,
}

static GIPA: OnceLock<Option<Gipa>> = OnceLock::new();
type PhysProc = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
static PHYS_PROC: OnceLock<Option<PhysProc>> = OnceLock::new();

fn gipa() -> Option<Gipa> {
    *GIPA.get_or_init(|| {
        // SAFETY: fixed app-private path; the ICD follows the Vulkan ABI.
        unsafe {
            let handle = libc::dlopen(ICD.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                return None;
            }
            let symbol = libc::dlsym(handle, c"vk_icdGetInstanceProcAddr".as_ptr());
            if symbol.is_null() {
                return None;
            }
            let phys = libc::dlsym(handle, c"vk_icdGetPhysicalDeviceProcAddr".as_ptr());
            if !phys.is_null() {
                PHYS_PROC.get_or_init(|| {
                    Some(std::mem::transmute::<*mut c_void, PhysProc>(phys))
                });
            }
            std::mem::forget(handle);
            Some(std::mem::transmute::<*mut c_void, Gipa>(symbol))
        }
    })
}

fn proc(name: &CStr) -> *mut c_void {
    let Some(gipa) = gipa() else {
        return std::ptr::null_mut();
    };
    // SAFETY: NULL instance requests a global driver entry point.
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
    // SAFETY: entries are NUL-terminated strings per the Vulkan ABI.
    let text = unsafe { CStr::from_ptr(name) }.to_bytes();
    if text == b"vkEnumerateInstanceExtensionProperties" {
        return vkEnumerateInstanceExtensionProperties as *const c_void as *mut c_void;
    }
    if text == b"vkCreateInstance" {
        return vkCreateInstance as *const c_void as *mut c_void;
    }
    if text == b"vkEnumerateInstanceVersion" {
        return vkEnumerateInstanceVersion as *const c_void as *mut c_void;
    }
    if text == b"vkEnumerateInstanceLayerProperties" {
        return vkEnumerateInstanceLayerProperties as *const c_void as *mut c_void;
    }
    // SAFETY: forwarded verbatim under the Vulkan ABI.
    unsafe { gipa(instance, name) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkCreateInstance(
    create_info: *const VkInstanceCreateInfo,
    allocator: *const c_void,
    instance: *mut *mut c_void,
) -> i32 {
    let function = proc(c"vkCreateInstance");
    if function.is_null() || create_info.is_null() {
        return -9;
    }
    // The ICD does not advertise VK_KHR_android_surface; enable everything
    // else and provide the surface entry point through this wrapper.
    // SAFETY: create_info follows the Vulkan ABI.
    let info = unsafe { &*create_info };
    let mut names: Vec<*const c_char> = Vec::new();
    for index in 0..info.enabled_extension_count as usize {
        // SAFETY: the caller's extension array has this many entries.
        let name = unsafe { *info.enabled_extension_names.add(index) };
        if name.is_null() {
            continue;
        }
        // SAFETY: entries are NUL-terminated strings per the Vulkan ABI.
        if unsafe { CStr::from_ptr(name) }.to_bytes() == &ANDROID_SURFACE[..ANDROID_SURFACE.len() - 1] {
            continue;
        }
        names.push(name);
    }
    let mut filtered = VkInstanceCreateInfo {
        s_type: info.s_type,
        p_next: info.p_next,
        flags: info.flags,
        application_info: info.application_info,
        enabled_layer_count: info.enabled_layer_count,
        enabled_layer_names: info.enabled_layer_names,
        enabled_extension_count: names.len() as u32,
        enabled_extension_names: names.as_ptr(),
    };
    let function: unsafe extern "C" fn(*const VkInstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    let _ = &mut filtered;
    unsafe { function(&filtered, allocator, instance) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkEnumerateInstanceExtensionProperties(
    layer_name: *const c_char,
    count: *mut u32,
    properties: *mut VkExtensionProperties,
) -> i32 {
    let function = proc(c"vkEnumerateInstanceExtensionProperties");
    if function.is_null() {
        if !count.is_null() {
            // SAFETY: writable count pointer per the Vulkan ABI.
            unsafe { *count = 0 };
        }
        return 0;
    }
    let function: unsafe extern "C" fn(*const c_char, *mut u32, *mut VkExtensionProperties) -> i32 =
        unsafe { std::mem::transmute(function) };
    let mut original = 0u32;
    let first = unsafe { function(layer_name, &mut original, std::ptr::null_mut()) };
    if first != 0 {
        return first;
    }
    if properties.is_null() {
        if !count.is_null() {
            // SAFETY: writable count pointer.
            unsafe { *count = original + 1 };
        }
        return 0;
    }
    let capacity = if count.is_null() { 0 } else { unsafe { *count } };
    if capacity < original + 1 {
        if !count.is_null() {
            // SAFETY: writable count pointer.
            unsafe { *count = original + 1 };
        }
        return 5; // VK_INCOMPLETE
    }
    // SAFETY: the caller supplied room for `capacity` entries; the driver
    // writes at most `original`.
    let result = unsafe { function(layer_name, &mut original, properties) };
    if result != 0 {
        return result;
    }
    // SAFETY: one extra entry fits by the capacity check above.
    let extra = unsafe { &mut *properties.add(original as usize) };
    extra.name[..ANDROID_SURFACE.len()].copy_from_slice(ANDROID_SURFACE);
    extra.version = 25;
    if !count.is_null() {
        unsafe { *count = original + 1 };
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vkEnumerateInstanceVersion(version: *mut u32) -> i32 {
    let function = proc(c"vkEnumerateInstanceVersion");
    if function.is_null() {
        if !version.is_null() {
            // SAFETY: writable version pointer per the Vulkan ABI.
            unsafe { *version = (1 << 22) | (1 << 12) };
        }
        return 0;
    }
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
            // SAFETY: writable count pointer per the Vulkan ABI.
            unsafe { *count = 0 };
        }
        return 0;
    }
    let function: unsafe extern "C" fn(*mut u32, *mut c_void) -> i32 =
        unsafe { std::mem::transmute(function) };
    unsafe { function(count, properties) }
}

// ---- ICD entry points -----------------------------------------------------
// The adrenotools hook loads this library in place of the vendor ICD; the AOSP
// loader calls the ICD interface. These forward to the full-driver surface above
// (which filters the Android surface extension for the Termux ICD).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vk_icdGetInstanceProcAddr(
    instance: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    unsafe { vkGetInstanceProcAddr(instance, name) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vk_icdNegotiateLoaderICDInterfaceVersion(version: *mut u32) -> i32 {
    if !version.is_null() {
        // SAFETY: writable version pointer per the ICD contract.
        unsafe { *version = 5 };
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vk_icdGetPhysicalDeviceProcAddr(
    instance: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    if let Some(Some(phys)) = PHYS_PROC.get() {
        // SAFETY: the ICD's physical-device getter follows the loader ABI.
        return unsafe { phys(instance, name) };
    }
    let Some(gipa) = gipa() else {
        return std::ptr::null_mut();
    };
    // SAFETY: fallback through the instance ABI.
    unsafe { gipa(instance, name) }
}

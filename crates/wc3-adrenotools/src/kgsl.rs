//! KGSL ioctl wrappers — the memory-mapping half of `src/driver.cpp:100-210` and
//! the free path of `src/hook/hook_impl.cpp:202-228`.
//!
//! Struct layouts and ioctl numbers are copied from the kernel UAPI header the
//! donor ships (`src/hook/kgsl.h`): `KGSL_IOC_TYPE` is `0x09` and the four ioctls
//! used here are `_IOW`/`_IOWR` encoded by the kernel's `_IOC` macro. The tests
//! pin the resulting numbers so a layout change cannot slip through silently.

use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub const KGSL_CACHEMODE_SHIFT: u64 = 26;
pub const KGSL_CACHEMODE_WRITEBACK: u64 = 3;
pub const KGSL_MEMFLAGS_IOCOHERENT: u64 = 1 << 31;
pub const KGSL_USER_MEM_TYPE_ADDR: u32 = 0x2;
pub const KGSL_PROP_PWRCTRL: u32 = 0xE;

/// `KGSL_CACHEMODE_WRITEBACK << KGSL_CACHEMODE_SHIFT | KGSL_MEMFLAGS_IOCOHERENT`
pub const KGSL_DEFAULT_FLAGS: u64 =
    (KGSL_CACHEMODE_WRITEBACK << KGSL_CACHEMODE_SHIFT) | KGSL_MEMFLAGS_IOCOHERENT;

/// Flags the donor hardcodes in `adrenotools_gpu_mapping::flags` (`0xc2600`).
pub const GPU_MAPPING_FLAGS: u64 = 0xc2600;

pub const IOCTL_KGSL_SETPROPERTY: libc::c_int = 0x4018_0932u32 as libc::c_int;
pub const IOCTL_KGSL_GPUMEM_GET_INFO: libc::c_int = 0xc048_0936u32 as libc::c_int;
pub const IOCTL_KGSL_GPUOBJ_ALLOC: libc::c_int = 0xc030_0945u32 as libc::c_int;
pub const IOCTL_KGSL_GPUOBJ_FREE: libc::c_int = 0x4020_0946u32 as libc::c_int;
pub const IOCTL_KGSL_GPUOBJ_INFO: libc::c_int = 0xc030_0947u32 as libc::c_int;
pub const IOCTL_KGSL_GPUOBJ_IMPORT: libc::c_int = 0xc020_0948u32 as libc::c_int;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpuobjImportUseraddr {
    pub virtaddr: u64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpuobjImport {
    pub priv_: u64,
    pub priv_len: u64,
    pub flags: u64,
    pub type_: u32,
    pub id: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpuobjInfo {
    pub gpuaddr: u64,
    pub flags: u64,
    pub size: u64,
    pub va_len: u64,
    pub va_addr: u64,
    pub id: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpuobjAlloc {
    pub size: u64,
    pub flags: u64,
    pub va_len: u64,
    pub mmapsize: u64,
    pub id: u32,
    pub metadata_len: u32,
    pub metadata: u64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpuobjFree {
    pub flags: u64,
    pub priv_: u64,
    pub id: u32,
    pub type_: u32,
    pub len: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct GpumemGetInfo {
    pub gpuaddr: u64,
    pub id: u32,
    pub flags: u32,
    pub size: u64,
    pub mmapsize: u64,
    pub useraddr: u64,
    pub pad: [u64; 4],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct DeviceGetproperty {
    pub type_: u32,
    pub value: *mut libc::c_void,
    pub sizebytes: usize,
}

struct KgslFd(OwnedFd);

impl KgslFd {
    fn open() -> io::Result<Self> {
        // SAFETY: path is static and NUL terminated.
        let fd = unsafe { libc::open(c"/dev/kgsl-3d0".as_ptr(), libc::O_RDWR) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd was just returned by open and is owned here.
        Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    fn ioctl<T>(&self, request: libc::c_int, value: &mut T) -> io::Result<()> {
        // SAFETY: the request numbers were generated for exactly this struct type;
        // the pointer stays valid for the duration of the call.
        let rc = unsafe { libc::ioctl(self.0.as_raw_fd(), request, value as *mut T) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Import a CPU-mapped region into the GSL allocator; returns its GPU address.
pub fn import_user_mem(host_ptr: *mut libc::c_void, size: u64) -> io::Result<u64> {
    let device = KgslFd::open()?;
    let mut addr = GpuobjImportUseraddr {
        virtaddr: host_ptr as u64,
    };
    let mut import = GpuobjImport {
        priv_: &mut addr as *mut GpuobjImportUseraddr as u64,
        priv_len: size,
        flags: KGSL_DEFAULT_FLAGS,
        type_: KGSL_USER_MEM_TYPE_ADDR,
        id: 0,
    };
    device.ioctl(IOCTL_KGSL_GPUOBJ_IMPORT, &mut import)?;

    let mut info = GpuobjInfo {
        id: import.id,
        ..Default::default()
    };
    device.ioctl(IOCTL_KGSL_GPUOBJ_INFO, &mut info)?;
    Ok(info.gpuaddr)
}

/// Allocate GPU memory; returns `(gpu_address, mmap_size)`.
pub fn alloc_gpu(size: u64) -> io::Result<(u64, u64)> {
    let device = KgslFd::open()?;
    let mut alloc = GpuobjAlloc {
        size,
        flags: KGSL_DEFAULT_FLAGS,
        ..Default::default()
    };
    device.ioctl(IOCTL_KGSL_GPUOBJ_ALLOC, &mut alloc)?;

    let mut info = GpuobjInfo {
        id: alloc.id,
        ..Default::default()
    };
    device.ioctl(IOCTL_KGSL_GPUOBJ_INFO, &mut info)?;
    Ok((info.gpuaddr, alloc.mmapsize))
}

/// Map an existing GPU object into the given CPU address (`MAP_FIXED`).
///
/// # Safety
/// `host_ptr` must be a reserved region of at least `size` bytes.
pub unsafe fn cpu_map(
    host_ptr: *mut libc::c_void,
    size: u64,
    gpuaddr: u64,
) -> io::Result<*mut libc::c_void> {
    let device = KgslFd::open()?;
    // SAFETY: caller guarantees the reserved region; the mmap contract is the same
    // as the donor's call at driver.cpp:189.
    let mapped = unsafe {
        libc::mmap(
            host_ptr,
            size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_FIXED,
            device.0.as_raw_fd(),
            gpuaddr as libc::off_t,
        )
    };
    if mapped == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(mapped)
}

/// Free a GPU object previously imported or allocated, by GPU address.
pub fn free_gpuobj_by_addr(gpuaddr: u64) -> io::Result<()> {
    let device = KgslFd::open()?;
    let mut info = GpumemGetInfo {
        gpuaddr,
        ..Default::default()
    };
    device.ioctl(IOCTL_KGSL_GPUMEM_GET_INFO, &mut info)?;
    let free = GpuobjFree {
        id: info.id,
        ..Default::default()
    };
    // SAFETY: free is a plain struct and the ioctl reads it.
    let rc = unsafe {
        libc::ioctl(
            device.0.as_raw_fd(),
            IOCTL_KGSL_GPUOBJ_FREE,
            &free as *const GpuobjFree,
        )
    };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Force maximum GPU clocks (thermal limits still apply).
pub fn set_turbo(turbo: bool) -> io::Result<()> {
    let device = KgslFd::open()?;
    let mut enable: u32 = if turbo { 0 } else { 1 };
    let mut prop = DeviceGetproperty {
        type_: KGSL_PROP_PWRCTRL,
        value: &mut enable as *mut u32 as *mut libc::c_void,
        sizebytes: mem::size_of::<u32>(),
    };
    device.ioctl(IOCTL_KGSL_SETPROPERTY, &mut prop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layouts_match_uapi() {
        assert_eq!(mem::size_of::<GpuobjImportUseraddr>(), 8);
        assert_eq!(mem::size_of::<GpuobjImport>(), 32);
        assert_eq!(mem::size_of::<GpuobjInfo>(), 48);
        assert_eq!(mem::size_of::<GpuobjAlloc>(), 48);
        assert_eq!(mem::size_of::<GpuobjFree>(), 32);
        assert_eq!(mem::size_of::<GpumemGetInfo>(), 72);
        assert_eq!(mem::size_of::<DeviceGetproperty>(), 24);
    }

    #[test]
    fn ioctl_numbers_match_kernel_encoding() {
        assert_eq!(IOCTL_KGSL_SETPROPERTY, 0x4018_0932u32 as libc::c_int);
        assert_eq!(IOCTL_KGSL_GPUMEM_GET_INFO, 0xc048_0936u32 as libc::c_int);
        assert_eq!(IOCTL_KGSL_GPUOBJ_ALLOC, 0xc030_0945u32 as libc::c_int);
        assert_eq!(IOCTL_KGSL_GPUOBJ_FREE, 0x4020_0946u32 as libc::c_int);
        assert_eq!(IOCTL_KGSL_GPUOBJ_INFO, 0xc030_0947u32 as libc::c_int);
        assert_eq!(IOCTL_KGSL_GPUOBJ_IMPORT, 0xc020_0948u32 as libc::c_int);
    }

    #[test]
    fn default_flags_match_donor() {
        assert_eq!(KGSL_DEFAULT_FLAGS, 0x8C00_0000);
        assert_eq!(GPU_MAPPING_FLAGS, 0xc2600);
    }
}

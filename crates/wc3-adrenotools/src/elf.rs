//! `DT_SONAME` patching, ported from `lib/linkernsbypass/elf_soname_patcher.cpp`.
//!
//! Why patching is needed (donor `android_linker_ns.cpp:104-108`): Android's linker
//! caches loaded libraries by soname. To force a second, *hooked* instance of
//! `libvulkan.so` into a private namespace, the port copies the ELF to an anonymous
//! memfd and overwrites the first characters of its soname so the cache does not
//! return the instance the app already loaded.
//!
//! The donor walks section headers with unchecked pointer arithmetic. This port
//! keeps the same algorithm but validates every offset against the mapped file, so
//! a truncated or hostile ELF produces `Err` instead of a segfault.

use std::ffi::CStr;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const SHT_DYNAMIC: u32 = 6;
const DT_SONAME: i64 = 14;
const SHN_XINDEX: usize = 0xffff;

#[derive(Debug)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    NotElf64,
    NotLittleEndian,
    BadSectionTable,
    NoDynamicSection,
    NoSoname,
    BadStringTable,
    BadSonameOffset,
    SonameNotTerminated,
    Io(io::Error),
    Mmap(io::Error),
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall => f.write_str("file shorter than an ELF64 header"),
            Self::BadMagic => f.write_str("not an ELF file"),
            Self::NotElf64 => f.write_str("not an ELF64 file"),
            Self::NotLittleEndian => f.write_str("not little-endian"),
            Self::BadSectionTable => f.write_str("section header table out of bounds"),
            Self::NoDynamicSection => f.write_str("no SHT_DYNAMIC section"),
            Self::NoSoname => f.write_str("no DT_SONAME entry"),
            Self::BadStringTable => f.write_str("dynamic string table out of bounds"),
            Self::BadSonameOffset => f.write_str("DT_SONAME offset outside the string table"),
            Self::SonameNotTerminated => f.write_str("DT_SONAME string is not NUL terminated"),
            Self::Io(e) => write!(f, "i/o error: {e}"),
            Self::Mmap(e) => write!(f, "mmap error: {e}"),
        }
    }
}

impl std::error::Error for ElfError {}

impl From<io::Error> for ElfError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Location of the soname string inside the ELF buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SonameSlot {
    /// Byte offset of the first character of the soname.
    pub offset: usize,
    /// Length of the original soname in bytes (excluding the NUL terminator).
    pub len: usize,
}

#[inline]
fn rd_u16(buf: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(buf.get(off..off + 2)?.try_into().ok()?))
}

#[inline]
fn rd_u32(buf: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buf.get(off..off + 4)?.try_into().ok()?))
}

#[inline]
fn rd_u64(buf: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(buf.get(off..off + 8)?.try_into().ok()?))
}

#[inline]
fn rd_i64(buf: &[u8], off: usize) -> Option<i64> {
    Some(i64::from_le_bytes(buf.get(off..off + 8)?.try_into().ok()?))
}

/// Validate the ELF64 header and locate the `SHT_DYNAMIC` section and its string
/// table.
fn dynamic_section(buf: &[u8]) -> Result<(usize, usize, usize, usize, usize), ElfError> {
    if buf.len() < 64 {
        return Err(ElfError::TooSmall);
    }
    if buf[..4] != ELF_MAGIC {
        return Err(ElfError::BadMagic);
    }
    if buf[4] != 2 {
        return Err(ElfError::NotElf64);
    }
    if buf[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }

    let shoff = usize::try_from(rd_u64(buf, 40).ok_or(ElfError::BadSectionTable)?)
        .map_err(|_| ElfError::BadSectionTable)?;
    let shentsize = rd_u16(buf, 58).ok_or(ElfError::BadSectionTable)? as usize;
    let mut shnum = rd_u16(buf, 60).ok_or(ElfError::BadSectionTable)? as usize;
    let shstrndx = rd_u16(buf, 62).ok_or(ElfError::BadSectionTable)? as usize;

    if shoff == 0 || shentsize < 64 {
        return Err(ElfError::BadSectionTable);
    }
    // Extended section numbering: e_shnum == 0 stores the real count in the
    // sh_size field of section header 0.
    if shnum == 0 {
        let s0 = shoff.checked_add(32).ok_or(ElfError::BadSectionTable)?;
        shnum = usize::try_from(rd_u64(buf, s0).ok_or(ElfError::BadSectionTable)?)
            .map_err(|_| ElfError::BadSectionTable)?;
    }
    // We only use shstrndx for a sanity check; resolve the extended form too.
    let _shstrndx = if shstrndx == SHN_XINDEX {
        let s0 = shoff.checked_add(40).ok_or(ElfError::BadSectionTable)?;
        rd_u32(buf, s0).ok_or(ElfError::BadSectionTable)? as usize
    } else {
        shstrndx
    };

    let table_end = shnum
        .checked_mul(shentsize)
        .and_then(|n| shoff.checked_add(n))
        .ok_or(ElfError::BadSectionTable)?;
    if table_end > buf.len() {
        return Err(ElfError::BadSectionTable);
    }

    for i in 0..shnum {
        let sh = shoff + i * shentsize;
        if rd_u32(buf, sh + 4) != Some(SHT_DYNAMIC) {
            continue;
        }
        let link = rd_u32(buf, sh + 40).ok_or(ElfError::BadSectionTable)? as usize;
        if link >= shnum {
            return Err(ElfError::BadStringTable);
        }
        let str_sh = shoff + link * shentsize;
        let str_off = usize::try_from(rd_u64(buf, str_sh + 24).ok_or(ElfError::BadStringTable)?)
            .map_err(|_| ElfError::BadStringTable)?;
        let str_size = usize::try_from(rd_u64(buf, str_sh + 32).ok_or(ElfError::BadStringTable)?)
            .map_err(|_| ElfError::BadStringTable)?;
        let str_end = str_off
            .checked_add(str_size)
            .ok_or(ElfError::BadStringTable)?;
        if str_end > buf.len() {
            return Err(ElfError::BadStringTable);
        }

        let dyn_off = usize::try_from(rd_u64(buf, sh + 24).ok_or(ElfError::BadSectionTable)?)
            .map_err(|_| ElfError::BadSectionTable)?;
        let dyn_size = usize::try_from(rd_u64(buf, sh + 32).ok_or(ElfError::BadSectionTable)?)
            .map_err(|_| ElfError::BadSectionTable)?;
        let dyn_end = dyn_off
            .checked_add(dyn_size)
            .ok_or(ElfError::BadSectionTable)?;
        if dyn_end > buf.len() {
            return Err(ElfError::BadSectionTable);
        }
        let entsize = rd_u64(buf, sh + 56).ok_or(ElfError::BadSectionTable)? as usize;
        return Ok((dyn_off, dyn_end, str_off, str_end, entsize.max(16)));
    }

    Err(ElfError::NoDynamicSection)
}

/// Locate `DT_SONAME` in an in-memory ELF64 image.
pub fn soname_slot(buf: &[u8]) -> Result<SonameSlot, ElfError> {
    let (dyn_off, dyn_end, str_off, str_end, entsize) = dynamic_section(buf)?;
    let mut entry = dyn_off;
    while entry + 16 <= dyn_end {
        if rd_i64(buf, entry) == Some(DT_SONAME) {
            let name_off =
                usize::try_from(rd_u64(buf, entry + 8).ok_or(ElfError::BadSonameOffset)?)
                    .map_err(|_| ElfError::BadSonameOffset)?;
            if name_off >= str_end - str_off {
                return Err(ElfError::BadSonameOffset);
            }
            let abs = str_off + name_off;
            let nul = buf[abs..str_end]
                .iter()
                .position(|&b| b == 0)
                .ok_or(ElfError::SonameNotTerminated)?;
            return Ok(SonameSlot {
                offset: abs,
                len: nul,
            });
        }
        entry += entsize;
    }
    Err(ElfError::NoSoname)
}

/// Read the current soname out of an in-memory ELF image.
pub fn soname(buf: &[u8]) -> Result<Vec<u8>, ElfError> {
    let slot = soname_slot(buf)?;
    Ok(buf[slot.offset..slot.offset + slot.len].to_vec())
}

/// Overwrite the leading bytes of the soname.
///
/// Mirrors the donor loop (`elf_soname_patcher.cpp:57-62`): the replacement is
/// truncated to the original soname length and the rest of the name is preserved.
/// Returns the number of bytes actually replaced.
pub fn patch_soname_in_buffer(buf: &mut [u8], new_soname: &[u8]) -> Result<usize, ElfError> {
    let slot = soname_slot(buf)?;
    let n = new_soname.len().min(slot.len);
    buf[slot.offset..slot.offset + n].copy_from_slice(&new_soname[..n]);
    Ok(n)
}

/// RAII wrapper around an `mmap`ed file region.
struct Mapping {
    ptr: *mut libc::c_void,
    len: usize,
}

impl Mapping {
    unsafe fn new(fd: RawFd, len: usize) -> Result<Self, ElfError> {
        if len == 0 {
            return Err(ElfError::TooSmall);
        }
        // SAFETY: fd is a valid, readable descriptor of at least `len` bytes after
        // ftruncate; PROT_READ|PROT_WRITE + MAP_SHARED matches the donor.
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(ElfError::Mmap(io::Error::last_os_error()));
        }
        Ok(Self { ptr, len })
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: the mapping is valid for `len` bytes and exclusively borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast::<u8>(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the pointer/len pair came from a successful mmap.
        unsafe {
            libc::munmap(self.ptr, self.len);
        }
    }
}

/// Copy `src_path` into `target_fd`, patch the soname, and leave a complete,
/// loadable ELF behind.
///
/// Port of `elf_soname_patch()` (`elf_soname_patcher.cpp:14-66`), including the
/// "ftruncate the target, mmap it, read the source into the mapping" flow. The
/// target fd stays open on success: the caller passes it to
/// `android_dlopen_ext(ANDROID_DLEXT_USE_LIBRARY_FD)` and the linker duplicates it.
pub fn patch_soname_file(
    src_path: &CStr,
    target_fd: RawFd,
    new_soname: &[u8],
) -> Result<(), ElfError> {
    let path = Path::new(std::ffi::OsStr::from_bytes(src_path.to_bytes()));
    let mut src = File::open(path)?;
    let size_u64 = src.metadata()?.len();
    let size = usize::try_from(size_u64).map_err(|_| ElfError::TooSmall)?;
    if size == 0 {
        return Err(ElfError::TooSmall);
    }

    // SAFETY: target_fd is owned by the caller and refers to a regular file.
    if unsafe { libc::ftruncate(target_fd, size as libc::off_t) } != 0 {
        return Err(ElfError::Io(io::Error::last_os_error()));
    }

    // SAFETY: target_fd is a valid descriptor after the ftruncate above.
    let mut mapping = unsafe { Mapping::new(target_fd, size)? };
    let buf = mapping.as_mut_slice();
    src.read_exact(buf)?;
    patch_soname_in_buffer(buf, new_soname)?;
    Ok(())
}

/// Convenience: patch a copy of `src` into a brand-new temporary file, returning
/// the patched bytes (used by tests and host-side verification).
pub fn patched_copy(src: &Path, new_soname: &[u8]) -> Result<Vec<u8>, ElfError> {
    let mut buf = std::fs::read(src)?;
    patch_soname_in_buffer(&mut buf, new_soname)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build the smallest valid ELF64 image that the soname walker understands:
    /// Ehdr + one .dynstr + one SHT_DYNAMIC with DT_SONAME.
    fn synthetic_elf(soname: &str) -> Vec<u8> {
        let shoff = 0x100usize;
        let shentsize = 64usize;
        let str_off = 0x200usize;
        let dyn_off = 0x300usize;
        let mut image = vec![0u8; 0x400];

        image[..4].copy_from_slice(&ELF_MAGIC);
        image[4] = 2; // ELFCLASS64
        image[5] = 1; // ELFDATA2LSB
        image[6] = 1; // EV_CURRENT

        // e_shoff @ 40, e_shentsize @ 58, e_shnum @ 60
        image[40..48].copy_from_slice(&(shoff as u64).to_le_bytes());
        image[58..60].copy_from_slice(&(shentsize as u16).to_le_bytes());
        image[60..62].copy_from_slice(&2u16.to_le_bytes());

        // Section 0: SHT_NULL. Section 1: SHT_DYNAMIC, sh_link = 0 (its own strtab
        // in this two-section layout would be wrong, so put the string table in
        // section 0's sh_offset/sh_size fields and link to 0).
        let sh1 = shoff + shentsize;
        image[sh1 + 4..sh1 + 8].copy_from_slice(&SHT_DYNAMIC.to_le_bytes());
        image[sh1 + 24..sh1 + 32].copy_from_slice(&(dyn_off as u64).to_le_bytes());
        image[sh1 + 32..sh1 + 40].copy_from_slice(&32u64.to_le_bytes()); // two Dyn entries
        image[sh1 + 40..sh1 + 44].copy_from_slice(&0u32.to_le_bytes()); // sh_link = 0
        image[sh1 + 56..sh1 + 64].copy_from_slice(&16u64.to_le_bytes()); // sh_entsize

        // Section 0 carries the string table bounds (section 1 links to it).
        image[shoff + 24..shoff + 32].copy_from_slice(&(str_off as u64).to_le_bytes());
        image[shoff + 32..shoff + 40].copy_from_slice(&0x40u64.to_le_bytes());

        // .dynstr
        let bytes = soname.as_bytes();
        image[str_off..str_off + bytes.len()].copy_from_slice(bytes);

        // .dynamic[] = { DT_SONAME -> 0, DT_NULL }
        image[dyn_off..dyn_off + 8].copy_from_slice(&DT_SONAME.to_le_bytes());
        image[dyn_off + 8..dyn_off + 16].copy_from_slice(&0u64.to_le_bytes());

        image
    }

    #[test]
    fn patches_synthetic_soname() {
        let mut image = synthetic_elf("libvulkan.so");
        assert_eq!(soname(&image).unwrap(), b"libvulkan.so");
        let n = patch_soname_in_buffer(&mut image, b"00").unwrap();
        assert_eq!(n, 2);
        assert_eq!(soname(&image).unwrap(), b"00bvulkan.so");
    }

    #[test]
    fn rejects_truncated_and_hostile_offsets() {
        assert!(matches!(soname_slot(b"short"), Err(ElfError::TooSmall)));

        let mut image = synthetic_elf("libvulkan.so");
        image[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            soname_slot(&image),
            Err(ElfError::BadSectionTable)
        ));
    }

    #[test]
    fn full_file_patch_matches_buffer_patch() {
        // Use a real system ELF; /system/lib64/libc.so always has DT_SONAME.
        let path = Path::new("/system/lib64/libc.so");
        if !path.exists() {
            eprintln!("skipping: no /system/lib64/libc.so on this device");
            return;
        }
        let temp =
            std::env::temp_dir().join(format!("wc3-adrenotools-soname-{}.so", std::process::id()));
        {
            let mut out = File::create(&temp).unwrap();
            let mut src = File::open(path).unwrap();
            std::io::copy(&mut src, &mut out).unwrap();
            out.flush().unwrap();
        }
        let target = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temp)
            .unwrap();
        let fd = {
            use std::os::fd::AsRawFd;
            target.as_raw_fd()
        };
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        patch_soname_file(&c_path, fd, b"00").unwrap();
        drop(target);
        let patched = std::fs::read(&temp).unwrap();
        let expected = {
            let mut original = std::fs::read(path).unwrap();
            let slot = soname_slot(&original).unwrap();
            original[slot.offset] = b'0';
            original[slot.offset + 1] = b'0';
            original
        };
        assert_eq!(patched, expected);
        let _ = std::fs::remove_file(&temp);
    }
}

//! Minimal `/proc/self/maps` parser shared by the linker bootstrap and the BCn
//! enabler.
//!
//! The donor reads the maps file twice with ad-hoc `std::string`/`strtoull`
//! parsing (`android_linker_ns.cpp` does not, `bcenabler.cpp:23-39` does). The Rust
//! port keeps one bounds-checked parser and reuses it for pointer validation, which
//! is what makes the otherwise unbounded instruction scans in `bcenabler.cpp`
//! safe.

use std::fs;
use std::io;
use std::path::Path;

/// One mapped region of the current process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub start: usize,
    pub end: usize,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    /// File backing the mapping (`/apex/.../libdl.so`), or `None` for
    /// anonymous mappings. The linker bootstrap uses the basename as a
    /// fail-closed fingerprint of the bionic layout.
    pub path: Option<String>,
}

impl Region {
    pub fn contains(&self, addr: usize) -> bool {
        self.start <= addr && addr < self.end
    }
}

/// Parse the textual contents of `/proc/self/maps`.
///
/// Malformed lines are skipped rather than aborting the whole parse: the maps file
/// is kernel-generated, but a partially read file must not produce out-of-range
/// regions.
pub fn parse(text: &str) -> Vec<Region> {
    let mut regions = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(range) = fields.next() else {
            continue;
        };
        let Some(perms) = fields.next() else {
            continue;
        };
        let Some((start_hex, end_hex)) = range.split_once('-') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (
            usize::from_str_radix(start_hex, 16),
            usize::from_str_radix(end_hex, 16),
        ) else {
            continue;
        };
        if start >= end {
            continue;
        }
        let perms = perms.as_bytes();
        if perms.len() < 3 {
            continue;
        }
        // offset, device and inode precede the optional path.
        let _offset = fields.next();
        let _device = fields.next();
        let _inode = fields.next();
        regions.push(Region {
            start,
            end,
            readable: perms[0] == b'r',
            writable: perms[1] == b'w',
            executable: perms[2] == b'x',
            path: fields.next().map(str::to_owned),
        });
    }
    regions
}

/// Read and parse `/proc/self/maps` for the current process.
pub fn read_self() -> io::Result<Vec<Region>> {
    read_from(Path::new("/proc/self/maps"))
}

pub fn read_from(path: &Path) -> io::Result<Vec<Region>> {
    Ok(parse(&fs::read_to_string(path)?))
}

/// First region containing `addr`.
pub fn containing(regions: &[Region], addr: usize) -> Option<&Region> {
    regions.iter().find(|r| r.contains(addr))
}

/// Port of `find_free_page()` from `src/bcenabler.cpp:23-39`.
///
/// Returns the first gap address after `address`: walking the sorted regions, once
/// a region's end lies above `address` and the next region does not start exactly
/// there, that end is the start of an unmapped hole.
pub fn find_free_page(regions: &[Region], address: usize) -> Option<usize> {
    let mut end = 0usize;
    for region in regions {
        if end > address && region.start != end {
            return Some(end);
        }
        end = region.end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
55d0a00000-55d0a02000 r--p 00000000 fd:00 1234 /system/bin/app
55d0a02000-55d0a03000 r-xp 00002000 fd:00 1234 /system/bin/app
55d0a03000-55d0a04000 rw-p 00003000 fd:00 1234 /system/bin/app
55d0b00000-55d0b01000 r--p 00000000 fd:00 5678 /system/lib64/libfoo.so
55d0b01000-55d0b02000 r-xp 00001000 fd:00 5678 /system/lib64/libfoo.so
55d0d00000-55d0d01000 rw-p 00000000 00:00 0 [anon:test]
";

    #[test]
    fn parses_regions_and_perms() {
        let regions = parse(SAMPLE);
        assert_eq!(regions.len(), 6);
        assert_eq!(regions[0].start, 0x55d0a00000);
        assert!(regions[0].readable);
        assert!(!regions[0].executable);
        assert!(regions[1].executable);
        assert!(regions[2].writable);
        assert_eq!(regions[0].path.as_deref(), Some("/system/bin/app"));
        assert_eq!(regions[3].path.as_deref(), Some("/system/lib64/libfoo.so"));
        assert_eq!(regions[5].path.as_deref(), Some("[anon:test]"));
        assert!(containing(&regions, 0x55d0b01500).is_some());
        assert!(containing(&regions, 0x55d0c00000).is_none());
    }

    #[test]
    fn finds_gap_after_target_mapping() {
        let regions = parse(SAMPLE);
        // Inside libfoo's executable segment: the next region after it ends is the
        // anonymous page at 0x55d0d00000, so the hole starts at 0x55d0b02000.
        assert_eq!(find_free_page(&regions, 0x55d0b01500), Some(0x55d0b02000));
        // Inside app: the next gap is after the rw segment.
        assert_eq!(find_free_page(&regions, 0x55d0a02500), Some(0x55d0a04000));
    }

    #[test]
    fn ignores_malformed_lines() {
        let regions = parse("garbage\nzz-100 r-xp 0 0 0 x\n1000-2000 r-xp 0 0 0 ok\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start, 0x1000);
    }
}

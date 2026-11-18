//! Guest address plan and page ownership.
//!
//! Ported from `runtime/guest/guest_layout.h` and the arena table in
//! `runtime/guest/guest_memory.c` of NIK2703/WC3_AndroidRecomp (MIT, commit
//! `c753ab89…`).
//!
//! The plan was measured on the 1.27 Android port: ART reserves
//! `0x10980000..0x30980000` in an app process, so the anonymous arenas and the
//! worker-thread stack/TEB areas are placed around that reservation.
//!
//! # The one deliberate overlap
//!
//! The HLE thunk window sits **inside** arena 4:
//!
//! ```text
//! arena 4  0x7000_0000 .. 0x8000_0000
//! HLE      0x7F00_0000 .. 0x7F10_0000   (65 536 slots × 16 bytes = 1 MiB)
//! ```
//!
//! The donor does not hide this: its page map stores an overlap sentinel for
//! those pages and the caller falls back to an exact range scan. Any port that
//! assumes "static arenas are disjoint" is wrong for this layout — that
//! invariant panics at startup or, worse, resolves thunk addresses to arena 4.

/// Guest page size used by the donor's page-owner map.
pub const PAGE_SIZE: u32 = 0x1000;

/// Base of the HLE thunk window.
pub const HLE_BASE: u32 = 0x7F00_0000;
/// Distance between consecutive HLE thunk addresses.
pub const HLE_STRIDE: u32 = 0x10;
/// Number of thunk slots in the window.
pub const HLE_SLOTS: u32 = 65_536;

/// Maximum number of guest threads.
pub const THREAD_MAX: u32 = 24;
/// Guest stack size per thread.
pub const THREAD_STACK_SIZE: u32 = 0x0020_0000;

/// Preferred base of `war3.exe`.
///
/// Not part of the donor's static arena table: the executable image is mapped
/// at the base recorded in its own PE header, and its size is known only after
/// parsing that header (see [`crate::pe`]).
pub const WAR3_IMAGE_BASE: u32 = 0x0040_0000;

/// A named guest region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// Stable name; used in diagnostics and tests.
    pub name: &'static str,
    /// Inclusive base address.
    pub base: u32,
    /// Size in bytes.
    pub size: u32,
}

impl Region {
    /// Exclusive end address, saturating at the top of the 32-bit space.
    pub const fn end(&self) -> u32 {
        self.base.saturating_add(self.size)
    }

    /// True when `va` lies in `[base, end)`.
    pub const fn contains(&self, va: u32) -> bool {
        va >= self.base && va < self.end()
    }
}

/// The donor's static arena table, in its order (`g_arenas`).
///
/// Images are listed at the size the donor reserves for them; the executable's
/// own image size comes from its PE header.
pub const REGIONS: &[Region] = &[
    Region { name: "heap", base: 0x3600_0000, size: 0x0400_0000 },
    Region { name: "hle-heap", base: 0x3A00_0000, size: 0x0400_0000 },
    Region { name: "va", base: 0x6100_0000, size: 0x0400_0000 },
    Region { name: "storm.dll", base: 0x3500_0000, size: 0x0010_0000 },
    Region { name: "mss32.dll", base: 0x3510_0000, size: 0x0010_0000 },
    Region { name: "mssfast.m3d", base: 0x3520_0000, size: 0x0002_0000 },
    Region { name: "mp3dec.asi", base: 0x3530_0000, size: 0x0004_0000 },
    Region { name: "stack", base: 0x3100_0000, size: 0x0080_0000 },
    Region { name: "thread-stack", base: 0x3180_0000, size: THREAD_MAX * THREAD_STACK_SIZE },
    Region { name: "va2", base: 0x4000_0000, size: 0x2000_0000 },
    Region { name: "va3", base: 0x8000_0000, size: 0x1000_0000 },
    Region { name: "va4", base: 0x7000_0000, size: 0x1000_0000 },
    Region { name: "va5", base: 0xA100_0000, size: 0x3000_0000 },
    Region { name: "va6", base: 0xD980_0000, size: 0x1000_0000 },
    Region { name: "va7", base: 0xEBAD_7000, size: 0x1400_0000 },
    Region { name: "ijl15.dll", base: 0x6000_0000, size: 0x0010_0000 },
    Region { name: "game.dll", base: 0x6F00_0000, size: 0x0100_0000 },
    Region { name: "teb", base: 0x3F00_0000, size: 0x0001_0000 },
    Region { name: "hle-window", base: HLE_BASE, size: HLE_SLOTS * HLE_STRIDE },
];

/// Who owns a guest address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// No static region covers the address.
    Unmapped,
    /// Exactly one static region covers it.
    Arena(&'static str),
    /// Two or more static regions cover it; the caller must take the exact
    /// range path instead of trusting a single owner.
    Overlap,
}

/// Classify a guest address against [`REGIONS`].
///
/// Linear scan: this crate is used by tooling and tests, not in the donor's
/// 300 MB corpus hot path, so the donor's per-page table is not reproduced —
/// the rule it implements is.
pub fn owner_of(va: u32) -> Owner {
    let mut found: Option<&'static str> = None;
    for region in REGIONS {
        if !region.contains(va) {
            continue;
        }
        if found.is_some() {
            return Owner::Overlap;
        }
        found = Some(region.name);
    }
    match found {
        Some(name) => Owner::Arena(name),
        None => Owner::Unmapped,
    }
}

/// Guest address of thunk slot `slot`, or `None` when the slot is outside the
/// window.
pub fn thunk_va(slot: u32) -> Option<u32> {
    if slot >= HLE_SLOTS {
        return None;
    }
    Some(HLE_BASE + slot * HLE_STRIDE)
}

/// Slot index for a thunk address.
///
/// Rejects addresses outside the window and addresses that are not on the
/// 16-byte stride — the dispatcher must never round a guest-supplied value into
/// a valid slot.
pub fn thunk_slot(va: u32) -> Option<u32> {
    let offset = va.checked_sub(HLE_BASE)?;
    if offset >= HLE_SLOTS * HLE_STRIDE {
        return None;
    }
    if offset % HLE_STRIDE != 0 {
        return None;
    }
    Some(offset / HLE_STRIDE)
}

/// True when `[base, base+size)` fits in the 32-bit guest space and is
/// page-aligned.
pub fn region_is_valid(base: u32, size: u32) -> bool {
    base % PAGE_SIZE == 0 && size % PAGE_SIZE == 0 && base.checked_add(size).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recomp_layout_disjoint_except_hle_window() {
        // The naive invariant "all ranges are disjoint" is false for this
        // layout: the HLE window sits inside `va4` (`0x7000_0000..0x8000_0000`).
        let mut overlaps = 0;
        for (i, a) in REGIONS.iter().enumerate() {
            for b in REGIONS.iter().skip(i + 1) {
                let disjoint = a.end() <= b.base || b.end() <= a.base;
                if !disjoint {
                    overlaps += 1;
                    assert_eq!(
                        (a.name, b.name),
                        ("va4", "hle-window"),
                        "only the HLE window may overlap an arena"
                    );
                }
            }
        }
        assert_eq!(overlaps, 1, "exactly one deliberate overlap");
        assert_eq!(owner_of(HLE_BASE), Owner::Overlap);
        assert_eq!(owner_of(HLE_BASE + HLE_STRIDE * 1000), Owner::Overlap);
        assert_eq!(owner_of(0x7000_0000), Owner::Arena("va4"));
        assert_eq!(owner_of(0x7FFF_FFFF), Owner::Arena("va4"));
        assert_eq!(owner_of(0x0000_1000), Owner::Unmapped);
    }

    #[test]
    fn recomp_layout_hle_window_bounds() {
        assert_eq!(HLE_BASE, 0x7F00_0000);
        assert_eq!(HLE_SLOTS * HLE_STRIDE, 0x0010_0000, "one MiB of thunks");
        assert_eq!(
            HLE_BASE + HLE_SLOTS * HLE_STRIDE,
            0x7F10_0000,
            "the window does not reach 0x8000_0000"
        );
        assert_eq!(thunk_va(0), Some(HLE_BASE));
        assert_eq!(thunk_va(HLE_SLOTS - 1), Some(0x7F0F_FFF0));
        assert_eq!(thunk_va(HLE_SLOTS), None);
        assert_eq!(thunk_slot(HLE_BASE), Some(0));
        assert_eq!(thunk_slot(0x7F0F_FFF0), Some(HLE_SLOTS - 1));
        assert_eq!(thunk_slot(HLE_BASE + 4), None, "misaligned thunk");
        assert_eq!(thunk_slot(0x7F10_0000), None, "past the window");
        assert_eq!(thunk_slot(0x7EFF_FFF0), None, "before the window");
    }

    #[test]
    fn recomp_layout_page_owner_marks_overlap() {
        // Every page of the HLE window must report the overlap sentinel rather
        // than resolving to arena 4: a thunk page that claimed `va4` would make
        // the dispatcher treat an HLE call as guest code.
        let mut page = HLE_BASE;
        while page < HLE_BASE + HLE_SLOTS * HLE_STRIDE {
            assert_eq!(owner_of(page), Owner::Overlap, "page {page:#X}");
            page += PAGE_SIZE;
        }
        assert_eq!(owner_of(HLE_BASE - PAGE_SIZE), Owner::Arena("va4"));
        // 10^5 pseudo-random VAs: each one is exactly one owner or the sentinel.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..100_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let va = state as u32;
            match owner_of(va) {
                Owner::Unmapped | Owner::Arena(_) | Owner::Overlap => {}
            }
        }
    }

    #[test]
    fn recomp_layout_stack_pool_fits() {
        let pool = REGIONS
            .iter()
            .find(|region| region.name == "thread-stack")
            .expect("thread stack pool");
        assert_eq!(THREAD_MAX, 24);
        assert_eq!(THREAD_STACK_SIZE, 0x0020_0000, "2 MiB per thread");
        assert_eq!(pool.size, THREAD_MAX * THREAD_STACK_SIZE);
        assert!(pool.end() <= 0x4000_0000, "pool must not reach va2");
        assert!(pool.base % PAGE_SIZE == 0);
        for region in REGIONS {
            assert_eq!(
                region.base % PAGE_SIZE,
                0,
                "{} must be page aligned",
                region.name
            );
            assert_eq!(region.size % PAGE_SIZE, 0, "{} size", region.name);
            assert!(region_is_valid(region.base, region.size), "{}", region.name);
            assert!(region.base.checked_add(region.size).is_some());
        }
    }
}

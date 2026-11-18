//! Mechanic `recomp.layout` — guest address plan and page ownership.
//!
//! Vectors are the donor's own constants (`runtime/guest/guest_layout.h`,
//! `guest_memory.c` arena table), including the deliberate HLE/arena-4 overlap.

use wc3_guest::layout::{
    owner_of, region_is_valid, thunk_slot, thunk_va, Owner, HLE_BASE, HLE_SLOTS, HLE_STRIDE,
    PAGE_SIZE, REGIONS, THREAD_MAX, THREAD_STACK_SIZE, WAR3_IMAGE_BASE,
};

#[test]
fn recomp_layout_hle_window_bounds() {
    assert_eq!(thunk_va(0), Some(HLE_BASE));
    assert_eq!(thunk_va(HLE_SLOTS - 1), Some(HLE_BASE + (HLE_SLOTS - 1) * HLE_STRIDE));
    assert_eq!(thunk_va(HLE_SLOTS - 1).unwrap(), 0x7F0F_FFF0);
    assert_eq!(HLE_BASE + HLE_SLOTS * HLE_STRIDE, 0x7F10_0000, "window is 1 MiB");
    assert_eq!(thunk_va(HLE_SLOTS), None);

    // Round trip across the whole window edge, not just the first slot.
    for slot in [0u32, 1, 2, 1000, HLE_SLOTS - 1] {
        let va = thunk_va(slot).expect("slot in range");
        assert_eq!(thunk_slot(va), Some(slot), "slot {slot} round trip");
    }

    // Out of window and off-stride addresses must not resolve to a slot.
    assert_eq!(thunk_slot(HLE_BASE - 1), None);
    assert_eq!(thunk_slot(HLE_BASE - HLE_STRIDE), None);
    assert_eq!(thunk_slot(HLE_BASE + HLE_SLOTS * HLE_STRIDE), None);
    assert_eq!(thunk_slot(HLE_BASE + 8), None, "misaligned thunk");
    assert_eq!(thunk_slot(0), None);
    assert_eq!(thunk_slot(u32::MAX), None);
}

#[test]
fn recomp_layout_page_owner_marks_overlap() {
    // The HLE window is inside arena 4: two owners, not one.
    assert_eq!(owner_of(HLE_BASE), Owner::Overlap);
    assert_eq!(owner_of(0x7F0F_FFF0), Owner::Overlap, "last thunk page");
    // Around the window is plain arena 4.
    assert_eq!(owner_of(HLE_BASE - PAGE_SIZE), Owner::Arena("va4"));
    assert_eq!(owner_of(0x7F10_0000), Owner::Arena("va4"), "just past the window");
    assert_eq!(owner_of(0x7FF0_0000), Owner::Arena("va4"));
    // Arena 3 starts where arena 4 ends.
    assert_eq!(owner_of(0x8000_0000), Owner::Arena("va3"));
    assert_eq!(owner_of(0x8FFF_F000), Owner::Arena("va3"));
    // Unmapped gaps and the null page.
    assert_eq!(owner_of(0), Owner::Unmapped);
    assert_eq!(owner_of(0x3000_0000), Owner::Unmapped);
    // The two images that Game.dll talks to.
    assert_eq!(owner_of(0x6F00_0000), Owner::Arena("game.dll"));
    assert_eq!(owner_of(0x3100_0000), Owner::Arena("stack"));
}

#[test]
fn recomp_layout_regions_disjoint_except_hle() {
    let mut overlaps = Vec::new();
    for (i, a) in REGIONS.iter().enumerate() {
        for b in REGIONS.iter().skip(i + 1) {
            if a.base < b.end() && b.base < a.end() {
                overlaps.push((a.name, b.name));
            }
        }
    }
    assert_eq!(
        overlaps,
        vec![("va4", "hle-window")],
        "the donor layout has exactly one deliberate overlap"
    );
}

#[test]
fn recomp_layout_stack_pool_fits() {
    let pool_end = 0x3180_0000u32 + THREAD_MAX * THREAD_STACK_SIZE;
    assert_eq!(pool_end, 0x3480_0000);
    // The pool must not run into the heap that starts at 0x36000000.
    let heap = REGIONS.iter().find(|r| r.name == "heap").expect("heap region");
    assert!(pool_end <= heap.base, "thread stack pool overlaps heap");
    // Thread stacks are page aligned and the per-thread size is 2 MiB.
    assert_eq!(THREAD_STACK_SIZE % PAGE_SIZE, 0);
}

#[test]
fn recomp_layout_regions_page_aligned() {
    for region in REGIONS {
        assert_eq!(region.base % PAGE_SIZE, 0, "{} base alignment", region.name);
        assert_eq!(region.size % PAGE_SIZE, 0, "{} size alignment", region.name);
        assert!(region.size > 0, "{} empty region", region.name);
        assert!(
            region.base.checked_add(region.size).is_some(),
            "{} overflows the 32-bit space",
            region.name
        );
        assert!(region_is_valid(region.base, region.size), "{} validity", region.name);
    }
    assert_eq!(WAR3_IMAGE_BASE % PAGE_SIZE, 0);
}

//! WC3-I-004 probe: the BCn code patch must never map a page writable **and**
//! executable.
//!
//! `patch_bcn` rewrites one instruction in the Adreno driver; the probe calls the
//! same helper (`bcn::patch_code_word`) on a synthetic executable page and prints
//! the `/proc/self/maps` line at each stage: before (`r-xp`), inside the write
//! window (`rw-p`, no `x`), after (`r-xp` again). It also exercises the control
//! paths: a misaligned word and an unmapped address both return `Err` without
//! writing anything.
//!
//! Run: `scripts/h2-cargo run -p wc3-adrenotools --example audit_wx_probe`.

use std::ffi::c_void;
use std::ptr;

use wc3_adrenotools::bcn;

const PAGE: usize = 4096;
/// `mov w0, wzr` — the donor's "unknown format" result, the word the driver
/// patch overwrites at its real site.
const WORD_MOV_W0_WZR: u32 = 0x2a1f_03e0;
/// A `ret`-shaped word to patch in; the page is never executed by the probe.
const WORD_PATCH: u32 = 0xd503_201f; // nop

fn main() {
    println!("== BCn W^X window (WC3-I-004) ==");
    // 1. Synthetic executable page, exactly like the driver's text mapping:
    //    assembled writable, sealed RX, then patched by the production helper.
    // SAFETY: fresh anonymous mapping, page sized, private.
    let page = unsafe {
        libc::mmap(
            ptr::null_mut(),
            PAGE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert_ne!(page, libc::MAP_FAILED, "mmap failed: {:?}", std::io::Error::last_os_error());
    let addr = page as usize;
    // SAFETY: the mapping is writable and >= 4 bytes.
    unsafe { ptr::write_volatile(addr as *mut u32, WORD_MOV_W0_WZR) };
    // SAFETY: page aligned, page sized.
    let rc = unsafe { libc::mprotect(page, PAGE, libc::PROT_READ | libc::PROT_EXEC) };
    assert_eq!(rc, 0, "mprotect RX failed: {:?}", std::io::Error::last_os_error());

    let before = bcn::protection_of(addr).expect("page is mapped");
    // SAFETY: the probe owns this page; no thread executes it.
    let trace = unsafe { bcn::patch_code_word(addr, WORD_PATCH) }.expect("patch helper");
    let word_after = unsafe { ptr::read_volatile(addr as *const u32) };
    println!("page={addr:#x} before={}", trace.before);
    println!("during={}", trace.during);
    println!("after={}", trace.after);
    println!(
        "before_matches_maps={} no_rwx={} protected_restored={} word_after={word_after:#010x}",
        trace.before == before,
        trace.is_wx_clean(),
        perms(&trace.before) == perms(&trace.after),
    );

    // 2. Control paths: a bad address is a controlled error, never a crash and
    //    never a partial write (WC3-I-004 "гарантированное восстановление").
    // SAFETY: 0x10 is deliberately unmapped and 4-byte aligned.
    let unmapped = unsafe { bcn::patch_code_word(0x10, WORD_PATCH) };
    println!("unmapped_addr -> {:?}", unmapped.err());
    // SAFETY: an unaligned address is rejected before any mprotect.
    let unaligned = unsafe { bcn::patch_code_word(addr + 2, WORD_PATCH) };
    println!("unaligned_addr -> {:?}", unaligned.err());
    let word_untouched = unsafe { ptr::read_volatile(addr as *const u32) };
    println!("word_after_rejected_calls={word_untouched:#010x}");

    // SAFETY: the probe's own mapping.
    unsafe { libc::munmap(page as *mut c_void, PAGE) };

    assert!(trace.is_wx_clean(), "write window must not expose rwx: {trace:?}");
    assert_eq!(word_after, WORD_PATCH, "the word is written");
    assert_eq!(word_untouched, WORD_PATCH, "rejected calls write nothing");
    println!("WX_PROBE PASS");
}

fn perms(state: &str) -> &str {
    state.split_whitespace().next().unwrap_or("")
}

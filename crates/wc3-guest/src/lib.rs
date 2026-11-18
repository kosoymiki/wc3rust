//! Guest machine model for the Warcraft III 1.27 static-recompilation path.
//!
//! This crate holds the runtime half of the recompilation contract: what a
//! lifted guest program needs in order to execute and be tested. It contains no
//! disassembler and no lifter — it is the machine the lifted code would run on:
//!
//! - [`cpu`] — x86-32 integer flag semantics, rotate/shift/bit-scan helpers and
//!   the fixed CPUID/RDTSC profile the guest observes.
//! - [`layout`] — the guest address plan (arenas, images, TEB, HLE thunk
//!   window) and its page-owner rule, including the one deliberate overlap.
//! - [`hle`] — the HLE thunk registry: a guest import is a thunk address in the
//!   reserved window, resolved by arithmetic, with an explicit arity per entry.
//! - [`pe`] — a bounds-checked PE32 image parser (headers, sections, exports by
//!   name and ordinal, imports by name and ordinal).
//!
//! # Provenance
//!
//! Ported from `NIK2703/WC3_AndroidRecomp`
//! (`https://github.com/NIK2703/WC3_AndroidRecomp`), commit
//! `c753ab89debcf729b1a815ec29a14b035c47a92f`, Copyright (c) 2026 NIK2703,
//! MIT licence. Donor sources per module:
//!
//! | module | donor file(s) |
//! |---|---|
//! | [`cpu`] | `runtime/cpu/cpu.h`, `tests/cpu_selftest.c` (test vectors) |
//! | [`layout`] | `runtime/guest/guest_layout.h`, `runtime/guest/guest_memory.c` |
//! | [`hle`] | `runtime/hle/abi/hle_abi.h`, `runtime/hle/abi/hle_abi.c` |
//! | [`pe`] | `runtime/guest/pe.c`, `runtime/guest/pe.h` |
//!
//! Deliberate differences from the donor, and why:
//!
//! - The donor's `CPU` is a frozen `#[repr(C)]` struct shared with C translation
//!   units it does not have here. Our [`cpu::Cpu`] is idiomatic Rust (bools for
//!   flags) and is **not** ABI-compatible; the donor's frozen offsets (64 / 216)
//!   are recorded as constants for anyone who later builds an FFI bridge.
//! - The donor resolves the page owner through a `u8`-per-page table because it
//!   runs a 300 MB corpus at speed. Here the region list is 19 entries, so the
//!   owner is a linear scan; the *rule* (including the overlap sentinel) is
//!   identical and pinned by tests.
//! - No x87/SSE yet: the donor's `x87.h` model is the next unit and is tracked
//!   in `docs/audit/status-donor-androidrecomp.md`. Integer semantics are
//!   complete for the instructions its lifter emits outside FPU code.

pub mod cpu;
pub mod hle;
pub mod layout;
pub mod pe;

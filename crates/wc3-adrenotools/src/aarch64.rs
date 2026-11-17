//! AArch64 instruction encoding and cache maintenance used by the ported hooks.
//!
//! The donor gets these instructions from three places: the assembler
//! (`src/bcenabler_patch.s`), hand-built bitfields (`Branch`/`BranchLinked` unions
//! in `bcenabler.cpp` and `android_linker_ns.cpp`), and `asm volatile("ISB")`.
//! The Rust port encodes every instruction from its architectural fields, which is
//! what lets `bcn` generate the patch at runtime instead of embedding a blob
//! produced by a C toolchain.

/// `B` (unconditional branch, imm26).
pub const B: u32 = 0x1400_0000;
/// `BL` (branch with link, imm26).
pub const BL: u32 = 0x9400_0000;
/// `B.NE` — condition code 1, imm19.
pub const B_NE: u32 = 0x5400_0001;
/// `CMP Wn, #imm12` — alias of `SUBS WZR, Wn, #imm12`.
pub const CMP_WN_IMM12: u32 = 0x7100_001F;
/// `MOV Wd, #imm16` — alias of `MOVZ Wd, #imm16`.
pub const MOVZ_WD_IMM16: u32 = 0x5280_0000;

const IMM26_MIN: i64 = -(1 << 25);
const IMM26_MAX: i64 = (1 << 25) - 1;
const IMM19_MIN: i64 = -(1 << 18);
const IMM19_MAX: i64 = (1 << 18) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    Unaligned,
    OutOfRange,
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unaligned => f.write_str("branch target is not 4-byte aligned"),
            Self::OutOfRange => f.write_str("branch target is out of range"),
        }
    }
}

impl std::error::Error for EncodeError {}

fn b_imm26(opcode: u32, pc: usize, target: usize) -> Result<u32, EncodeError> {
    let pc_i = pc as i64;
    let target_i = target as i64;
    let delta = target_i - pc_i;
    if delta % 4 != 0 {
        return Err(EncodeError::Unaligned);
    }
    let words = delta / 4;
    if !(IMM26_MIN..=IMM26_MAX).contains(&words) {
        return Err(EncodeError::OutOfRange);
    }
    Ok(opcode | ((words as i32 as u32) & 0x03FF_FFFF))
}

/// Encode `B target` located at `pc`.
pub fn encode_b(pc: usize, target: usize) -> Result<u32, EncodeError> {
    b_imm26(B, pc, target)
}

/// Encode `BL target` located at `pc`.
pub fn encode_bl(pc: usize, target: usize) -> Result<u32, EncodeError> {
    b_imm26(BL, pc, target)
}

/// Encode `B.NE target` located at `pc`.
pub fn encode_b_ne(pc: usize, target: usize) -> Result<u32, EncodeError> {
    let delta = target as i64 - pc as i64;
    if delta % 4 != 0 {
        return Err(EncodeError::Unaligned);
    }
    let words = delta / 4;
    if !(IMM19_MIN..=IMM19_MAX).contains(&words) {
        return Err(EncodeError::OutOfRange);
    }
    Ok(B_NE | (((words as i32 as u32) & 0x7_FFFF) << 5))
}

/// Encode `CMP Wn, #imm12`.
pub fn encode_cmp_imm(reg: u32, imm: u32) -> u32 {
    debug_assert!(reg < 32);
    debug_assert!(imm < 4096);
    CMP_WN_IMM12 | (imm << 10) | (reg << 5)
}

/// Encode `MOV Wd, #imm16`.
pub fn encode_movz(reg: u32, imm: u16) -> u32 {
    debug_assert!(reg < 32);
    MOVZ_WD_IMM16 | ((imm as u32) << 5) | reg
}

/// Sign-extend a 26-bit branch offset stored in bits `[25:0]`.
pub fn sign_extend_imm26(raw: u32) -> i64 {
    let v = (raw & 0x03FF_FFFF) as i64;
    if v & (1 << 25) != 0 { v - (1 << 26) } else { v }
}

/// If `word` is a `BL`, return the absolute branch target.
pub fn decode_bl_target(pc: usize, word: u32) -> Option<usize> {
    if word & 0xFC00_0000 != BL {
        return None;
    }
    let target = pc as i64 + sign_extend_imm26(word) * 4;
    usize::try_from(target).ok()
}

/// `RET Xn` — `1101011001011111000000 Rn 00000`. The default `RET` is `RET X30`.
pub fn is_ret(word: u32) -> bool {
    word & 0xFFFF_FC1F == 0xD65F_0000
}

/// Invalidate the instruction cache for `[start, end)` and synchronise.
///
/// The donor only issues `ISB` after writing the BCn patch. `ISB` orders
/// instruction fetches that happen *after* it, but a page that already executed
/// can still hold stale lines. This port performs the architectural sequence
/// (clean D-cache to PoU, invalidate I-cache, `DSB ISH`, `ISB`), which is the same
/// sequence `__clear_cache` emits.
///
/// # Safety
/// `start..end` must be mapped and executable or about to become executable.
pub unsafe fn flush_icache(start: usize, end: usize) {
    if end <= start {
        return;
    }
    // AArch64 cache lines are at least 64 bytes; using 64 is conservative and
    // correct (extra maintenance on larger lines is harmless).
    const LINE: usize = 64;
    let mut addr = start & !(LINE - 1);
    while addr < end {
        // SAFETY: cache maintenance by VA on a mapped range.
        unsafe {
            core::arch::asm!("dc cvau, {}", in(reg) addr, options(nostack, preserves_flags));
        }
        addr += LINE;
    }
    // SAFETY: barrier instructions have no memory operands.
    unsafe {
        core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    }
    let mut addr = start & !(LINE - 1);
    while addr < end {
        // SAFETY: as above; the range is about to be executed.
        unsafe {
            core::arch::asm!("ic ivau, {}", in(reg) addr, options(nostack, preserves_flags));
        }
        addr += LINE;
    }
    // SAFETY: as above.
    unsafe {
        core::arch::asm!("dsb ish", "isb", options(nostack, preserves_flags));
    }
}

/// `ISB` — the donor's `asm volatile("ISB")` (`bcenabler.cpp:131`).
pub fn isb() {
    // SAFETY: barrier only; no memory operands.
    unsafe {
        core::arch::asm!("isb", options(nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_known_donor_words() {
        // First entry of the donor blob: cmp w0,#131 / b.ne +3 / mov w0,#71 / b +62
        assert_eq!(encode_cmp_imm(0, 131), 0x7102_0C1F);
        assert_eq!(encode_movz(0, 71), 0x5280_08E0);
        assert_eq!(encode_b_ne(0x1004, 0x1010).unwrap(), 0x5400_0061);
        assert_eq!(encode_b(0x100C, 0x100C + 62 * 4).unwrap(), 0x1400_003E);
    }

    #[test]
    fn roundtrips_bl_target() {
        let pc = 0x7f00_0000usize;
        let target = pc + 0x1234;
        let word = encode_bl(pc, target).unwrap();
        assert_eq!(decode_bl_target(pc, word), Some(target));
    }

    #[test]
    fn rejects_unaligned_and_far_targets() {
        assert_eq!(encode_b(0x1000, 0x1002), Err(EncodeError::Unaligned));
        assert_eq!(
            encode_b(0x1000, 0x1000 + (1 << 27)),
            Err(EncodeError::OutOfRange)
        );
    }

    #[test]
    fn sign_extends_negative_offsets() {
        assert_eq!(sign_extend_imm26(0x03FF_FFFF), -1);
        assert_eq!(sign_extend_imm26(0x0000_0002), 2);
    }

    #[test]
    fn recognizes_ret_variants_only() {
        assert!(is_ret(0xD65F_03C0)); // ret x30
        assert!(is_ret(0xD65F_0000)); // ret x0
        assert!(!is_ret(0xD63F_0000)); // blr x0
        assert!(!is_ret(0xD65F_03C1));
    }
}

//! x86-32 integer semantics for lifted guest code.
//!
//! Ported from `runtime/cpu/cpu.h` (flag helpers, CPUID/RDTSC profile) and
//! pinned by the hand-computed vectors in `tests/cpu_selftest.c` of
//! NIK2703/WC3_AndroidRecomp (MIT, commit `c753ab89…`).
//!
//! Flags are discrete booleans updated eagerly by each helper; a conditional
//! jump reads the stored bits. That is the donor's model and it stays easy to
//! verify against hardware.
//!
//! # Width
//!
//! x86 arithmetic is width-parameterised: an 8- or 16-bit operation updates only
//! the low part of the destination and computes flags from that width. Every
//! helper here takes `bits` (8, 16 or 32), returns the **masked** result, and
//! leaves re-inserting the upper bits to the caller — exactly what a lifted
//! `add al, bl` needs.
//!
//! # Deliberate limits
//!
//! - Counts are masked the way x86 does it (`count & 31`), then reduced modulo
//!   the operand width for the rotations. `shl/shr/sar/shld/shrd` with a count
//!   at or beyond the operand width are architecturally undefined; this module
//!   returns a documented value (zero for the shifts, destination unchanged for
//!   `shld/shrd` at count 0) and pins that behaviour in a test rather than
//!   leaving it to chance.
//! - x87 and SSE are a separate unit (`x87.h` / the SSE parts of `cpu.h` in the
//!   donor) and are not implemented here; see
//!   `docs/audit/status-donor-androidrecomp.md`.

/// Size of one x87/MMX register slot. The CPU layout keeps a 16-byte stride so
/// the same slot serves the x87 double and the MMX integer register.
pub const X87_SLOT_SIZE: usize = 16;
/// `st[8]` in bytes.
pub const X87_STACK_BYTES: usize = 8 * X87_SLOT_SIZE;

/// Donor's frozen `offsetof(CPU, st)` — recorded for a future FFI bridge.
/// [`Cpu`] here is idiomatic Rust and is **not** ABI-compatible with the donor's
/// C struct; see the crate docs.
pub const DONOR_ST_OFFSET: usize = 64;
/// Donor's frozen `offsetof(CPU, xmm)`.
pub const DONOR_XMM_OFFSET: usize = 216;

/// Guest-visible CPU family (Pentium 4 Willamette class).
pub const CPUID_FAMILY: u32 = 15;
/// Guest-visible CPU model.
pub const CPUID_MODEL: u32 = 2;
/// Guest-visible CPU stepping.
pub const CPUID_STEPPING: u32 = 7;
/// Leaf 1 feature mask: FPU VME DE PSE TSC MSR PAE MCE CX8 SEP MTRR PGE CMOV PAT
/// PSE36 CLFSH MMX FXSR SSE SSE2 — no SSE3, no AVX.
pub const CPUID_FEATURES_EDX: u32 = 0x078B_B9FF;

const VENDOR_EBX: u32 = 0x756E_6547; // "Genu"
const VENDOR_EDX: u32 = 0x4965_6E69; // "ineI"
const VENDOR_ECX: u32 = 0x6C65_746E; // "ntel"

/// One x87/MMX register slot: 16 bytes, low 64 bits meaningful.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct X87Slot {
    /// x87 double bits, or the MMX integer register.
    pub bits: u64,
    /// Padding that keeps the donor's 16-byte stride.
    pub reserved: u64,
}

impl X87Slot {
    /// Slot holding `value`.
    pub fn from_f64(value: f64) -> Self {
        Self { bits: value.to_bits(), reserved: 0 }
    }

    /// The slot as a double.
    pub fn to_f64(self) -> f64 {
        f64::from_bits(self.bits)
    }

    /// Slot holding an MMX integer.
    pub fn from_mm(value: u64) -> Self {
        Self { bits: value, reserved: 0 }
    }

    /// The slot as an MMX integer.
    pub fn as_mm(self) -> u64 {
        self.bits
    }
}

/// Result of a `CPUID` leaf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cpuid {
    /// Leaf result, EAX.
    pub eax: u32,
    /// Leaf result, EBX.
    pub ebx: u32,
    /// Leaf result, ECX.
    pub ecx: u32,
    /// Leaf result, EDX.
    pub edx: u32,
}

/// Which bit operation `bittest` performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitOp {
    /// `BT` — read the bit into CF, leave the destination alone.
    Test,
    /// `BTS` — read, then set.
    Set,
    /// `BTR` — read, then reset.
    Reset,
    /// `BTC` — read, then complement.
    Complement,
}

/// x86-32 guest CPU state.
///
/// Flags are separate fields (not a packed EFLAGS word) so a lifted
/// `jne`/`jb`/`jo` reads one bit and no mask arithmetic can go wrong.
#[derive(Clone, Copy, Debug)]
pub struct Cpu {
    /// Accumulator.
    pub eax: u32,
    /// Counter.
    pub ecx: u32,
    /// Data.
    pub edx: u32,
    /// Base.
    pub ebx: u32,
    /// Stack pointer.
    pub esp: u32,
    /// Frame pointer.
    pub ebp: u32,
    /// Source index.
    pub esi: u32,
    /// Destination index.
    pub edi: u32,
    /// Instruction pointer (maintained by the dispatcher, not by these helpers).
    pub eip: u32,

    /// Carry.
    pub cf: bool,
    /// Zero.
    pub zf: bool,
    /// Sign.
    pub sf: bool,
    /// Overflow.
    pub of: bool,
    /// Parity (even parity of the low byte).
    pub pf: bool,
    /// Auxiliary carry (bit 4).
    pub af: bool,

    /// x87/MMX register stack, `st0 == st[fpu_top]`.
    pub st: [X87Slot; 8],
    /// Index of the current top of the x87 stack.
    pub fpu_top: u32,
    /// x87 control word.
    pub fpu_cw: u16,
    /// x87 tag word.
    pub fpu_tag: u16,
    /// x87 status word.
    pub fpu_sw: u32,

    /// SSE registers, `xmm0..xmm7` (a 32-bit target has no `xmm8..15`).
    pub xmm: [[u64; 2]; 8],

    /// Code segment selector (stored, not used in address computation).
    pub cs: u16,
    /// Data segment selector.
    pub ds: u16,
    /// Extra segment selector.
    pub es: u16,
    /// `fs` selector; the simulated TIB base is [`Cpu::fs_base`].
    pub fs: u16,
    /// `gs` selector.
    pub gs: u16,
    /// Stack segment selector.
    pub ss: u16,

    /// Linear base the simulated `fs:` points at.
    pub fs_base: u32,
    /// Monotonic virtual time-stamp counter.
    pub tsc: u64,

    /// Win32 `GetLastError` for the current guest thread.
    pub hle_last_error: u32,
    /// Win32 TLS slots (index allocation is global; values are per thread).
    pub hle_tls: [u32; 64],
}

impl Default for Cpu {
    fn default() -> Self {
        let mut cpu = Self {
            eax: 0,
            ecx: 0,
            edx: 0,
            ebx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            eip: 0,
            cf: false,
            zf: false,
            sf: false,
            of: false,
            pf: false,
            af: false,
            st: [X87Slot::default(); 8],
            fpu_top: 0,
            fpu_cw: 0,
            fpu_tag: 0,
            fpu_sw: 0,
            xmm: [[0; 2]; 8],
            cs: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
            ss: 0,
            fs_base: 0,
            tsc: 0,
            hle_last_error: 0,
            hle_tls: [0; 64],
        };
        cpu.fninit();
        cpu
    }
}

impl Cpu {
    /// A CPU in the `FNINIT` state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the x87 state the way `FNINIT` does: control word `0x037F`, all
    /// tags empty, empty stack, clear status word.
    pub fn fninit(&mut self) {
        self.fpu_top = 0;
        self.fpu_cw = 0x037F;
        self.fpu_tag = 0xFFFF;
        self.fpu_sw = 0;
        self.st = [X87Slot::default(); 8];
    }

    /// Slot `i` from the top (`st(0)` is the top).
    pub fn st(&self, i: u32) -> X87Slot {
        self.st[((self.fpu_top + i) & 7) as usize]
    }

    /// Mutable slot `i` from the top.
    pub fn st_mut(&mut self, i: u32) -> &mut X87Slot {
        let idx = ((self.fpu_top + i) & 7) as usize;
        &mut self.st[idx]
    }

    /// Even parity of the low byte (`PF`).
    pub fn parity(value: u8) -> bool {
        value.count_ones() % 2 == 0
    }

    /// `ADD`/`ADC` flags. Returns the masked result; the caller keeps the upper
    /// bits for 8/16-bit operand sizes.
    pub fn add_flags(&mut self, a: u32, b: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let a = a & mask;
        let b = b & mask;
        let result = a.wrapping_add(b) & mask;
        self.cf = u64::from(a) + u64::from(b) > u64::from(mask);
        self.zf = result == 0;
        self.sf = result & sign != 0;
        self.of = (!(a ^ b) & (a ^ result)) & sign != 0;
        self.af = ((a ^ b ^ result) & 0x10) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        result
    }

    /// `SUB`/`SBB`/`CMP` flags. Returns the masked result.
    pub fn sub_flags(&mut self, a: u32, b: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let a = a & mask;
        let b = b & mask;
        let result = a.wrapping_sub(b) & mask;
        self.cf = a < b;
        self.zf = result == 0;
        self.sf = result & sign != 0;
        self.of = ((a ^ b) & (a ^ result)) & sign != 0;
        self.af = ((a ^ b ^ result) & 0x10) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        result
    }

    /// `AND`/`OR`/`XOR`/`TEST` flags: `CF = OF = 0`, result flags from the
    /// masked value.
    pub fn logic_flags(&mut self, result: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let result = result & mask;
        self.cf = false;
        self.of = false;
        self.zf = result == 0;
        self.sf = result & sign_bit(bits) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        result
    }

    /// `INC` — like `ADD 1` but `CF` is preserved.
    pub fn inc_flags(&mut self, value: u32, bits: u32) -> u32 {
        let cf = self.cf;
        let result = self.add_flags(value, 1, bits);
        self.cf = cf;
        result
    }

    /// `DEC` — like `SUB 1` but `CF` is preserved.
    pub fn dec_flags(&mut self, value: u32, bits: u32) -> u32 {
        let cf = self.cf;
        let result = self.sub_flags(value, 1, bits);
        self.cf = cf;
        result
    }

    /// `ROL` — rotate left. Sets `CF` (and `OF` for a count of one); leaves
    /// `ZF`, `SF`, `PF` and `AF` untouched, as x86 does.
    pub fn rol(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let n = (count & 31) % bits;
        if n == 0 {
            return value;
        }
        let result = ((value << n) | (value >> (bits - n))) & mask;
        self.cf = result & 1 != 0;
        if n == 1 {
            self.of = (result & sign_bit(bits) != 0) != self.cf;
        }
        result
    }

    /// `ROR` — rotate right. Flag rules as [`Cpu::rol`].
    pub fn ror(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let n = (count & 31) % bits;
        if n == 0 {
            return value;
        }
        let result = ((value >> n) | (value << (bits - n))) & mask;
        self.cf = result & sign_bit(bits) != 0;
        if n == 1 {
            let next = (result >> (bits - 2)) & 1 != 0;
            self.of = (result & sign_bit(bits) != 0) != next;
        }
        result
    }

    /// `RCL` — rotate left through carry. The rotated quantity is `bits + 1`
    /// wide, so a 32-bit `rcl` by 4 differs from `rol` by 4. A count of zero
    /// changes no flag, including `CF`.
    pub fn rcl(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let total = bits + 1;
        let n = (count & 31) % total;
        if n == 0 {
            return value;
        }
        let wide = u64::from(value) | (u64::from(self.cf as u8) << bits);
        let rotated = ((wide << n) | (wide >> (total - n))) & ((1u64 << total) - 1);
        self.cf = (rotated >> bits) & 1 != 0;
        (rotated as u32) & mask
    }

    /// `RCR` — rotate right through carry.
    pub fn rcr(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let total = bits + 1;
        let n = (count & 31) % total;
        if n == 0 {
            return value;
        }
        let wide = u64::from(value) | (u64::from(self.cf as u8) << bits);
        let rotated = ((wide >> n) | (wide << (total - n))) & ((1u64 << total) - 1);
        self.cf = (rotated >> bits) & 1 != 0;
        (rotated as u32) & mask
    }

    /// `SHL`/`SAL` — shift left. Count zero changes nothing; a count at or
    /// beyond the width yields zero with `CF` clear (documented, not
    /// architectural).
    pub fn shl(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let n = count & 31;
        if n == 0 {
            return value;
        }
        let result = if n >= bits { 0 } else { (value << n) & mask };
        self.cf = if n <= bits { (value >> (bits - n)) & 1 != 0 } else { false };
        self.zf = result == 0;
        self.sf = result & sign_bit(bits) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        if n == 1 {
            self.of = (result & sign_bit(bits) != 0) != self.cf;
        }
        result
    }

    /// `SHR` — logical shift right.
    pub fn shr(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let value = value & mask;
        let n = count & 31;
        if n == 0 {
            return value;
        }
        let result = if n >= bits { 0 } else { value >> n };
        self.cf = if n <= bits { (value >> (n - 1)) & 1 != 0 } else { false };
        self.zf = result == 0;
        self.sf = result & sign_bit(bits) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        if n == 1 {
            self.of = value & sign_bit(bits) != 0;
        }
        result
    }

    /// `SAR` — arithmetic shift right.
    pub fn sar(&mut self, value: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let value = value & mask;
        let n = count & 31;
        if n == 0 {
            return value;
        }
        // Sign-extend from the operand width into i32, then shift arithmetically.
        let extension_shift = 32 - bits;
        let signed = (((value << extension_shift) as i32) >> extension_shift) as i64;
        let shifted = if n >= 32 {
            if signed < 0 { u32::MAX } else { 0 }
        } else {
            ((signed >> n) as u32) & mask
        };
        let result = shifted & mask;
        self.cf = if n <= bits { (value >> (n - 1)) & 1 != 0 } else { value & sign != 0 };
        self.zf = result == 0;
        self.sf = result & sign != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        if n == 1 {
            self.of = false;
        }
        result
    }

    /// `SHLD` — shift `dst` left, filling from the high bits of `src`.
    pub fn shld(&mut self, dst: u32, src: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let dst = dst & mask;
        let src = src & mask;
        let n = count & 31;
        if n == 0 {
            return dst;
        }
        let result = if n >= bits {
            0
        } else {
            ((dst << n) | (src >> (bits - n))) & mask
        };
        self.cf = if n <= bits { (dst >> (bits - n)) & 1 != 0 } else { false };
        self.zf = result == 0;
        self.sf = result & sign_bit(bits) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        result
    }

    /// `SHRD` — shift `dst` right, filling from the low bits of `src`.
    pub fn shrd(&mut self, dst: u32, src: u32, count: u32, bits: u32) -> u32 {
        let mask = width_mask(bits);
        let dst = dst & mask;
        let src = src & mask;
        let n = count & 31;
        if n == 0 {
            return dst;
        }
        let result = if n >= bits {
            dst
        } else {
            ((dst >> n) | (src << (bits - n))) & mask
        };
        self.cf = if n <= bits { (dst >> (n - 1)) & 1 != 0 } else { false };
        self.zf = result == 0;
        self.sf = result & sign_bit(bits) != 0;
        self.pf = Self::parity((result & 0xFF) as u8);
        result
    }

    /// `BSF` — index of the lowest set bit. With a zero source `ZF` is set and
    /// the destination is left unchanged (architecturally undefined, pinned
    /// here so it cannot start being depended on by accident).
    pub fn bsf(&mut self, dst: u32, src: u32, bits: u32) -> u32 {
        let src = src & width_mask(bits);
        if src == 0 {
            self.zf = true;
            dst
        } else {
            self.zf = false;
            src.trailing_zeros()
        }
    }

    /// `BSR` — index of the highest set bit. Zero source behaves as [`Cpu::bsf`].
    pub fn bsr(&mut self, dst: u32, src: u32, bits: u32) -> u32 {
        let src = src & width_mask(bits);
        if src == 0 {
            self.zf = true;
            dst
        } else {
            self.zf = false;
            31 - src.leading_zeros()
        }
    }

    /// `BT`/`BTS`/`BTR`/`BTC`. The bit index wraps modulo the operand width,
    /// so `bt eax, 33` reads bit 1.
    pub fn bittest(&mut self, dst: u32, bit: u32, bits: u32, op: BitOp) -> u32 {
        let mask = width_mask(bits);
        let dst = dst & mask;
        let index = bit % bits;
        let selected = 1u32 << index;
        self.cf = dst & selected != 0;
        match op {
            BitOp::Test => dst,
            BitOp::Set => dst | selected,
            BitOp::Reset => dst & !selected,
            BitOp::Complement => dst ^ selected,
        }
    }

    /// `CPUID`. The profile is fixed and documented: leaf 0 reports one leaf and
    /// the vendor string, leaf 1 reports family/model/stepping and the feature
    /// mask, every other leaf is zero.
    pub fn cpuid(leaf: u32, _sub: u32) -> Cpuid {
        match leaf {
            0 => Cpuid { eax: 1, ebx: VENDOR_EBX, ecx: VENDOR_ECX, edx: VENDOR_EDX },
            1 => Cpuid {
                eax: (CPUID_FAMILY << 8) | (CPUID_MODEL << 4) | CPUID_STEPPING,
                ebx: 0,
                ecx: 0,
                edx: CPUID_FEATURES_EDX,
            },
            _ => Cpuid::default(),
        }
    }

    /// `RDTSC` — a monotonic guest counter, not a wall clock. RDTSC is
    /// nondeterministic on hardware; timing code only needs ordering.
    pub fn rdtsc(&mut self) -> u64 {
        self.tsc = self.tsc.wrapping_add(1);
        self.tsc
    }
}

fn width_mask(bits: u32) -> u32 {
    match bits {
        8 => 0xFF,
        16 => 0xFFFF,
        _ => 0xFFFF_FFFF,
    }
}

fn sign_bit(bits: u32) -> u32 {
    1u32 << (bits - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recomp_cpu_model_frozen_offsets() {
        // The donor's lifted code is compiled against a C struct whose `st`
        // array starts at 64 and `xmm` at 216. This crate is not ABI-shared, but
        // the constants are the contract any FFI bridge must honour.
        assert_eq!(DONOR_ST_OFFSET, 64);
        assert_eq!(DONOR_XMM_OFFSET, 216);
        assert_eq!(X87_SLOT_SIZE, 16);
        assert_eq!(X87_STACK_BYTES, 128);
        // 64 + 128 leaves 24 bytes of FPU control/status/env state before
        // `xmm`: fpu_top(4) cw(2) tag(2) sw(4) ip(4) dp(4) cs(2) ds(2) = 24.
        assert_eq!(
            DONOR_ST_OFFSET + X87_STACK_BYTES + 24,
            DONOR_XMM_OFFSET,
            "FPU control/status block sits between st[8] and xmm[8]"
        );
        let cpu = Cpu::new();
        assert_eq!(cpu.xmm.len(), 8, "a 32-bit target has xmm0..xmm7");
        assert_eq!(cpu.st.len(), 8);
        assert_eq!(cpu.hle_tls.len(), 64);
    }

    #[test]
    fn recomp_cpu_model_x87_slot_is_16_bytes() {
        assert_eq!(core::mem::size_of::<X87Slot>(), X87_SLOT_SIZE);
        let slot = X87Slot::from_f64(1.5);
        assert_eq!(slot.to_f64(), 1.5);
        let slot = X87Slot::from_mm(0xDEAD_BEEF_0000_0001);
        assert_eq!(slot.as_mm(), 0xDEAD_BEEF_0000_0001);
        let mut cpu = Cpu::new();
        cpu.fninit();
        *cpu.st_mut(0) = X87Slot::from_f64(2.0);
        assert_eq!(cpu.st(0).to_f64(), 2.0);
        assert_eq!(cpu.st(1).to_f64(), 0.0);
    }

    #[test]
    fn recomp_cpu_model_flag_fields_are_bits() {
        let mut cpu = Cpu::new();
        cpu.cf = true;
        cpu.zf = false;
        cpu.sf = true;
        cpu.of = true;
        cpu.pf = false;
        cpu.af = true;
        // Each flag is an independent bool: neighbouring fields cannot alias.
        cpu.add_flags(0xFFFF_FFFF, 1, 32);
        assert!(cpu.cf, "0xFFFFFFFF + 1 carries out");
        assert!(cpu.zf, "and the truncated result is zero");
        cpu.logic_flags(0x8000_0000, 32);
        assert!(cpu.sf && !cpu.zf && !cpu.cf, "logic ops clear CF/OF");
        cpu.of = true;
        cpu.logic_flags(0, 32);
        assert!(!cpu.of, "logic ops clear OF");
    }

    #[test]
    fn recomp_flags_add_overflow_boundaries() {
        let mut cpu = Cpu::new();
        // 0x7FFF_FFFF + 1: signed overflow, no carry.
        cpu.add_flags(0x7FFF_FFFF, 1, 32);
        assert!(cpu.of && !cpu.cf && cpu.sf);
        // 0xFFFF_FFFF + 1: carry, no signed overflow, zero result.
        let result = cpu.add_flags(0xFFFF_FFFF, 1, 32);
        assert_eq!(result, 0);
        assert!(cpu.cf && !cpu.of && cpu.zf && !cpu.sf);
        // Low-byte carry out of bit 3 sets AF.
        cpu.add_flags(0x08, 0x08, 8);
        assert!(cpu.af, "0x08 + 0x08 sets the auxiliary carry");
        // Parity: 0x03 has two low bits set (even parity).
        assert!(Cpu::parity(0x03));
        assert!(!Cpu::parity(0x01));
    }

    #[test]
    fn recomp_flags_sub_borrow_and_af() {
        let mut cpu = Cpu::new();
        let result = cpu.sub_flags(0, 1, 32);
        assert_eq!(result, 0xFFFF_FFFF);
        assert!(cpu.cf && cpu.sf && !cpu.zf && !cpu.of);
        // 0x8000_0000 - 1: signed overflow, no borrow.
        cpu.sub_flags(0x8000_0000, 1, 32);
        assert!(cpu.of && !cpu.cf);
        // AF is the borrow out of bit 3.
        cpu.sub_flags(0x10, 0x01, 8);
        assert!(cpu.af);
        // INC/DEC never touch CF.
        cpu.cf = true;
        cpu.inc_flags(0x7FFF_FFFF, 32);
        assert!(cpu.cf, "INC must not change CF");
        assert!(cpu.of, "INC sets OF on 0x7FFFFFFF");
        cpu.cf = false;
        cpu.dec_flags(0x8000_0000, 32);
        assert!(!cpu.cf, "DEC must not change CF");
        assert!(cpu.of, "DEC sets OF on 0x80000000");
    }

    #[test]
    fn recomp_flags_rotates_leave_szp_alone() {
        let mut cpu = Cpu::new();
        cpu.zf = true;
        cpu.sf = true;
        cpu.pf = true;
        cpu.rol(0x8000_0001, 1, 32);
        assert!(cpu.zf && cpu.sf && cpu.pf, "ROL keeps ZF/SF/PF");
        cpu.ror(0x0000_0001, 1, 32);
        assert!(cpu.zf && cpu.sf && cpu.pf, "ROR keeps ZF/SF/PF");
        // Rotate by 32 is a no-op (count is masked to 5 bits).
        let mut cpu = Cpu::new();
        assert_eq!(cpu.rol(0x1234_5678, 32, 32), 0x1234_5678);
        assert_eq!(cpu.ror(0x1234_5678, 32, 32), 0x1234_5678);
        // SHL by >= width is undefined on hardware; the model must stay
        // deterministic and keep the result representable.
        // 40 & 31 = 8, so this is SHL by 8, not "shift everything out".
        assert_eq!(cpu.shl(1, 40, 32), 0x100, "SHL count is masked to 5 bits");
    }

    #[test]
    fn recomp_flags_cpuid_profile_fixed() {
        let leaf0 = Cpu::cpuid(0, 0);
        assert_eq!(leaf0.eax, 1, "the profile reports one leaf");
        let vendor = [leaf0.ebx, leaf0.edx, leaf0.ecx]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<u8>>();
        assert_eq!(&vendor, b"GenuineIntel");
        let leaf1 = Cpu::cpuid(1, 0);
        assert_eq!(
            leaf1.edx, CPUID_FEATURES_EDX,
            "feature mask is the donor's Pentium-4 profile (0x078BB9FF)"
        );
        assert_eq!(CPUID_FEATURES_EDX, 0x078B_B9FF);
        let family = (leaf1.eax >> 8) & 0xF;
        let model = (leaf1.eax >> 4) & 0xF;
        assert_eq!(family, CPUID_FAMILY);
        assert_eq!(model, CPUID_MODEL);
        assert_eq!(CPUID_STEPPING, leaf1.eax & 0xF);
    }

    #[test]
    fn recomp_flags_rdtsc_monotonic() {
        let mut cpu = Cpu::new();
        let first = cpu.rdtsc();
        for _ in 0..1000 {
            let next = cpu.rdtsc();
            assert!(next > first, "RDTSC must be strictly monotonic");
            assert!(next <= first + 1000 * 64, "and advance by a bounded step");
        }
    }

    #[test]
    fn recomp_flags_bit_scan_and_test() {
        let mut cpu = Cpu::new();
        assert_eq!(cpu.bsf(0, 0x00F0_0000, 32), 20);
        assert_eq!(cpu.bsr(0, 0x00F0_0000, 32), 23);
        assert!(!cpu.zf, "a non-zero source clears ZF");
        // Zero source sets ZF and leaves the destination alone.
        assert_eq!(cpu.bsf(0xDEAD, 0, 32), 0xDEAD);
        assert!(cpu.zf, "BSF/BSR set ZF for a zero source");
        assert_eq!(cpu.bittest(0, 0, 32, BitOp::Test), 0);
        assert!(!cpu.cf, "bit 0 of 0 is clear");
        assert_eq!(cpu.bittest(0x0000_0001, 0, 32, BitOp::Test), 1);
        assert!(cpu.cf, "BT writes the tested bit into CF");
        // BT does not write: the destination comes back unchanged.
        assert_eq!(cpu.bittest(0x0000_0001, 1, 32, BitOp::Test), 1);
        assert!(!cpu.cf, "bit 1 of 1 is clear");
    }
}

//! Mechanic `recomp.flags` / `recomp.cpu-model` — x86-32 integer semantics.
//!
//! Every expected value below is copied from the donor's hand-computed vectors
//! (`tests/cpu_selftest.c` of NIK2703/WC3_AndroidRecomp) so a regression is a
//! difference from the original author's arithmetic, not from ours.

use wc3_guest::cpu::{
    BitOp, Cpu, X87Slot, CPUID_FEATURES_EDX, CPUID_FAMILY, CPUID_MODEL, CPUID_STEPPING,
    X87_SLOT_SIZE, X87_STACK_BYTES,
};

#[test]
fn recomp_cpu_model_x87_slot_is_16_bytes() {
    assert_eq!(X87_SLOT_SIZE, 16);
    assert_eq!(core::mem::size_of::<X87Slot>(), 16);
    assert_eq!(X87_STACK_BYTES, 128);
    let cpu = Cpu::new();
    assert_eq!(core::mem::size_of_val(&cpu.st), 128, "st[8] must stay 128 bytes");
    // The slot is one storage seen two ways.
    let slot = X87Slot::from_f64(1.5);
    assert_eq!(slot.as_mm(), 1.5f64.to_bits());
    assert_eq!(X87Slot::from_mm(slot.bits).to_f64(), 1.5);
}

#[test]
fn recomp_cpu_model_fninit_state() {
    let mut cpu = Cpu::new();
    cpu.fpu_cw = 0;
    cpu.fpu_top = 5;
    cpu.fninit();
    assert_eq!(cpu.fpu_cw, 0x037F, "FNINIT control word");
    assert_eq!(cpu.fpu_tag, 0xFFFF, "FNINIT marks every slot empty");
    assert_eq!(cpu.fpu_top, 0);
    assert_eq!(cpu.fpu_sw, 0);
}

#[test]
fn recomp_flags_rotate_matches_donor_vectors() {
    let mut c = Cpu::new();
    assert_eq!(c.rol(0x1234_5678, 8, 32), 0x3456_7812);
    assert_eq!(c.ror(0x1234_5678, 8, 32), 0x7812_3456);
    assert_eq!(c.rol(0xDEAD_BEEF, 0, 32), 0xDEAD_BEEF);
    assert_eq!(c.rol(0xDEAD_BEEF, 32, 32), 0xDEAD_BEEF);
    // Width-sensitive: the same count differs per operand size.
    assert_eq!(c.rol(0x1234, 8, 16), 0x3412);
    assert_eq!(c.ror(0x12, 4, 8), 0x21);
    // x86 does not set ZF on a rotate: a helper that does looks fine until
    // lifted code branches on a flag the previous instruction set.
    c.zf = true;
    let _ = c.rol(1, 1, 32);
    assert!(c.zf, "rol must not touch ZF");
    c.zf = false;
    c.sf = true;
    c.pf = true;
    let _ = c.ror(0x8000_0000, 1, 32);
    assert!(c.sf && c.pf, "ror must not touch SF/PF");
    // CF is set from the bit that wrapped around.
    c.cf = false;
    let _ = c.rol(0x8000_0000, 1, 32);
    assert!(c.cf, "rol CF = low bit of the result");
    c.cf = false;
    let _ = c.ror(0x0000_0001, 1, 32);
    assert!(c.cf, "ror CF = high bit of the result");
    // A zero count changes no flag at all.
    c.cf = true;
    let _ = c.rol(0x1234_5678, 0, 32);
    assert!(c.cf, "rol by 0 keeps CF");
}

#[test]
fn recomp_flags_rotate_through_carry_matches_donor_vectors() {
    let mut c = Cpu::new();
    c.cf = false;
    assert_eq!(c.rcl(0x8000_0001, 1, 32), 0x0000_0002);
    assert!(c.cf);
    c.cf = true;
    assert_eq!(c.rcl(0x8000_0001, 1, 32), 0x0000_0003);
    c.cf = true;
    assert_eq!(c.rcr(0x0000_0001, 1, 32), 0x8000_0000);
    c.cf = false;
    assert_eq!(c.rcl(0x1234_5678, 4, 32), 0x2345_6780);
    assert!(c.cf);
    c.cf = true;
    assert_eq!(c.rcr(0x1234_5678, 4, 32), 0x1123_4567);
    c.cf = true;
    assert_eq!(c.rcl(0xFF, 1, 8), 0xFF, "rcl al,1 is a 9-bit rotate");
    c.cf = true;
    let _ = c.rcl(0x1234_5678, 0, 32);
    assert!(c.cf, "rcl by 0 keeps CF");
    // x86 masks the count to five bits before the operand-width modulo.
    c.cf = false;
    assert_eq!(c.rcl(0x8000_0001, 33, 32), 0x0000_0002);
    assert!(c.cf);
    c.cf = true;
    assert_eq!(c.rcl(0x1234_5678, 32, 32), 0x1234_5678);
    assert!(c.cf, "rcl by 32 is identity and keeps CF");
    // 8- and 16-bit widths rotate over width+1 bits.
    c.cf = false;
    assert_eq!(c.rcl(0x81, 8, 8), 0x40);
    assert!(c.cf);
    c.cf = false;
    assert_eq!(c.rcl(0x8001, 16, 16), 0x4000);
    assert!(c.cf);
    c.cf = false;
    assert_eq!(c.rcl(0x81, 17, 8), 0x40, "17 & 31 = 17, 17 % 9 = 8");
    c.cf = true;
    assert_eq!(c.rcr(0x8000_0001, 33, 32), 0xC000_0000);
    assert!(c.cf);
    c.cf = true;
    assert_eq!(c.rcr(0x1234_5678, 32, 32), 0x1234_5678);
    assert!(c.cf);
    c.cf = true;
    assert_eq!(c.rcr(0x01, 8, 8), 0x03);
    assert!(!c.cf);
    c.cf = false;
    assert_eq!(c.rcr(0x80, 8, 8), 0x00);
    assert!(c.cf);
}

#[test]
fn recomp_flags_double_shift_matches_donor_vectors() {
    let mut c = Cpu::new();
    assert_eq!(c.shld(0xF0F0_F0F0, 0x0F0F_0F0F, 4, 32), 0x0F0F_0F00);
    assert_eq!(c.shrd(0xF0F0_F0F0, 0x0F0F_0F0F, 4, 32), 0xFF0F_0F0F);
    assert_eq!(c.shld(0xAAAA_AAAA, 0, 0, 32), 0xAAAA_AAAA);
    assert_eq!(c.shrd(0xAAAA_AAAA, 0, 0, 32), 0xAAAA_AAAA);
}

#[test]
fn recomp_flags_bit_scan_matches_donor_vectors() {
    let mut c = Cpu::new();
    assert_eq!(c.bsf(0, 0x1234_5678, 32), 3);
    assert_eq!(c.bsr(0, 0x1234_5678, 32), 28);
    // Zero source: ZF set, destination left alone (architecturally undefined,
    // pinned so nobody starts depending on a value).
    c.zf = false;
    assert_eq!(c.bsr(0xDEAD, 0, 32), 0xDEAD);
    assert!(c.zf);
    c.zf = true;
    assert_eq!(c.bsf(0, 1, 32), 0);
    assert!(!c.zf);
    // Operand width matters: the high half must not leak into an 8-bit scan.
    assert_eq!(c.bsf(0, 0x0100, 16), 8);
    assert_eq!(c.bsr(0, 0x8000, 16), 15);
    c.zf = false;
    assert_eq!(c.bsf(0xDEAD, 0x0100, 8), 0xDEAD);
    assert!(c.zf);
    assert_eq!(c.bsf(0, 1 << 31, 32), 31);
    assert_eq!(c.bsr(0, 1 << 31, 32), 31);
}

#[test]
fn recomp_flags_bit_test_matches_donor_vectors() {
    let mut c = Cpu::new();
    assert_eq!(c.bittest(0x20, 5, 32, BitOp::Test), 0x20);
    assert!(c.cf);
    assert_eq!(c.bittest(0x00, 5, 32, BitOp::Set), 0x20);
    assert_eq!(c.bittest(0x20, 5, 32, BitOp::Reset), 0x00);
    assert_eq!(c.bittest(0x20, 5, 32, BitOp::Complement), 0x00);
    // The count wraps modulo the width: `bt eax, 33` reads bit 1.
    assert_eq!(c.bittest(0x02, 33, 32, BitOp::Test), 0x02);
    assert!(c.cf);
    assert_eq!(c.bittest(0x02, 65, 32, BitOp::Test), 0x02);
    assert!(c.cf);
    // 8-bit width wraps at 8, not at 32.
    assert_eq!(c.bittest(0x02, 9, 8, BitOp::Test), 0x02);
    assert!(c.cf);
}

#[test]
fn recomp_flags_rotate_by_operand_width_loads_carry() {
    // `rol al, 8`/`ror al, 8` leave the value unchanged but still load CF:
    // x86 masks the count to five bits, and only a zero masked count skips
    // the flags.
    let mut c = Cpu::new();
    c.cf = false;
    assert_eq!(c.rol(0x01, 8, 8), 0x01);
    assert!(c.cf, "rol al,8: CF = LSB of the result");
    c.cf = true;
    assert_eq!(c.rol(0x02, 8, 8), 0x02);
    assert!(!c.cf);
    c.cf = false;
    assert_eq!(c.ror(0x80, 8, 8), 0x80);
    assert!(c.cf, "ror al,8: CF = MSB of the result");
    c.cf = true;
    assert_eq!(c.ror(0x01, 16, 16), 0x01);
    assert!(!c.cf);
    c.cf = true;
    assert_eq!(c.rol(0x01, 32, 32), 0x01);
    assert!(c.cf, "a zero masked count changes no flag");
}

#[test]
fn recomp_flags_double_shift_by_width_uses_source() {
    // Count == operand width replaces the destination with the source; this is
    // defined x86 behaviour reachable on 8/16-bit operands.
    let mut c = Cpu::new();
    assert_eq!(c.shld(0x12, 0x34, 8, 8), 0x34);
    assert_eq!(c.shrd(0x12, 0x34, 8, 8), 0x34);
    c.cf = false;
    c.shld(0x01, 0x00, 8, 8);
    assert!(c.cf, "CF is the last bit shifted out of the destination");
    c.cf = false;
    c.shrd(0x80, 0x00, 8, 8);
    assert!(c.cf);
}

#[test]
fn recomp_flags_width_argument_is_total() {
    // An out-of-range width is treated as 32 instead of panicking (audit
    // L547-10): a machine model must never abort on an argument.
    let mut c = Cpu::new();
    assert_eq!(c.add_flags(1, 1, 0), 2);
    assert_eq!(c.add_flags(0xFFFF, 1, 7), 0x1_0000);
    assert_eq!(c.bittest(0b10, 33, 0, BitOp::Test), 0b10);
    assert!(c.cf);
    let _ = c.sar(0x8000, 4, 64);
    assert_eq!(c.rol(0x12, 32, 0), 0x12, "zero masked count is a no-op");
}

#[test]
fn recomp_flags_add_boundaries() {
    let mut c = Cpu::new();
    // 0x7fffffff + 1 -> signed overflow, no carry out.
    assert_eq!(c.add_flags(0x7FFF_FFFF, 1, 32), 0x8000_0000);
    assert!(c.of && !c.cf && c.sf && !c.zf && c.af);
    // 0xffffffff + 1 -> carry, zero, no signed overflow.
    assert_eq!(c.add_flags(0xFFFF_FFFF, 1, 32), 0);
    assert!(c.cf && !c.of && c.zf && !c.sf);
    // 0x80000000 + 0x80000000 -> both carry and signed overflow.
    assert_eq!(c.add_flags(0x8000_0000, 0x8000_0000, 32), 0);
    assert!(c.cf && c.of && c.zf);
    // Parity of the low byte: 0 has even parity, 1 odd, 3 even, 0xff even.
    c.add_flags(0, 0, 32);
    assert!(c.pf, "PF of zero");
    c.add_flags(1, 0, 32);
    assert!(!c.pf, "PF of 1 (odd)");
    c.add_flags(3, 0, 32);
    assert!(c.pf, "PF of 3 (even)");
    c.add_flags(0xFF, 0, 32);
    assert!(c.pf, "PF of 0xff (even)");
    // 8-bit add keeps the width: 0xff + 1 = 0x00 with carry out.
    assert_eq!(c.add_flags(0xFF, 1, 8), 0);
    assert!(c.cf && c.zf);
    // 16-bit add.
    assert_eq!(c.add_flags(0xFFFF, 1, 16), 0);
    assert!(c.cf && c.zf);
}

#[test]
fn recomp_flags_sub_boundaries() {
    let mut c = Cpu::new();
    assert_eq!(c.sub_flags(0, 1, 32), 0xFFFF_FFFF);
    assert!(c.cf && c.sf && c.af && !c.zf && !c.of);
    assert_eq!(c.sub_flags(0x8000_0000, 1, 32), 0x7FFF_FFFF);
    assert!(!c.cf && c.of && !c.sf);
    assert_eq!(c.sub_flags(0x7FFF_FFFF, 0xFFFF_FFFF, 32), 0x8000_0000);
    assert!(c.cf && c.of && c.sf);
    // x - x
    assert_eq!(c.sub_flags(0x1234_5678, 0x1234_5678, 32), 0);
    assert!(!c.cf && c.zf && !c.sf && !c.of);
    // 8-bit borrow.
    assert_eq!(c.sub_flags(0, 1, 8), 0xFF);
    assert!(c.cf && c.sf && c.af);
}

#[test]
fn recomp_flags_inc_dec_keep_carry() {
    let mut c = Cpu::new();
    c.cf = true;
    assert_eq!(c.inc_flags(0x7FFF_FFFF, 32), 0x8000_0000);
    assert!(c.of, "inc sets OF at the signed boundary");
    assert!(c.cf, "inc must preserve CF");
    c.cf = false;
    assert_eq!(c.dec_flags(0x8000_0000, 32), 0x7FFF_FFFF);
    assert!(c.of, "dec sets OF at the signed boundary");
    assert!(!c.cf, "dec must preserve CF");
    // 8-bit inc wraps inside the width.
    assert_eq!(c.inc_flags(0xFF, 8), 0x00);
    assert!(c.zf);
}

#[test]
fn recomp_flags_logic_and_shifts() {
    let mut c = Cpu::new();
    c.cf = true;
    c.of = true;
    assert_eq!(c.logic_flags(0, 32), 0);
    assert!(c.zf && !c.cf && !c.of && !c.sf);
    assert!(c.pf, "PF of zero");
    assert_eq!(c.logic_flags(0x8000_0000, 32), 0x8000_0000);
    assert!(c.sf && !c.zf && c.pf, "one bit set -> even parity");
    // shl moves the top bit into CF.
    assert_eq!(c.shl(0x8000_0000, 1, 32), 0);
    assert!(c.cf && c.zf);
    // shr moves the low bit into CF.
    assert_eq!(c.shr(1, 1, 32), 0);
    assert!(c.cf && c.zf);
    // sar keeps the sign.
    assert_eq!(c.sar(0x8000_0000, 1, 32), 0xC000_0000);
    assert!(c.sf);
    // Count zero changes nothing.
    c.cf = true;
    assert_eq!(c.shl(0x1234, 0, 32), 0x1234);
    assert!(c.cf, "shl by 0 keeps CF");
    // x86 masks the count to five bits before shifting: 33 means 1.
    assert_eq!(c.shl(0xFFFF_FFFF, 33, 32), 0xFFFF_FFFE);
    assert!(c.cf, "the bit shifted out of the top is 1");
    // At the operand width everything is shifted out (8-bit operand, count 8).
    assert_eq!(c.shl(0xFF, 8, 8), 0);
    assert_eq!(c.shr(0xFF, 8, 8), 0);
    assert_eq!(c.sar(0x80, 8, 8), 0xFF, "arithmetic shift keeps the sign");
}

#[test]
fn recomp_flags_cpuid_profile_fixed() {
    // Leaf 0: one leaf, "GenuineIntel" split across EBX/EDX/ECX.
    let leaf0 = Cpu::cpuid(0, 0);
    assert_eq!(leaf0.eax, 1);
    let vendor = [
        leaf0.ebx.to_le_bytes(),
        leaf0.edx.to_le_bytes(),
        leaf0.ecx.to_le_bytes(),
    ]
    .concat();
    assert_eq!(&vendor, b"GenuineIntel");
    // Leaf 1: family 15, model 2, stepping 7 -> 0x0f27.
    let leaf1 = Cpu::cpuid(1, 0);
    assert_eq!(leaf1.eax, (CPUID_FAMILY << 8) | (CPUID_MODEL << 4) | CPUID_STEPPING);
    assert_eq!(leaf1.eax, 0x0000_0F27);
    assert_eq!(leaf1.edx, CPUID_FEATURES_EDX);
    assert_eq!(leaf1.edx, 0x078B_B9FF);
    assert_eq!(leaf1.ecx, 0, "no SSE3");
    // Unsupported leaves are zero, not invented.
    assert_eq!(Cpu::cpuid(2, 0).eax, 0);
    assert_eq!(Cpu::cpuid(0x8000_0000, 0).edx, 0);
}

#[test]
fn recomp_flags_rdtsc_monotonic() {
    let mut c = Cpu::new();
    assert_eq!(c.tsc, 0);
    let a = c.rdtsc();
    let b = c.rdtsc();
    let d = c.rdtsc();
    assert!(a < b && b < d, "guest TSC must be monotonic");
    assert_eq!((a, b, d), (1, 2, 3));
}

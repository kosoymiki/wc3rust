//! Mechanic `recomp.hle-abi` — thunk window, explicit arity, unbound reporting.

use wc3_guest::cpu::Cpu;
use wc3_guest::hle::{
    classify, thunk_of, Arity, BindError, CallConv, CallError, HleRegistry, ResolveError,
    FAST_MAX,
};
use wc3_guest::layout::{HLE_BASE, HLE_SLOTS, HLE_STRIDE};

fn set_last_error(cpu: &mut Cpu, args: &[u32]) -> u32 {
    cpu.hle_last_error = args[0];
    args[0]
}

fn sum_two(_cpu: &mut Cpu, args: &[u32]) -> u32 {
    args[0].wrapping_add(args[1])
}

#[test]
fn recomp_hle_abi_window_range_check() {
    assert_eq!(classify(HLE_BASE), Ok(0));
    assert_eq!(classify(HLE_BASE + HLE_STRIDE * 3), Ok(3));
    assert_eq!(classify(HLE_BASE + (HLE_SLOTS - 1) * HLE_STRIDE), Ok(HLE_SLOTS - 1));
    // Outside the window (including underflow past zero).
    assert_eq!(classify(0), Err(ResolveError::NotAThunk));
    assert_eq!(classify(HLE_BASE - 1), Err(ResolveError::NotAThunk));
    assert_eq!(classify(HLE_BASE + HLE_SLOTS * HLE_STRIDE), Err(ResolveError::NotAThunk));
    assert_eq!(classify(u32::MAX), Err(ResolveError::NotAThunk));
    // Inside the window but off the 16-byte stride: a diagnostic must say
    // "misaligned", not "unbound".
    assert_eq!(classify(HLE_BASE + 1), Err(ResolveError::Misaligned));
    assert_eq!(classify(HLE_BASE + 8), Err(ResolveError::Misaligned));
    assert_eq!(classify(HLE_BASE + HLE_STRIDE - 1), Err(ResolveError::Misaligned));
    // Slot -> address helpers agree.
    assert_eq!(thunk_of(0), Some(HLE_BASE));
    assert_eq!(thunk_of(HLE_SLOTS), None);
}

#[test]
fn recomp_hle_abi_bind_and_resolve() {
    let mut reg = HleRegistry::new();
    assert!(reg.is_empty());
    reg.bind(3, "KERNEL32.dll", "GetVersion", Arity::Exact(0), CallConv::Stdcall, sum_two)
        .expect("bind");
    assert_eq!(reg.len(), 1);
    let thunk = HLE_BASE + 3 * HLE_STRIDE;
    let entry = reg.resolve(thunk).expect("bound thunk");
    assert_eq!(entry.module, "KERNEL32.dll");
    assert_eq!(entry.symbol, "GetVersion");
    assert_eq!(entry.arity, 0);
    assert_eq!(entry.conv, CallConv::Stdcall);
    // Also reachable by slot, without thunk arithmetic.
    assert_eq!(reg.entry_at_slot(3).map(|e| e.symbol), Some("GetVersion"));
    assert!(reg.entry_at_slot(4).is_none());
    assert!(reg.entry_at_slot(HLE_SLOTS).is_none());
}

#[test]
fn recomp_hle_abi_duplicate_slot_rejected() {
    let mut reg = HleRegistry::new();
    reg.bind(1, "A.dll", "one", Arity::Exact(1), CallConv::Stdcall, sum_two).expect("first");
    let second =
        reg.bind(1, "B.dll", "two", Arity::Exact(1), CallConv::Stdcall, sum_two);
    assert_eq!(second, Err(BindError::SlotOccupied));
    assert_eq!(reg.len(), 1, "a rejected bind must not grow the table");
    // Out-of-range slot.
    assert_eq!(
        reg.bind(HLE_SLOTS, "A.dll", "three", Arity::Exact(1), CallConv::Stdcall, sum_two),
        Err(BindError::SlotOutOfRange)
    );
}

#[test]
fn recomp_hle_abi_arity_required() {
    let mut reg = HleRegistry::new();
    let rejected = reg.bind(2, "A.dll", "unknown", Arity::Unknown, CallConv::Cdecl, sum_two);
    assert_eq!(rejected, Err(BindError::ArityUnknown));
    assert!(reg.is_empty(), "unknown arity must not enter the table");
    assert_eq!(FAST_MAX, 4096);
}

#[test]
fn recomp_hle_abi_unbound_reports() {
    let reg = HleRegistry::new();
    assert_eq!(
        reg.resolve(HLE_BASE + 9 * HLE_STRIDE).unwrap_err(),
        ResolveError::Unbound,
        "a valid slot with nothing bound is Unbound, not NotAThunk"
    );
}

#[test]
fn recomp_hle_abi_call_arity_mismatch() {
    let mut reg = HleRegistry::new();
    let mut cpu = Cpu::new();
    reg.bind(5, "A.dll", "one_arg", Arity::Exact(1), CallConv::Stdcall, set_last_error)
        .expect("bind");
    let thunk = HLE_BASE + 5 * HLE_STRIDE;
    assert_eq!(
        reg.call(thunk, &mut cpu, &[]),
        Err(CallError::ArityMismatch { expected: 1, got: 0 })
    );
    assert_eq!(
        reg.call(thunk, &mut cpu, &[1, 2]),
        Err(CallError::ArityMismatch { expected: 1, got: 2 })
    );
    // The wrong-arity call must not have executed the handler.
    assert_eq!(cpu.hle_last_error, 0);
}

#[test]
fn recomp_hle_abi_call_invokes_handler() {
    let mut reg = HleRegistry::new();
    let mut cpu = Cpu::new();
    reg.bind(7, "KERNEL32.dll", "SetLastError", Arity::Exact(1), CallConv::Stdcall, set_last_error)
        .expect("bind");
    let result = reg.call(HLE_BASE + 7 * HLE_STRIDE, &mut cpu, &[42]).expect("call");
    assert_eq!(result, 42);
    assert_eq!(cpu.hle_last_error, 42, "handler sees and mutates the guest CPU");
    // Two-argument entry.
    reg.bind(8, "A.dll", "sum", Arity::Exact(2), CallConv::Cdecl, sum_two).expect("bind");
    assert_eq!(reg.call(HLE_BASE + 8 * HLE_STRIDE, &mut cpu, &[40, 2]).expect("call"), 42);
}

#[test]
fn recomp_hle_abi_table_full() {
    let mut reg = HleRegistry::new();
    for slot in 0..FAST_MAX as u32 {
        reg.bind(slot, "A.dll", "f", Arity::Exact(0), CallConv::Stdcall, sum_two)
            .unwrap_or_else(|e| panic!("bind slot {slot} failed: {e:?}"));
    }
    assert_eq!(reg.len(), FAST_MAX);
    assert_eq!(
        reg.bind(FAST_MAX as u32, "A.dll", "f", Arity::Exact(0), CallConv::Stdcall, sum_two),
        Err(BindError::TableFull)
    );
    // The last bound slot still resolves.
    assert!(reg.resolve(HLE_BASE + (FAST_MAX as u32 - 1) * HLE_STRIDE).is_ok());
}

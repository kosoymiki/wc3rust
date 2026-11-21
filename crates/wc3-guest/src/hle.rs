//! HLE thunk registry.
//!
//! Ported from `runtime/hle/abi/hle_abi.h` and `hle_abi.c` of
//! NIK2703/WC3_AndroidRecomp (MIT, commit `c753ab89…`).
//!
//! A guest import is a **thunk address** inside the reserved window
//! (`0x7F00_0000 + 16·slot`, see [`crate::layout`]). The dispatcher maps a thunk
//! to a registry entry with integer arithmetic — no hash table, no lock and no
//! allocation on the call path — and every entry carries an **explicit arity and
//! calling convention**.
//!
//! The arity rule is load-bearing, not bookkeeping: a stdcall/cdecl mismatch
//! does not fail at the call site, it shows up much later as a wild pointer (the
//! donor's own note in `hle_android.c:9-12`). So an entry cannot be bound with
//! an unknown arity, and a call whose argument count disagrees is rejected
//! instead of executed.
//!
//! # Names are owned
//!
//! The donor binds static string literals; here the import names come from the
//! parsed PE image at runtime. [`HleEntry`] therefore owns its `module` and
//! `symbol` (`String`). Keeping them `&'static str` would require a
//! process-lifetime intern pool, and a forged image with many distinct names
//! would grow that pool without bound. Binding is a boot-time operation, so the
//! two allocations per entry never touch the call path.
//!
//! # Calling convention
//!
//! [`CallConv::stack_bytes`] states the ABI contract the dispatcher must apply:
//! a stdcall callee pops its own arguments, a cdecl callee leaves them to the
//! caller. The registry does not touch `esp` itself — it receives the arguments
//! as a slice — but it publishes the number so the lifted call site can.

use crate::cpu::Cpu;
use crate::layout::{HLE_BASE, HLE_SLOTS, HLE_STRIDE};

/// Maximum number of bound entries (the donor's dense-registry bound).
pub const FAST_MAX: usize = 4096;

const SENTINEL: u16 = u16::MAX;

/// Win32 calling convention of an import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallConv {
    /// Callee pops the arguments.
    Stdcall,
    /// Caller pops the arguments.
    Cdecl,
}

impl CallConv {
    /// True when the callee removes the arguments from the stack.
    pub fn pops_args(self) -> bool {
        matches!(self, CallConv::Stdcall)
    }

    /// Bytes of argument stack the callee removes after the call.
    pub fn stack_bytes(self, arity: u8) -> u32 {
        if self.pops_args() {
            u32::from(arity) * 4
        } else {
            0
        }
    }
}

/// Arity of an import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arity {
    /// Number of 32-bit argument slots.
    Exact(u8),
    /// Not yet established — cannot be bound.
    Unknown,
}

/// Why a binding was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindError {
    /// Slot index is outside the thunk window.
    SlotOutOfRange,
    /// Another entry already owns the slot.
    SlotOccupied,
    /// [`Arity::Unknown`] was supplied.
    ArityUnknown,
    /// The dense table is full.
    TableFull,
}

/// Why a thunk could not be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// Address is outside the thunk window.
    NotAThunk,
    /// Address is inside the window but not on the 16-byte stride.
    Misaligned,
    /// Address is a valid thunk slot with nothing bound.
    Unbound,
}

/// Why a call was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallError {
    /// The thunk did not resolve.
    Resolve(ResolveError),
    /// The call supplied a different number of arguments than the entry
    /// declares.
    ArityMismatch {
        /// Declared argument count.
        expected: u8,
        /// Supplied argument count.
        got: usize,
    },
}

impl From<ResolveError> for CallError {
    fn from(value: ResolveError) -> Self {
        CallError::Resolve(value)
    }
}

/// A guest→host call target.
pub type Handler = fn(&mut Cpu, &[u32]) -> u32;

/// One bound import.
#[derive(Clone, Debug)]
pub struct HleEntry {
    /// Guest DLL name (`KERNEL32.dll`, …).
    pub module: String,
    /// Import name (`GetVersion`, …).
    pub symbol: String,
    /// Declared argument count.
    pub arity: u8,
    /// Declared calling convention.
    pub conv: CallConv,
    /// Host implementation.
    pub handler: Handler,
}

/// Slot-indexed HLE registry.
///
/// Storage mirrors the donor: a dense `u16` slot→index map over the whole
/// window plus a dense entry vector, so resolution is two array reads.
pub struct HleRegistry {
    slot_to_index: Vec<u16>,
    entries: Vec<HleEntry>,
}

impl Default for HleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HleRegistry {
    /// An empty registry sized for the whole thunk window.
    pub fn new() -> Self {
        Self { slot_to_index: vec![SENTINEL; HLE_SLOTS as usize], entries: Vec::new() }
    }

    /// Bind `handler` to `slot`.
    pub fn bind(
        &mut self,
        slot: u32,
        module: impl Into<String>,
        symbol: impl Into<String>,
        arity: Arity,
        conv: CallConv,
        handler: Handler,
    ) -> Result<(), BindError> {
        if slot >= HLE_SLOTS {
            return Err(BindError::SlotOutOfRange);
        }
        let arity = match arity {
            Arity::Exact(count) => count,
            Arity::Unknown => return Err(BindError::ArityUnknown),
        };
        let index = slot as usize;
        if self.slot_to_index[index] != SENTINEL {
            return Err(BindError::SlotOccupied);
        }
        if self.entries.len() >= FAST_MAX {
            return Err(BindError::TableFull);
        }
        self.entries.push(HleEntry {
            module: module.into(),
            symbol: symbol.into(),
            arity,
            conv,
            handler,
        });
        self.slot_to_index[index] = (self.entries.len() - 1) as u16;
        Ok(())
    }

    /// Number of bound entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is bound.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry bound to `slot`, if any (bypassing the thunk arithmetic).
    pub fn entry_at_slot(&self, slot: u32) -> Option<&HleEntry> {
        if slot >= HLE_SLOTS {
            return None;
        }
        let index = self.slot_to_index[slot as usize];
        if index == SENTINEL {
            None
        } else {
            self.entries.get(index as usize)
        }
    }

    /// Resolve a thunk address.
    pub fn resolve(&self, thunk: u32) -> Result<&HleEntry, ResolveError> {
        let slot = classify(thunk)?;
        self.entry_at_slot(slot).ok_or(ResolveError::Unbound)
    }

    /// Resolve and call. The argument count must equal the entry's declared
    /// arity.
    pub fn call(&mut self, thunk: u32, cpu: &mut Cpu, args: &[u32]) -> Result<u32, CallError> {
        let (arity, handler) = {
            let entry = self.resolve(thunk)?;
            (entry.arity, entry.handler)
        };
        if args.len() != arity as usize {
            return Err(CallError::ArityMismatch { expected: arity, got: args.len() });
        }
        Ok(handler(cpu, args))
    }
}

/// Window slot for a thunk address, distinguishing "not a thunk" from
/// "misaligned" so a diagnostic can say which.
pub fn classify(thunk: u32) -> Result<u32, ResolveError> {
    let offset = thunk.checked_sub(HLE_BASE).ok_or(ResolveError::NotAThunk)?;
    if offset >= HLE_SLOTS * HLE_STRIDE {
        return Err(ResolveError::NotAThunk);
    }
    if offset % HLE_STRIDE != 0 {
        return Err(ResolveError::Misaligned);
    }
    Ok(offset / HLE_STRIDE)
}

/// Thunk address for a slot, or `None` outside the window.
pub fn thunk_of(slot: u32) -> Option<u32> {
    crate::layout::thunk_va(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;

    fn arity(count: u8) -> Arity {
        Arity::Exact(count)
    }

    fn get_version(cpu: &mut Cpu, _args: &[u32]) -> u32 {
        cpu.eax = 0x0000_000A;
        0x0000_000A
    }

    fn add_two(_cpu: &mut Cpu, args: &[u32]) -> u32 {
        args[0].wrapping_add(args[1])
    }

    #[test]
    fn recomp_hle_bind_rejects_duplicate_and_bad_slots() {
        let mut registry = HleRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(
            registry.bind(HLE_SLOTS, "KERNEL32.dll", "X", arity(0), CallConv::Stdcall, get_version),
            Err(BindError::SlotOutOfRange)
        );
        registry
            .bind(7, "KERNEL32.dll", "GetVersion", arity(0), CallConv::Stdcall, get_version)
            .expect("bind");
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.bind(7, "KERNEL32.dll", "Other", arity(0), CallConv::Stdcall, get_version),
            Err(BindError::SlotOccupied)
        );
        assert_eq!(
            registry.bind(8, "KERNEL32.dll", "Vararg", Arity::Unknown, CallConv::Stdcall, get_version),
            Err(BindError::ArityUnknown),
            "a vararg import cannot be checked at the call site"
        );
    }

    #[test]
    fn recomp_hle_bind_accepts_runtime_names() {
        // Import names come from the parsed image, not from a literal; the
        // registry owns them, so no interning and no process-lifetime pool.
        let mut registry = HleRegistry::new();
        let module = String::from("KERNEL32.dll");
        let symbol = format!("{}Version", "Get");
        registry
            .bind(3, module.clone(), symbol.clone(), arity(0), CallConv::Stdcall, get_version)
            .expect("bind");
        let entry = registry.resolve(thunk_of(3).unwrap()).expect("bound");
        assert_eq!(entry.module, module);
        assert_eq!(entry.symbol, symbol);
    }

    #[test]
    fn recomp_hle_callconv_stack_contract() {
        // stdcall: the callee pops its arguments; cdecl: the caller does.
        assert!(CallConv::Stdcall.pops_args());
        assert!(!CallConv::Cdecl.pops_args());
        assert_eq!(CallConv::Stdcall.stack_bytes(0), 0);
        assert_eq!(CallConv::Stdcall.stack_bytes(3), 12);
        assert_eq!(CallConv::Cdecl.stack_bytes(3), 0);
        assert_eq!(CallConv::Cdecl.stack_bytes(255), 0);
    }

    #[test]
    fn recomp_hle_resolve_is_thunk_arithmetic() {
        let mut registry = HleRegistry::new();
        registry
            .bind(3, "KERNEL32.dll", "GetVersion", arity(0), CallConv::Stdcall, get_version)
            .expect("bind");
        let thunk = thunk_of(3).expect("slot 3 is inside the window");
        assert_eq!(thunk, HLE_BASE + 3 * HLE_STRIDE);
        let entry = registry.resolve(thunk).expect("bound thunk");
        assert_eq!(entry.symbol, "GetVersion");
        assert_eq!(entry.arity, 0);
        assert_eq!(classify(thunk), Ok(3));
        // Misaligned and out-of-window addresses are not thunks.
        assert_eq!(classify(thunk + 4), Err(ResolveError::Misaligned));
        assert_eq!(classify(0x7F10_0000), Err(ResolveError::NotAThunk));
        assert_eq!(classify(0x1000), Err(ResolveError::NotAThunk));
        // In-window but unbound.
        assert_eq!(
            registry.resolve(thunk_of(4).unwrap()).unwrap_err(),
            ResolveError::Unbound
        );
    }

    #[test]
    fn recomp_hle_call_checks_arity_and_runs_handler() {
        let mut registry = HleRegistry::new();
        registry
            .bind(1, "guest.dll", "AddTwo", arity(2), CallConv::Stdcall, add_two)
            .expect("bind");
        registry
            .bind(2, "KERNEL32.dll", "GetVersion", arity(0), CallConv::Stdcall, get_version)
            .expect("bind");
        let mut cpu = Cpu::new();
        let add = thunk_of(1).unwrap();
        assert_eq!(registry.call(add, &mut cpu, &[2, 40]), Ok(42));
        assert_eq!(
            registry.call(add, &mut cpu, &[2]),
            Err(CallError::ArityMismatch { expected: 2, got: 1 }),
            "a short argument list is a bug, not a default"
        );
        assert_eq!(
            registry.call(add, &mut cpu, &[1, 2, 3]),
            Err(CallError::ArityMismatch { expected: 2, got: 3 })
        );
        let version = thunk_of(2).unwrap();
        assert_eq!(registry.call(version, &mut cpu, &[]), Ok(0x0A));
        assert_eq!(cpu.eax, 0x0A, "the handler sees the real CPU state");
    }
}

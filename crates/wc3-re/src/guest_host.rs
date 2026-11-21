//! Guest host: the runtime that owns a lifted PE32 image, its CPU and its HLE
//! thunks.
//!
//! This is the consumer of [`wc3_guest`]: the crate on its own is a machine
//! model, and this module is where the three parts are used together —
//! the image is mapped into the guest address plan, every import is bound to a
//! thunk slot in the HLE window, and a call from guest code becomes
//! `resolve thunk -> check arity -> run handler on the guest CPU`.
//!
//! The Rust port does not execute lifted C in-tree yet (the lifter is a later
//! donor unit); this host is the seam the lifted code will call into, and it is
//! exercised by a synthetic image in the tests so the seam is proven before the
//! lifter lands.
//!
//! # Image-base rule
//!
//! `war3.exe` is mapped at the base recorded in its own header and the layout
//! deliberately does **not** list the executable in its static arena table
//! (see [`wc3_guest::layout::WAR3_IMAGE_BASE`]). `boot` therefore accepts
//! exactly one base, [`WAR3_IMAGE_BASE`], and requires the whole
//! `[base, base + SizeOfImage)` range to stay clear of every static region.
//! Accepting any address that happens to be covered by a region would map code
//! over the HLE thunk window, a thread stack, the TEB or the heap (audit
//! `L547-02`).
//!
//! # Provenance
//!
//! Guest model ported from `NIK2703/WC3_AndroidRecomp`
//! (`runtime/guest/guest_layout.h`, `runtime/guest/guest_memory.c`,
//! `runtime/hle/abi/hle_abi.{h,c}`, `runtime/guest/pe.{c,h}`), commit
//! `c753ab89debcf729b1a815ec29a14b035c47a92f`, MIT. See `crates/wc3-guest`.

use std::sync::Arc;

use wc3_guest::cpu::Cpu;
use wc3_guest::hle::{Arity, CallConv, CallError, HleEntry, HleRegistry, ResolveError};
use wc3_guest::layout::{self, HLE_BASE, HLE_SLOTS, HLE_STRIDE, PAGE_SIZE, WAR3_IMAGE_BASE};
use wc3_guest::pe::{PeError, PeImage};

/// Why a guest image could not be booted.
#[derive(Debug)]
pub enum BootError {
    /// The PE32 parser rejected the bytes.
    Image(PeError),
    /// The image's preferred base is not the address the layout reserved for it.
    ImageBase { expected: u32, found: u32 },
    /// `[base, base + SizeOfImage)` is empty, overflows or collides with a
    /// static guest region.
    ImageRange { base: u32, size: u32 },
    /// `AddressOfEntryPoint` lies outside `SizeOfImage`.
    EntryOutOfRange { rva: u32, size_of_image: u32 },
    /// An import the image needs has no HLE binding and cannot be resolved.
    UnboundImport { module: String, symbol: String },
    /// The image needs more thunk slots than the window has.
    ThunkWindowExhausted,
}

impl core::fmt::Display for BootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Image(err) => write!(f, "pe32 parse failed: {err:?}"),
            Self::ImageBase { expected, found } => {
                write!(f, "image base {found:#X} != layout base {expected:#X}")
            }
            Self::ImageRange { base, size } => {
                write!(f, "image range {base:#X}+{size:#X} collides with the guest layout")
            }
            Self::EntryOutOfRange { rva, size_of_image } => {
                write!(f, "entry RVA {rva:#X} is outside the image (size {size_of_image:#X})")
            }
            Self::UnboundImport { module, symbol } => {
                write!(f, "unbound import {module}!{symbol}")
            }
            Self::ThunkWindowExhausted => write!(f, "HLE thunk window exhausted"),
        }
    }
}

impl std::error::Error for BootError {}

/// One bound import: guest thunk → host entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundImport {
    /// Guest DLL name, shared with the parsed image's descriptor.
    pub module: Arc<str>,
    /// Host symbol name (`#5` for an import by ordinal).
    pub symbol: String,
    /// Import ordinal when the image imported by ordinal.
    pub ordinal: Option<u16>,
    /// Guest thunk address the loader writes into the IAT.
    pub thunk: u32,
}

impl BoundImport {
    /// The module name as a string slice.
    pub fn module_str(&self) -> &str {
        &self.module
    }
}

/// A booted guest image: the mapped image, its CPU and its thunk table.
pub struct GuestHost {
    pub cpu: Cpu,
    pub image_base: u32,
    pub entry: u32,
    pub exports: Vec<(String, u32)>,
    pub imports: Vec<BoundImport>,
    registry: HleRegistry,
}

impl core::fmt::Debug for GuestHost {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GuestHost")
            .field("image_base", &self.image_base)
            .field("entry", &self.entry)
            .field("imports", &self.imports)
            .field("exports", &self.exports)
            .finish_non_exhaustive()
    }
}

impl GuestHost {
    /// Boots a PE32 image: validates the image against the guest address plan,
    /// then binds every import by name through `bind` (which receives
    /// `(module, symbol, slot)` and installs the handler).
    ///
    /// The guest thunk address for slot `n` is `HLE_BASE + n * 16`; callers
    /// propagate that value into the import table in place of the loader's
    /// pointer, exactly as the donor's `wc3_pe_load` does.
    ///
    /// A `bind` that reports success without installing an entry is treated as
    /// a failure: the host never records a thunk that resolves to `Unbound`.
    pub fn boot(
        bytes: &[u8],
        mut bind: impl FnMut(&mut HleRegistry, &str, &str, u32) -> bool,
    ) -> Result<Self, BootError> {
        let image = PeImage::parse(bytes).map_err(BootError::Image)?;
        if image.image_base != WAR3_IMAGE_BASE {
            // The layout reserves exactly one base for the game image; a
            // mismatch means the caller handed us a different binary.
            return Err(BootError::ImageBase {
                expected: WAR3_IMAGE_BASE,
                found: image.image_base,
            });
        }
        if image.size_of_image == 0 {
            return Err(BootError::ImageRange { base: image.image_base, size: 0 });
        }
        let end = image
            .image_base
            .checked_add(image.size_of_image)
            .ok_or(BootError::ImageRange {
                base: image.image_base,
                size: image.size_of_image,
            })?;
        for region in layout::REGIONS {
            if image.image_base < region.end() && region.base < end {
                return Err(BootError::ImageRange {
                    base: image.image_base,
                    size: image.size_of_image,
                });
            }
        }
        let entry = if image.entry_rva == 0 {
            // DLLs may have no entry point.
            image.image_base
        } else {
            if image.entry_rva >= image.size_of_image {
                return Err(BootError::EntryOutOfRange {
                    rva: image.entry_rva,
                    size_of_image: image.size_of_image,
                });
            }
            image.image_base + image.entry_rva
        };

        let mut registry = HleRegistry::new();
        let mut imports = Vec::new();
        let mut next_slot = 0u32;
        for import in &image.imports {
            let symbol = match (&import.name, import.ordinal) {
                (Some(name), _) => name.to_string(),
                (None, Some(ordinal)) => format!("#{ordinal}"),
                (None, None) => continue,
            };
            let mut slot = next_slot;
            loop {
                if slot >= HLE_SLOTS {
                    return Err(BootError::ThunkWindowExhausted);
                }
                if registry.entry_at_slot(slot).is_none() {
                    break;
                }
                slot += 1;
            }
            let bound = bind(&mut registry, &import.module, &symbol, slot);
            if !bound || registry.entry_at_slot(slot).is_none() {
                return Err(BootError::UnboundImport {
                    module: import.module.to_string(),
                    symbol,
                });
            }
            next_slot = slot + 1;
            imports.push(BoundImport {
                module: Arc::clone(&import.module),
                symbol,
                ordinal: import.ordinal,
                thunk: HLE_BASE + slot * HLE_STRIDE,
            });
        }
        Ok(Self {
            cpu: Cpu::new(),
            image_base: image.image_base,
            entry,
            exports: image
                .exports
                .iter()
                .filter_map(|export| export.name.clone().map(|name| (name, export.rva)))
                .collect(),
            imports,
            registry,
        })
    }

    /// The thunk address an import was bound to.
    pub fn thunk_for(&self, symbol: &str) -> Option<u32> {
        self.imports
            .iter()
            .find(|import| import.symbol == symbol)
            .map(|import| import.thunk)
    }

    /// One guest call: resolve the thunk, check the arity, run the handler with
    /// the guest CPU (so `esp`/`eip` bookkeeping stays with the caller).
    ///
    /// The argument-stack bytes the callee pops are
    /// [`HleEntry::conv`]`.stack_bytes(arity)`; the dispatcher applies them when
    /// it models the `ret`.
    pub fn call(&mut self, thunk: u32, args: &[u32]) -> Result<u32, CallError> {
        self.cpu.eip = thunk;
        self.registry.call(thunk, &mut self.cpu, args)
    }

    /// Resolve without calling (used by the dispatcher to build its table).
    pub fn resolve(&self, thunk: u32) -> Result<(&str, u8), ResolveError> {
        let entry = self.registry.resolve(thunk)?;
        Ok((entry.symbol.as_str(), entry.arity))
    }

    /// The full registry entry: module, symbol, arity and calling convention.
    pub fn entry(&self, thunk: u32) -> Result<&HleEntry, ResolveError> {
        self.registry.resolve(thunk)
    }

    /// Number of bound imports.
    pub fn import_count(&self) -> usize {
        self.registry.len()
    }
}

/// Convenience: bind an import to a stdcall handler with the given arity.
pub fn bind_fixed(
    registry: &mut HleRegistry,
    module: &str,
    symbol: &str,
    slot: u32,
    arity: u8,
    handler: wc3_guest::hle::Handler,
) -> bool {
    registry
        .bind(slot, module, symbol, Arity::Exact(arity), CallConv::Stdcall, handler)
        .is_ok()
}

/// Guest page size re-export for callers that map the image.
pub const GUEST_PAGE: u32 = PAGE_SIZE;

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic PE32: one `.text` section, one export `W3Boot`, one import
    /// `KERNEL32.dll!GetVersion`. Built from the PE/COFF field layout, not from
    /// donor code.
    fn synthetic_image() -> Vec<u8> {
        const SEC_TABLE: usize = 0x58 + 0xE0;
        let mut bytes = vec![0u8; 0x800];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        bytes[coff..coff + 2].copy_from_slice(&0x014Cu16.to_le_bytes());
        bytes[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[coff + 16..coff + 18].copy_from_slice(&0xE0u16.to_le_bytes());
        let opt = coff + 20;
        bytes[opt..opt + 2].copy_from_slice(&0x010Bu16.to_le_bytes());
        bytes[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[opt + 28..opt + 32].copy_from_slice(&WAR3_IMAGE_BASE.to_le_bytes());
        bytes[opt + 56..opt + 60].copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
        bytes[opt + 92..opt + 96].copy_from_slice(&16u32.to_le_bytes());
        // Data directories: export at opt+96, import at opt+104.
        bytes[opt + 96..opt + 100].copy_from_slice(&0x1100u32.to_le_bytes());
        bytes[opt + 104..opt + 108].copy_from_slice(&0x1200u32.to_le_bytes());
        let section = SEC_TABLE;
        bytes[section..section + 5].copy_from_slice(b".text");
        bytes[section + 8..section + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[section + 16..section + 20].copy_from_slice(&0x600u32.to_le_bytes());
        bytes[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes());

        // Export directory at RVA 0x1100 (file 0x300): one named export
        // "W3Boot" at RVA 0x1000.
        let dir = 0x300usize;
        bytes[dir + 12..dir + 16].copy_from_slice(&0x1100u32.to_le_bytes()); // name rva
        bytes[dir + 16..dir + 20].copy_from_slice(&1u32.to_le_bytes()); // base ordinal
        bytes[dir + 20..dir + 24].copy_from_slice(&1u32.to_le_bytes()); // number of funcs
        bytes[dir + 24..dir + 28].copy_from_slice(&1u32.to_le_bytes()); // number of names
        bytes[dir + 28..dir + 32].copy_from_slice(&0x1130u32.to_le_bytes()); // funcs rva
        bytes[dir + 32..dir + 36].copy_from_slice(&0x1140u32.to_le_bytes()); // names rva
        bytes[dir + 36..dir + 40].copy_from_slice(&0x1150u32.to_le_bytes()); // ordinals rva
        // funcs table (RVA 0x1130 -> file 0x330)
        bytes[0x330..0x334].copy_from_slice(&0x1000u32.to_le_bytes());
        // names table (RVA 0x1140 -> file 0x340): one name pointer
        bytes[0x340..0x344].copy_from_slice(&0x1160u32.to_le_bytes());
        // ordinals (RVA 0x1150 -> file 0x350)
        bytes[0x350..0x352].copy_from_slice(&0u16.to_le_bytes());
        // name string (RVA 0x1160 -> file 0x360)
        bytes[0x360..0x366].copy_from_slice(b"W3Boot");
        // directory name (RVA 0x1100 -> file 0x300+) is the export DLL name;
        // reuse the same area's tail.
        bytes[0x380..0x388].copy_from_slice(b"guest.exe"[..8].as_ref());

        // Import descriptor at RVA 0x1200 (file 0x400): one module.
        let idir = 0x400usize;
        bytes[idir..idir + 4].copy_from_slice(&0x1230u32.to_le_bytes()); // ILT rva
        bytes[idir + 12..idir + 16].copy_from_slice(&0x1250u32.to_le_bytes()); // name rva
        bytes[idir + 16..idir + 20].copy_from_slice(&0x1230u32.to_le_bytes()); // IAT rva
        // ILT (RVA 0x1230 -> file 0x430): one name import, then terminator.
        bytes[0x430..0x434].copy_from_slice(&0x1270u32.to_le_bytes());
        // DLL name (RVA 0x1250 -> file 0x450)
        let dll = b"KERNEL32.dll\0";
        bytes[0x450..0x450 + dll.len()].copy_from_slice(dll);
        // Import name (RVA 0x1270 -> file 0x470): hint + "GetVersion"
        bytes[0x472..0x47C].copy_from_slice(b"GetVersion");
        bytes
    }

    fn get_version(cpu: &mut Cpu, _args: &[u32]) -> u32 {
        cpu.eax = 0x1DB1_0105;
        0x1DB1_0105
    }

    fn boot_with_get_version(bytes: &[u8]) -> Result<GuestHost, BootError> {
        GuestHost::boot(bytes, |registry, module, symbol, slot| {
            bind_fixed(registry, module, symbol, slot, 0, get_version)
        })
    }

    fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn recomp_host_boots_image_and_dispatches_hle_call() {
        let bytes = synthetic_image();
        let host = GuestHost::boot(&bytes, |registry, module, symbol, slot| {
            assert_eq!(module, "KERNEL32.dll");
            assert_eq!(symbol, "GetVersion");
            bind_fixed(registry, module, symbol, slot, 0, get_version)
        })
        .expect("boot synthetic image");

        assert_eq!(host.image_base, WAR3_IMAGE_BASE);
        assert_eq!(host.entry, WAR3_IMAGE_BASE + 0x1000);
        assert_eq!(host.import_count(), 1);
        assert!(host.imports[0].thunk >= HLE_BASE);
        assert_eq!(host.imports[0].module_str(), "KERNEL32.dll");
        assert_eq!(host.imports[0].ordinal, None);
        assert_eq!(host.thunk_for("GetVersion"), Some(host.imports[0].thunk));
        // The export census is visible to the dispatcher.
        assert_eq!(host.exports, vec![("W3Boot".to_string(), 0x1000)]);

        // A guest call: the thunk resolves, the arity is checked, the handler
        // runs against the guest CPU register file.
        let mut host = host;
        let thunk = host.thunk_for("GetVersion").expect("bound thunk");
        assert_eq!(host.resolve(thunk), Ok(("GetVersion", 0)));
        let entry = host.entry(thunk).expect("registry entry");
        assert_eq!(entry.module, "KERNEL32.dll");
        assert_eq!(entry.conv, CallConv::Stdcall);
        assert_eq!(entry.conv.stack_bytes(entry.arity), 0);
        assert_eq!(host.call(thunk, &[]), Ok(0x1DB1_0105));
        assert_eq!(host.cpu.eax, 0x1DB1_0105);
        assert_eq!(host.cpu.eip, thunk, "eip points at the thunk during a call");
        assert_eq!(
            host.call(thunk, &[1]),
            Err(CallError::ArityMismatch {
                expected: 0,
                got: 1
            })
        );
    }

    #[test]
    fn recomp_host_binds_ordinal_imports() {
        let mut bytes = synthetic_image();
        // ILT entry at file 0x430: import by ordinal 5 of KERNEL32.dll.
        set_u32(&mut bytes, 0x430, 0x8000_0005);
        let host = boot_with_get_version(&bytes).expect("boot ordinal import");
        assert_eq!(host.import_count(), 1);
        assert_eq!(host.imports[0].symbol, "#5");
        assert_eq!(host.imports[0].ordinal, Some(5));
        assert_eq!(host.thunk_for("#5"), Some(host.imports[0].thunk));
    }

    #[test]
    fn recomp_host_refuses_an_unbound_import() {
        let bytes = synthetic_image();
        let err = GuestHost::boot(&bytes, |_, _, _, _| false).unwrap_err();
        match err {
            BootError::UnboundImport { module, symbol } => {
                assert_eq!(module, "KERNEL32.dll");
                assert_eq!(symbol, "GetVersion");
            }
            other => panic!("expected UnboundImport, got {other:?}"),
        }
    }

    #[test]
    fn recomp_host_refuses_a_bind_that_did_not_bind() {
        // A callback that claims success without installing an entry must not
        // leave a thunk that resolves to Unbound.
        let bytes = synthetic_image();
        let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
        assert!(matches!(err, BootError::UnboundImport { .. }), "{err}");
    }

    #[test]
    fn recomp_host_refuses_a_foreign_image_base() {
        let mut bytes = synthetic_image();
        let opt = 0x58usize;
        bytes[opt + 28..opt + 32].copy_from_slice(&0x1000_0000u32.to_le_bytes());
        let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
        assert!(
            matches!(err, BootError::ImageBase { found: 0x1000_0000, .. }),
            "a non-game image must not be mapped into the game arena: {err}"
        );
    }

    #[test]
    fn recomp_host_refuses_bases_over_reserved_guest_state() {
        // Audit L547-02: the HLE thunk window, stacks, TEB and heap all resolve
        // to a layout owner; none of them may host the game image.
        for base in [0x7F00_0000u32, 0x3100_0000, 0x3180_0000, 0x3F00_0000, 0x3600_0000] {
            let mut bytes = synthetic_image();
            set_u32(&mut bytes, 0x58 + 28, base);
            let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
            assert!(
                matches!(err, BootError::ImageBase { found, .. } if found == base),
                "base {base:#X} must be refused, got {err}"
            );
        }
    }

    #[test]
    fn recomp_host_refuses_an_image_that_collides_with_an_arena() {
        // Correct base, but SizeOfImage reaches the stack pool at 0x3100_0000.
        let mut bytes = synthetic_image();
        set_u32(&mut bytes, 0x58 + 56, 0x3100_0000);
        let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
        assert!(matches!(err, BootError::ImageRange { size: 0x3100_0000, .. }), "{err}");
        // SizeOfImage larger than the address space: the range overflows.
        let mut bytes = synthetic_image();
        set_u32(&mut bytes, 0x58 + 56, 0xFFFF_FFFF);
        let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
        assert!(matches!(err, BootError::ImageRange { .. }), "{err}");
        // Zero SizeOfImage is not an image.
        let mut bytes = synthetic_image();
        set_u32(&mut bytes, 0x58 + 56, 0);
        assert!(matches!(
            GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err(),
            BootError::ImageRange { size: 0, .. }
        ));
    }

    #[test]
    fn recomp_host_refuses_entry_outside_the_image() {
        let mut bytes = synthetic_image();
        set_u32(&mut bytes, 0x58 + 16, 0x3000); // SizeOfImage is 0x2000
        let err = GuestHost::boot(&bytes, |_, _, _, _| true).unwrap_err();
        assert!(
            matches!(err, BootError::EntryOutOfRange { rva: 0x3000, size_of_image: 0x2000 }),
            "{err}"
        );
    }

    #[test]
    fn recomp_host_rejects_truncated_bytes() {
        let err = GuestHost::boot(b"not a pe", |_, _, _, _| true).unwrap_err();
        assert!(matches!(err, BootError::Image(_)));
    }

    #[test]
    fn recomp_host_registry_is_the_guest_thunk_table() {
        // The consumer must use the crate's own arithmetic: slot n is
        // HLE_BASE + n*16 and nothing else.
        let bytes = synthetic_image();
        let host = boot_with_get_version(&bytes).expect("boot");
        assert_eq!(
            host.thunk_for("GetVersion"),
            Some(wc3_guest::layout::thunk_va(0).unwrap()),
            "slot 0 is the first free slot, so the thunk is HLE_BASE"
        );
        assert_eq!(host.resolve(HLE_BASE), Ok(("GetVersion", 0)));
    }
}

//! Mechanic `recomp.host` / `recomp.hle-abi` — the guest-host seam against the
//! operator's shipped PE32 binaries.
//!
//! The synthetic-image tests live in `wc3-re::guest_host` (unit tests) and cover
//! the validation rules exhaustively. These tests close the seam on the real
//! pair: `War3.exe` must boot at its header base with every import bound, and
//! `Game.dll` must be refused because the layout reserves that base for the
//! game DLL, not the host executable. Both skip when the (untracked) analysis
//! corpus is absent, as `real_corpus.rs` does.

use std::path::PathBuf;

use wc3_guest::cpu::Cpu;
use wc3_guest::hle::{CallConv, HleRegistry};
use wc3_guest::layout::{HLE_BASE, WAR3_IMAGE_BASE};
use wc3_re::guest_host::{bind_fixed, BootError, GuestHost};

fn shipped(name: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/game_files")
        .join(name);
    std::fs::read(path).ok()
}

fn noop(_cpu: &mut Cpu, _args: &[u32]) -> u32 {
    0
}

fn bind_all(registry: &mut HleRegistry, module: &str, symbol: &str, slot: u32) -> bool {
    bind_fixed(registry, module, symbol, slot, 0, noop)
}

#[test]
fn recomp_host_boots_shipped_war3_exe() {
    let Some(bytes) = shipped("Files100_War3.exe") else {
        return;
    };
    let host = GuestHost::boot(&bytes, bind_all).expect("boot the shipped War3.exe");
    assert_eq!(host.image_base, WAR3_IMAGE_BASE);
    assert_eq!(host.entry, WAR3_IMAGE_BASE + 0x67fdc);
    assert_eq!(host.import_count(), 487);
    assert_eq!(host.imports.len(), 487);
    // Slots are dense and start at the first thunk in the window.
    assert_eq!(host.imports[0].thunk, HLE_BASE);
    assert_eq!(host.imports[486].thunk, HLE_BASE + 486 * 16);
    // 68 ordinal imports (43 Storm services + 25 wsock32) + 419 named imports.
    assert_eq!(host.imports.iter().filter(|i| i.ordinal.is_some()).count(), 68);
    assert_eq!(host.imports.iter().filter(|i| i.symbol.starts_with('#')).count(), 68);
    assert_eq!(
        host.imports
            .iter()
            .filter(|i| i.ordinal.is_some() && i.module_str().eq_ignore_ascii_case("storm.dll"))
            .count(),
        43
    );
    assert_eq!(
        host.imports
            .iter()
            .filter(|i| i.ordinal.is_some() && i.module_str().eq_ignore_ascii_case("wsock32.dll"))
            .count(),
        25
    );
    // All 695 host exports are visible to the dispatcher; the names form the
    // A1…LD alphabet the shipped binary uses (ordinal 1 is `A1`, 695 is `LD`).
    assert_eq!(host.exports.len(), 695);
    assert!(host.exports.iter().all(|(name, _)| !name.is_empty()));
    assert_eq!(host.exports.first().map(|(name, _)| name.as_str()), Some("A1"));
    assert_eq!(host.exports.last().map(|(name, _)| name.as_str()), Some("LD"));
    let unique: std::collections::BTreeSet<&str> =
        host.exports.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(unique.len(), 695, "export names are unique");

    // The ABI record exposed to the dispatcher carries the convention.
    let entry = host.entry(host.imports[0].thunk).expect("first bound entry");
    assert_eq!(entry.conv, CallConv::Stdcall);
    assert_eq!(entry.arity, 0);
    assert_eq!(entry.conv.stack_bytes(entry.arity), 0);
    assert_eq!(host.resolve(host.imports[0].thunk), Ok((entry.symbol.as_str(), 0)));
}

#[test]
fn recomp_host_refuses_shipped_game_dll_base() {
    let Some(bytes) = shipped("Files100_Game.dll") else {
        return;
    };
    let err = GuestHost::boot(&bytes, bind_all).unwrap_err();
    assert!(
        matches!(err, BootError::ImageBase { expected, found } if expected == WAR3_IMAGE_BASE && found == 0x6f00_0000),
        "Game.dll must not boot as war3.exe: {err}"
    );
}

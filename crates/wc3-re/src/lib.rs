//! Warcraft III reverse-engineering port (`wc3-re`).
//!
//! This crate is the canonical resource-access and script-execution layer for
//! the wc3rust Rust port. It is built from the operator's own local
//! Warcraft III installation (RoC + TFT) and the disassembly of the shipped
//! binaries. Every format statement in the modules below is anchored either to
//! a byte offset in a real local file or to a donor implementation (file:line
//! recorded in `analysis/agent-re/RESOURCE_ACCESS.md` and in the module docs).
//!
//! Layers:
//! - [`storm`] — Storm/MPQ virtual filesystem (archives, listfile, patch/locale
//!   priority, block-table introspection, codec provenance)
//! - [`blp`] — BLP1/BLP2 texture access (paletted/JPEG/DXT/BGRA, alpha depth,
//!   mip chains)
//! - [`mdx`] — MDX chunk scanner plus typed model access (geosets, materials,
//!   layers, textures, sequences, attachments)
//! - [`slk`] / [`txt`] / [`wts`] — WC3 spreadsheet/INI/string tables
//! - [`trigger`] — `UI\TriggerData.txt` + `UI\TriggerStrings.txt` registry
//! - [`objectdata`] — SLK + `*Func.txt` + `*Strings.txt` object database and the
//!   four-character object-ID mapping used by maps and scripts
//! - [`mapfiles`] — W3M/W3X container: every inner file (`war3map.j`, `w3e`,
//!   `w3i`, `w3u`, `w3a`, `w3t`, `w3b`, `w3d`, `w3q`, `w3h`, `wts`, …)
//! - [`skin`] / [`terrain`] / [`icons`] — UI skin resolution, TerrainArt tile
//!   resolution, command-button icon inventory
//! - [`jass`] — JASS2 lexer/parser/interpreter, native registry, triggers
//! - [`ai`] — AI-script parsing/execution over the same JASS front end
//! - [`engine`] / [`host`] / [`storm_api`] — models derived from Game.dll and
//!   War3.exe (loop/state, host import/export split, Storm ordinal facade)
//! - [`scan`] — whole-install verification used by `wc3re scan`

pub mod abilities;
pub mod ai;
pub mod blp;
pub mod art;
pub mod catalog;
pub mod consumption;
pub mod creeps;
pub mod cutscenes;
pub mod engine;
pub mod gameplay;
pub mod error;
pub mod guest_host;
pub mod host;
pub mod icons;
pub mod jass;
pub mod manifest;
pub mod mapboot;
pub mod mapcontent;
pub mod mapfiles;
pub mod mapobjects;
pub mod mpqex;
pub mod mdx;
pub mod objectdata;
pub mod scan;
pub mod skin;
pub mod slk;
pub mod storm;
pub mod storm_api;
pub mod terrain;
pub mod trigger;
pub mod txt;
pub mod wts;

pub use error::{ReError, Result};

/// Four-character object code (e.g. `hfoo`) packed in the order the engine
/// stores it: byte 0 is the most significant byte. JASS rawcodes (`'hfoo'`)
/// evaluate to this integer.
pub fn fourcc(code: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (slot, byte) in out.iter_mut().zip(code.bytes()) {
        *slot = byte;
    }
    out
}

/// Decode a four-character code as stored by the engine.
pub fn fourcc_str(code: &[u8; 4]) -> String {
    code.iter().map(|&b| b as char).collect()
}

/// Convert a JASS rawcode integer to its four bytes.
pub fn rawcode_bytes(value: i64) -> [u8; 4] {
    let v = value as u32;
    [
        ((v >> 24) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    ]
}

/// Convert four bytes to a JASS rawcode integer.
pub fn rawcode_value(code: &[u8; 4]) -> i64 {
    i64::from(u32::from_be_bytes(*code))
}

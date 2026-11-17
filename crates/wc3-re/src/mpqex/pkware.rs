//! PKWARE DCL codec — canonical implementation lives in `wc3-mpq`.
//!
//! This module is a compatibility re-export. The original file copied
//! `crates/wc3-mpq/src/pkware.rs` and added the end-of-input (`0x306`)
//! acceptance needed by encrypted `.w3m` sectors; that fix now lives in the
//! canonical module, so no duplicate decoder remains here. Callers keep using
//! `crate::mpqex::pkware::explode` exactly as before.
//!
//! Provenance: `crates/wc3-mpq/src/pkware.rs` (StormLib `pklib/explode.c`
//! port), fix verified by `mpqex::tests::reads_encrypted_pkware_wts_from_roc_map`.

pub use wc3_mpq::pkware::*;

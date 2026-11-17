//! Huffman codec — canonical implementation lives in `wc3-mpq`.
//!
//! This module is a compatibility re-export of `wc3_mpq::huffman`; the byte
//! streams were already identical (verified with `diff`), so the copy is gone
//! and `crate::mpqex::huffman::decompress` resolves to the single decoder.

pub use wc3_mpq::huffman::*;

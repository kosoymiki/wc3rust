//! Encrypted multi-sector MPQ access — compatibility facade over `wc3-mpq`.
//!
//! Historically this module carried its own hash/block-table reader plus copies
//! of the PKWARE/Huffman/ADPCM decoders. The byte-level fix now lives in the
//! canonical reader (`wc3-mpq/src/{stream,lib,pkware}.rs`), together with the
//! single codec implementation, so `MpqEx` is a thin compatibility facade:
//!
//! - base-offset detection is shared with `wc3-formats::map::map_mpq_base`;
//! - table parsing, decryption and sector decoding are `wc3_mpq::MpqArchive`;
//! - [`FileMetaEx`] is a projection of `wc3_mpq::FileMeta`;
//! - `pkware/huffman/adpcm` are pure re-exports of `wc3-mpq` (no copy).
//!
//! `StormFs` and `MapBundle` call `MpqArchive` directly; `MpqEx` remains only
//! for compatibility/tools (`wc3re extract`) and is not on the game read path.

pub mod adpcm;
pub mod huffman;
pub mod pkware;

use std::path::{Path, PathBuf};

use wc3_mpq::MpqArchive;

use crate::error::{ReError, Result};

pub const MPQ_MAGIC: &[u8; 4] = b"MPQ\x1a";

/// Decoded per-file metadata, including the declared codec mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetaEx {
    pub size: u32,
    pub compressed_size: u32,
    pub flags: u32,
    pub codecs: String,
}

/// Correcting archive view used for encrypted multi-sector files.
#[derive(Debug)]
pub struct MpqEx {
    archive: MpqArchive,
    path: PathBuf,
    base: u64,
}

impl MpqEx {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let base = wc3_formats::map::map_mpq_base(&path)
            .map_err(|err| ReError::Parse(format!("{}: {err}", path.display())))?;
        Self::open_at(path, base)
    }

    pub fn open_at(path: impl AsRef<Path>, base: u64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let archive = MpqArchive::open_at(&path, base)?;
        Ok(Self {
            archive,
            path,
            base,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn listfile(&self) -> &[String] {
        self.archive.listfile()
    }

    pub fn block_count(&self) -> usize {
        self.archive.block_count()
    }

    /// Convenience used by map/archive fallbacks.
    pub fn read_at(path: impl AsRef<Path>, name: &str) -> Result<Vec<u8>> {
        Self::open(path)?.read(name)
    }

    pub fn meta(&self, name: &str) -> Option<FileMetaEx> {
        let (_, meta) = self.archive.meta(name)?;
        Some(FileMetaEx {
            size: meta.size,
            compressed_size: meta.compressed_size,
            flags: meta.flags,
            codecs: meta.declared_usage().describe(),
        })
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        self.archive.read(name).map_err(ReError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_encrypted_pkware_wts_from_roc_map() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign/Human04.w3m");
        if !path.is_file() {
            eprintln!("skip: Human04.w3m missing");
            return;
        }
        let archive = MpqEx::open(&path).expect("open map");
        let wts = archive.read("war3map.wts").expect("read wts");
        let text = String::from_utf8_lossy(&wts);
        assert!(
            text.contains("STRING 0"),
            "wts head: {:?}",
            &text[..80.min(text.len())]
        );
        let script = archive.read("war3map.j").expect("read script");
        assert_eq!(script.len(), 221_222);
    }

    #[test]
    fn reads_all_inner_files() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Maps/Campaign/Human04.w3m");
        if !path.is_file() {
            return;
        }
        let archive = MpqEx::open(&path).unwrap();
        for name in [
            "war3map.w3e",
            "war3map.w3i",
            "war3map.wpm",
            "war3map.w3u",
            "war3map.wtg",
            "war3map.wct",
            "war3map.w3r",
            "war3map.w3c",
            "war3map.w3s",
            "war3map.doo",
            "war3mapUnits.doo",
            "war3map.mmp",
            "war3map.shd",
            "war3mapMap.blp",
        ] {
            let bytes = archive.read(name).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(!bytes.is_empty(), "{name}");
        }
    }
}

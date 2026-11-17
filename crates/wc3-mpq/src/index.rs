//! Persistent archive index and codec provenance.
//!
//! The index is the fast random-access path for the asset pipeline: it stores
//! `name -> block` mappings plus the block header for every listfile entry, so a
//! consumer can enumerate, filter and size up the archive without re-deriving
//! the hash table or re-reading the listfile. It is validated against an
//! [`ArchiveFingerprint`] so a stale index is rejected instead of silently
//! pointing at the wrong blocks.
//!
//! Format (`H2MQI\x01`): little-endian, all fields fixed width except names.
//!
//! ```text
//! magic        8 bytes  "H2MQI\x01\x00\x00"
//! version      u32
//! archive_size u64
//! hash_count   u32
//! block_count  u32
//! table_digest u64      FNV-1a over decrypted (offset,csize,size,flags)
//! entry_count  u32
//! entries      entry_count x { name_len u32, name bytes,
//!                              block_index u32, size u32, compressed u32,
//!                              flags u32, codec u16 }
//! ```

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::{MpqArchive, MpqError};

/// Compression bits actually observed while decoding a file.
///
/// MPQ stores a per-file mask, but WC3 also emits per-sector tags that do not
/// follow the file mask (WAV sectors tagged ADPCM inside a zlib-masked file),
/// so the observed set is the trustworthy provenance record.
///
/// The Warcraft III engine decodes five codecs (`0x01`, `0x02`, `0x08`,
/// `0x40`, `0x80`); the catalogue also carries bits for codes that later
/// StormLib releases added (bzip2, LZMA, sparse) so a *declared* file mask can
/// be reported faithfully even though the reader never decodes those bits.
/// That makes the set a `u16`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodecUsage(pub u16);

impl CodecUsage {
    pub const STORED: u16 = 0x0001;
    pub const ZLIB: u16 = 0x0002;
    pub const PKWARE: u16 = 0x0004;
    pub const BZIP2: u16 = 0x0008;
    pub const LZMA: u16 = 0x0010;
    pub const ADPCM_MONO: u16 = 0x0020;
    pub const ADPCM_STEREO: u16 = 0x0040;
    pub const HUFFMAN: u16 = 0x0080;
    /// Run-length "sparse" codec (tag 0x20): catalogue-only, never decoded.
    pub const SPARSE: u16 = 0x0100;
    /// A sector stored with zero compressed length: the decoder emits zeros.
    pub const ZERO: u16 = 0x0200;

    pub fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn merge(&mut self, other: CodecUsage) {
        self.0 |= other.0;
    }

    /// Human-readable list of observed codecs ("stored", "zlib+adpcm-mono").
    pub fn describe(self) -> String {
        const NAMES: [(u16, &str); 10] = [
            (CodecUsage::STORED, "stored"),
            (CodecUsage::ZLIB, "zlib"),
            (CodecUsage::PKWARE, "pkware"),
            (CodecUsage::BZIP2, "bzip2"),
            (CodecUsage::LZMA, "lzma"),
            (CodecUsage::ADPCM_MONO, "adpcm-mono"),
            (CodecUsage::ADPCM_STEREO, "adpcm-stereo"),
            (CodecUsage::HUFFMAN, "huffman"),
            (CodecUsage::SPARSE, "sparse"),
            (CodecUsage::ZERO, "zero"),
        ];
        if self.0 == 0 {
            return "none".to_string();
        }
        let mut parts = Vec::new();
        for (bit, name) in NAMES {
            if self.has(bit) {
                parts.push(name);
            }
        }
        parts.join("+")
    }
}

/// Per-file metadata as stored in the MPQ block table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMeta {
    pub block_index: u32,
    pub size: u32,
    pub compressed_size: u32,
    pub flags: u32,
    /// Declared per-file compression mask (`(flags >> 8) & 0xFF`).
    pub mask: u8,
}

impl FileMeta {
    pub fn is_single_unit(self) -> bool {
        self.flags & 0x0100_0000 != 0
    }

    pub fn is_encrypted(self) -> bool {
        self.flags & 0x0001_0000 != 0
    }

    pub fn is_compressed(self) -> bool {
        self.flags & 0x0000_0200 != 0
    }

    /// Declared codec set from the file mask (not the observed sector tags).
    pub fn declared_usage(self) -> CodecUsage {
        let mut bits = 0u16;
        if !self.is_compressed() || self.mask == 0 {
            return CodecUsage(CodecUsage::STORED);
        }
        if self.mask & 0x01 != 0 {
            bits |= CodecUsage::HUFFMAN;
        }
        if self.mask & 0x02 != 0 {
            bits |= CodecUsage::ZLIB;
        }
        if self.mask & 0x08 != 0 {
            bits |= CodecUsage::PKWARE;
        }
        // 0x10/0x20 are not decodable by the WC3 engine (they are a later
        // StormLib extension); they are still reported when a foreign producer
        // declares them, and a read of such a file fails with
        // `UnsupportedCompression` instead of guessing.
        if self.mask & 0x10 != 0 {
            bits |= CodecUsage::BZIP2;
        }
        if self.mask & 0x20 != 0 {
            bits |= CodecUsage::SPARSE;
        }
        if self.mask & 0x40 != 0 {
            bits |= CodecUsage::ADPCM_MONO;
        }
        if self.mask & 0x80 != 0 {
            bits |= CodecUsage::ADPCM_STEREO;
        }
        CodecUsage(bits)
    }
}

/// Cheap identity of an archive's block table, used to reject stale indexes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArchiveFingerprint {
    pub archive_size: u64,
    pub hash_count: u32,
    pub block_count: u32,
    pub table_digest: u64,
}

impl ArchiveFingerprint {
    /// FNV-1a over the decrypted block table. Detects added/removed/resized
    /// blocks without hashing the 700 MB payload.
    pub(crate) fn compute(
        archive_size: u64,
        hash_count: u32,
        blocks: &[(u32, u32, u32, u32)],
    ) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for (offset, csize, size, flags) in blocks {
            for value in [*offset, *csize, *size, *flags] {
                for byte in value.to_le_bytes() {
                    hash ^= byte as u64;
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        Self {
            archive_size,
            hash_count,
            block_count: blocks.len() as u32,
            table_digest: hash,
        }
    }
}

/// One listfile entry with its block header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: String,
    pub block_index: u32,
    pub size: u32,
    pub compressed_size: u32,
    pub flags: u32,
    pub codec: u16,
}

impl IndexEntry {
    pub fn meta(&self) -> FileMeta {
        FileMeta {
            block_index: self.block_index,
            size: self.size,
            compressed_size: self.compressed_size,
            flags: self.flags,
            mask: ((self.flags >> 8) & 0xFF) as u8,
        }
    }
}

const INDEX_MAGIC: &[u8; 8] = b"H2MQI\x01\x00\x00";
const INDEX_VERSION: u32 = 2;
const MAX_INDEX_ENTRIES: u32 = 4_000_000;

/// In-memory archive index. Cheap to build from an open archive, cheap to load
/// from disk (`load`), and usable as an `AssetDb` manifest.
#[derive(Debug, Clone, Default)]
pub struct MpqIndex {
    pub fingerprint: Option<ArchiveFingerprint>,
    entries: Vec<IndexEntry>,
    by_name: HashMap<String, usize>,
}

impl MpqIndex {
    /// Build the index from an already-open archive (reads the block table that
    /// is already in memory; no payload I/O).
    pub fn build(archive: &MpqArchive) -> Self {
        let mut entries = Vec::with_capacity(archive.listfile().len());
        for name in archive.listfile() {
            if let Some((block_index, meta)) = archive.meta(name) {
                entries.push(IndexEntry {
                    name: crate::normalize_name(name),
                    block_index,
                    size: meta.size,
                    compressed_size: meta.compressed_size,
                    flags: meta.flags,
                    codec: meta.declared_usage().0,
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut by_name = HashMap::with_capacity(entries.len());
        for (slot, entry) in entries.iter().enumerate() {
            by_name.insert(entry.name.to_ascii_uppercase(), slot);
        }
        Self {
            fingerprint: Some(archive.fingerprint()),
            entries,
            by_name,
        }
    }

    /// Build an index from raw directory listings (no archive open).
    pub fn from_entries(fingerprint: ArchiveFingerprint, entries: Vec<IndexEntry>) -> Self {
        let mut entries = entries;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let mut by_name = HashMap::with_capacity(entries.len());
        for (slot, entry) in entries.iter().enumerate() {
            by_name.insert(entry.name.to_ascii_uppercase(), slot);
        }
        Self {
            fingerprint: Some(fingerprint),
            entries,
            by_name,
        }
    }

    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&IndexEntry> {
        let key = crate::normalize_name(name).to_ascii_uppercase();
        self.by_name.get(&key).map(|slot| &self.entries[*slot])
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Total uncompressed size over all indexed entries.
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size as u64).sum()
    }

    pub fn total_compressed_size(&self) -> u64 {
        self.entries.iter().map(|e| e.compressed_size as u64).sum()
    }

    /// Entries whose name ends with `ext` (case-insensitive, extension without
    /// dot). Single pass, no allocation per entry.
    pub fn with_extension(&self, ext: &str) -> impl Iterator<Item = &IndexEntry> {
        let suffix = format!(".{}", ext.to_ascii_lowercase());
        self.entries.iter().filter(move |entry| {
            entry.name.len() >= suffix.len()
                && entry.name[entry.name.len() - suffix.len()..]
                    .eq_ignore_ascii_case(&suffix)
        })
    }

    /// Codec histogram over the declared masks, for coverage reports.
    pub fn codec_histogram(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<u16, usize> = HashMap::new();
        for entry in &self.entries {
            *counts.entry(entry.codec).or_default() += 1;
        }
        let mut rows: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(bits, count)| (CodecUsage(bits).describe(), count))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        rows
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let fp = self.fingerprint.unwrap_or_default();
        let mut out = Vec::with_capacity(64 + self.entries.len() * 48);
        out.extend_from_slice(INDEX_MAGIC);
        out.extend_from_slice(&INDEX_VERSION.to_le_bytes());
        out.extend_from_slice(&fp.archive_size.to_le_bytes());
        out.extend_from_slice(&fp.hash_count.to_le_bytes());
        out.extend_from_slice(&fp.block_count.to_le_bytes());
        out.extend_from_slice(&fp.table_digest.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&(entry.name.len() as u32).to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
            out.extend_from_slice(&entry.block_index.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
            out.extend_from_slice(&entry.compressed_size.to_le_bytes());
            out.extend_from_slice(&entry.flags.to_le_bytes());
            out.extend_from_slice(&entry.codec.to_le_bytes());
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MpqError> {
        if bytes.len() < 40 || &bytes[..8] != INDEX_MAGIC {
            return Err(MpqError::Corrupt("bad MPQ index magic"));
        }
        let mut cursor = 8usize;
        let mut take = |n: usize| -> Result<&[u8], MpqError> {
            let end = cursor
                .checked_add(n)
                .ok_or(MpqError::Corrupt("index overflow"))?;
            let slice = bytes
                .get(cursor..end)
                .ok_or(MpqError::Corrupt("index truncated"))?;
            cursor = end;
            Ok(slice)
        };
        // Total readers: the `take(n)` slices are exact, and a short slice is a
        // typed error rather than a slice-index panic.
        let read_u32 = |b: &[u8]| -> Result<u32, MpqError> {
            b.first_chunk::<4>()
                .copied()
                .map(u32::from_le_bytes)
                .ok_or(MpqError::Corrupt("index field truncated"))
        };
        let read_u64 = |b: &[u8]| -> Result<u64, MpqError> {
            b.first_chunk::<8>()
                .copied()
                .map(u64::from_le_bytes)
                .ok_or(MpqError::Corrupt("index field truncated"))
        };

        let version = read_u32(take(4)?)?;
        if version != INDEX_VERSION {
            return Err(MpqError::Corrupt("unsupported MPQ index version"));
        }
        let fingerprint = ArchiveFingerprint {
            archive_size: read_u64(take(8)?)?,
            hash_count: read_u32(take(4)?)?,
            block_count: read_u32(take(4)?)?,
            table_digest: read_u64(take(8)?)?,
        };
        let count = read_u32(take(4)?)?;
        if count > MAX_INDEX_ENTRIES {
            return Err(MpqError::Corrupt("implausible index entry count"));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name_len = read_u32(take(4)?)? as usize;
            if name_len > 4096 {
                return Err(MpqError::Corrupt("implausible index name length"));
            }
            let name = std::str::from_utf8(take(name_len)?)
                .map_err(|_| MpqError::Corrupt("index name is not UTF-8"))?
                .to_string();
            let block_index = read_u32(take(4)?)?;
            let size = read_u32(take(4)?)?;
            let compressed_size = read_u32(take(4)?)?;
            let flags = read_u32(take(4)?)?;
            let codec = take(2)?
                .first_chunk::<2>()
                .copied()
                .map(u16::from_le_bytes)
                .ok_or(MpqError::Corrupt("index codec truncated"))?;
            entries.push(IndexEntry {
                name,
                block_index,
                size,
                compressed_size,
                flags,
                codec,
            });
        }
        let mut index = Self {
            fingerprint: Some(fingerprint),
            entries,
            by_name: HashMap::new(),
        };
        index.by_name = HashMap::with_capacity(index.entries.len());
        for (slot, entry) in index.entries.iter().enumerate() {
            index.by_name.insert(entry.name.to_ascii_uppercase(), slot);
        }
        Ok(index)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<usize> {
        let bytes = self.to_bytes();
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &bytes)?;
        Ok(bytes.len())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, MpqError> {
        let bytes = std::fs::read(path).map_err(MpqError::Io)?;
        Self::from_bytes(&bytes)
    }

    /// True when this index was built from the archive currently at `path`.
    pub fn matches(&self, archive: &MpqArchive) -> bool {
        self.fingerprint == Some(archive.fingerprint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u32) -> IndexEntry {
        IndexEntry {
            name: crate::normalize_name(name),
            block_index: 7,
            size,
            compressed_size: size / 2,
            flags: 0x8000_0000 | 0x0000_0200 | (0x02 << 8),
            codec: CodecUsage::ZLIB,
        }
    }

    #[test]
    fn roundtrips_entries() {
        let fp = ArchiveFingerprint {
            archive_size: 1234,
            hash_count: 16,
            block_count: 4,
            table_digest: 0xdead_beef,
        };
        let index = MpqIndex::from_entries(
            fp,
            vec![entry("a\\b.wav", 100), entry("c\\d.mdx", 200)],
        );
        let bytes = index.to_bytes();
        let back = MpqIndex::from_bytes(&bytes).expect("parse");
        assert_eq!(back.len(), 2);
        assert_eq!(back.get("A/B.WAV").map(|e| e.size), Some(100));
        assert_eq!(back.total_size(), 300);
        assert_eq!(back.codec_histogram(), vec![("zlib".to_string(), 2)]);
        assert_eq!(back.fingerprint, Some(fp));
    }

    #[test]
    fn rejects_truncated_and_foreign_indexes() {
        assert!(MpqIndex::from_bytes(b"not an index").is_err());
        let index = MpqIndex::from_entries(
            ArchiveFingerprint::default(),
            vec![entry("a.wav", 1)],
        );
        let mut bytes = index.to_bytes();
        bytes.truncate(bytes.len() - 3);
        assert!(MpqIndex::from_bytes(&bytes).is_err());
    }

    #[test]
    fn extension_filter_is_case_insensitive() {
        let index = MpqIndex::from_entries(
            ArchiveFingerprint::default(),
            vec![entry("a.wav", 1), entry("b.WAV", 2), entry("c.mdx", 3)],
        );
        let names: Vec<&str> = index.with_extension("wav").map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a.wav", "b.WAV"]);
    }
}

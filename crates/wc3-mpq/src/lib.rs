//! MPQ archive reader — StormLib-compatible core.
//!
//! Covers the layout used by classic Warcraft III archives: v0 header, encrypted
//! hash/block tables, locale/platform-aware hash lookup, and per-sector
//! decompression over exactly the five codecs the engine implements
//! (`0x01` Huffman, `0x02` zlib, `0x08` PKWARE DCL, `0x40` ADPCM mono,
//! `0x80` ADPCM stereo). Any other codec bit is reported as
//! [`MpqError::UnsupportedCompression`] with the exact bits instead of being
//! guessed. `(listfile)` is read when present but is never required: the engine
//! has no `(listfile)` handling at all, so names resolve by hash.
//!
//! This is a clean-room implementation from the MPQ format description; the
//! StormLib donor is used only as a behavioural oracle in tests.

// Codec implementations are the canonical MPQ codecs for the whole workspace.
// `wc3-re::mpqex` and any other reader re-export these modules instead of
// carrying a second copy (see analysis/tech-lead/WC3RE_MERGE_MATRIX.md).
pub mod adpcm;
pub mod huffman;
pub mod index;
pub mod pkware;
pub mod stream;

pub use index::{ArchiveFingerprint, CodecUsage, FileMeta, IndexEntry, MpqIndex};
pub use stream::MpqFileReader;

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MPQ_MAGIC: &[u8; 4] = b"MPQ\x1a";
pub(crate) const MPQ_FILE_EXISTS: u32 = 0x8000_0000;
pub(crate) const MPQ_FILE_ENCRYPTED: u32 = 0x0001_0000;
/// Obsolete "fix key" flag; the identical bit is `MPQ_FILE_KEY_V2` in StormLib
/// (`StormLib.h:225`): `key = (key + file_pos) ^ file_size`.
pub(crate) const MPQ_FILE_FIX_KEY: u32 = 0x0002_0000;
pub(crate) const MPQ_FILE_SINGLE_UNIT: u32 = 0x0100_0000;
pub(crate) const MPQ_FILE_DELETE_MARKER: u32 = 0x0200_0000;
pub(crate) const MPQ_FILE_COMPRESS: u32 = 0x0000_0200;
/// `MPQ_FILE_IMPLODE`: solo PKWARE DCL stream without a codec tag.
pub(crate) const MPQ_FILE_IMPLODE: u32 = 0x0000_0100;
pub(crate) const MPQ_FILE_SECTOR_CRC: u32 = 0x0400_0000;

/// Flags StormLib honours for a generic MPQ (`StormLib.h:241-250`) and for a
/// Warcraft III map (`StormLib.h:252-259`). The map set deliberately drops
/// `MPQ_FILE_SINGLE_UNIT` and `MPQ_FILE_PATCH_FILE`: Storm.dll from Warcraft III
/// does not know those flags, and map protectors abuse them to make tools read
/// a sector file as one unit. StormLib applies the mask in
/// `SBaseFileTable.cpp:1056` (`pBlock->dwFlags & ha->dwValidFileFlags`).
pub const MPQ_FILE_VALID_FLAGS: u32 = MPQ_FILE_IMPLODE
    | MPQ_FILE_COMPRESS
    | MPQ_FILE_ENCRYPTED
    | MPQ_FILE_FIX_KEY
    | 0x0010_0000 // MPQ_FILE_PATCH_FILE
    | MPQ_FILE_SINGLE_UNIT
    | MPQ_FILE_DELETE_MARKER
    | MPQ_FILE_SECTOR_CRC
    | 0x1000_0000 // MPQ_FILE_SIGNATURE
    | MPQ_FILE_EXISTS;

pub const MPQ_FILE_VALID_FLAGS_W3X: u32 = MPQ_FILE_IMPLODE
    | MPQ_FILE_COMPRESS
    | MPQ_FILE_ENCRYPTED
    | MPQ_FILE_FIX_KEY
    | MPQ_FILE_DELETE_MARKER
    | MPQ_FILE_SECTOR_CRC
    | 0x1000_0000 // MPQ_FILE_SIGNATURE
    | MPQ_FILE_EXISTS;

/// Compression identifiers as StormLib names them (`StormLib.h:278-286`).
///
/// Only the first five exist in the Warcraft III Storm.dll read path
/// (`0x1500f7c0`): both shipped builds carry exactly five `(mask, handler)`
/// slots. `MPQ_COMPRESSION_BZIP2`, `MPQ_COMPRESSION_SPARSE` and
/// `MPQ_COMPRESSION_LZMA` come from later StormLib releases and are listed here
/// so a rejected tag can be named in diagnostics; the reader never decodes
/// them.
pub const MPQ_COMPRESSION_HUFFMANN: u32 = 0x01;
pub const MPQ_COMPRESSION_ZLIB: u32 = 0x02;
pub const MPQ_COMPRESSION_PKWARE: u32 = 0x08;
pub const MPQ_COMPRESSION_BZIP2: u32 = 0x10;
pub const MPQ_COMPRESSION_LZMA: u32 = 0x12;
pub const MPQ_COMPRESSION_SPARSE: u32 = 0x20;
pub const MPQ_COMPRESSION_ADPCM_MONO: u32 = 0x40;
pub const MPQ_COMPRESSION_ADPCM_STEREO: u32 = 0x80;
/// `0x41` / `0x81` are ordinary v0 mask combinations (Huffman, then ADPCM), not
/// special patterns: the engine applies every set bit in table order.
pub const MPQ_COMPRESSION_HUFFMANN_ADPCM_MONO: u32 = 0x41;
pub const MPQ_COMPRESSION_HUFFMANN_ADPCM_STEREO: u32 = 0x81;

/// Sanity ceiling on a declared table size. A malformed/protected header may
/// declare millions of entries; the block table is cut to what the file holds
/// (StormLib `SBaseFileTable.cpp:585-590`), so the ceiling only bounds the
/// hash-table probe loop and any zero-fill.
const MAX_TABLE_ENTRIES: u32 = 8 << 20;

/// Hard ceiling on one decoded MPQ file: the declared `size` and
/// `compressed_size` of a block, and therefore any buffer the reader allocates
/// for it.
///
/// The archive format itself allows `u32::MAX`, which is 4 GiB of address space
/// from a 16-byte block-table entry — the input is untrusted (maps and archives
/// are user-supplied), so the reader refuses anything above this ceiling with
/// [`MpqError::SizeLimitExceeded`] before allocating.
///
/// 512 MiB is far above every shipped Warcraft III entry (measured by
/// `largest_shipped_entry_is_below_the_cap`: the largest block is 10 416 294 B
/// in `War3x.mpq`, 6 591 551 B in `War3.mpq` and 1 236 952 B in
/// `War3xlocal.mpq`, out of 287 MB / 418 MB / 51 MB archives). It leaves room
/// for custom-map payloads while keeping a single allocation survivable on a
/// phone.
pub const MAX_FILE_SIZE: u64 = 512 << 20;

/// Hard ceiling on one decoded sector / single-unit output.
///
/// The MPQ header stores the sector size as a shift that `open` validates to
/// `<= 15`, so `512 << 15` = 16 MiB is the largest sector any archive can
/// declare; a decode stage that wants more than that is malformed.
pub const MAX_SECTOR_SIZE: usize = 512 << 15;

/// Upper bound on the `(sector_count + 1) * 4` offset table.
///
/// `MAX_FILE_SIZE / 512` sectors is the most a legal file can have (the
/// smallest sector shift is 0), so the table itself stays below 4 MiB and the
/// count can never overflow `usize` on a 32-bit Android target.
pub(crate) const MAX_SECTORS: usize = (MAX_FILE_SIZE / 512) as usize;

/// Canonical Warcraft III file names probed when a map archive has no
/// `(listfile)`. Order matters only for reporting; every entry is a pure hash
/// lookup against the archive hash table.
pub const KNOWN_WC3_NAMES: &[&str] = &[
    // Map payloads.
    "war3map.w3e",
    "war3map.w3i",
    "war3map.wpm",
    "war3map.doo",
    "war3mapUnits.doo",
    "war3map.w3u",
    "war3map.w3t",
    "war3map.w3a",
    "war3map.w3d",
    "war3map.w3b",
    "war3map.w3h",
    "war3map.w3q",
    "war3map.w3s",
    "war3map.w3c",
    "war3map.w3r",
    "war3map.wts",
    // Editor-only formats: Game.dll contains no `.wct`/`.wtg` string at all
    // (0 references in both builds), so a runtime never opens them. They are
    // listed because map archives ship them and tools index them.
    "war3map.wct",
    "war3map.wtg",
    "war3map.shd",
    "war3map.mmp",
    "war3map.imp",
    "war3map.j",
    "war3map.lua",
    "war3mapMap.blp",
    "war3mapPreview.tga",
    "war3mapPath.tga",
    "war3mapMisc.txt",
    "war3mapSkin.txt",
    "war3mapExtra.txt",
    "war3mapImported\\war3mapImported.blp",
    // Reforged skin variants.
    "war3mapSkin.w3u",
    "war3mapSkin.w3t",
    "war3mapSkin.w3a",
    "war3mapSkin.w3d",
    "war3mapSkin.w3b",
    "war3mapSkin.w3h",
    "war3mapSkin.w3q",
    // Internal archives.
    "(listfile)",
    "(attributes)",
    "(signature)",
];

#[derive(Debug)]
pub enum MpqError {
    Io(io::Error),
    BadMagic([u8; 4]),
    UnsupportedVersion(u16),
    FileNotFound(String),
    Deleted(String),
    UnsupportedCompression(u32),
    Corrupt(&'static str),
    /// A declared size (block `size`/`compressed_size`, sector table, decoded
    /// output) exceeds the reader's hard resource limit.
    ///
    /// `requested` is what the archive asked for, `limit` the ceiling from
    /// [`MAX_FILE_SIZE`] / [`MAX_SECTOR_SIZE`]. The reader rejects the entry
    /// before allocating, so a hostile header cannot drive an OOM.
    SizeLimitExceeded { requested: u64, limit: u64 },
    /// A codec stage produced more bytes than the unit declared (`expected`).
    ///
    /// Decoding is bounded by the declared size, so the stream is rejected at
    /// the cap instead of being buffered; a valid MPQ unit never exceeds its
    /// block-table `size`.
    Oversize { expected: u64 },
    /// An index was supplied for an archive whose block table has changed.
    StaleIndex,
}

impl fmt::Display for MpqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o error: {e}"),
            Self::BadMagic(m) => write!(f, "bad MPQ magic {m:?}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported MPQ header version {v}"),
            Self::FileNotFound(name) => write!(f, "file not found in archive: {name}"),
            Self::Deleted(name) => write!(f, "file is marked deleted: {name}"),
            Self::UnsupportedCompression(codec) => {
                write!(f, "unsupported MPQ compression bits 0x{codec:04x}")
            }
            Self::Corrupt(why) => write!(f, "corrupt MPQ: {why}"),
            Self::SizeLimitExceeded { requested, limit } => write!(
                f,
                "MPQ size limit exceeded: {requested} bytes requested, hard limit {limit}"
            ),
            Self::Oversize { expected } => write!(
                f,
                "MPQ decompressed unit exceeds its declared size of {expected} bytes"
            ),
            Self::StaleIndex => write!(
                f,
                "MPQ index does not match this archive (block table changed)"
            ),
        }
    }
}

impl std::error::Error for MpqError {}

impl From<io::Error> for MpqError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// 0x500-entry crypt table shared by the hash and decrypt routines.
pub(crate) fn crypt_table() -> &'static [u32; 0x500] {
    static TABLE: OnceLock<[u32; 0x500]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        for i in 0..0x100usize {
            let mut index = i;
            for _ in 0..5 {
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2AAAAB;
                let high = (seed & 0xFFFF) << 16;
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2AAAAB;
                let low = seed & 0xFFFF;
                table[index] = high | low;
                index += 0x100;
            }
        }
        table
    })
}

/// MPQ string hash (`kind` 0/1/2 for table offset / name A / name B).
pub fn hash_string(name: &[u8], kind: u32) -> u32 {
    let table = crypt_table();
    let mut seed1: u32 = 0x7FED_7FED;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for &byte in name {
        let c = byte.to_ascii_uppercase() as usize;
        let value = table[((kind as usize) << 8) + c];
        seed1 = value ^ (seed1.wrapping_add(seed2));
        seed2 = (c as u32)
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
    seed1
}

fn decrypt_words(data: &mut [u32], key: u32) {
    let table = crypt_table();
    let mut seed1 = key;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for word in data.iter_mut() {
        seed2 = seed2.wrapping_add(table[0x400 + (seed1 & 0xFF) as usize]);
        let value = *word ^ seed1.wrapping_add(seed2);
        seed1 = ((!seed1 << 21).wrapping_add(0x1111_1111)) | (seed1 >> 11);
        seed2 = value
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
        *word = value;
    }
}

fn read_u32_slice(bytes: &[u8]) -> Vec<u32> {
    // `first_chunk` on an exact 4-byte chunk is total; no index can fail here.
    bytes
        .chunks_exact(4)
        .filter_map(|chunk| chunk.first_chunk::<4>().copied())
        .map(u32::from_le_bytes)
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Header {
    hash_table_pos: u32,
    block_table_pos: u32,
    hash_table_count: u32,
    block_table_count: u32,
    pub(crate) block_size: u16,
    /// A table (or both) extended past EOF and was cut to what the file holds.
    pub(crate) truncated_tables: bool,
    /// Archive is a Warcraft III map: the block flags are masked with
    /// [`MPQ_FILE_VALID_FLAGS_W3X`] exactly like StormLib does.
    pub(crate) wc3_map: bool,
}

/// One 16-byte hash-table entry: `name1`, `name2`, `wLocale` (low u16 of word
/// 2), `wPlatform` (high u16 of word 2), `block_index`.
#[derive(Debug, Clone, Copy)]
struct HashEntry {
    name1: u32,
    name2: u32,
    locale: u16,
    platform: u16,
    block_index: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockEntry {
    pub(crate) offset: u32,
    pub(crate) compressed_size: u32,
    pub(crate) size: u32,
    pub(crate) flags: u32,
}

pub struct MpqArchive {
    pub(crate) file: File,
    path: PathBuf,
    pub(crate) base: u64,
    pub(crate) header: Header,
    hash_table: Vec<HashEntry>,
    /// `hash_table_count - 1`; the declared count is a power of two, but a
    /// truncated table holds fewer entries than declared.
    hash_mask: u32,
    pub(crate) block_table: Vec<BlockEntry>,
    listfile: Vec<String>,
    by_name: HashMap<String, u32>,
    fingerprint: ArchiveFingerprint,
    /// Locale used to resolve hash entries, `0` (`LANG_NEUTRAL`) by default.
    ///
    /// Storm.dll keeps this in a global set by `SFileSetLocale` (ordinal 272,
    /// `0x15045784`) and applies it in the lookup at `0x1501b3b5..0x1501b3d0`:
    /// an entry whose `wLocale` is non-zero matches only when it equals the
    /// current locale. Warcraft III itself never calls `SFileSetLocale`
    /// (no reference to the import in either Game.dll build), and the shipped
    /// global is zero-initialised (`.data` `0x15045784 = 0`), so the engine
    /// reads neutral entries only.
    locale: u16,
    /// Platform filter; same rule and evidence at `0x1501b3bf..0x1501b3dc`
    /// (`0x15045786`, also zero-initialised).
    platform: u16,
    /// Bytes this handle has read from the archive file (`pread`), for the
    /// memory budget's "no archive is slurped" assertion.
    bytes_read: AtomicU64,
    /// Number of `pread` calls behind those bytes.
    read_calls: AtomicU64,
}

impl fmt::Debug for MpqArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MpqArchive")
            .field("path", &self.path)
            .field("blocks", &self.block_table.len())
            .field("listfile", &self.listfile.len())
            .finish()
    }
}

impl MpqArchive {
    /// Read exactly `buf.len()` bytes at `offset`, counting the transfer.
    ///
    /// Every archive read goes through here (`MpqFileReader` sectors, the hash
    /// and block tables): one `pread`, no whole-file slurp, no memory-mapped
    /// copy of a 300 MB archive.
    pub(crate) fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        use std::os::unix::fs::FileExt as _;
        self.bytes_read
            .fetch_add(buf.len() as u64, Ordering::Relaxed);
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        self.file.read_exact_at(buf, offset)
    }

    /// Bytes retained by a sector cache inside this reader.
    ///
    /// The reader has no sector cache: every request streams the sectors it
    /// needs and the per-file buffers die with the call, so the archive cannot
    /// hold a resident copy of itself (War3x.mpq is 287 MB). The budget in
    /// `analysis/h2/MEMORY_BUDGET.md` caps this at 16 MB; it is 0 and stays 0
    /// unless a cache is added here on purpose.
    pub fn storm_cache_bytes(&self) -> usize {
        0
    }

    /// `(bytes read from the archive file, pread calls)` since open.
    ///
    /// Proof that loading the install streams what it needs instead of
    /// slurping the archive: a full asset load reads a fraction of the file.
    pub fn bytes_read(&self) -> (u64, u64) {
        (
            self.bytes_read.load(Ordering::Relaxed),
            self.read_calls.load(Ordering::Relaxed),
        )
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, MpqError> {
        Self::open_at(path, 0)
    }

    /// Open an MPQ embedded at `base` (Warcraft III `.w3m` maps carry a 512-byte
    /// `HM3W` header before the MPQ data).
    pub fn open_at(path: impl AsRef<Path>, base: u64) -> Result<Self, MpqError> {
        Self::open_at_indexed(path, base, None)
    }

    /// Bytes of the container header scanned for an embedded archive.
    ///
    /// A classic RoC `.w3m` map puts the archive after its 512-byte `HM3W`
    /// header; 4 KiB also covers protected maps that pad the header. The same
    /// window and rule are used by `wc3_formats::map::map_mpq_base`.
    pub const CONTAINER_SCAN: usize = 4096;

    /// Archive base offset for a file header: `0` when `head` starts with the
    /// MPQ magic, the signature offset when `head` is a `.w3m` (`HM3W`) map
    /// container, `None` for anything else.
    pub fn mpq_base(head: &[u8]) -> Option<u64> {
        if head.starts_with(MPQ_MAGIC) {
            return Some(0);
        }
        if head.starts_with(b"HM3W") {
            return head
                .windows(MPQ_MAGIC.len())
                .position(|window| window == MPQ_MAGIC)
                .map(|offset| offset as u64);
        }
        None
    }

    /// Open a bare archive or a `.w3m` map container in one call.
    ///
    /// The install's `.mpq` files (`base = 0`) and the shipped campaign `.w3m`
    /// maps (`HM3W` header, archive at the first `MPQ\x1a` in the first 4 KiB)
    /// both go through here, so tooling does not re-derive the base per caller.
    /// A file with neither magic is `BadMagic` with its first four bytes; an
    /// `HM3W` container without a signature is also `BadMagic` (`HM3W`), the
    /// same typed error a wrong archive signature produces.
    pub fn open_any(path: impl AsRef<Path>) -> Result<Self, MpqError> {
        let path = path.as_ref();
        let mut head = [0u8; Self::CONTAINER_SCAN];
        let file = File::open(path)?;
        let read = file.read_at(&mut head, 0)?;
        let head = &head[..read];
        let base = Self::mpq_base(head).ok_or_else(|| {
            let mut magic = [0u8; 4];
            let taken = read.min(4);
            magic[..taken].copy_from_slice(&head[..taken]);
            MpqError::BadMagic(magic)
        })?;
        Self::open_at(path, base)
    }

    /// Open the archive using a prebuilt index: skips the listfile read and the
    /// hash probe for every name. The index must match the archive block table
    /// or [`MpqError::StaleIndex`] is returned.
    pub fn open_with_index(
        path: impl AsRef<Path>,
        index: &MpqIndex,
    ) -> Result<Self, MpqError> {
        Self::open_at_indexed(path, 0, Some(index))
    }

    fn open_at_indexed(
        path: impl AsRef<Path>,
        base: u64,
        index: Option<&MpqIndex>,
    ) -> Result<Self, MpqError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let file_size = file.metadata()?.len();
        let mut header_bytes = [0u8; 32];
        file.read_exact_at(&mut header_bytes, base)?;
        if header_bytes.first_chunk::<4>() != Some(&MPQ_MAGIC) {
            return Err(MpqError::BadMagic([
                header_bytes[0],
                header_bytes[1],
                header_bytes[2],
                header_bytes[3],
            ]));
        }
        // `header_bytes` is a `[u8; 32]` filled by `read_exact_at`; the field
        // accessors below use compile-time-checked constant indices, so a short
        // read is an `Err` from `read_exact_at` and never an index panic.
        let declared_version = u16::from_le_bytes([header_bytes[12], header_bytes[13]]);
        // The engine reads a v0 header and nothing else: `SFileOpenArchive`
        // checks `MPQ\x1a` plus `headerSize >= 0x20` and then uses the v1
        // fields, with no version dispatch (`0x1501bffc..0x1501c060`). A
        // declared v1 is therefore treated as v0 — StormLib documents the same
        // behaviour for Warcraft III maps ("ignores the format version value",
        // `SBaseFileTable.cpp:474-481`) — and a v2+ header, whose 64-bit table
        // offsets and HET/BET tables this reader does not implement, is
        // rejected explicitly instead of being misparsed.
        if declared_version > 1 {
            return Err(MpqError::UnsupportedVersion(declared_version));
        }
        // StormLib decides the map type from the extension or the `HM3W`
        // signature; both select the tighter Warcraft III flag mask.
        let mut magic = [0u8; 4];
        file.read_exact_at(&mut magic, 0)?;
        let wc3_map = magic == *b"HM3W"
            || path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "w3m" | "w3x" | "w3n"));
        let mut header = Header {
            hash_table_pos: u32::from_le_bytes([
                header_bytes[16],
                header_bytes[17],
                header_bytes[18],
                header_bytes[19],
            ]),
            block_table_pos: u32::from_le_bytes([
                header_bytes[20],
                header_bytes[21],
                header_bytes[22],
                header_bytes[23],
            ]),
            hash_table_count: u32::from_le_bytes([
                header_bytes[24],
                header_bytes[25],
                header_bytes[26],
                header_bytes[27],
            ]),
            block_table_count: u32::from_le_bytes([
                header_bytes[28],
                header_bytes[29],
                header_bytes[30],
                header_bytes[31],
            ]),
            block_size: u16::from_le_bytes([header_bytes[14], header_bytes[15]]),
            truncated_tables: false,
            wc3_map,
        };
        if header.hash_table_count == 0
            || header.block_table_count == 0
            || !header.hash_table_count.is_power_of_two()
            || header.hash_table_count > MAX_TABLE_ENTRIES
            || header.block_table_count > MAX_TABLE_ENTRIES
        {
            return Err(MpqError::Corrupt("invalid table sizes"));
        }
        if header.block_size & 0xFF00 != 0 {
            // "Only the low byte of sector size is really used" (StormLib).
            header.block_size &= 0xFF;
        }
        if header.block_size > 15 {
            return Err(MpqError::Corrupt("sector size shift out of range"));
        }

        // Tables may extend past EOF. Storm.dll reads as much as possible and
        // zero-fills the rest; protected maps rely on it (StormLib
        // `SBaseCommon.cpp:1054-1074`, `SBaseFileTable.cpp:585-590`).
        let load_table = |pos: u32,
                          count: u32,
                          key: &[u8]|
         -> Result<(Vec<u32>, bool), MpqError> {
            let declared = count as usize * 16;
            let start = base + pos as u64;
            if start >= file_size {
                return Err(MpqError::Corrupt("table offset past end of archive"));
            }
            let available = (file_size - start) as usize;
            let read_len = declared.min(available) & !3usize;
            let mut raw = vec![0u8; read_len];
            file.read_exact_at(&mut raw, start)?;
            let mut words = read_u32_slice(&raw);
            decrypt_words(&mut words, hash_string(key, 3));
            Ok((words, read_len < declared))
        };

        let (hash_words, hash_cut) =
            load_table(header.hash_table_pos, header.hash_table_count, b"(hash table)")?;
        let (block_words, block_cut) =
            load_table(header.block_table_pos, header.block_table_count, b"(block table)")?;
        header.truncated_tables = hash_cut || block_cut;

        let mut hash_table = Vec::with_capacity(hash_words.len() / 4);
        for entry in hash_words.chunks_exact(4) {
            hash_table.push(HashEntry {
                name1: entry[0],
                name2: entry[1],
                locale: entry[2] as u16,
                platform: (entry[2] >> 16) as u16,
                block_index: entry[3],
            });
        }
        let valid_flags = if header.wc3_map {
            MPQ_FILE_VALID_FLAGS_W3X
        } else {
            MPQ_FILE_VALID_FLAGS
        };
        let mut block_table = Vec::with_capacity(block_words.len() / 4);
        for entry in block_words.chunks_exact(4) {
            block_table.push(BlockEntry {
                offset: entry[0],
                compressed_size: entry[1],
                size: entry[2],
                flags: entry[3] & valid_flags,
            });
        }
        let block_headers: Vec<(u32, u32, u32, u32)> = block_table
            .iter()
            .map(|b| (b.offset, b.compressed_size, b.size, b.flags))
            .collect();
        let fingerprint =
            ArchiveFingerprint::compute(file_size, header.hash_table_count, &block_headers);

        let mut archive = Self {
            file,
            path,
            base,
            header,
            hash_table,
            hash_mask: header.hash_table_count - 1,
            block_table,
            listfile: Vec::new(),
            by_name: HashMap::new(),
            fingerprint,
            locale: 0,
            platform: 0,
            bytes_read: AtomicU64::new(0),
            read_calls: AtomicU64::new(0),
        };
        if let Some(index) = index {
            if index.fingerprint != Some(fingerprint) {
                return Err(MpqError::StaleIndex);
            }
            archive.listfile = dedup_names(index.entries().iter().map(|e| e.name.clone()));
            archive.by_name = index
                .entries()
                .iter()
                .map(|e| (e.name.clone(), e.block_index))
                .collect();
            return Ok(archive);
        }
        let listfile = archive
            .read_raw(b"(listfile)")
            .map(|(bytes, _)| bytes)
            .unwrap_or_default();
        archive.listfile = dedup_names(
            listfile
                .split(|b| *b == b'\r' || *b == b'\n' || *b == 0u8)
                .filter(|line| !line.is_empty())
                .map(|line| String::from_utf8_lossy(line).into_owned()),
        );
        for name in &archive.listfile {
            if let Some(index) = archive.find_block(name) {
                archive.by_name.insert(normalize_name(name), index);
            }
        }
        Ok(archive)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn listfile(&self) -> &[String] {
        &self.listfile
    }

    /// Locale the archive resolves hash entries with (`0` = neutral).
    pub fn locale(&self) -> u16 {
        self.locale
    }

    /// Select the locale used for name lookup, as `SFileSetLocale` does.
    ///
    /// The default is `0` (neutral): only entries whose `wLocale` is zero are
    /// visible, which is what the Warcraft III engine does (`0x1501b3b5`,
    /// zero-initialised global `0x15045784`). Passing a locale makes entries
    /// tagged with it resolvable as well; the highest-priority matching entry
    /// still wins in probe order.
    pub fn set_locale(&mut self, locale: u16) {
        if self.locale == locale {
            return;
        }
        self.locale = locale;
        self.rebuild_name_cache();
    }

    /// Platform the archive resolves hash entries with (`0` = neutral).
    pub fn platform(&self) -> u16 {
        self.platform
    }

    /// Select the platform filter (`0x1501b3bf..0x1501b3dc`, global
    /// `0x15045786`). Same semantics as [`MpqArchive::set_locale`].
    pub fn set_platform(&mut self, platform: u16) {
        if self.platform == platform {
            return;
        }
        self.platform = platform;
        self.rebuild_name_cache();
    }

    /// Re-resolve the `(listfile)` cache after a locale/platform change.
    fn rebuild_name_cache(&mut self) {
        self.by_name.clear();
        for name in &self.listfile {
            if let Some(index) = self.find_block(name) {
                self.by_name.insert(normalize_name(name), index);
            }
        }
    }

    pub fn block_count(&self) -> usize {
        self.block_table.len()
    }

    /// Uncompressed sector size (`512 << header.block_size`).
    pub fn sector_size(&self) -> usize {
        512usize << self.header.block_size
    }

    /// Identity of the block table; compare with [`MpqIndex::fingerprint`].
    pub fn fingerprint(&self) -> ArchiveFingerprint {
        self.fingerprint
    }

    /// True when the archive can resolve `name`.
    ///
    /// The `(listfile)` cache only covers names the archive lists; archives
    /// without one (every shipped WC3 map) resolve through the hash table, so
    /// the probe is the authoritative answer and the cache is a shortcut.
    pub fn contains(&self, name: &str) -> bool {
        let normalized = normalize_name(name);
        self.by_name.contains_key(&normalized) || self.find_block(&normalized).is_some()
    }

    fn resolve(&self, name: &str) -> Result<(u32, BlockEntry), MpqError> {
        let normalized = normalize_name(name);
        let block_index = self
            .by_name
            .get(&normalized)
            .copied()
            .or_else(|| self.find_block(&normalized))
            .ok_or_else(|| MpqError::FileNotFound(normalized.clone()))?;
        let block = *self
            .block_table
            .get(block_index as usize)
            .ok_or(MpqError::Corrupt("block index out of range"))?;
        Ok((block_index, block))
    }

    /// Block-table metadata for a file, without reading its payload.
    pub fn meta(&self, name: &str) -> Option<(u32, FileMeta)> {
        let (block_index, block) = self.resolve(name).ok()?;
        Some((
            block_index,
            FileMeta {
                block_index,
                size: block.size,
                compressed_size: block.compressed_size,
                flags: block.flags,
                mask: ((block.flags >> 8) & 0xFF) as u8,
            },
        ))
    }

    /// Iterate listfile names with their block metadata.
    pub fn files(&self) -> impl Iterator<Item = (&str, FileMeta)> + '_ {
        self.listfile
            .iter()
            .filter_map(move |name| self.meta(name).map(|(_, meta)| (name.as_str(), meta)))
    }

    /// Decryption key for a file (name hash, optionally fixed up by offset/size).
    pub(crate) fn file_key(&self, name: &str, block: &BlockEntry) -> u32 {
        let normalized = normalize_name(name);
        let base_name = normalized.rsplit(['\\', '/']).next().unwrap_or(&normalized);
        let mut key = hash_string(base_name.to_ascii_uppercase().as_bytes(), 3);
        if block.flags & MPQ_FILE_FIX_KEY != 0 {
            key = key.wrapping_add(block.offset) ^ block.size;
        }
        key
    }

    fn find_block(&self, name: &str) -> Option<u32> {
        let normalized = normalize_name(name);
        let upper = normalized.to_ascii_uppercase();
        let name1 = hash_string(upper.as_bytes(), 1);
        let name2 = hash_string(upper.as_bytes(), 2);
        let start = hash_string(upper.as_bytes(), 0);
        let mut index = (start & self.hash_mask) as usize;
        for _ in 0..=self.hash_mask as usize {
            let Some(entry) = self.hash_table.get(index).copied() else {
                // The table was cut at EOF (malformed/protected archive): the
                // missing tail behaves like a free entry.
                return None;
            };
            if entry.block_index == 0xFFFF_FFFF {
                // Empty slot terminates probing; deleted slots have the index set
                // to 0xFFFFFFFE and are skipped.
                return None;
            }
            if entry.name1 == name1 && entry.name2 == name2 {
                // Storm.dll 0x1501b3b5..0x1501b3dc: a non-neutral locale or
                // platform on the hash entry only matches when it equals the
                // archive's current value. Otherwise probing continues, so a
                // duplicate name stored under another locale is skipped
                // instead of being returned.
                let locale_ok = entry.locale == 0 || entry.locale == self.locale;
                let platform_ok = entry.platform == 0 || entry.platform == self.platform;
                if locale_ok && platform_ok {
                    let block = entry.block_index;
                    return ((block as usize) < self.block_table.len()).then_some(block);
                }
            }
            index = (index + 1) & self.hash_mask as usize;
        }
        None
    }

    /// True when the archive carries a parsed `(listfile)` with entries.
    ///
    /// WC3 maps frequently omit it; names are then resolved by hashing the
    /// requested path, and [`MpqArchive::discover_known_names`] recovers the
    /// canonical map files by probing them.
    pub fn has_listfile(&self) -> bool {
        !self.listfile.is_empty()
    }

    /// Block-table entries marked `MPQ_FILE_EXISTS` and not deleted: the files
    /// StormLib lists for the archive.
    pub fn existing_block_count(&self) -> usize {
        self.block_table
            .iter()
            .filter(|block| {
                block.flags & MPQ_FILE_EXISTS != 0 && block.flags & MPQ_FILE_DELETE_MARKER == 0
            })
            .count()
    }

    /// Existing block-table entries that no name in `(listfile)` or the
    /// probed-name set resolves to. Unnamed blocks can still be read by index
    /// ([`MpqArchive::read_block`]). Deleted/absent entries (`MPQ_FILE_EXISTS`
    /// clear) are not counted: StormLib omits them from file listings too.
    pub fn unnamed_block_count(&self) -> usize {
        let mut named = vec![false; self.block_table.len()];
        for index in self.by_name.values() {
            if let Some(slot) = named.get_mut(*index as usize) {
                *slot = true;
            }
        }
        self.block_table
            .iter()
            .enumerate()
            .filter(|(slot, block)| {
                !named[*slot]
                    && block.flags & MPQ_FILE_EXISTS != 0
                    && block.flags & MPQ_FILE_DELETE_MARKER == 0
            })
            .count()
    }

    /// True when the block/hash table extended past EOF and was cut to fit
    /// (Spazzler/NP_Protect protected maps).
    pub fn is_malformed(&self) -> bool {
        self.header.truncated_tables
    }

    /// Declared entries in the header versus the entries actually readable.
    pub fn table_counts(&self) -> (u32, usize, u32, usize) {
        (
            self.header.hash_table_count,
            self.hash_table.len(),
            self.header.block_table_count,
            self.block_table.len(),
        )
    }

    /// Probe candidate paths against the hash table and register every match.
    ///
    /// This is the listfile-less path: naming is a hash lookup, so a candidate
    /// list is enough to recover the files of a protected map whose `(listfile)`
    /// entry is missing. Returns the number of newly registered names.
    pub fn probe_names<I, S>(&mut self, candidates: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut added = 0;
        for candidate in candidates {
            let name = candidate.as_ref();
            let normalized = normalize_name(name);
            if self.by_name.contains_key(&normalized) {
                continue;
            }
            if let Some(index) = self.find_block(&normalized) {
                // A hash hit for a deleted/absent block is not a file StormLib
                // would list; keep the name map equal to the readable set.
                let readable = self
                    .block_table
                    .get(index as usize)
                    .is_some_and(|block| {
                        block.flags & MPQ_FILE_EXISTS != 0
                            && block.flags & MPQ_FILE_DELETE_MARKER == 0
                    });
                if readable {
                    self.by_name.insert(normalized, index);
                    added += 1;
                }
            }
        }
        added
    }

    /// Probe the canonical Warcraft III map and data file names.
    ///
    /// Used for archives without `(listfile)`; every name is a plain hash
    /// lookup, so absent files cost one hash each and no I/O.
    pub fn discover_known_names(&mut self) -> usize {
        self.probe_names(KNOWN_WC3_NAMES.iter().copied())
    }

    /// The StormLib pseudo-name for an unnamed block: `File%08d.<ext>`
    /// (`SBaseCommon.cpp:1713-1740`). The index is the block index, the
    /// extension is cosmetic.
    pub fn pseudo_name(block_index: u32, extension: &str) -> String {
        format!("File{block_index:08}.{extension}")
    }

    /// Parse a StormLib pseudo-name (`File########.ext`) back to a block index.
    pub fn parse_pseudo_name(name: &str) -> Option<u32> {
        let rest = name.strip_prefix("File").or_else(|| name.strip_prefix("file"))?;
        let (digits, ext) = rest.split_once('.')?;
        if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) || ext.is_empty() {
            return None;
        }
        digits.parse().ok()
    }

    /// Raw block-table entry `(offset, compressed_size, size, flags)` for a
    /// block index. Diagnostics only: reading still goes through `read`/`stream`.
    pub fn block_entry(&self, block_index: u32) -> Option<(u32, u32, u32, u32)> {
        self.block_table
            .get(block_index as usize)
            .map(|b| (b.offset, b.compressed_size, b.size, b.flags))
    }

    /// Streaming reader for one block, resolving its key the same way for named
    /// and nameless blocks.
    fn block_reader(&self, block_index: u32) -> Result<MpqFileReader<'_>, MpqError> {
        let block = *self
            .block_table
            .get(block_index as usize)
            .ok_or(MpqError::Corrupt("block index out of range"))?;
        let name = self
            .by_name
            .iter()
            .find(|(_, index)| **index == block_index)
            .map(|(name, _)| name.clone());
        match name {
            Some(name) => {
                self.check_readable(&name, block)?;
                let key = self.file_key(&name, &block);
                MpqFileReader::new(self, block, key, &name)
            }
            None => {
                if block.flags & MPQ_FILE_DELETE_MARKER != 0 || block.flags & MPQ_FILE_EXISTS == 0 {
                    return Err(MpqError::FileNotFound(format!("#{block_index}")));
                }
                let key = if block.flags & MPQ_FILE_ENCRYPTED != 0 {
                    crate::stream::detect_key_by_content(self, &block)?
                } else {
                    0
                };
                MpqFileReader::new(self, block, key, "")
            }
        }
    }

    /// Read a file by block index without a name: the read path for listfile-less
    /// and pseudo-named entries.
    ///
    /// Encrypted blocks need the file name to derive the key; when the index
    /// belongs to a named entry the name is used, otherwise the content-key
    /// detection used by StormLib is applied.
    pub fn read_block(&self, block_index: u32) -> Result<Vec<u8>, MpqError> {
        let block = *self
            .block_table
            .get(block_index as usize)
            .ok_or(MpqError::Corrupt("block index out of range"))?;
        let mut reader = self.block_reader(block_index)?;
        let mut out = Vec::with_capacity(block.size as usize);
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(err) => return Err(restore_sector_error(err)),
            }
        }
        Ok(out)
    }

    /// Read at most `len` bytes from the start of a block.
    ///
    /// Movie discovery only needs the container magic of a nameless block, and
    /// an archive can declare hundreds of 256 MiB blocks: decoding each one in
    /// full to inspect four bytes (WC3-MPQ-002) is unbounded work for a
    /// four-byte answer. The prefix read decompresses only the sectors it
    /// needs.
    pub fn read_block_prefix(&self, block_index: u32, len: usize) -> Result<Vec<u8>, MpqError> {
        let mut reader = self.block_reader(block_index)?;
        let mut out = Vec::with_capacity(len.min(64));
        let mut chunk = [0u8; 8192];
        while out.len() < len {
            let want = (len - out.len()).min(chunk.len());
            match reader.read(&mut chunk[..want]) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(err) => return Err(restore_sector_error(err)),
            }
        }
        Ok(out)
    }

    /// Read a file by its archive path (`/` and `\` are interchangeable).
    pub fn read(&self, name: &str) -> Result<Vec<u8>, MpqError> {
        self.read_raw(normalize_name(name).as_bytes()).map(|(bytes, _)| bytes)
    }

    /// Read a file and report the codec tags actually used per sector.
    pub fn read_with_info(&self, name: &str) -> Result<(Vec<u8>, CodecUsage), MpqError> {
        self.read_raw(normalize_name(name).as_bytes())
    }

    /// Read a file into an existing buffer (allocation-free when `out` already
    /// has capacity for the payload).
    pub fn read_into(&self, name: &str, out: &mut Vec<u8>) -> Result<(), MpqError> {
        let normalized = normalize_name(name);
        let (_, block) = self.resolve(&normalized)?;
        self.check_readable(&normalized, block)?;
        out.clear();
        if block.compressed_size == 0 || block.size == 0 {
            return Ok(());
        }
        out.reserve(block.size as usize);
        let key = self.file_key(&normalized, &block);
        let mut reader = MpqFileReader::new(self, block, key, &normalized)?;
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(err) => return Err(restore_sector_error(err)),
            }
        }
        Ok(())
    }

    /// Open a random-access reader; seeking and reading decode one sector at a
    /// time. This is the fast path for large files (music, terrain, models).
    pub fn stream(&self, name: &str) -> Result<MpqFileReader<'_>, MpqError> {
        let normalized = normalize_name(name);
        let (_, block) = self.resolve(&normalized)?;
        self.check_readable(&normalized, block)?;
        let key = self.file_key(&normalized, &block);
        MpqFileReader::new(self, block, key, &normalized)
    }

    fn check_readable(&self, normalized: &str, block: BlockEntry) -> Result<(), MpqError> {
        if std::env::var_os("H2_MPQ_DEBUG").is_some() {
            eprintln!(
                "MPQ_DEBUG offset={} csize={} size={} flags=0x{:08x}",
                block.offset, block.compressed_size, block.size, block.flags
            );
        }
        if block.flags & MPQ_FILE_DELETE_MARKER != 0 {
            return Err(MpqError::Deleted(normalized.to_string()));
        }
        if block.flags & MPQ_FILE_EXISTS == 0 {
            return Err(MpqError::FileNotFound(normalized.to_string()));
        }
        // Validate the declared sizes before any caller reserves a buffer:
        // `size` and `compressed_size` come straight from the block table and a
        // crafted entry can name 4 GiB. See [`MAX_FILE_SIZE`].
        check_block_sizes(block)?;
        Ok(())
    }

    fn read_raw(&self, name: &[u8]) -> Result<(Vec<u8>, CodecUsage), MpqError> {
        let name = String::from_utf8_lossy(name).into_owned();
        let normalized = normalize_name(&name);
        let (_, block) = self.resolve(&normalized)?;
        self.check_readable(&normalized, block)?;
        if block.compressed_size == 0 || block.size == 0 {
            return Ok((Vec::new(), CodecUsage::default()));
        }
        let key = self.file_key(&normalized, &block);
        let mut reader = MpqFileReader::new(self, block, key, &normalized)?;
        let mut out = Vec::with_capacity(block.size as usize);
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(err) => return Err(restore_sector_error(err)),
            }
        }
        Ok((out, reader.codecs()))
    }

    /// Decode one MPQ unit (a whole single-unit file or one sector).
    ///
    /// The dispatch is the one in the Warcraft III Storm.dll — the only engine
    /// that reads these archives:
    /// - neither `MPQ_FILE_COMPRESS` nor `MPQ_FILE_IMPLODE`: stored
    ///   (`0x15018a55`).
    /// - solo `MPQ_FILE_IMPLODE`: a tag-less PKWARE DCL stream expanded into
    ///   the engine's `0x3134`-byte work buffer (`0x15018a6f`, `0x15018b13`).
    /// - both bits set (`0x300`): the engine tests `(flags & 0xFF00) == 0x100`
    ///   and `== 0x200` (`0x15018a6f`, `0x15018a76`); both fail for `0x300`, so
    ///   the unit is copied verbatim at `0x15018b98` and no tag is consumed.
    /// - a unit whose raw length is not smaller than the expected output is
    ///   stored, not compressed (`0x15018a55..0x15018a5b`).
    /// - otherwise the first byte is a codec bit mask over the five handlers of
    ///   `SFileDecompress` (`0x1500f7c0`), applied in table order PKWARE, zlib,
    ///   Huffman, ADPCM-stereo, ADPCM-mono. Any other bit — bzip2 `0x10`,
    ///   sparse `0x20`, LZMA `0x12`, or an unassigned bit — has no handler in
    ///   either Storm.dll build and is reported as
    ///   [`MpqError::UnsupportedCompression`] instead of being guessed.
    pub(crate) fn decompress_into(
        &self,
        data: &[u8],
        flags: u32,
        expected: usize,
        declared_len: usize,
        out: &mut Vec<u8>,
        usage: &mut CodecUsage,
    ) -> Result<(), MpqError> {
        decompress_unit(data, flags, expected, declared_len, out, usage)
    }
}

/// Validate the declared sizes of one block-table entry against the hard
/// resource limits before anybody allocates for it.
///
/// `size` and `compressed_size` are untrusted `u32`s from the archive; the
/// block table itself is only 16 bytes per entry, so a hostile 16-byte entry
/// would otherwise be able to request a 4 GiB buffer.
fn check_block_sizes(block: BlockEntry) -> Result<(), MpqError> {
    let requested = (block.size as u64).max(block.compressed_size as u64);
    if requested > MAX_FILE_SIZE {
        return Err(MpqError::SizeLimitExceeded {
            requested,
            limit: MAX_FILE_SIZE,
        });
    }
    Ok(())
}

/// Decode one unit by flags and tag; see [`MpqArchive::decompress_into`].
fn decompress_unit(
    data: &[u8],
    flags: u32,
    expected: usize,
    declared_len: usize,
    out: &mut Vec<u8>,
    usage: &mut CodecUsage,
) -> Result<(), MpqError> {
    out.clear();
    // `expected` is the unit's declared output size and doubles as the hard
    // decode budget: no stage may append more than this. It is validated here
    // so a caller cannot pass an unchecked size from the block table.
    if expected as u64 > MAX_FILE_SIZE {
        return Err(MpqError::SizeLimitExceeded {
            requested: expected as u64,
            limit: MAX_FILE_SIZE,
        });
    }
    let compressed = flags & MPQ_FILE_COMPRESS != 0;
    let implode = flags & MPQ_FILE_IMPLODE != 0;
    if !compressed && !implode {
        out.extend_from_slice(&data[..data.len().min(expected)]);
        usage.merge(CodecUsage(CodecUsage::STORED));
        return Ok(());
    }
    if implode && !compressed {
        let bytes = pkware::explode_limited(data, expected).map_err(|err| match err {
            pkware::ExplodeError::SizeLimit => MpqError::Oversize {
                expected: expected as u64,
            },
            pkware::ExplodeError::Malformed => MpqError::Corrupt("PKWARE explode failed"),
        })?;
        out.extend_from_slice(&bytes[..bytes.len().min(expected)]);
        usage.merge(CodecUsage(CodecUsage::PKWARE));
        return Ok(());
    }
    if compressed && implode {
        // `0x300`: no engine handler is selected (see the doc above).
        out.extend_from_slice(&data[..data.len().min(expected)]);
        usage.merge(CodecUsage(CodecUsage::STORED));
        return Ok(());
    }
    if declared_len >= expected {
        // Stored sector inside a compressed file (StormLib reads it as-is).
        out.extend_from_slice(&data[..data.len().min(expected)]);
        usage.merge(CodecUsage(CodecUsage::STORED));
        return Ok(());
    }
    if data.is_empty() {
        // Zero-length sector: a sparse block of zeros.
        out.resize(expected, 0);
        usage.merge(CodecUsage(CodecUsage::ZERO));
        return Ok(());
    }
    let tag = data[0] as u32;
    let payload = &data[1..];
    decode_v0_mask(tag, payload, expected, out, usage)?;
    // Symmetric size check: the declared size is a hard limit in both
    // directions. A short unit is tolerated only for the stored fallback
    // (`out == payload`, i.e. the codec declined and the bytes were copied);
    // an over-long unit is never valid and used to be accepted silently.
    if out.len() > expected {
        return Err(MpqError::Oversize {
            expected: expected as u64,
        });
    }
    if out.len() < expected && out.len() != payload.len() {
        return Err(MpqError::Corrupt(
            "decompressed sector shorter than expected",
        ));
    }
    Ok(())
}

/// Mask of every codec bit the Warcraft III Storm.dll implements.
///
/// Both builds carry exactly five `(mask, handler)` slots and stop at the same
/// table bound (`cmp edi, 0x1503e330` + `jge` in RoC, `0x15046360` in TFT), so
/// bzip2 `0x10` and sparse `0x20` do not exist on the read path even though
/// later StormLib versions implement them.
pub const MPQ_CODEC_MASK_WC3: u32 = MPQ_COMPRESSION_PKWARE
    | MPQ_COMPRESSION_ZLIB
    | MPQ_COMPRESSION_HUFFMANN
    | MPQ_COMPRESSION_ADPCM_STEREO
    | MPQ_COMPRESSION_ADPCM_MONO;

/// v0 archives: the tag is a bit mask; decoders run in `SFileDecompress` table
/// order (PKWARE, zlib, Huffman, ADPCM-stereo, ADPCM-mono).
fn decode_v0_mask(
    tag: u32,
    payload: &[u8],
    expected: usize,
    out: &mut Vec<u8>,
    usage: &mut CodecUsage,
) -> Result<(), MpqError> {
    const TABLE: [(u32, u16); 5] = [
        (MPQ_COMPRESSION_PKWARE, CodecUsage::PKWARE),
        (MPQ_COMPRESSION_ZLIB, CodecUsage::ZLIB),
        (MPQ_COMPRESSION_HUFFMANN, CodecUsage::HUFFMAN),
        (MPQ_COMPRESSION_ADPCM_STEREO, CodecUsage::ADPCM_STEREO),
        (MPQ_COMPRESSION_ADPCM_MONO, CodecUsage::ADPCM_MONO),
    ];
    if tag & !MPQ_CODEC_MASK_WC3 != 0 {
        // bzip2 (0x10), sparse (0x20), LZMA (0x12) and unassigned bits have no
        // handler in the engine; guessing one would corrupt the output.
        return Err(MpqError::UnsupportedCompression(tag));
    }
    let mut current: Vec<u8> = payload.to_vec();
    for (bit, usage_bit) in TABLE {
        if tag & bit == 0 {
            continue;
        }
        usage.merge(CodecUsage(usage_bit));
        current = decode_one(bit, &current, expected)?;
    }
    out.extend_from_slice(&current);
    Ok(())
}

/// Decode a single codec stage.
///
/// Every Warcraft III codec is size-agnostic (the stream carries no output
/// length), so `expected` — the unit's declared output size from the block
/// table — is enforced as a hard budget: each decoder stops at `expected + 1`
/// bytes and the stage is rejected with [`MpqError::Oversize`] if it wants
/// more. Without that cap a 26-byte zlib stream can inflate to 4 KiB (or a
/// few MiB to gigabytes) and grow the process past the declared size.
fn decode_one(codec: u32, payload: &[u8], expected: usize) -> Result<Vec<u8>, MpqError> {
    match codec {
        MPQ_COMPRESSION_ZLIB => {
            // `take(expected + 1)`: `read_to_end` can never hand the decoder
            // more than one byte past the declared size, so the failure mode is
            // a typed error, not an unbounded allocation.
            let budget = (expected as u64).saturating_add(1);
            let mut decoder = flate2::read::ZlibDecoder::new(payload).take(budget);
            let mut out = Vec::with_capacity(expected.min(64 * 1024));
            if let Err(err) = decoder.read_to_end(&mut out) {
                if out.is_empty() {
                    // Sectors that do not compress are stored verbatim.
                    return Ok(payload.to_vec());
                }
                return Err(MpqError::Io(err));
            }
            if out.len() > expected {
                return Err(MpqError::Oversize {
                    expected: expected as u64,
                });
            }
            Ok(out)
        }
        MPQ_COMPRESSION_PKWARE => pkware::explode_limited(payload, expected).map_err(|err| {
            match err {
                pkware::ExplodeError::SizeLimit => MpqError::Oversize {
                    expected: expected as u64,
                },
                pkware::ExplodeError::Malformed => MpqError::Corrupt("PKWARE explode failed"),
            }
        }),
        MPQ_COMPRESSION_HUFFMANN => huffman::decompress_limited(payload, expected).map_err(
            |err| match err {
                huffman::HuffmanError::SizeLimit => MpqError::Oversize {
                    expected: expected as u64,
                },
                huffman::HuffmanError::Malformed => MpqError::Corrupt("Huffman decode failed"),
            },
        ),
        // ADPCM output is derived from the input length (at most one i16 per
        // payload byte plus one header sample per channel), so it cannot
        // amplify; the caller still checks the result against `expected`.
        MPQ_COMPRESSION_ADPCM_MONO => adpcm::decompress(payload, 1),
        MPQ_COMPRESSION_ADPCM_STEREO => adpcm::decompress(payload, 2),
        other => Err(MpqError::UnsupportedCompression(other)),
    }
}

/// Sector readers report `io::Error`; the public error type wants a structured
/// [`MpqError`]. `MpqFileReader::read` wraps the original error inside the
/// `io::Error`, so the typed cause (`Oversize`, `SizeLimitExceeded`,
/// `Corrupt`) survives the `Read` boundary; anything else is mapped to the
/// same fixed message this crate used before.
fn restore_sector_error(err: io::Error) -> MpqError {
    let kind = err.kind();
    match err.into_inner() {
        Some(inner) => match inner.downcast::<MpqError>() {
            Ok(mpq) => *mpq,
            Err(_) => MpqError::Corrupt(leak_kind(kind)),
        },
        None => MpqError::Corrupt(leak_kind(kind)),
    }
}

fn leak_kind(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::UnexpectedEof => "archive ended inside a sector",
        io::ErrorKind::InvalidData => "corrupt sector payload",
        _ => "sector read failed",
    }
}



pub(crate) fn normalize_name(name: &str) -> String {
    name.replace('/', "\\")
}

/// Case-insensitive dedup under MPQ naming rules, keeping the first spelling.
///
/// A `(listfile)` is a plain name table and hostile/broken archives repeat or
/// case-vary entries. MPQ resolves names case-insensitively (the hash walk
/// upper-cases, `SFileOpenFileEx` is case-insensitive), so two names that
/// differ only in case can never resolve to different blocks. Deduping once at
/// ingestion keeps every consumer linear in the number of distinct names — the
/// `MapBundle::open` cross-check is O(N^2) on duplicates (WC3-I-021).
fn dedup_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in names {
        if seen.insert(normalize_name(&name).to_ascii_uppercase()) {
            out.push(name);
        }
    }
    out
}

pub(crate) fn decrypt_bytes(data: &mut [u8], key: u32) {
    // In-place: no staging allocation, so the streaming reader decrypts a
    // sector without touching the heap.
    let table = crypt_table();
    let mut seed1 = key;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for chunk in data.chunks_exact_mut(4) {
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        seed2 = seed2.wrapping_add(table[0x400 + (seed1 & 0xFF) as usize]);
        let value = word ^ seed1.wrapping_add(seed2);
        seed1 = ((!seed1 << 21).wrapping_add(0x1111_1111)) | (seed1 >> 11);
        seed2 = value
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Option<PathBuf> {
        let path = std::env::var_os("WC3_MPQ")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
            });
        path.is_file().then_some(path)
    }

    /// The engine implements exactly five codecs (`0x01`, `0x02`, `0x08`,
    /// `0x40`, `0x80`) and rejects every other tag bit.
    #[test]
    fn codec_tags_match_stormlib_dispatch() {
        use std::io::Write as _;
        let plain = vec![b'm'; 512];
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&plain).unwrap();
        let zlib = encoder.finish().unwrap();
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decode_v0_mask(MPQ_COMPRESSION_ZLIB, &zlib, plain.len(), &mut out, &mut usage).unwrap();
        assert_eq!(out, plain);
        assert!(usage.has(CodecUsage::ZLIB));

        // 0x41 is an ordinary mask combination (Huffman, then ADPCM mono),
        // inside the engine mask; only the decoders reject a garbage payload.
        assert_eq!(MPQ_COMPRESSION_HUFFMANN | MPQ_COMPRESSION_ADPCM_MONO, 0x41);
        assert_eq!(0x41 & !MPQ_CODEC_MASK_WC3, 0);

        // Tags the engine has no handler for: unassigned 0x04, bzip2, sparse
        // and LZMA all come back as an explicit unsupported-codec error.
        for tag in [
            0x04u32,
            MPQ_COMPRESSION_BZIP2,
            MPQ_COMPRESSION_SPARSE,
            MPQ_COMPRESSION_LZMA,
        ] {
            let mut out = Vec::new();
            let mut usage = CodecUsage::default();
            assert!(
                matches!(
                    decode_v0_mask(tag, &[0; 16], 16, &mut out, &mut usage),
                    Err(MpqError::UnsupportedCompression(reported)) if reported == tag
                ),
                "tag 0x{tag:x} must be rejected"
            );
        }
    }

    /// `IMPLODE | COMPRESS` (`0x300`) has no engine handler: the unit is
    /// copied verbatim and no tag byte is consumed (`0x15018a6f`/`0x15018a76`
    /// fail their equality tests, `0x15018b98` copies raw).
    #[test]
    fn implode_compress_300_copies_raw_without_a_tag() {
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(
            &[0x02, 0xDE, 0xAD, 0xBE, 0xEF],
            MPQ_FILE_IMPLODE | MPQ_FILE_COMPRESS,
            64,
            5,
            &mut out,
            &mut usage,
        )
        .unwrap();
        assert_eq!(out, vec![0x02, 0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(usage.has(CodecUsage::STORED));
        assert!(!usage.has(CodecUsage::ZLIB));
    }

    #[test]
    fn stored_and_zero_sectors_are_detected() {
        // A unit whose declared length equals the expected output is stored.
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(
            b"plain",
            MPQ_FILE_COMPRESS | (MPQ_COMPRESSION_ZLIB << 8),
            5,
            5,
            &mut out,
            &mut usage,
        )
        .unwrap();
        assert_eq!(out, b"plain");
        assert!(usage.has(CodecUsage::STORED));

        // Zero-length compressed unit: a sparse block of zeros.
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(
            &[],
            MPQ_FILE_COMPRESS | (MPQ_COMPRESSION_ZLIB << 8),
            64,
            0,
            &mut out,
            &mut usage,
        )
        .unwrap();
        assert_eq!(out, vec![0u8; 64]);
        assert!(usage.has(CodecUsage::ZERO));

        // A zlib sector: tag byte 0x02 + deflate stream. Compressed data must
        // be smaller than the expected output for the tag to apply.
        use std::io::Write as _;
        let plain = vec![b'm'; 512];
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&plain).unwrap();
        let mut sector = vec![MPQ_COMPRESSION_ZLIB as u8];
        sector.extend_from_slice(&encoder.finish().unwrap());
        assert!(sector.len() < plain.len());
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(
            &sector,
            MPQ_FILE_COMPRESS | (MPQ_COMPRESSION_ZLIB << 8),
            plain.len(),
            sector.len(),
            &mut out,
            &mut usage,
        )
        .unwrap();
        assert_eq!(out, plain);
        assert!(usage.has(CodecUsage::ZLIB));
    }

    #[test]
    fn pseudo_names_round_trip() {
        for index in [0u32, 7, 103, 4_000_000] {
            let name = MpqArchive::pseudo_name(index, "blp");
            assert_eq!(MpqArchive::parse_pseudo_name(&name), Some(index));
        }
        assert_eq!(MpqArchive::parse_pseudo_name("war3map.w3e"), None);
        assert_eq!(MpqArchive::parse_pseudo_name("File123.blp"), None);
    }

    #[test]
    fn hash_is_stable() {
        // Values pinned from the reference algorithm (used as decryption keys).
        assert_eq!(hash_string(b"(hash table)", 3), 0xC3AF_3770);
        assert_eq!(hash_string(b"(block table)", 3), 0xEC83_B3A3);
    }

    #[test]
    fn reads_real_archive_files() {
        let Some(path) = fixture() else {
            eprintln!("skipping: WC3_MPQ fixture not available");
            return;
        };
        let archive = MpqArchive::open(path).expect("open archive");
        assert!(archive.block_count() > 10_000);
        assert!(archive.listfile().len() > 1_000);

        let mdx = archive
            .read("Units/Human/Footman/Footman.mdx")
            .expect("Footman.mdx");
        assert_eq!(&mdx[..4], b"MDLX");
        assert_eq!(mdx.len(), 105_022);

        let blp = archive.read("Textures/Footman.blp").expect("Footman.blp");
        assert_eq!(&blp[..4], b"BLP1");
        assert_eq!(blp.len(), 98_795);

        let slk = archive.read("Units/UnitData.slk").expect("UnitData.slk");
        assert!(slk.starts_with(b"ID;PWXL"));

        let listfile = archive.read("(listfile)").expect("listfile");
        assert!(listfile.len() > 100_000);
    }

    #[test]
    fn missing_file_is_reported() {
        let Some(path) = fixture() else {
            return;
        };
        let archive = MpqArchive::open(path).expect("open");
        assert!(matches!(
            archive.read("no/such/file.xyz"),
            Err(MpqError::FileNotFound(_))
        ));
    }

    /// `memory.mpq`: the archive is streamed, never slurped.
    ///
    /// War3x.mpq is 287 MB and War3.mpq 418 MB; holding a copy of either (or of
    /// the whole block table's payload) would blow the 350 MB budget on its own.
    /// The reader opens the file, `pread`s the sectors a request needs and keeps
    /// no sector cache, so the counter must stay near the bytes actually asked
    /// for and `storm_cache_bytes` must be 0 (budget: <= 16 MB).
    #[test]
    fn archive_is_streamed_and_caches_nothing() {
        let Some(path) = fixture() else {
            return;
        };
        let size = std::fs::metadata(&path).expect("metadata").len();
        let archive = MpqArchive::open(&path).expect("open");
        let (after_open, _) = archive.bytes_read();
        assert!(
            after_open < size / 4,
            "open read {after_open} B of a {size} B archive (header + tables only)"
        );
        let names: Vec<String> = archive
            .files()
            .map(|(name, _)| name.to_string())
            .take(200)
            .collect();
        assert!(names.len() >= 200, "listfile has enough entries");
        let mut asked = 0u64;
        let mut read = 0u64;
        for name in &names {
            if let Ok(bytes) = archive.read(name) {
                asked += bytes.len() as u64;
                read += 1;
            }
        }
        assert!(read >= 150, "most listfile entries resolve ({read}/200)");
        let (total, calls) = archive.bytes_read();
        // Every payload byte plus the per-file sector table/log: a slurp would
        // be `size` (418 MB), not `asked`.
        assert!(
            total <= asked + 2 * 1024 * 1024,
            "read {total} B for {asked} B of payload over {calls} preads"
        );
        assert!(
            total < size,
            "reading 200 files ({total} B) must stay below the archive size ({size} B)"
        );
        assert!(
            archive.storm_cache_bytes() <= 16 * 1024 * 1024,
            "sector cache {} B exceeds the 16 MB budget",
            archive.storm_cache_bytes()
        );
        assert_eq!(archive.storm_cache_bytes(), 0, "no sector cache exists");
        println!(
            "MPQ_STREAM archive_mb={:.1} read_mb={:.1} payload_mb={:.1} preads={calls} cache_bytes={}",
            size as f64 / (1024.0 * 1024.0),
            total as f64 / (1024.0 * 1024.0),
            asked as f64 / (1024.0 * 1024.0),
            archive.storm_cache_bytes()
        );
    }

    // ---------------------------------------------------------------------
    // AUD-003 — decompression output is bounded by the declared size.
    // ---------------------------------------------------------------------

    /// Compress with the same zlib flavour Storm uses (RFC 1950 wrapper).
    fn zlib(plain: &[u8]) -> Vec<u8> {
        use std::io::Write as _;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(plain).unwrap();
        encoder.finish().unwrap()
    }

    /// A v0 compression unit: the codec bit mask byte followed by the payload.
    fn unit(tag: u32, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![tag as u8];
        data.extend_from_slice(payload);
        data
    }

    fn decode(data: &[u8], expected: usize) -> Result<Vec<u8>, MpqError> {
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(data, MPQ_FILE_COMPRESS, expected, data.len(), &mut out, &mut usage)?;
        Ok(out)
    }

    #[test]
    fn aud003_zlib_output_exactly_expected_is_accepted() {
        let plain = vec![b'm'; 512];
        let sector = unit(MPQ_COMPRESSION_ZLIB, &zlib(&plain));
        assert_eq!(decode(&sector, plain.len()).expect("exact size"), plain);

        // The last sector of a file is shorter than the sector size; the unit
        // is still accepted when its output matches the declared remainder.
        let tail = vec![b't'; 137];
        let sector = unit(MPQ_COMPRESSION_ZLIB, &zlib(&tail));
        assert_eq!(decode(&sector, tail.len()).expect("short tail"), tail);
    }

    #[test]
    fn aud003_zlib_output_one_byte_over_declared_is_rejected() {
        let plain = vec![b'x'; 4096];
        let sector = unit(MPQ_COMPRESSION_ZLIB, &zlib(&plain));
        // Declare one byte less than the stream actually contains.
        let err = decode(&sector, plain.len() - 1).expect_err("expected+1 must fail");
        assert!(
            matches!(err, MpqError::Oversize { expected } if expected == plain.len() as u64 - 1),
            "unexpected error: {err}"
        );
        // One byte over is the boundary: the same stream with the correct size
        // still decodes.
        assert_eq!(decode(&sector, plain.len()).unwrap().len(), plain.len());
    }

    #[test]
    fn aud003_zlib_output_far_over_declared_is_rejected() {
        // 32 MiB of zeros compress to a few hundred KiB: a ~100x amplification
        // that a crafted sector can pair with a tiny declared size. The decoder
        // must stop at the declared size (`take(expected + 1)`) instead of
        // materialising the whole expansion.
        let huge = vec![0u8; 32 * 1024 * 1024];
        let compressed = zlib(&huge);
        assert!(
            compressed.len() < 512 * 1024,
            "zlib stream is {} B for {} B of input",
            compressed.len(),
            huge.len()
        );
        let sector = unit(MPQ_COMPRESSION_ZLIB, &compressed);
        // The declared size stays above the compressed unit (so the unit is a
        // real decode, not a stored sector) and far below the 32 MiB expansion.
        let expected = 1024 * 1024;
        let err = decode(&sector, expected).expect_err("far oversize must fail");
        assert!(
            matches!(err, MpqError::Oversize { expected: reported } if reported as usize == expected),
            "unexpected error: {err}"
        );
        // The same stream with the correct declared size is accepted, so the
        // rejection is about the declaration, not about the data.
        let decoded = decode(&sector, huge.len()).expect("declared size honoured");
        assert_eq!(decoded.len(), huge.len());
        assert!(decoded.iter().all(|b| *b == 0));
    }

    #[test]
    fn aud003_multi_codec_chain_decodes_each_stage_within_budget() {
        // A real WC3 chain shape: zlib, then ADPCM-mono (the archive census
        // reads `zlib+adpcm-mono+huffman` files). The zlib stream expands the
        // ADPCM payload, which the ADPCM stage turns into PCM.
        let mut adpcm_stream = vec![0u8; 6];
        adpcm_stream[1] = 0; // shift 0
        adpcm_stream.extend((0..4096).map(|i| (i % 0x7f) as u8));
        let pcm = adpcm::decompress(&adpcm_stream, 1).expect("standalone decode");
        let sector = unit(
            MPQ_COMPRESSION_ZLIB | MPQ_COMPRESSION_ADPCM_MONO,
            &zlib(&adpcm_stream),
        );
        let chained = decode(&sector, pcm.len()).expect("chain decodes");
        assert_eq!(chained, pcm, "chain output matches the standalone decoder");
        assert_eq!(chained.len(), pcm.len());

        // Any stage that would exceed the declared output is rejected. The
        // declared size must stay below the unit length so the stored-sector
        // fast path does not apply.
        let err = decode(&sector, pcm.len() - 1).expect_err("chain oversize");
        assert!(matches!(err, MpqError::Oversize { .. }), "{err}");
        let err = decode(&sector, 2048).expect_err("chain far oversize");
        assert!(matches!(err, MpqError::Oversize { .. }), "{err}");
    }

    #[test]
    fn aud003_malformed_mask_is_rejected_before_decoding() {
        // Bits outside the five WC3 handlers (bzip2 0x10 here) must fail the
        // whole unit up front; the zlib bit alone would otherwise decode.
        let plain = vec![b'a'; 4096];
        let sector = unit(MPQ_COMPRESSION_ZLIB | MPQ_COMPRESSION_BZIP2, &zlib(&plain));
        assert!(sector.len() < plain.len(), "unit must be a real decode");
        let err = decode(&sector, plain.len()).expect_err("mask with bzip2 must fail");
        assert!(
            matches!(err, MpqError::UnsupportedCompression(tag)
                if tag == MPQ_COMPRESSION_ZLIB | MPQ_COMPRESSION_BZIP2),
            "unexpected error: {err}"
        );
        // The same unit without the unsupported bit decodes.
        let ok = unit(MPQ_COMPRESSION_ZLIB, &zlib(&plain));
        assert_eq!(decode(&ok, plain.len()).unwrap(), plain);
    }

    #[test]
    fn aud003_stored_sector_fallback_is_preserved() {
        // A sector whose compressed length is not smaller than the declared
        // output is stored verbatim (declared_len >= expected).
        let plain = b"not compressed at all";
        let out = decode(plain, plain.len()).expect("stored sector must read");
        assert_eq!(out, plain);

        // Non-zlib bytes behind a zlib tag with a larger declared size: the
        // decoder declines immediately and the payload is copied verbatim
        // (StormLib's stored fallback; the unit ends up `out == payload`).
        let garbage = unit(MPQ_COMPRESSION_ZLIB, b"plain bytes, not deflate");
        let out = decode(&garbage, 40).expect("stored fallback");
        assert_eq!(out, b"plain bytes, not deflate");
    }

    #[test]
    fn aud003_zero_sector_is_sparse_without_allocation_above_the_declaration() {
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        decompress_unit(&[], MPQ_FILE_COMPRESS, 8192, 0, &mut out, &mut usage)
            .expect("zero sector");
        assert_eq!(out, vec![0u8; 8192]);
        assert!(usage.has(CodecUsage::ZERO));
    }

    #[test]
    fn aud003_declared_size_above_the_cap_is_rejected() {
        // The declared-size guard rejects before any decode/allocation, so the
        // unit is refused with a typed error rather than driving an OOM.
        let mut out = Vec::new();
        let mut usage = CodecUsage::default();
        let err = decompress_unit(
            b"x",
            MPQ_FILE_COMPRESS,
            (MAX_FILE_SIZE + 1) as usize,
            1,
            &mut out,
            &mut usage,
        )
        .expect_err("over-cap unit must be rejected");
        assert!(
            matches!(err, MpqError::SizeLimitExceeded { requested, limit }
                if requested == MAX_FILE_SIZE + 1 && limit == MAX_FILE_SIZE),
            "unexpected error: {err}"
        );

        // Block-table guard: exactly at the cap is allowed, one byte over is
        // not (no allocation happens in either call).
        let entry = |size: u32| BlockEntry {
            offset: 0,
            compressed_size: size,
            size,
            flags: MPQ_FILE_EXISTS,
        };
        check_block_sizes(entry(MAX_FILE_SIZE as u32)).expect("cap boundary is legal");
        assert!(matches!(
            check_block_sizes(entry(MAX_FILE_SIZE as u32 + 1)),
            Err(MpqError::SizeLimitExceeded { .. })
        ));
    }

    // ------------------------- synthetic archives -------------------------

    struct TestEntry {
        name: Option<&'static str>,
        data: Vec<u8>,
        size: u32,
        compressed_size: u32,
        flags: u32,
    }

    /// Encrypt one MPQ table (`decrypt_words` inverted): the same seed walk
    /// with the plaintext word producing the ciphertext.
    fn encrypt_words(data: &mut [u32], key: u32) {
        let table = crypt_table();
        let mut seed1 = key;
        let mut seed2: u32 = 0xEEEE_EEEE;
        for word in data.iter_mut() {
            seed2 = seed2.wrapping_add(table[0x400 + (seed1 & 0xFF) as usize]);
            let plain = *word;
            *word = plain ^ seed1.wrapping_add(seed2);
            seed1 = ((!seed1 << 21).wrapping_add(0x1111_1111)) | (seed1 >> 11);
            seed2 = plain
                .wrapping_add(seed2)
                .wrapping_add(seed2 << 5)
                .wrapping_add(3);
        }
    }

    /// Minimal MPQ v0 writer: header, encrypted hash/block tables, payloads.
    /// Test-only; it exists to drive the reader through real archive layouts
    /// (sector tables, stored sectors, over-cap block entries).
    fn build_archive(sector_shift: u16, entries: &[TestEntry]) -> Vec<u8> {
        let hash_count: u32 = 16;
        let hash_pos = 32u32;
        let block_pos = hash_pos + hash_count * 16;
        let data_pos = block_pos + entries.len() as u32 * 16;
        let mut cursor = data_pos;
        let mut offsets = Vec::with_capacity(entries.len());
        for entry in entries {
            offsets.push(cursor);
            cursor = cursor
                .checked_add(entry.data.len() as u32)
                .expect("test archive too large");
        }

        let mut hash = vec![0u32; (hash_count * 4) as usize];
        for slot in 0..hash_count as usize {
            hash[slot * 4 + 3] = 0xFFFF_FFFF; // empty slot marker
        }
        for (index, entry) in entries.iter().enumerate() {
            let Some(name) = entry.name else { continue };
            let upper = name.to_ascii_uppercase();
            let mut slot = (hash_string(upper.as_bytes(), 0) & (hash_count - 1)) as usize;
            while hash[slot * 4 + 3] != 0xFFFF_FFFF {
                slot = (slot + 1) % hash_count as usize;
            }
            hash[slot * 4] = hash_string(upper.as_bytes(), 1);
            hash[slot * 4 + 1] = hash_string(upper.as_bytes(), 2);
            hash[slot * 4 + 2] = 0; // locale + platform neutral
            hash[slot * 4 + 3] = index as u32;
        }

        let mut block = vec![0u32; entries.len() * 4];
        for (index, entry) in entries.iter().enumerate() {
            block[index * 4] = offsets[index];
            block[index * 4 + 1] = entry.compressed_size;
            block[index * 4 + 2] = entry.size;
            block[index * 4 + 3] = entry.flags;
        }

        let mut out = vec![0u8; cursor as usize];
        out[0..4].copy_from_slice(MPQ_MAGIC);
        out[4..8].copy_from_slice(&32u32.to_le_bytes());
        out[8..12].copy_from_slice(&(cursor).to_le_bytes());
        out[12..14].copy_from_slice(&0u16.to_le_bytes());
        out[14..16].copy_from_slice(&sector_shift.to_le_bytes());
        out[16..20].copy_from_slice(&hash_pos.to_le_bytes());
        out[20..24].copy_from_slice(&block_pos.to_le_bytes());
        out[24..28].copy_from_slice(&hash_count.to_le_bytes());
        out[28..32].copy_from_slice(&(entries.len() as u32).to_le_bytes());

        encrypt_words(&mut hash, hash_string(b"(hash table)", 3));
        encrypt_words(&mut block, hash_string(b"(block table)", 3));
        for (i, word) in hash.iter().enumerate() {
            out[(hash_pos as usize + i * 4)..(hash_pos as usize + i * 4 + 4)]
                .copy_from_slice(&word.to_le_bytes());
        }
        for (i, word) in block.iter().enumerate() {
            out[(block_pos as usize + i * 4)..(block_pos as usize + i * 4 + 4)]
                .copy_from_slice(&word.to_le_bytes());
        }
        for (index, entry) in entries.iter().enumerate() {
            let start = offsets[index] as usize;
            out[start..start + entry.data.len()].copy_from_slice(&entry.data);
        }
        out
    }

    /// Compress `plain` into MPQ sectors (storing a sector when compression
    /// does not shrink it), returning the block payload and declared size.
    fn zlib_sectors(plain: &[u8], sector_size: usize) -> (Vec<u8>, u32) {
        let count = plain.len().div_ceil(sector_size);
        let mut sectors: Vec<Vec<u8>> = Vec::with_capacity(count);
        for index in 0..count {
            let start = index * sector_size;
            let end = (start + sector_size).min(plain.len());
            let chunk = &plain[start..end];
            let unit = unit(MPQ_COMPRESSION_ZLIB, &zlib(chunk));
            sectors.push(if unit.len() < chunk.len() {
                unit
            } else {
                chunk.to_vec()
            });
        }
        let table_bytes = (count + 1) * 4;
        let mut data = vec![0u8; table_bytes];
        let mut offset = table_bytes;
        for (index, sector) in sectors.iter().enumerate() {
            data[index * 4..index * 4 + 4].copy_from_slice(&(offset as u32).to_le_bytes());
            data.extend_from_slice(sector);
            offset += sector.len();
        }
        data[count * 4..count * 4 + 4].copy_from_slice(&(offset as u32).to_le_bytes());
        (data, plain.len() as u32)
    }

    fn open_archive(tag: &str, bytes: &[u8]) -> (PathBuf, MpqArchive) {
        let path = std::env::temp_dir().join(format!(
            "wc3-mpq-audit-{}-{tag}.mpq",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("write test archive");
        let archive = MpqArchive::open(&path).expect("open test archive");
        (path, archive)
    }

    #[test]
    fn aud003_oversize_sector_in_a_real_archive_is_a_typed_error() {
        // One 4096-byte sector is declared, but its zlib stream really holds
        // 8192 bytes. The archive must report `Oversize`, not return a corrupt
        // buffer.
        let sector = unit(MPQ_COMPRESSION_ZLIB, &zlib(&vec![b'a'; 8192]));
        let mut data = vec![0u8; 8];
        data[0..4].copy_from_slice(&8u32.to_le_bytes());
        data[4..8].copy_from_slice(&(8 + sector.len() as u32).to_le_bytes());
        data.extend_from_slice(&sector);
        let entries = vec![TestEntry {
            name: Some("oversize.bin"),
            compressed_size: data.len() as u32,
            data,
            size: 4096,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_COMPRESS,
        }];
        let bytes = build_archive(3, &entries);
        let (path, archive) = open_archive("oversize-sector", &bytes);
        let err = archive.read("oversize.bin").expect_err("must reject oversize");
        assert!(
            matches!(err, MpqError::Oversize { expected: 4096 }),
            "unexpected error: {err}"
        );
        // The streaming path must fail the same way (no silent truncation).
        let mut reader = archive.stream("oversize.bin").expect("stream");
        let mut sink = Vec::new();
        let streamed = std::io::Read::read_to_end(&mut reader, &mut sink);
        assert!(streamed.is_err(), "stream must reject the oversize sector");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn aud003_block_entry_above_the_file_size_cap_is_rejected_before_allocating() {
        // A 16-byte block-table entry declaring 4 GiB: the reader must refuse
        // it up front (`SizeLimitExceeded`) rather than reserve 4 GiB.
        let entries = vec![TestEntry {
            name: Some("huge.bin"),
            data: b"tiny".to_vec(),
            size: u32::MAX,
            compressed_size: u32::MAX,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_COMPRESS,
        }];
        let bytes = build_archive(3, &entries);
        let (path, archive) = open_archive("over-cap", &bytes);
        let err = archive.read("huge.bin").expect_err("over-cap block rejected");
        assert!(
            matches!(err, MpqError::SizeLimitExceeded { requested, limit }
                if requested == u32::MAX as u64 && limit == MAX_FILE_SIZE),
            "unexpected error: {err}"
        );
        // `stream` and `read_block` reach the same guard.
        assert!(matches!(
            archive.stream("huge.bin"),
            Err(MpqError::SizeLimitExceeded { .. })
        ));
        assert!(matches!(
            archive.read_block(0),
            Err(MpqError::SizeLimitExceeded { .. })
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn aud003_large_but_legitimate_archive_decodes_cover_to_cover() {
        // 8 MiB, 2048 sectors of 4 KiB: exercises the sector offset table
        // (2049 entries), the per-sector budget and the full-file assembly.
        let size = 8 * 1024 * 1024;
        let plain: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let (payload, declared) = zlib_sectors(&plain, 4096);
        assert!(
            (declared as u64) < MAX_FILE_SIZE,
            "fixture must stay below the hard cap"
        );
        let entries = vec![TestEntry {
            name: Some("large.bin"),
            compressed_size: payload.len() as u32,
            data: payload,
            size: declared,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_COMPRESS,
        }];
        let bytes = build_archive(3, &entries);
        let (path, archive) = open_archive("large", &bytes);
        let read = archive.read("large.bin").expect("large file reads");
        assert_eq!(read.len(), plain.len());
        assert_eq!(read, plain, "byte-identical round trip");

        // Random access through the streaming reader must agree with `read`.
        let mut reader = archive.stream("large.bin").expect("stream");
        let mut streamed = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut streamed).expect("stream read");
        assert_eq!(streamed, plain);
        assert!(reader.codecs().has(CodecUsage::ZLIB));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn aud003_stored_sectors_in_a_compressed_file_read_verbatim() {
        // `MPQ_FILE_COMPRESS` on a multi-sector file whose sectors are not
        // smaller than the declared output: every sector is stored verbatim
        // (`declared_len >= expected`, StormLib copies the bytes).
        let plain: Vec<u8> = (0..9000).map(|i| (i % 251) as u8).collect();
        let sector_size = 4096usize;
        let count = plain.len().div_ceil(sector_size);
        let mut data = vec![0u8; (count + 1) * 4];
        let mut offset = (count + 1) * 4;
        for index in 0..count {
            let start = index * sector_size;
            let end = (start + sector_size).min(plain.len());
            data[index * 4..index * 4 + 4].copy_from_slice(&(offset as u32).to_le_bytes());
            data.extend_from_slice(&plain[start..end]);
            offset += end - start;
        }
        data[count * 4..count * 4 + 4].copy_from_slice(&(offset as u32).to_le_bytes());

        let entries = vec![TestEntry {
            name: Some("stored.bin"),
            compressed_size: data.len() as u32,
            data,
            size: plain.len() as u32,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_COMPRESS,
        }];
        let bytes = build_archive(3, &entries);
        let (path, archive) = open_archive("stored", &bytes);
        let read = archive.read("stored.bin").expect("stored read");
        assert_eq!(read, plain);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn aud003_zero_length_sector_streams_zeros() {
        // A compressed multi-sector block where the middle sector has a zero
        // compressed length: that sector is a sparse run of zeros
        // (StormLib `SFileReadFile.cpp:203-206`).
        let mut plain = vec![0xAAu8; 9000];
        plain[4096..8192].fill(0); // the sparse sector's content
        let first = unit(MPQ_COMPRESSION_ZLIB, &zlib(&plain[0..4096]));
        let last = unit(MPQ_COMPRESSION_ZLIB, &zlib(&plain[8192..9000]));
        let table_bytes = 4 * 4;
        let mut data = vec![0u8; table_bytes];
        let mut offset = table_bytes;
        let write_offset = |index: usize, value: u32, data: &mut Vec<u8>| {
            data[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        };
        write_offset(0, offset as u32, &mut data);
        data.extend_from_slice(&first);
        offset += first.len();
        // Sector 1 is a zero-length range: offsets[1] == offsets[2].
        write_offset(1, offset as u32, &mut data);
        write_offset(2, offset as u32, &mut data);
        data.extend_from_slice(&last);
        offset += last.len();
        write_offset(3, offset as u32, &mut data);

        let entries = vec![TestEntry {
            name: Some("sparse.bin"),
            compressed_size: data.len() as u32,
            data,
            size: plain.len() as u32,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_COMPRESS,
        }];
        let bytes = build_archive(3, &entries);
        let (path, archive) = open_archive("sparse", &bytes);
        let read = archive.read("sparse.bin").expect("sparse read");
        assert_eq!(read.len(), plain.len());
        assert_eq!(read, plain, "sparse sector must be zeros");
        std::fs::remove_file(&path).ok();
    }

    /// The hard cap must stay above every shipped Warcraft III entry — the
    /// limit is a safety net, not a compatibility cut. Measured on the local
    /// install; the test prints the observed maximum.
    #[test]
    fn largest_shipped_entry_is_below_the_cap() {
        let candidates = [
            std::env::var_os("WC3_MPQ")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
                }),
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3x.mpq"),
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3xlocal.mpq"),
        ];
        let mut checked = 0usize;
        for path in candidates {
            if !path.is_file() {
                continue;
            }
            let archive = MpqArchive::open(&path).expect("open shipped archive");
            let mut max = (0u32, String::new());
            for index in 0..archive.block_count() as u32 {
                let Some((_, _, size, flags)) = archive.block_entry(index) else {
                    continue;
                };
                if flags & MPQ_FILE_EXISTS == 0 {
                    continue;
                }
                if size > max.0 {
                    max = (size, MpqArchive::pseudo_name(index, "bin"));
                }
            }
            println!(
                "MPQ_MAX_BLOCK {}: {} bytes ({})",
                path.display(),
                max.0,
                max.1
            );
            assert!(
                max.0 as u64 <= MAX_FILE_SIZE,
                "{} declares a {}-byte entry above the {MAX_FILE_SIZE}-byte cap",
                path.display(),
                max.0
            );
            checked += 1;
        }
        if checked == 0 {
            eprintln!("skipping: no shipped archive fixture available");
        }
    }
}

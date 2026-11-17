//! Random-access file reader over an MPQ archive.
//!
//! `MpqArchive::read` loads a whole file; this reader decodes one sector at a
//! time so a 20 MB music file or a 3.6 MB MP3 never has to be resident as a
//! whole just to be inspected. Uncompressed, unencrypted files are read
//! directly from the archive with `pread` (no copy through a staging buffer).
//!
//! Allocation behaviour: construction allocates the sector offset table plus
//! two reusable buffers; `read`/`seek` never allocate.

use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;

use crate::index::CodecUsage;
use crate::{
    BlockEntry, MpqArchive, MpqError, MPQ_FILE_COMPRESS, MPQ_FILE_ENCRYPTED,
    MPQ_FILE_SINGLE_UNIT,
};

/// Recover the encryption key of a block whose name is unknown by testing the
/// content signatures StormLib recognises (`SBaseCommon.cpp:627-680`).
///
/// This is the read path for listfile-less archives: StormLib lists unnamed
/// blocks as `File########.ext` and, when such a block is encrypted, derives
/// the key from the plaintext it expects (RIFF/WAVE, PE/EXE, XML). Only probes
/// that StormLib itself performs are attempted; anything else is reported as
/// unsupported instead of guessed.
pub(crate) fn detect_key_by_content(
    archive: &MpqArchive,
    block: &BlockEntry,
) -> Result<u32, MpqError> {
    let mut head = [0u8; 16];
    let sector_size = (block.compressed_size as usize).min(archive.sector_size());
    let read = sector_size.min(head.len());
    archive.read_exact_at(&mut head[..read], archive.base + block.offset as u64)?;
    if read < 16 {
        return Err(MpqError::Corrupt("encrypted block too short for key detection"));
    }
    if let Some(key) = detect_by_known_content(&head, 0x4646_4952, (block.size as u64).wrapping_sub(8) as u32) {
        return Ok(key);
    }
    if let Some(key) = detect_by_known_content(&head, 0x0090_5A4D, 3) {
        return Ok(key);
    }
    if let Some(key) = detect_by_known_content(&head, 0x6D78_3F3C, 0x6576_206C) {
        return Ok(key);
    }
    Err(MpqError::Corrupt("cannot detect encryption key of unnamed block"))
}

/// Inverse of one MPQ decrypt step for the first two dwords: given the expected
/// plaintext dwords, recover `key` (StormLib `DetectFileKeyByKnownContent`).
fn detect_by_known_content(encrypted: &[u8; 16], want0: u32, want1: u32) -> Option<u32> {
    let table = crate::crypt_table();
    // Fixed-size array with constant indices: bounds are a compile-time check.
    let enc0 = u32::from_le_bytes([
        encrypted[0], encrypted[1], encrypted[2], encrypted[3],
    ]);
    let enc1 = u32::from_le_bytes([
        encrypted[4], encrypted[5], encrypted[6], encrypted[7],
    ]);
    let key1_plus_key2 = (enc0 ^ want0).wrapping_sub(0xEEEE_EEEE);
    for i in 0..0x100u32 {
        let key1 = key1_plus_key2.wrapping_sub(table[0x400 + i as usize]);
        let mut key2: u32 = 0xEEEE_EEEE;
        key2 = key2.wrapping_add(table[0x400 + (key1 & 0xFF) as usize]);
        if (enc0 ^ key1.wrapping_add(key2)) != want0 {
            continue;
        }
        let saved = key1;
        let key1 = ((!key1 << 21).wrapping_add(0x1111_1111)) | (key1 >> 11);
        key2 = want0
            .wrapping_add(key2)
            .wrapping_add(key2 << 5)
            .wrapping_add(3);
        key2 = key2.wrapping_add(table[0x400 + (key1 & 0xFF) as usize]);
        if (enc1 ^ key1.wrapping_add(key2)) == want1 {
            return Some(saved);
        }
    }
    None
}


pub struct MpqFileReader<'a> {
    archive: &'a MpqArchive,
    block: BlockEntry,
    key: u32,
    len: u64,
    pos: u64,
    sector_size: usize,
    sector_count: u32,
    /// Sector offset table (only for the compressed multi-sector path).
    offsets: Option<Vec<u32>>,
    /// Uncompressed and unencrypted: `pread` straight out of the archive.
    direct: bool,
    /// Encrypted but not compressed: sectors are decrypted with `key + index`
    /// (StormLib `SFileReadFile.cpp`: `DecryptMpqBlock(..., dwFileKey + dwIndex)`),
    /// which is *not* the same as decrypting the file as one unit. The 34
    /// campaign loading backgrounds differ from StormLib output from byte 4096
    /// onwards when this distinction is ignored.
    encrypted_sectors: bool,
    /// Fully decoded payload (single-unit / encrypted-uncompressed files).
    whole: Option<Vec<u8>>,
    /// Reused per-sector source buffer.
    raw_buf: Vec<u8>,
    /// Reused per-sector decode output.
    sector_buf: Vec<u8>,
    sector_index: u32,
    sector_valid: bool,
    codecs: CodecUsage,
}

impl<'a> MpqFileReader<'a> {
    pub(crate) fn new(archive: &'a MpqArchive, block: BlockEntry, key: u32, name: &str) -> Result<Self, MpqError> {
        // `MPQ_FILE_IMPLODE` is a solo tag-less PKWARE stream, so a file that
        // carries only that flag still needs the decode path rather than a
        // direct copy.
        let compressed = block.flags & (MPQ_FILE_COMPRESS | crate::MPQ_FILE_IMPLODE) != 0;
        let encrypted = block.flags & MPQ_FILE_ENCRYPTED != 0;
        let single_unit = block.flags & MPQ_FILE_SINGLE_UNIT != 0;
        let len = block.size as u64;
        let mut reader = Self {
            archive,
            block,
            key,
            len,
            pos: 0,
            sector_size: 512usize << archive.header.block_size,
            sector_count: 0,
            offsets: None,
            direct: false,
            encrypted_sectors: false,
            whole: None,
            raw_buf: Vec::new(),
            sector_buf: Vec::new(),
            sector_index: 0,
            sector_valid: false,
            codecs: CodecUsage::default(),
        };
        if block.compressed_size == 0 || block.size == 0 {
            return Ok(reader);
        }
        if !compressed && !encrypted {
            // Encrypted-flags-free stored files are contiguous bytes.
            reader.direct = true;
            reader.codecs = CodecUsage(CodecUsage::STORED);
            return Ok(reader);
        }
        if encrypted && !compressed {
            reader.encrypted_sectors = true;
            reader.sector_count =
                ((block.size as usize).div_ceil(reader.sector_size)) as u32;
            reader.codecs = CodecUsage(CodecUsage::STORED);
            return Ok(reader);
        }
        if single_unit {
            let compressed = block.compressed_size as usize;
            let mut data = vec![0u8; compressed];
            archive.read_exact_at(&mut data, archive.base + block.offset as u64)?;
            if encrypted {
                // MPQ encrypts whole dwords; a trailing 1..3-byte remainder is
                // stored in plaintext and must not be padded-and-decrypted.
                crate::decrypt_bytes(&mut data, key);
            }
            let mut out = Vec::with_capacity(block.size as usize);
            let mut usage = CodecUsage::default();
            archive.decompress_into(
                &data,
                block.flags,
                block.size as usize,
                compressed,
                &mut out,
                &mut usage,
            )?;
            out.truncate(block.size as usize);
            reader.whole = Some(out);
            reader.codecs = usage;
            return Ok(reader);
        }

        // Compressed, multi-sector: only the sector offset table is read up
        // front (4 bytes per sector), then sectors are decoded on demand.
        let sector_count = (block.size as usize).div_ceil(reader.sector_size);
        if sector_count > 1_000_000 {
            return Err(MpqError::Corrupt("implausible sector count"));
        }
        // The engine allocates and reads exactly `(sectorCount + 1) * 4` bytes
        // and never touches a `MPQ_FILE_SECTOR_CRC` dword (`0x1501cdcd..0x1501cde8`,
        // `0x15018646`); StormLib's extra CRC entry (`SBaseCommon.cpp:1258-1268`)
        // is a later addition and reading one dword less is byte-identical for
        // the offsets that are used.
        let table_bytes = (sector_count + 1) * 4;
        if (block.compressed_size as usize) < (sector_count + 1) * 4 {
            return Err(MpqError::Corrupt("sector offset table truncated"));
        }
        let mut table = vec![0u8; table_bytes];
        archive.read_exact_at(&mut table, archive.base + block.offset as u64)?;
        if encrypted {
            crate::decrypt_bytes(&mut table, key.wrapping_sub(1));
        }
        let offsets: Vec<u32> = table
            .chunks_exact(4)
            .filter_map(|c| c.first_chunk::<4>().copied())
            .map(u32::from_le_bytes)
            .collect();
        reader.sector_count = sector_count as u32;
        reader.offsets = Some(offsets);
        reader.raw_buf = Vec::with_capacity(reader.sector_size + 256);
        reader.sector_buf = Vec::with_capacity(reader.sector_size);
        let _ = name;
        Ok(reader)
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Codec bits observed so far (sector tags are accumulated lazily).
    pub fn codecs(&self) -> CodecUsage {
        self.codecs
    }

    fn ensure_sector(&mut self, sector: u32) -> Result<(), MpqError> {
        if self.sector_valid && self.sector_index == sector {
            return Ok(());
        }
        if self.encrypted_sectors {
            let start = sector as usize * self.sector_size;
            if start >= self.block.size as usize {
                return Err(MpqError::Corrupt("sector index past end of file"));
            }
            let n = (self.block.size as usize - start).min(self.sector_size);
            self.raw_buf.clear();
            self.raw_buf.resize(n, 0);
            self.archive.read_exact_at(
                &mut self.raw_buf,
                self.archive.base + self.block.offset as u64 + start as u64,
            )?;
            crate::decrypt_bytes(&mut self.raw_buf, self.key.wrapping_add(sector));
            std::mem::swap(&mut self.raw_buf, &mut self.sector_buf);
            self.sector_index = sector;
            self.sector_valid = true;
            return Ok(());
        }
        let offsets = self
            .offsets
            .as_ref()
            .ok_or(MpqError::Corrupt("no sector table"))?;
        let start = *offsets
            .get(sector as usize)
            .ok_or(MpqError::Corrupt("sector index out of range"))? as usize;
        let end = *offsets
            .get(sector as usize + 1)
            .ok_or(MpqError::Corrupt("sector index out of range"))? as usize;
        if start > end || end > self.block.compressed_size as usize {
            return Err(MpqError::Corrupt("sector offsets out of bounds"));
        }
        let sector_bytes = end - start;
        self.raw_buf.clear();
        if sector_bytes == 0 {
            // Sparse block: a zero compressed length means the whole sector is
            // zeros (StormLib `SFileReadFile.cpp:203-206`).
            self.sector_buf.clear();
            self.sector_buf.resize(
                (self.block.size as usize - sector as usize * self.sector_size)
                    .min(self.sector_size),
                0,
            );
            self.codecs.merge(CodecUsage(CodecUsage::ZERO));
            self.sector_index = sector;
            self.sector_valid = true;
            return Ok(());
        }
        // Encrypted sectors are read through the dword-aligned StormLib path;
        // stored sectors keep the exact compressed length.
        self.raw_buf.resize(sector_bytes, 0);
        self.archive.read_exact_at(
            &mut self.raw_buf,
            self.archive.base + self.block.offset as u64 + start as u64,
        )?;
        if self.block.flags & MPQ_FILE_ENCRYPTED != 0 {
            // Whole dwords only: the trailing 1..3 bytes are plaintext.
            crate::decrypt_bytes(&mut self.raw_buf, self.key.wrapping_add(sector));
        }
        let sector_start = sector as usize * self.sector_size;
        let expected = (self.block.size as usize - sector_start).min(self.sector_size);
        let raw = std::mem::take(&mut self.raw_buf);
        let result = self.archive.decompress_into(
            &raw,
            self.block.flags,
            expected,
            sector_bytes,
            &mut self.sector_buf,
            &mut self.codecs,
        );
        self.raw_buf = raw;
        result?;
        self.sector_index = sector;
        self.sector_valid = true;
        Ok(())
    }

}

impl Read for MpqFileReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let want = buf.len().min((self.len - self.pos) as usize);
        if let Some(whole) = &self.whole {
            let start = self.pos as usize;
            let end = start + want;
            if end > whole.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "decoded payload shorter than declared size",
                ));
            }
            buf[..want].copy_from_slice(&whole[start..end]);
            self.pos += want as u64;
            return Ok(want);
        }
        if self.direct {
            let read = self.archive.file.read_at(
                &mut buf[..want],
                self.archive.base + self.block.offset as u64 + self.pos,
            )?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "archive ended inside stored file",
                ));
            }
            self.pos += read as u64;
            return Ok(read);
        }
        let mut written = 0usize;
        while written < want {
            let sector = (self.pos / self.sector_size as u64) as u32;
            if sector >= self.sector_count {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sector index past end of file",
                ));
            }
            self.ensure_sector(sector)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let sector_start = sector as u64 * self.sector_size as u64;
            let within = (self.pos - sector_start) as usize;
            let available = self.sector_buf.len().saturating_sub(within);
            if available == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "decoded sector is empty",
                ));
            }
            let take = available.min(want - written);
            buf[written..written + take]
                .copy_from_slice(&self.sector_buf[within..within + take]);
            written += take;
            self.pos += take as u64;
        }
        Ok(written)
    }
}

impl Seek for MpqFileReader<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.len as i64 + offset,
            SeekFrom::Current(offset) => self.pos as i64 + offset,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = (target as u64).min(self.len);
        Ok(self.pos)
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> Option<PathBuf> {
        let path = std::env::var_os("WC3_MPQ")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
            });
        path.is_file().then_some(path)
    }

    #[test]
    fn streams_match_read_for_real_files() {
        let Some(path) = fixture() else {
            eprintln!("skipping: WC3_MPQ fixture missing");
            return;
        };
        let archive = MpqArchive::open(&path).expect("open");
        // A small compressed file, a large compressed one, and the listfile.
        for name in [
            "Units/Human/Footman/Footman.mdx",
            "Sound/Music/mp3Music/Mainscreen.mp3",
            "Textures/Footman.blp",
        ] {
            let whole = match archive.read(name) {
                Ok(bytes) => bytes,
                Err(err) => {
                    eprintln!("skipping {name}: {err}");
                    continue;
                }
            };
            let mut reader = archive.stream(name).expect("stream");
            assert_eq!(reader.len() as usize, whole.len(), "{name} length");
            let mut streamed = Vec::new();
            reader.read_to_end(&mut streamed).expect("read_to_end");
            assert_eq!(streamed, whole, "{name} content");
        }
    }

    #[test]
    fn seeks_to_arbitrary_offsets() {
        let Some(path) = fixture() else {
            return;
        };
        let archive = MpqArchive::open(&path).expect("open");
        let name = "Textures/Footman.blp";
        let Ok(whole) = archive.read(name) else {
            return;
        };
        let mut reader = archive.stream(name).expect("stream");
        for offset in [0u64, 1, 511, 512, 4096, whole.len() as u64 / 2] {
            if offset as usize >= whole.len() {
                continue;
            }
            reader.seek(SeekFrom::Start(offset)).expect("seek");
            let mut chunk = [0u8; 17];
            let read = reader.read(&mut chunk).expect("read");
            let end = (offset as usize + read).min(whole.len());
            assert_eq!(&chunk[..read], &whole[offset as usize..end], "offset {offset}");
        }
    }
}

//! Archive-reader parity with the Warcraft III Storm.dll, on synthetic archives.
//!
//! Every fixture here is built byte by byte: a 32-byte v0 header, an encrypted
//! hash table, an encrypted block table and stored payloads. The crypto is the
//! engine's own arithmetic (crypt table seed `0x00100001`, `*125+3 mod
//! 0x2AAAAB`; hash seeds `0x7FED7FED`/`0xEEEEEEEE`; block cipher seed
//! `0xEEEEEEEE` with `table[0x400 + key & 0xFF]`, rotation `0x15`/`0x0B` and
//! `0x11111111`) as measured in `analysis/agent-re/formats/ENGINE_MAP.md` §1.1,
//! so the fixtures do not rely on the reader they test.
//!
//! Canonical mechanic ids:
//! - `mpq.locale-platform-filter` → [`mpq_locale_platform_filter`]
//! - `mpq.unknown-codec-rejected` → [`mpq_unknown_codec_rejected`]
//! - `mpq.300-raw-copy` → [`mpq_implode_compress_copies_raw`]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use wc3_mpq::{MpqArchive, MpqError};

const MPQ_FILE_IMPLODE: u32 = 0x0000_0100;
const MPQ_FILE_COMPRESS: u32 = 0x0000_0200;
const MPQ_FILE_SINGLE_UNIT: u32 = 0x0100_0000;
const MPQ_FILE_EXISTS: u32 = 0x8000_0000;
const HASH_TABLE_ENTRIES: u32 = 16;

// ---------------------------------------------------------------- engine crypto

fn crypt_table() -> &'static [u32; 0x500] {
    static TABLE: std::sync::OnceLock<[u32; 0x500]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        for i in 0..0x100usize {
            // Layout from the Storm initialiser (`0x1501bee0..0x1501bf2a`):
            // each entry consumes two `*125+3 mod 0x2AAAAB` steps and fills
            // one of the five rows.
            for k in 0..5 {
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2AAA_AB;
                let hi = (seed & 0xFFFF) << 16;
                seed = (seed.wrapping_mul(125).wrapping_add(3)) % 0x2AAA_AB;
                table[i + k * 0x100] = hi | (seed & 0xFFFF);
            }
        }
        table
    })
}

fn hash_string(name: &str, kind: u32) -> u32 {
    let table = crypt_table();
    let mut seed1: u32 = 0x7FED_7FED;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for ch in name.bytes() {
        let c = ch.to_ascii_uppercase() as u32;
        seed1 = table[((kind << 8) + c) as usize] ^ seed1.wrapping_add(seed2);
        seed2 = seed2
            .wrapping_mul(0x21)
            .wrapping_add(c)
            .wrapping_add(seed1)
            .wrapping_add(3);
    }
    seed1
}

/// MPQ block cipher. Encryption and decryption share the keystream because the
/// `seed2` recurrence advances on the **plaintext** word in both directions
/// (`EncryptMpqBlock` keeps the pre-XOR word, `DecryptMpqBlock` the post-XOR
/// one — `donors/StormLib/src/SBaseCommon.cpp:431-462,494-520`).
fn crypt_words(words: &mut [u32], key: u32) {
    let table = crypt_table();
    let mut seed: u32 = 0xEEEE_EEEE;
    let mut key = key;
    for word in words.iter_mut() {
        seed = seed.wrapping_add(table[0x400 + (key & 0xFF) as usize]);
        let plain = *word;
        *word = plain ^ seed.wrapping_add(key);
        seed = plain
            .wrapping_add(seed)
            .wrapping_add(seed << 5)
            .wrapping_add(3);
        key = ((!key << 0x15).wrapping_add(0x1111_1111)) | (key >> 0x0B);
    }
}

// ------------------------------------------------------------------- fixtures

struct FixtureBlock {
    name: &'static str,
    locale: u16,
    platform: u16,
    flags: u32,
    /// Uncompressed size recorded in the block table.
    size: u32,
    data: Vec<u8>,
}

impl FixtureBlock {
    fn stored(name: &'static str, payload: &[u8]) -> Self {
        Self {
            name,
            locale: 0,
            platform: 0,
            flags: MPQ_FILE_EXISTS,
            size: payload.len() as u32,
            data: payload.to_vec(),
        }
    }
}

fn hash_entry_word(locale: u16, platform: u16) -> u32 {
    (locale as u32) | ((platform as u32) << 16)
}

/// Build a v0 archive. Hash entries are placed in the given order starting at
/// each name's home slot, so probe order is fully controlled.
fn build_archive(blocks: &[FixtureBlock]) -> Vec<u8> {
    let mut hash_words = vec![0xFFFF_FFFFu32; HASH_TABLE_ENTRIES as usize * 4];
    let mut used = vec![false; HASH_TABLE_ENTRIES as usize];
    let data_start = 32 + HASH_TABLE_ENTRIES as usize * 16 + blocks.len() * 16;
    let mut block_words = Vec::with_capacity(blocks.len() * 4);
    let mut data = Vec::new();
    let mut cursor = data_start;
    for (index, block) in blocks.iter().enumerate() {
        let upper = block.name.to_ascii_uppercase();
        let name1 = hash_string(&upper, 1);
        let name2 = hash_string(&upper, 2);
        let home = (hash_string(&upper, 0) & (HASH_TABLE_ENTRIES - 1)) as usize;
        let mut slot = home;
        let mut placed = false;
        for _ in 0..HASH_TABLE_ENTRIES {
            if !used[slot] {
                used[slot] = true;
                placed = true;
                break;
            }
            slot = (slot + 1) & (HASH_TABLE_ENTRIES as usize - 1);
        }
        assert!(placed, "hash table exhausted");
        let base = slot * 4;
        hash_words[base] = name1;
        hash_words[base + 1] = name2;
        hash_words[base + 2] = hash_entry_word(block.locale, block.platform);
        hash_words[base + 3] = index as u32;

        block_words.extend_from_slice(&[
            cursor as u32,
            block.data.len() as u32,
            block.size,
            block.flags,
        ]);
        data.extend_from_slice(&block.data);
        cursor += block.data.len();
    }
    crypt_words(&mut hash_words, hash_string("(hash table)", 3));
    crypt_words(&mut block_words, hash_string("(block table)", 3));

    let mut out = Vec::with_capacity(cursor);
    out.extend_from_slice(b"MPQ\x1a");
    out.extend_from_slice(&32u32.to_le_bytes()); // header size
    out.extend_from_slice(&(cursor as u32).to_le_bytes()); // archive size
    out.extend_from_slice(&0u16.to_le_bytes()); // version 0
    out.extend_from_slice(&3u16.to_le_bytes()); // sector size = 512 << 3
    out.extend_from_slice(&32u32.to_le_bytes()); // hash table offset
    out.extend_from_slice(&(32 + HASH_TABLE_ENTRIES * 16).to_le_bytes()); // block table
    out.extend_from_slice(&HASH_TABLE_ENTRIES.to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    for word in &hash_words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for word in &block_words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&data);
    out
}

fn write_archive(bytes: &[u8]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "wc3-mpq-parity-{}-{n}.mpq",
        std::process::id()
    ));
    std::fs::write(&path, bytes).expect("write fixture archive");
    path
}

fn open_fixture(blocks: &[FixtureBlock]) -> (PathBuf, MpqArchive) {
    let path = write_archive(&build_archive(blocks));
    let archive = MpqArchive::open(&path).expect("open fixture archive");
    (path, archive)
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

// ----------------------------------------------------------------------- tests

/// Canonical id: `mpq.locale-platform-filter`.
///
/// One name stored three times — under locale `0x409` (enUS), under platform 1,
/// and neutrally. The engine skips a hash entry whose `wLocale`/`wPlatform` is
/// non-zero and does not equal the current value (`0x1501b3b5..0x1501b3dc`), so
/// the neutral entry is the one the engine returns; a reader that ignores those
/// fields returns the locale-tagged payload at the head of the probe chain.
#[test]
fn mpq_locale_platform_filter() {
    const NAME: &str = "war3map.w3e";
    let blocks = vec![
        FixtureBlock {
            name: NAME,
            locale: 0x409,
            platform: 0,
            flags: MPQ_FILE_EXISTS,
            size: 10,
            data: b"LOCALE-409".to_vec(),
        },
        FixtureBlock {
            name: NAME,
            locale: 0,
            platform: 1,
            flags: MPQ_FILE_EXISTS,
            size: 10,
            data: b"PLATFORM-1".to_vec(),
        },
        FixtureBlock {
            name: NAME,
            locale: 0,
            platform: 0,
            flags: MPQ_FILE_EXISTS,
            size: 7,
            data: b"NEUTRAL".to_vec(),
        },
    ];
    let (path, mut archive) = open_fixture(&blocks);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(archive.locale(), 0);
        assert_eq!(archive.platform(), 0);
        // Neutral lookup: the two tagged duplicates are skipped.
        assert_eq!(archive.read(NAME).unwrap(), b"NEUTRAL");
        assert!(archive.contains(NAME));
        // Selecting the locale makes its entry the first match in probe order.
        archive.set_locale(0x409);
        assert_eq!(archive.read(NAME).unwrap(), b"LOCALE-409");
        // Platform behaves the same way.
        archive.set_locale(0);
        archive.set_platform(1);
        assert_eq!(archive.read(NAME).unwrap(), b"PLATFORM-1");
        // Both filters at once still follow probe order.
        archive.set_locale(0x409);
        assert_eq!(archive.read(NAME).unwrap(), b"LOCALE-409");
        // Back to neutral: the engine's default state.
        archive.set_locale(0);
        archive.set_platform(0);
        assert_eq!(archive.read(NAME).unwrap(), b"NEUTRAL");
    }));
    cleanup(&path);
    result.unwrap();
}

/// Canonical id: `mpq.unknown-codec-rejected`.
///
/// The engine's `SFileDecompress` table has five slots — `0x01`, `0x02`,
/// `0x08`, `0x40`, `0x80` — and no handler for bzip2 `0x10`, sparse `0x20`,
/// LZMA `0x12` or an unassigned bit. A reader that guesses one of them produces
/// wrong bytes; the reader must report the exact tag instead.
#[test]
fn mpq_unknown_codec_rejected() {
    for tag in [0x10u32, 0x20, 0x12, 0x04, 0x0C] {
        let payload = vec![tag as u8, 0x00, 0x00, 0x00, 0x00, 0x7F];
        let blocks = vec![FixtureBlock {
            name: "war3map.w3i",
            locale: 0,
            platform: 0,
            flags: MPQ_FILE_EXISTS | MPQ_FILE_SINGLE_UNIT | MPQ_FILE_COMPRESS | (tag << 8),
            size: 64,
            data: payload,
        }];
        let (path, archive) = open_fixture(&blocks);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match archive.read("war3map.w3i") {
                Err(MpqError::UnsupportedCompression(reported)) => assert_eq!(reported, tag),
                other => panic!("tag 0x{tag:x}: expected UnsupportedCompression, got {other:?}"),
            }
        }));
        cleanup(&path);
        result.unwrap();
    }
    // A whitelisted codec reaches its decoder instead: Huffman over garbage is
    // a decode error, never `UnsupportedCompression`.
    let blocks = vec![FixtureBlock {
        name: "war3map.doo",
        locale: 0,
        platform: 0,
        flags: MPQ_FILE_EXISTS
            | MPQ_FILE_SINGLE_UNIT
            | MPQ_FILE_COMPRESS
            | (wc3_mpq::MPQ_COMPRESSION_HUFFMANN << 8),
        size: 64,
        data: vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF],
    }];
    let (path, archive) = open_fixture(&blocks);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match archive.read("war3map.doo") {
            Err(MpqError::UnsupportedCompression(tag)) => {
                panic!("huffman 0x01 must be whitelisted, reported 0x{tag:x}")
            }
            Err(_) => {}
            Ok(bytes) => panic!("garbage huffman payload decoded to {} bytes", bytes.len()),
        }
    }));
    cleanup(&path);
    result.unwrap();
}

/// Canonical id: `mpq.300-raw-copy`.
///
/// `MPQ_FILE_IMPLODE | MPQ_FILE_COMPRESS` (`0x300`) selects neither engine
/// branch: `0x15018a6f` tests `(flags & 0xFF00) == 0x100` and `0x15018a76`
/// tests `== 0x200`, so the unit falls through to the raw copy at
/// `0x15018b98`. The payload below starts with `0x02`, which a tag-first reader
/// would feed to the zlib decoder; the engine returns it untouched.
#[test]
fn mpq_implode_compress_copies_raw() {
    let raw = b"\x02RAW-NOT-A-TAG";
    let blocks = vec![FixtureBlock {
        name: "war3map.wpm",
        locale: 0,
        platform: 0,
        flags: MPQ_FILE_EXISTS | MPQ_FILE_SINGLE_UNIT | MPQ_FILE_IMPLODE | MPQ_FILE_COMPRESS,
        size: raw.len() as u32,
        data: raw.to_vec(),
    }];
    let (path, archive) = open_fixture(&blocks);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(archive.read("war3map.wpm").unwrap(), raw);
    }));
    cleanup(&path);
    result.unwrap();
}


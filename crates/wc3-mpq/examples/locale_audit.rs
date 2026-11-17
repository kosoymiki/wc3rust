//! How the locale/platform hash filter affects a real archive.
//!
//! The engine resolves a hash entry only when its `wLocale`/`wPlatform` is zero
//! or equals the current value (`0x1501b3b5..0x1501b3dc`); Warcraft III leaves
//! both globals at zero (`0x15045784`/`0x15045786` are zero-initialised and no
//! Game.dll build calls `SFileSetLocale`). This tool answers the practical
//! question for a shipped archive: does applying that filter change which block
//! any `(listfile)` name resolves to?
//!
//! It reads the hash table directly with the engine crypto (independent of
//! `MpqArchive`) and prints:
//! - entries with a non-zero locale / platform,
//! - names where the unfiltered pick (first name-hash match, what a reader
//!   ignoring the fields would return) differs from the neutral pick,
//! - names that resolve **only** through a tagged entry (a real regression
//!   risk: the engine would not find them at neutral locale).
//!
//! Usage: `cargo run -p wc3-mpq --example locale_audit -- <archive.mpq> [...]`

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::OnceLock;

use wc3_mpq::MpqArchive;

fn crypt_table() -> &'static [u32; 0x500] {
    static TABLE: OnceLock<[u32; 0x500]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 0x500];
        let mut seed: u32 = 0x0010_0001;
        for i in 0..0x100usize {
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

fn decrypt(words: &mut [u32], key: u32) {
    let table = crypt_table();
    let mut seed1 = key;
    let mut seed2: u32 = 0xEEEE_EEEE;
    for word in words.iter_mut() {
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

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes"))
}

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert!(!args.is_empty(), "usage: locale_audit <archive.mpq> ...");
    for path in args {
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let base = if &bytes[..4] == b"MPQ\x1a" { 0usize } else { 0x200 };
        let header = &bytes[base..base + 32];
        let hash_pos = u32_at(header, 16) as usize + base;
        let hash_count = u32_at(header, 24) as usize;
        let block_count = u32_at(header, 28) as usize;
        let mut hash_words: Vec<u32> = bytes[hash_pos..hash_pos + hash_count * 16]
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("4")))
            .collect();
        decrypt(&mut hash_words, hash_string("(hash table)", 3));
        let mask = hash_count as u32 - 1;

        let archive = MpqArchive::open_at(&path, base as u64)
            .unwrap_or_else(|err| panic!("open {}: {err}", path.display()));
        let names = archive.listfile();

        let mut tagged_locale = 0usize;
        let mut tagged_platform = 0usize;
        let mut tagged_both = 0usize;
        for entry in hash_words.chunks_exact(4) {
            if entry[3] == 0xFFFF_FFFF || entry[3] == 0xFFFF_FFFE {
                continue;
            }
            let locale = entry[2] as u16;
            let platform = (entry[2] >> 16) as u16;
            if locale != 0 {
                tagged_locale += 1;
            }
            if platform != 0 {
                tagged_platform += 1;
            }
            if locale != 0 && platform != 0 {
                tagged_both += 1;
            }
        }

        let mut differ = 0usize;
        let mut neutral_miss = 0usize;
        let mut examples: Vec<String> = Vec::new();
        for name in names {
            let upper = name.to_ascii_uppercase();
            let name1 = hash_string(&upper, 1);
            let name2 = hash_string(&upper, 2);
            let home = (hash_string(&upper, 0) & mask) as usize;
            let mut unfiltered: Option<u32> = None;
            let mut neutral: Option<u32> = None;
            for step in 0..=mask as usize {
                let slot = (home + step) & mask as usize;
                let base = slot * 4;
                let block = hash_words[base + 3];
                if block == 0xFFFF_FFFF {
                    break;
                }
                if block != 0xFFFF_FFFE
                    && hash_words[base] == name1
                    && hash_words[base + 1] == name2
                {
                    if unfiltered.is_none() {
                        unfiltered = Some(block);
                    }
                    let locale = hash_words[base + 2] as u16;
                    let platform = (hash_words[base + 2] >> 16) as u16;
                    // Current locale/platform are neutral here, so only an
                    // entry with both fields zero can match.
                    if locale == 0 && platform == 0 {
                        if neutral.is_none() {
                            neutral = Some(block);
                        }
                    }
                }
            }
            if unfiltered != neutral {
                differ += 1;
                if neutral.is_none() {
                    neutral_miss += 1;
                }
                if examples.len() < 10 {
                    examples.push(format!(
                        "{name}: unfiltered={unfiltered:?} neutral={neutral:?}"
                    ));
                }
            }
        }

        println!(
            "{}: base=0x{base:x} hash={hash_count} blocks={block_count} listfile={}",
            path.display(),
            names.len()
        );
        println!(
            "  hash entries: locale!=0 {tagged_locale}, platform!=0 {tagged_platform}, both {tagged_both}"
        );
        println!(
            "  listfile names: resolution differs {differ}, resolves only via a tagged entry {neutral_miss}"
        );
        for line in &examples {
            println!("    {line}");
        }
    }
}

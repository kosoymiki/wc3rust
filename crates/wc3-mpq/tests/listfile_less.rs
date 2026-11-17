//! Listfile-less and protected-archive behaviour, checked against StormLib.
//!
//! Two fixtures from the War3Net donor corpus exercise the paths that a
//! `(listfile)`-only reader misses:
//!
//! - `PKCompressed.w3x` has 321 blocks and **no** `(listfile)`. StormLib names
//!   the unnamed blocks `File########.ext` and reads them by index; the
//!   expected bytes below were extracted with `tools/storm_probe` (StormLib
//!   oracle) and pinned by SHA-256.
//! - `Jurassic Park Survival EE v6.4.w3x` is a Spazzler-protected map: the
//!   only real `MPQ\x1a` signature sits at offset 1536 behind a fake header
//!   whose block-table size (92 673) runs far past EOF. StormLib cuts the
//!   block table to the 150 entries the file actually holds and opens the map.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wc3_mpq::MpqArchive;

fn map_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps")
        .join(name);
    path.is_file().then_some(path)
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn listfile_less_archive_reads_blocks_by_index() {
    let Some(path) = map_path("PKCompressed.w3x") else {
        eprintln!("skipping: PKCompressed.w3x fixture missing");
        return;
    };
    let mut archive = MpqArchive::open(&path).expect("open listfile-less archive");
    assert!(!archive.has_listfile(), "fixture intentionally has no (listfile)");
    assert_eq!(archive.block_count(), 321, "block table entries");
    // The hash table still carries the original map names even though the
    // listfile was stripped, so probing recovers them without a listfile.
    assert_eq!(archive.discover_known_names(), 13);
    // 316 existing files (StormLib lists exactly 316), 5 deleted entries.
    assert_eq!(archive.existing_block_count(), 316);
    assert_eq!(archive.unnamed_block_count(), 303);
    // A name recovered by probing resolves to its block and reads: the map
    // flags are masked with StormLib's W3X set, so the bogus SINGLE_UNIT bit
    // no longer hides the sector offset table.
    let w3e = archive.read("war3map.w3e").expect("read war3map.w3e");
    assert_eq!(&w3e[..4], b"W3E!");
    assert_eq!(w3e.len(), 390_548);

    // StormLib: `File00000103.blp`, 10880 bytes, BLP1 header.
    let bytes = archive.read_block(103).expect("read block 103");
    assert_eq!(bytes.len(), 10_880);
    assert_eq!(&bytes[..4], b"BLP1");
    assert_eq!(
        sha256(&bytes),
        "4915edda09a754f20785a92bf5f7d7e32df7fdf47421edbce959b3dd1f4b492f",
        "bytes must match the StormLib extraction"
    );

    // Pseudo-name plumbing: "File########.ext" addresses the same block.
    let name = MpqArchive::pseudo_name(103, "blp");
    assert_eq!(MpqArchive::parse_pseudo_name(&name), Some(103));
    assert_eq!(archive.read_block(103).unwrap(), bytes);
}

#[test]
fn protected_archive_truncated_block_table_opens() {
    let Some(path) = map_path("Jurassic Park Survival EE v6.4.w3x") else {
        eprintln!("skipping: protected map fixture missing");
        return;
    };
    // The real MPQ header is the fake SPAZZLER one at 1536; the corrupted copy
    // at offset 512 is not a valid signature.
    let archive = MpqArchive::open_at(&path, 1536).expect("open protected map");
    assert!(archive.is_malformed(), "block table was cut at EOF");
    let (hash_declared, hash_real, block_declared, block_real) = archive.table_counts();
    assert_eq!(hash_declared as usize, hash_real, "hash table fits");
    assert_eq!(block_declared, 92_673, "declared block count from the fake header");
    assert_eq!(block_real, 150, "entries that physically fit the file");
    // StormLib lists exactly 150 files for this map.
    assert_eq!(archive.block_count(), 150);
    assert!(archive.has_listfile(), "protected map still ships a listfile");
    assert!(archive.listfile().len() >= 100);
    // A real payload reads end-to-end.
    let wpm = archive.read("war3map.wpm").expect("war3map.wpm");
    assert_eq!(&wpm[..4], b"MP3W");
}

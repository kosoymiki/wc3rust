//! Shipped-archive census: every archive in the extracted tree opens through
//! `MpqArchive::open_any`, every listed entry decodes, the codec tags stay
//! inside the engine's five-codec whitelist, and every campaign `.w3m` map
//! container opens at its embedded base.
//!
//! Mechanic: `assets.mpq` — StormLib-compatible MPQ read of every file.
//! Oracle: the operator's extracted retail tree (`analysis/war3_full3`). The
//! test skips with an explicit line when the tree is absent (desktop checkout).

use std::io::Read;
use std::path::{Path, PathBuf};

use wc3_mpq::{MpqArchive, MpqError};

/// Engine codec mask: PKWARE 0x08, zlib 0x02, Huffman 0x01, ADPCM stereo 0x80,
/// ADPCM mono 0x40. Any other declared bit has no handler in Storm.dll.
const WC3_CODEC_MASK: u16 = 0x00CB;

fn tree() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    root.is_dir().then_some(root)
}

fn files_with_ext(root: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root)
        .expect("read tree root")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
        .collect();
    out.sort();
    out
}

/// Stream every listed entry and return `(entries, decoded_bytes, failures)`.
fn decode_listed(archive: &MpqArchive) -> (usize, u64, usize) {
    let names = archive.listfile();
    let mut bytes = 0u64;
    let mut failures = 0usize;
    let mut buffer = vec![0u8; 64 * 1024];
    for name in names {
        let mut reader = match archive.stream(name) {
            Ok(reader) => reader,
            Err(error) => {
                eprintln!("FAIL {name}: {error}");
                failures += 1;
                continue;
            }
        };
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => bytes += n as u64,
                Err(error) => {
                    eprintln!("FAIL {name}: {error}");
                    failures += 1;
                    break;
                }
            }
        }
    }
    (names.len(), bytes, failures)
}

/// 16 shipped `.mpq` archives: 665 entries, all decode, codecs inside 0xCB.
#[test]
fn assets_mpq_shipped_archives_census() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let archives = files_with_ext(&root, "mpq");
    assert_eq!(archives.len(), 16, "shipped .mpq count");

    let mut entries = 0usize;
    let mut failures = 0usize;
    let mut bytes = 0u64;
    let mut stored_only = 0usize;
    let mut zlib_declared = 0usize;
    for path in &archives {
        let mut archive = MpqArchive::open_any(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(
            archive.listfile(),
            MpqArchive::open(path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
                .listfile(),
            "open_any must equal open for a bare archive: {}",
            path.display()
        );
        if !archive.has_listfile() {
            archive.discover_known_names();
        }
        assert!(
            !archive.listfile().is_empty(),
            "{}: no listfile",
            path.display()
        );
        for name in archive.listfile() {
            let usage = archive
                .meta(name)
                .unwrap_or_else(|| panic!("{}: no meta for {name}", path.display()))
                .1
                .declared_usage();
            assert_eq!(
                usage.0 & !WC3_CODEC_MASK,
                0,
                "{}: {name} declares a codec outside the engine table: {:#06x}",
                path.display(),
                usage.0
            );
        }
        let (count, decoded, failed) = decode_listed(&archive);
        entries += count;
        bytes += decoded;
        failures += failed;

        // Per-archive codec mix: the shipped set is 14 stored-only archives
        // and two zlib archives (`War3_low.mpq`, `War3_med.mpq`).
        let compressed = archive
            .listfile()
            .iter()
            .filter(|name| {
                archive
                    .meta(name)
                    .is_some_and(|(_, meta)| meta.declared_usage().has(0x02))
            })
            .count();
        if compressed == 0 {
            stored_only += 1;
        } else {
            zlib_declared += compressed;
        }
    }
    eprintln!(
        "archives={} entries={entries} decodes={bytes} failures={failures} \
         stored_only_archives={stored_only} zlib_entries={zlib_declared}",
        archives.len()
    );
    assert_eq!(failures, 0, "every listed entry must decode");
    assert_eq!(entries, 685, "shipped .mpq entry count");
    assert_eq!(stored_only, 14, "stored-only archives");
    assert_eq!(zlib_declared, 172, "zlib-declared entries");
    assert!(bytes > 30_000_000, "decoded payload floor");
}

/// All 44 campaign `.w3m` containers open at the embedded MPQ base (512 for an
/// `HM3W` header) and answer their inner map files.
#[test]
fn assets_mpq_map_containers_open() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let maps: Vec<PathBuf> = std::fs::read_dir(root.join("Maps/Campaign"))
        .expect("campaign directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("w3m"))
        })
        .collect();
    assert_eq!(maps.len(), 44, "campaign map count");

    let mut with_terrain = 0usize;
    let mut with_script = 0usize;
    let mut inner_files = 0usize;
    let mut failures = 0usize;
    for path in &maps {
        let head = std::fs::read(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let base = MpqArchive::mpq_base(&head)
            .unwrap_or_else(|| panic!("{}: no MPQ signature", path.display()));
        assert_eq!(base, 512, "{}: HM3W payload base", path.display());
        let mut archive = MpqArchive::open_any(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        archive.discover_known_names();
        assert!(
            !archive.listfile().is_empty(),
            "{}: no inner file names",
            path.display()
        );
        let (count, _, failed) = decode_listed(&archive);
        inner_files += count;
        failures += failed;
        if archive.contains("war3map.w3e") {
            with_terrain += 1;
        }
        if archive.contains("war3map.j") || archive.contains("scripts\\war3map.j") {
            with_script += 1;
        }
        // The map info file is the one inner file every shipped map carries;
        // read it through the archive so the plumbing is proven end to end, not
        // just the entry lookup. Its first dword is the W3I format version.
        let info = archive
            .read("war3map.w3i")
            .unwrap_or_else(|error| panic!("{}: war3map.w3i: {error}", path.display()));
        assert!(info.len() >= 8, "{}: short war3map.w3i", path.display());
        let version = u32::from_le_bytes([info[0], info[1], info[2], info[3]]);
        assert!(
            (8..=32).contains(&version),
            "{}: war3map.w3i version {version}",
            path.display()
        );
        if archive.contains("war3map.w3e") {
            let terrain = archive
                .read("war3map.w3e")
                .unwrap_or_else(|error| panic!("{}: war3map.w3e: {error}", path.display()));
            assert_eq!(
                terrain.get(..4),
                Some(&b"W3E!"[..]),
                "{}: terrain magic",
                path.display()
            );
        }
    }
    eprintln!(
        "maps={} inner_files={inner_files} with_terrain={with_terrain} \
         with_script={with_script} failures={failures}",
        maps.len()
    );
    assert_eq!(failures, 0, "every discovered inner file must decode");
    assert_eq!(with_terrain, 44, "every campaign map carries terrain");
    assert_eq!(with_script, 44, "every campaign map carries a script");
    assert!(inner_files >= 44 * 8, "map payload floor");
}

/// A file with neither magic is still a typed `BadMagic`, and a `.w3m` without
/// an embedded signature is rejected at the container level rather than read
/// as a bogus archive.
#[test]
fn assets_mpq_open_any_rejects_foreign_files() {
    let dir = std::env::temp_dir().join(format!("wc3-mpq-open-any-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let plain = dir.join("plain.bin");
    std::fs::write(&plain, b"not an archive").expect("write");
    assert!(matches!(
        MpqArchive::open_any(&plain),
        Err(MpqError::BadMagic(_))
    ));
    let container = dir.join("headless.w3m");
    std::fs::write(&container, b"HM3W no archive here").expect("write");
    assert!(matches!(
        MpqArchive::open_any(&container),
        Err(MpqError::BadMagic(_))
    ));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

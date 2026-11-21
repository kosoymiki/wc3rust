//! Lane 509 acceptance for the MPQ layer itself: the three retail archives open,
//! the campaign maps live inside them (RoC in `War3.mpq`, TFT in
//! `War3xlocal.mpq`), and a missing file is a typed `FileNotFound`.
//!
//! The install is the operator's device layout; a desktop checkout with the
//! same files under `WC3_MPQ_DIR` runs the identical assertions, otherwise the
//! test skips with an explicit line.

use std::path::{Path, PathBuf};

fn archive_dir() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("WC3_MPQ_DIR") {
        roots.push(PathBuf::from(dir));
    }
    roots.push(PathBuf::from("/data/data/org.herofolends.p0/files"));
    roots.push(PathBuf::from("/data/data/org.wc3rust.p0/files"));
    roots.push(PathBuf::from(
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso",
    ));
    roots
        .into_iter()
        .find(|root| root.join("War3.mpq").is_file())
}

fn open(dir: &Path, name: &str) -> wc3_mpq::MpqArchive {
    let path = dir.join(name);
    wc3_mpq::MpqArchive::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every mounted archive answers its own listfile and a real game file.
#[test]
fn retail_archives_open_and_read() {
    let Some(dir) = archive_dir() else {
        eprintln!("skip: no MPQ install (set WC3_MPQ_DIR)");
        return;
    };
    for name in ["War3.mpq", "War3x.mpq", "War3xlocal.mpq"] {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("skip {name}: absent in {}", dir.display());
            continue;
        }
        let archive = open(&dir, name);
        let listfile = archive.listfile().len();
        assert!(listfile >= 1000, "{name}: listfile={listfile}");
        // Localisation layer carries text, the data layers carry the sheets.
        let probe = if name == "War3xlocal.mpq" {
            "UI\\MiscUI.txt"
        } else {
            "Units\\UnitData.slk"
        };
        let bytes = archive
            .read(probe)
            .unwrap_or_else(|error| panic!("{name}: {probe}: {error}"));
        assert!(bytes.len() > 5_000, "{name}: {probe}={} B", bytes.len());
        println!(
            "MPQ_READ archive={name} listfile={listfile} blocks={} probe={probe} bytes={}",
            archive.block_count(),
            bytes.len()
        );
    }
}

/// RoC chapters are entries of `War3.mpq`, TFT chapters of `War3xlocal.mpq`.
#[test]
fn campaign_maps_are_inside_the_archives() {
    let Some(dir) = archive_dir() else {
        eprintln!("skip: no MPQ install (set WC3_MPQ_DIR)");
        return;
    };
    let war3 = open(&dir, "War3.mpq");
    let orc01 = "Maps\\Campaign\\Orc01.w3m";
    let (_, meta) = war3
        .meta(orc01)
        .unwrap_or_else(|| panic!("{orc01} missing from War3.mpq"));
    let bytes = war3.read(orc01).expect("read Orc01");
    assert_eq!(bytes.len() as u32, meta.size);
    assert!(bytes.starts_with(b"HM3W"), "Orc01 must start with HM3W");
    let roc = war3
        .listfile()
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("maps\\campaign\\")
                && (lower.ends_with(".w3m") || lower.ends_with(".w3x"))
        })
        .count();
    assert!(roc >= 40, "RoC campaign entries={roc}");
    println!("MPQ_CAMPAIGN archive=War3.mpq entry={orc01} bytes={} roc_entries={roc}", bytes.len());

    let local = open(&dir, "War3xlocal.mpq");
    let tft = local
        .listfile()
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("maps\\frozenthrone\\campaign\\")
                && (lower.ends_with(".w3m") || lower.ends_with(".w3x"))
        })
        .count();
    assert!(tft >= 30, "TFT campaign entries={tft}");
    let first = local
        .listfile()
        .iter()
        .find(|name| name.to_ascii_lowercase().starts_with("maps\\frozenthrone\\campaign\\"))
        .expect("a TFT chapter");
    let bytes = local.read(first).expect("read a TFT chapter");
    assert!(bytes.starts_with(b"HM3W"), "{first} must start with HM3W");
    println!("MPQ_CAMPAIGN archive=War3xlocal.mpq tft_entries={tft} sample={first} bytes={}", bytes.len());
}

/// A missing entry fails loudly and identifies the name.
#[test]
fn missing_entry_is_file_not_found() {
    let Some(dir) = archive_dir() else {
        eprintln!("skip: no MPQ install (set WC3_MPQ_DIR)");
        return;
    };
    let archive = open(&dir, "War3.mpq");
    let error = archive
        .read("Units\\NoSuchFile_509.mdx")
        .expect_err("missing entry must fail");
    match error {
        wc3_mpq::MpqError::FileNotFound(name) => {
            assert!(name.to_ascii_lowercase().contains("nosuchfile_509"));
            println!("MPQ_MISSING name={name}");
        }
        other => panic!("missing entry yielded {other:?}"),
    }
    assert!(!archive.contains("Units\\NoSuchFile_509.mdx"));
}

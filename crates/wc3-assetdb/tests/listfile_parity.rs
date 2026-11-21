//! Listfile parity: the extracted shipped tree against the archive listing that
//! produced it (`analysis/war3_mpq_listing.txt`, one `FILE <path> <size>` line
//! per archive entry).
//!
//! Mechanic: `assets.mpq` — StormLib-compatible MPQ read of every file.
//!
//! Oracle: the operator's extracted retail tree. Every on-disk file must appear
//! in the listing with the same size, and the only listed names without a file
//! are StormLib's two metadata pseudo-entries `(listfile)` and `(attributes)`.
//! The test skips with an explicit line when the tree or listing is absent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use wc3_assetdb::source::{AssetSource, DirSource};

fn oracle() -> Option<(PathBuf, PathBuf)> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tree = base.join("analysis/war3_full3");
    let listing = base.join("analysis/war3_mpq_listing.txt");
    (tree.is_dir() && listing.is_file()).then_some((tree, listing))
}

/// `path` (archive spelling, lowercased) → declared size.
fn read_listing(path: &Path) -> BTreeMap<String, u64> {
    let text = std::fs::read_to_string(path).expect("listing is UTF-8");
    let mut entries = BTreeMap::new();
    let mut archives = 0usize;
    for line in text.lines() {
        if line.starts_with("ARCHIVE ") {
            archives += 1;
            continue;
        }
        let Some(rest) = line.strip_prefix("FILE ") else {
            continue;
        };
        let (name, size) = rest
            .rsplit_once(' ')
            .unwrap_or_else(|| panic!("malformed listing line: {line}"));
        let size: u64 = size.parse().unwrap_or_else(|_| panic!("bad size: {line}"));
        entries.insert(name.to_ascii_lowercase(), size);
    }
    eprintln!("listing archives={archives} files={}", entries.len());
    assert_eq!(archives, 1, "one archive header");
    entries
}

#[test]
fn assets_mpq_listfile_1to1_shipped_tree() {
    let Some((tree, listing)) = oracle() else {
        eprintln!("skip: no extracted tree or listing (analysis/war3_full3)");
        return;
    };
    let listed = read_listing(&listing);
    assert_eq!(listed.len(), 10525, "listed archive entries");

    let source = DirSource::open(&tree).expect("extracted tree opens");
    let files = source.files();
    assert_eq!(files.len(), 10523, "extracted file count");

    let mut disk = BTreeSet::new();
    let mut unmatched = Vec::new();
    let mut disk_bytes = 0u64;
    for (name, size) in &files {
        let key = name.to_ascii_lowercase();
        disk.insert(key.clone());
        match listed.get(&key) {
            Some(declared) => assert_eq!(
                size, declared,
                "size drift between the tree and the archive listing: {name}"
            ),
            None => unmatched.push(name.clone()),
        }
        disk_bytes += size;
    }
    assert!(
        unmatched.is_empty(),
        "{} on-disk files are absent from the listing, first: {:?}",
        unmatched.len(),
        &unmatched[..unmatched.len().min(5)]
    );

    let listed_only: Vec<&str> = listed
        .keys()
        .filter(|name| !disk.contains(*name))
        .map(String::as_str)
        .collect();
    assert_eq!(
        listed_only,
        vec!["(attributes)", "(listfile)"],
        "only StormLib's metadata pseudo-entries are listed without a file"
    );

    let listed_bytes: u64 = files
        .iter()
        .filter_map(|(name, _)| listed.get(&name.to_ascii_lowercase()))
        .sum();
    eprintln!(
        "tree={} listed={} matched={} bytes={} listed_bytes={}",
        files.len(),
        listed.len(),
        files.len(),
        disk_bytes,
        listed_bytes
    );
    assert_eq!(disk_bytes, listed_bytes, "total byte count");
}

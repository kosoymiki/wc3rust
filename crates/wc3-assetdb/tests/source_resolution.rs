//! Provider layering and path-resolution behaviour.
//!
//! Fixtures: two temporary loose trees (no archive needed) for priority and
//! ambiguity, and the real `War3.mpq` for listfile resolution of stale retail
//! sound-table paths.

use std::path::PathBuf;

use wc3_assetdb::source::DirSource;
use wc3_assetdb::{AssetDb, Resolution};

fn mpq_path() -> Option<PathBuf> {
    let path = PathBuf::from(
        std::env::var_os("WC3_MPQ")
            .unwrap_or_else(|| "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq".into()),
    );
    path.is_file().then_some(path)
}

fn index_path() -> Option<PathBuf> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-assets/war3.index");
    path.is_file().then_some(path)
}

fn open_mpq_db() -> Option<AssetDb> {
    let mpq = mpq_path()?;
    match index_path() {
        Some(index) => AssetDb::open_mpq_best(mpq, index).ok().map(|(db, _)| db),
        None => AssetDb::open_mpq(mpq).ok(),
    }
}

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("h2-assets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp tree");
        Self { root }
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.root.join(relative.replace('\\', "/"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dirs");
        }
        std::fs::write(path, bytes).expect("write fixture");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn layer_priority_prefers_the_first_source() {
    let high = TempTree::new("high");
    let low = TempTree::new("low");
    high.write("Textures\\T.blp", b"BLP1-high-priority");
    low.write("Textures\\T.blp", b"BLP1-low-priority");
    let mut db = AssetDb::open_sources(
        vec![
            Box::new(DirSource::open(&high.root).expect("open high")),
            Box::new(DirSource::open(&low.root).expect("open low")),
        ],
        "test-layers",
    )
    .expect("db");
    assert_eq!(db.source_kind(), "layered");
    assert_eq!(db.layer_labels().len(), 2);
    let bytes = db.read("textures/t.blp").expect("read");
    assert_eq!(&bytes[..], b"BLP1-high-priority", "first layer must win");
    // Removing the high layer must fall through to the low one.
    let mut db = AssetDb::open_sources(
        vec![Box::new(DirSource::open(&low.root).expect("open low"))],
        "low-only",
    )
    .expect("db");
    let bytes = db.read("Textures\\T.blp").expect("read");
    assert_eq!(&bytes[..], b"BLP1-low-priority");
}

#[test]
fn ambiguity_is_not_guessed() {
    let tree = TempTree::new("ambiguous");
    tree.write("A\\X.wav", b"aaa");
    tree.write("B\\X.wav", b"bbb");
    let db = AssetDb::open_sources(
        vec![Box::new(DirSource::open(&tree.root).expect("open"))],
        "ambiguous",
    )
    .expect("db");
    // Exact request for a non-existent directory: two basename candidates.
    match db.resolve_asset("C\\X.wav") {
        Resolution::Ambiguous { candidates, .. } => {
            assert_eq!(candidates.len(), 2, "both candidates must be reported");
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    assert!(db.sound_path("C\\X.wav").is_none(), "no path may be chosen");
    // An exact hit still wins over the ambiguous basename.
    assert!(matches!(
        db.resolve_asset("A\\X.wav"),
        Resolution::Exact { .. }
    ));
}

#[test]
fn listfile_resolves_stale_pigfarm_path() {
    let Some(mut db) = open_mpq_db() else {
        eprintln!("skipping: War3.mpq not available");
        return;
    };
    // Retail table says Buildings\Orc\PigFarm\; the file ships under Buildings\Other\.
    let requested = "Buildings\\Orc\\PigFarm\\PigFarmWhat1.wav";
    match db.resolve_asset(requested) {
        Resolution::ListfileMatch { path, layer, .. } => {
            assert_eq!(layer, "War3.mpq");
            assert!(
                path.eq_ignore_ascii_case("buildings\\Other\\PigFarm\\PigFarmWhat1.wav"),
                "canonical path {path}"
            );
            let canonical = db.sound_path(requested).expect("sound_path");
            let sound = db.sound_shared(&canonical).expect("decode resolved sound");
            assert!(sound.frames() > 100, "frames {}", sound.frames());
            assert!(!sound.is_silent(), "resolved sound must not be silent");
        }
        other => panic!("expected a listfile match, got {other:?}"),
    }
}

#[test]
fn absent_retail_path_reports_searched_layers() {
    let Some(db) = open_mpq_db() else {
        return;
    };
    // Arch Druid is The Frozen Throne content; War3.mpq has no such file.
    let requested = "Units\\NightElf\\HeroArchDruid\\HeroArchDruidWhat1.wav";
    match db.resolve_asset(requested) {
        Resolution::Absent { searched, .. } => {
            assert_eq!(searched, vec!["War3.mpq".to_string()]);
        }
        other => panic!("expected Absent, got {other:?}"),
    }
    assert!(db.sound_path(requested).is_none());
}

#[test]
fn exact_path_resolves_case_insensitively_with_canonical_spelling() {
    let Some(db) = open_mpq_db() else {
        return;
    };
    match db.resolve_asset("units/human/footman/footman.mdx") {
        Resolution::Exact { path, .. } => {
            assert!(path.eq_ignore_ascii_case("Units\\Human\\Footman\\Footman.mdx"));
        }
        other => panic!("expected Exact, got {other:?}"),
    }
}

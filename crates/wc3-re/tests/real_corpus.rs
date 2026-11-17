//! Whole-corpus checks that are fast enough for `cargo test`.
//!
//! The full decode/execute sweep (all 3,935 BLP decoded, all 44 map `main`
//! executed) is `wc3re scan`; this test keeps the same fixtures under test but
//! only parses headers for images, so it runs in seconds on a phone.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, ext, out);
        } else if path
            .extension()
            .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(ext))
        {
            out.push(path);
        }
    }
}

#[test]
fn all_blp_headers_and_mdx_models_parse() {
    let root = root();
    if !root.is_dir() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let mut blps = Vec::new();
    walk(&root, "blp", &mut blps);
    assert_eq!(blps.len(), 3935);
    for path in &blps {
        let bytes = std::fs::read(path).unwrap();
        let probe = wc3_re::blp::probe(&bytes)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        assert!(probe.width > 0 && probe.height > 0);
        assert!(probe.mip_count > 0);
    }
    let mut mdxs = Vec::new();
    walk(&root, "mdx", &mut mdxs);
    assert_eq!(mdxs.len(), 2048);
    for path in &mdxs {
        let bytes = std::fs::read(path).unwrap();
        wc3_re::mdx::summarize(&bytes)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    }
}

#[test]
fn all_tables_scripts_and_maps_parse() {
    let root = root();
    if !root.is_dir() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let mut slks = Vec::new();
    walk(&root, "slk", &mut slks);
    assert_eq!(slks.len(), 33);
    for path in slks {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        wc3_re::slk::Sheet::parse(name, &std::fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    }
    for name in ["common.j", "Blizzard.j", "Cheats.j"] {
        let source = std::fs::read_to_string(root.join("Scripts").join(name)).unwrap();
        let program =
            wc3_re::jass::parse(&source).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(!program.natives.is_empty() || !program.functions.is_empty());
    }
    let scripts = root.join("Scripts");
    let census = wc3_re::ai::scan_ai_dir(&scripts, 7).expect("AI census");
    assert_eq!(census.scripts, 91);
    assert_eq!(census.failed.len(), 0);
    assert_eq!(census.init_runs, 91);
    let maps = wc3_re::mapfiles::discover_maps(&root.join("Maps")).expect("discover maps");
    assert_eq!(maps.len(), 44);
    let mut wts_entries = 0;
    for path in maps {
        let bundle = wc3_re::mapfiles::MapBundle::open(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        assert!(bundle.errors.is_empty(), "{}: {:?}", path.display(), bundle.errors);
        assert!(bundle.script.is_some(), "{} has no script", path.display());
        wts_entries += bundle.wts.as_ref().map_or(0, |wts| wts.entries.len());
    }
    assert_eq!(wts_entries, 4428);
}

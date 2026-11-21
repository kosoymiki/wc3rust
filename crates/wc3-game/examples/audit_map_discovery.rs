//! WC3-I-020 probe: map discovery terminates on symlink cycles.
//!
//! Builds a temporary tree with a self-link, a parent link, a two-directory
//! cycle and two real maps, then runs `wc3_re::mapfiles::discover_maps`. The old
//! walk used `Path::is_dir()` (follows symlinks) and recursed until the stack or
//! the user gave out; the current walk keys visited directories by canonical
//! path and bounds depth/entries. The probe also checks a permission-denied
//! directory is skipped instead of failing the whole discovery.
//!
//! Run: `scripts/h2-cargo run --release -p wc3-game --example audit_map_discovery`.

use std::fs;
use std::path::Path;
use std::time::Instant;

use wc3_re::mapfiles::discover_maps;

fn touch(path: &Path) {
    fs::write(path, b"W3X").expect("write map file");
}

fn link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("symlink");
    #[cfg(not(unix))]
    let _ = (target, link);
}

fn main() {
    let root = std::env::temp_dir().join(format!("wc3-i020-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let maps = root.join("maps");
    let a = maps.join("a");
    let b = maps.join("b");
    fs::create_dir_all(&a).expect("mkdir a");
    fs::create_dir_all(&b).expect("mkdir b");
    touch(&a.join("real-one.w3x"));
    touch(&b.join("real-two.w3m"));
    touch(&b.join("skip.txt"));
    // Cycles: a/self -> a, a/parent -> root, b/to-a -> a, a/to-b -> b.
    link(&a, &a.join("self"));
    link(&root, &a.join("parent"));
    link(&a, &b.join("to-a"));
    link(&b, &a.join("to-b"));
    // A symlinked *file* is still discovered by extension.
    link(&a.join("real-one.w3x"), &b.join("linked.w3n"));
    // Permission-denied directory: skipped, not fatal.
    let locked = maps.join("locked");
    fs::create_dir_all(&locked).expect("mkdir locked");
    touch(&locked.join("hidden.w3x"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o000));
    }

    println!("== map discovery symlink cycles (WC3-I-020) ==");
    let started = Instant::now();
    let found = discover_maps(&root).expect("discovery returns");
    let elapsed = started.elapsed();
    let mut names: Vec<String> = found
        .iter()
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect();
    names.sort();
    println!("root={} elapsed_ms={}", root.display(), elapsed.as_millis());
    println!("found={} {names:?}", found.len());
    println!(
        "real_maps_ok={} linked_file_ok={} locked_skipped={}",
        names.iter().any(|name| name.ends_with("a/real-one.w3x"))
            && names.iter().any(|name| name.ends_with("b/real-two.w3m")),
        names.iter().any(|name| name.ends_with("b/linked.w3n")),
        !names.iter().any(|name| name.ends_with("locked/hidden.w3x")),
    );
    assert!(
        elapsed.as_secs() < 10,
        "discovery must terminate on cycles, took {elapsed:?}"
    );
    assert_eq!(
        found.iter().filter(|path| path.ends_with("real-one.w3x")).count(),
        1,
        "a cyclic link must not duplicate a map"
    );

    // Restore permissions before cleanup (best effort: the files are ours).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o700));
    }
    let _ = fs::remove_dir_all(&root);
    println!("MAP_DISCOVERY_PROBE PASS");
}

//! Audit lane H probe: map content classification + data-layer arithmetic.
//!
//! 1. `war3map.doo` records are spawned as destructables by
//!    `wc3_re::engine::spawn_map_entities`; check how many Prologue01 records
//!    are actually `Doodads.slk` rows (decorations) rather than
//!    `DestructableData.slk` rows.
//! 2. `UpgradeView::gold_cost` multiplies a map-controlled `i32` modifier; a
//!    `war3map.w3q` override can overflow it.

use std::path::{Path, PathBuf};

use wc3_data::source::{DirSource, MemorySource};
use wc3_data::GameDatabase;
use wc3_re::mapfiles::MapBundle;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/audit_h_prologue01.w3m");
    if !tmp.is_file() {
        println!("prologue temp map missing: {}", tmp.display());
        return;
    }
    let root = repo.join("analysis/war3x_data");
    println!("data root={}", root.display());
    let db = GameDatabase::load(&DirSource::new(&root)).expect("load game data");
    let bundle = MapBundle::open(&tmp).expect("open map");
    let doodads = bundle.doodads.as_deref().unwrap_or_default();
    let (mut destructable_rows, mut doodad_rows, mut unknown) = (0usize, 0usize, 0usize);
    let mut unknown_ids = std::collections::BTreeSet::new();
    for placed in doodads {
        let id = wc3_re::fourcc_str(&placed.type_id);
        if db.destructable(&id).is_some() {
            destructable_rows += 1;
        } else if db.doodad(&id).is_some() {
            doodad_rows += 1;
        } else {
            unknown += 1;
            unknown_ids.insert(id);
        }
    }
    println!(
        "war3map.doo records={} destructable_rows={} doodad_rows={} unknown={} unknown_sample={:?}",
        doodads.len(),
        destructable_rows,
        doodad_rows,
        unknown,
        unknown_ids.iter().take(12).collect::<Vec<_>>()
    );

    // 2. i32 overflow in an upgrade cost, from a map-controlled object field.
    let mut source = MemorySource::new();
    source.insert(
        "Units/UpgradeData.slk",
        b"ID;PWXL;N;E\nB;Y2;X4;D0\n\
C;Y1;X1;K\"upgradeid\"\nC;X2;K\"goldbase\"\nC;X3;K\"goldmod\"\nC;X4;K\"maxlevel\"\n\
C;Y2;X1;K\"Xaud\"\nC;X2;K1\nC;X3;K2147483647\nC;X4;K3\n"
            .to_vec(),
    );
    let data = GameDatabase::load(&source).expect("load synthetic upgrade");
    let upgrade = data.upgrade("Xaud").expect("upgrade row");
    let result = std::thread::scope(|scope| scope.spawn(|| upgrade.gold_cost(3)).join());
    match result {
        Ok(value) => println!("upgrade gold_cost(level 3) with goldmod=i32::MAX: {value:?}"),
        Err(_) => println!("upgrade gold_cost(level 3) with goldmod=i32::MAX: PANIC (i32 overflow)"),
    }

    // 3. DirSource path resolution with `..` (hygiene check).
    let base = PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/audit_h_traversal");
    let _ = std::fs::create_dir_all(base.join("root"));
    let _ = std::fs::write(base.join("secret.txt"), b"outside");
    let dir = DirSource::new(base.join("root"));
    println!(
        "DirSource::resolve(\"../secret.txt\") = {:?}",
        dir.resolve("../secret.txt")
    );
}

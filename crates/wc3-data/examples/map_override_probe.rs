//! What a map's object data resolves to on top of the shipped database.
//!
//! Opens `DotF_v1.4.w3x`, applies its `war3map.wts` + `war3map.w3u/.w3a/.w3t`,
//! and prints the resolved fields of a few custom rows next to their prototypes
//! — the measurement behind `tests/data_coverage.rs::
//! wts_and_object_data_override_the_profile_text` and the prototype-fallback
//! rule in `GameDatabase::object`.
//!
//! ```text
//! cargo run -p wc3-data --example map_override_probe
//! ```

use std::path::Path;

use wc3_data::source::DirSource;
use wc3_data::GameDatabase;
use wc3_formats::gamedata::{ObjectKind, Wts};
use wc3_formats::map::map_mpq_base;
use wc3_mpq::MpqArchive;

fn main() {
    let root = Path::new("/data/data/com.termux/files/home/src/wc3rust");
    let mut data = GameDatabase::load(&DirSource::new(root.join("analysis/war3x_data"))).expect("load");
    // The example needs a map that carries `war3map.wts` + object data. The
    // War3Net donor tree this originally pointed at is not part of this
    // checkout, so the map is an explicit argument; the donor path stays only
    // as the default for trees that still have it.
    let map = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            root.join(
                "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/DotF_v1.4.w3x",
            )
        });
    if !map.is_file() {
        eprintln!("map fixture not found: {}", map.display());
        eprintln!("Pass an MPQ .w3x that carries war3map.wts and war3map.w3u/.w3a/.w3t:");
        eprintln!("  cargo run -p wc3-data --example map_override_probe -- /path/to/map.w3x");
        eprintln!("For extracted object data without an archive, use dump_gamedata:");
        eprintln!("  dump_gamedata --object-data analysis/war3_map/Human04/war3map.w3u \\");
        eprintln!("                --wts analysis/war3_map/Human04/war3map.wts objectdata");
        std::process::exit(2);
    }
    let base = map_mpq_base(&map).expect("header");
    let archive = MpqArchive::open_at(&map, base).expect("open");
    let wts = Wts::parse(&archive.read("war3map.wts").expect("wts")).expect("parse");
    data.localizer_mut()
        .set_wts_entries(wts.iter().map(|(id, text)| (id, text.to_string())).collect());
    for (file, kind) in [
        ("war3map.w3u", ObjectKind::Unit),
        ("war3map.w3a", ObjectKind::Ability),
        ("war3map.w3t", ObjectKind::Item),
    ] {
        let bytes = archive.read(file).expect("object data");
        data.apply_object_data(file, kind, &bytes).expect("apply");
    }
    for id in ["U000", "Ucrl", "I000", "ankh", "A000", "A001"] {
        match data.object(id) {
            Some(info) => println!(
                "{id}: kind={:?} name={:?} icon={:?} hotkey={:?} model={:?} tip={:?} gold={:?} hp={:?} level={:?}",
                info.kind,
                info.name,
                info.icon,
                info.hotkey,
                info.model,
                info.ubertip.is_some() || info.tip.is_some(),
                info.stats.gold_cost,
                info.stats.hp,
                info.stats.level,
            ),
            None => println!("{id}: <no row>"),
        }
    }
    println!("prototype U000 = {:?}", data.object_data().prototype_of("U000"));
    println!("prototype I000 = {:?}", data.object_data().prototype_of("I000"));
}

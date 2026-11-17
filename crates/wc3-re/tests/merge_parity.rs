//! Merge-parity gates for the `wc3-re` → base absorption.
//!
//! Each test pins one deduplication decision from
//! `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`:
//!
//! 1. `wc3-re::mpqex` codecs are type-identical re-exports of `wc3-mpq`.
//! 2. `wc3-re::slk::Sheet` delegates to `wc3_formats::slk` and still produces
//!    the scanner's row/column totals.
//! 3. `wc3-re::mapobjects` and `wc3-formats::objects` (now a projection over
//!    `wc3-formats::gamedata`) agree on a real v1 two-section `war3map.w3u`.
//! 4. `wc3-re::mapfiles::MapBundle` is the single map reader: it carries the
//!    real terrain, placed units, object counts and the WTS-resolved name
//!    (`wc3-formats::map::W3Map` was removed).

use std::path::{Path, PathBuf};

fn install() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3")
}

fn fixture(path: &str) -> Option<Vec<u8>> {
    std::fs::read(install().join(path)).ok()
}

#[test]
fn mpqex_codecs_are_wc3_mpq_reexports() {
    // Type identity: if these assignments compile, there is exactly one codec
    // implementation (the wrapper re-exports the canonical module).
    let _: fn(&[u8]) -> Result<Vec<u8>, wc3_mpq::pkware::ExplodeError> =
        wc3_re::mpqex::pkware::explode;
    let _: fn(&[u8]) -> Result<Vec<u8>, wc3_mpq::huffman::HuffmanError> =
        wc3_re::mpqex::huffman::decompress;
    let _: fn(&[u8], usize) -> Result<Vec<u8>, wc3_mpq::MpqError> =
        wc3_re::mpqex::adpcm::decompress;
}

#[test]
fn slk_delegation_preserves_scanner_totals() {
    let root = install();
    if !root.is_dir() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let mut files = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "slk") {
                out.push(path);
            }
        }
    }
    walk(&root, &mut files);
    assert_eq!(files.len(), 33, "SLK file count");
    let mut rows = 0;
    let mut columns = 0;
    for path in &files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let sheet = wc3_re::slk::Sheet::parse(name.clone(), &std::fs::read(path).unwrap())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        rows += sheet.len();
        columns += sheet.columns.len();
    }
    // Values from `wc3re scan` before the delegation; the canonical parser must
    // produce a byte-identical view.
    assert_eq!(rows, 6785, "SLK rows");
    assert_eq!(columns, 815, "SLK columns");
}

fn re_value(value: &wc3_re::mapobjects::ModValue) -> String {
    use wc3_re::mapobjects::ModValue;
    match value {
        ModValue::Int(v) => v.to_string(),
        ModValue::Real(v) | ModValue::Unreal(v) => v.to_string(),
        ModValue::String(v) => v.clone(),
        ModValue::Int64(v) => v.to_string(),
    }
}

fn formats_value(value: &wc3_formats::objects::ModValue) -> String {
    use wc3_formats::objects::ModValue;
    match value {
        ModValue::Int(v) => v.to_string(),
        ModValue::Real(v) | ModValue::Unreal(v) => v.to_string(),
        ModValue::String(v) => v.clone(),
    }
}

#[test]
fn object_data_parsers_agree_on_real_v1_map() {
    let Some(bytes) = fixture("../war3_map/Human04/war3map.w3u") else {
        eprintln!("skip: Human04 w3u missing");
        return;
    };
    let re = wc3_re::mapobjects::MapObjectData::parse(&bytes).expect("wc3-re parse");
    let formats = wc3_formats::objects::ObjectData::parse_as(
        wc3_formats::gamedata::ObjectKind::Unit,
        &bytes,
    )
    .expect("wc3-formats parse");
    assert_eq!(re.entry_count(), formats.entries.len(), "entry count");
    for (left, right) in re.entries().zip(formats.entries.iter()) {
        assert_eq!(left.old_id, right.old_id, "old id");
        assert_eq!(left.new_id, right.new_id, "new id");
        assert_eq!(
            left.modifications.len(),
            right.modifications.len(),
            "modification count for {}",
            wc3_re::fourcc_str(&left.new_id)
        );
        for (lmod, rmod) in left.modifications.iter().zip(right.modifications.iter()) {
            assert_eq!(lmod.id, rmod.id, "modification id");
            assert_eq!(
                re_value(&lmod.value),
                formats_value(&rmod.value),
                "value for {}",
                wc3_re::fourcc_str(&lmod.id)
            );
        }
    }
}

#[test]
fn object_db_uses_canonical_typed_sheets_and_metadata() {
    let root = install();
    if !root.is_dir() {
        eprintln!("skip: extracted install missing");
        return;
    }
    let db = wc3_re::objectdata::ObjectDb::load_dir(&root).expect("object db");
    assert!(db.metadata_table.is_some(), "UnitMetaData MetaTable");
    let sheet = db.sheets.get("UnitData").expect("typed UnitData sheet");
    assert!(sheet.contains("hfoo"), "footman row");
    assert!(sheet.cell("hfoo", "unitID").is_some(), "unitID cell");
    let meta = db.metadata.get("uhpm").expect("uhpm metadata projection");
    assert_eq!(meta.slk, "UnitBalance");
    assert_eq!(meta.field, "HP");
}

#[test]
fn map_bundle_reads_terrain_units_objects_and_wts() {
    let root = install();
    let map = root.join("Maps/Campaign/Human04.w3m");
    if !map.is_file() {
        eprintln!("skip: Human04.w3m missing");
        return;
    }
    let bundle = wc3_re::mapfiles::MapBundle::open(&map).expect("MapBundle");
    let terrain = bundle.terrain.as_ref().expect("terrain");
    assert!(terrain.width > 2 && terrain.height > 2, "terrain grid");
    let units = bundle
        .units
        .as_ref()
        .expect("Human04 war3mapUnits.doo must parse (v7/sub9 random_type=-1 carries no payload)");
    assert_eq!(units.len(), 90, "Human04 war3mapUnits.doo placement count");
    assert!(
        units.iter().all(|unit| unit.type_id != [0, 0, 0, 0]),
        "every placed unit has a fourcc type"
    );
    assert!(
        bundle.errors.is_empty(),
        "Human04 map files parse without errors: {:?}",
        bundle.errors
    );
    let unit_objects = bundle
        .objects
        .get("war3map.w3u")
        .map_or(0, |objects| objects.entry_count());
    assert!(unit_objects > 0, "unit object data parsed");
    let localized = bundle.localized_info().expect("localized info");
    assert!(
        !localized.name.starts_with("TRIGSTR_"),
        "unresolved map name: {}",
        localized.name
    );
}

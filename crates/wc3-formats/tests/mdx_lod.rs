//! Geoset LOD: what the shipped models actually carry, and where the field lives.
//!
//! The geoset chunk has a level-of-detail index from MDX v900 on (`lod` +
//! `lod_name`). Two facts have to be separated:
//!
//! * the **parser contract** — a v900 file must expose `lod`, a v800 file must
//!   report `0` (no such field on disk). That is proven white-box against the
//!   geoset reader in `crates/wc3-formats/src/mdx.rs`
//!   (`geoset_lod_fields_follow_the_file_version`), because the public
//!   `MdxModel::parse` gate is classic-only;
//! * the **installed archive** — every `.mdx` in `War3.mpq` is v800, so no
//!   shipped geoset carries a LOD index and a "draw only the lowest LOD" filter
//!   is a no-op for this install. A doubled body therefore cannot come from LOD
//!   here; the renderer must look at its draw loop / skinning instead.

use std::path::Path;

use wc3_formats::mdx::{Geoset, MdxModel};
use wc3_mpq::MpqArchive;

const MPQ: &str = "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq";

// ---- synthetic v800 model (the version the archive ships) ---------------

fn push_tag(out: &mut Vec<u8>, tag: &[u8; 4]) {
    out.extend_from_slice(tag);
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// One v800 geoset: a single triangle, no extents, one UV set.
fn geoset_v800(material_id: u32) -> Vec<u8> {
    let mut body = Vec::new();
    push_tag(&mut body, b"VRTX");
    push_u32(&mut body, 3);
    for vertex in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] {
        for component in vertex {
            push_f32(&mut body, component);
        }
    }
    push_tag(&mut body, b"NRMS");
    push_u32(&mut body, 3);
    for _ in 0..3 {
        for component in [0.0f32, 0.0, 1.0] {
            push_f32(&mut body, component);
        }
    }
    push_tag(&mut body, b"PTYP");
    push_u32(&mut body, 1);
    push_u32(&mut body, 4); // triangle list
    push_tag(&mut body, b"PCNT");
    push_u32(&mut body, 1);
    push_u32(&mut body, 3);
    push_tag(&mut body, b"PVTX");
    push_u32(&mut body, 3);
    for index in [0u16, 1, 2] {
        body.extend_from_slice(&index.to_le_bytes()); // v800 face indices are u16
    }
    push_tag(&mut body, b"GNDX");
    push_u32(&mut body, 3);
    body.extend_from_slice(&[0, 0, 0]);
    push_tag(&mut body, b"MTGC");
    push_u32(&mut body, 1);
    push_u32(&mut body, 1); // one matrix in group 0
    push_tag(&mut body, b"MATS");
    push_u32(&mut body, 1);
    push_u32(&mut body, 0); // bone 0
    push_u32(&mut body, material_id);
    push_u32(&mut body, 0); // selection group
    push_u32(&mut body, 0); // selection flags
    // v800: no lod / lod_name fields at all.
    push_f32(&mut body, 1.0); // bounds radius
    for component in [0.0f32, 0.0, 0.0] {
        push_f32(&mut body, component);
    }
    for component in [1.0f32, 0.0, 1.0] {
        push_f32(&mut body, component);
    }
    push_u32(&mut body, 0); // no per-sequence extents
    push_tag(&mut body, b"UVAS");
    push_u32(&mut body, 1);
    push_tag(&mut body, b"UVBS");
    push_u32(&mut body, 3);
    for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        push_f32(&mut body, uv[0]);
        push_f32(&mut body, uv[1]);
    }

    let mut out = Vec::new();
    push_u32(&mut out, (body.len() + 4) as u32); // inclusive size
    out.extend_from_slice(&body);
    out
}

fn model_v800() -> Vec<u8> {
    let mut geosets = Vec::new();
    geosets.extend_from_slice(&geoset_v800(0));
    geosets.extend_from_slice(&geoset_v800(1));

    let mut out = Vec::new();
    push_tag(&mut out, b"MDLX");
    push_tag(&mut out, b"VERS");
    push_u32(&mut out, 4);
    push_u32(&mut out, 800);
    push_tag(&mut out, b"GEOS");
    push_u32(&mut out, geosets.len() as u32);
    out.extend_from_slice(&geosets);
    out
}

#[test]
fn v800_geosets_report_lod_zero() {
    let model = MdxModel::parse(&model_v800()).expect("parse v800 model");
    assert_eq!(model.version, 800);
    assert_eq!(model.geosets.len(), 2);
    assert_eq!(
        model.geosets.iter().map(|geoset| geoset.lod).collect::<Vec<_>>(),
        [0, 0]
    );
    assert!(model.geosets.iter().all(|geoset| geoset.lod_name.is_empty()));
    assert_eq!(Geoset::lowest_lod(&model.geosets), Some(0));
    assert!(model.geosets.iter().all(|geoset| geoset.drawn_at(0)));
    // Geometry is unaffected by the version branch.
    assert_eq!(model.geosets[0].faces, [0, 1, 2]);
    assert_eq!(model.geosets[0].vertices.len(), 3);
    assert_eq!(model.geosets[0].uv_sets.len(), 1);
    assert_eq!(model.geosets[1].material_id, 1);
}

/// The public entry point refuses non-classic models instead of half-parsing
/// them, so a v900+ LOD index can never reach a consumer of this parser today.
#[test]
fn non_classic_models_are_rejected_not_half_parsed() {
    let mut bytes = Vec::new();
    push_tag(&mut bytes, b"MDLX");
    push_tag(&mut bytes, b"VERS");
    push_u32(&mut bytes, 4);
    push_u32(&mut bytes, 901);
    let error = MdxModel::parse(&bytes).expect_err("v901 must be rejected");
    let text = format!("{error}");
    assert!(
        text.contains("version"),
        "unexpected rejection reason: {text}"
    );
}

// ---- installed archive --------------------------------------------------

fn archive() -> Option<MpqArchive> {
    let path = Path::new(MPQ);
    if !path.is_file() {
        eprintln!("skip: {MPQ} not installed");
        return None;
    }
    Some(MpqArchive::open(path).expect("open War3.mpq"))
}

fn model(archive: &MpqArchive, path: &str) -> MdxModel {
    let bytes = archive
        .read(path)
        .unwrap_or_else(|error| panic!("read {path}: {error}"));
    MdxModel::parse(&bytes).unwrap_or_else(|error| panic!("parse {path}: {error}"))
}

fn lod_groups(model: &MdxModel) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = Vec::new();
    for geoset in &model.geosets {
        match out.iter_mut().find(|(lod, _)| *lod == geoset.lod) {
            Some((_, count)) => *count += 1,
            None => out.push((geoset.lod, 1)),
        }
    }
    out.sort();
    out
}

/// Geoset inventory of the unit models named in the report, with the measured
/// LOD answer: one group, LOD 0.
#[test]
fn shipped_unit_models_have_one_lod_group() {
    let Some(archive) = archive() else { return };
    for (path, expected_geosets) in [
        ("units\\orc\\Peon\\Peon.mdx", 5usize),
        ("units\\human\\Footman\\Footman.mdx", 5),
        ("units\\nightelf\\Archer\\Archer.mdx", 4),
    ] {
        let model = model(&archive, path);
        println!(
            "{path}: version={} geosets={} materials={} bones={}",
            model.version,
            model.geosets.len(),
            model.materials.len(),
            model.bones.len()
        );
        for (index, geoset) in model.geosets.iter().enumerate() {
            println!(
                "  geoset[{index}] lod={} lod_name={:?} material={} selection={} verts={} tris={}",
                geoset.lod,
                geoset.lod_name,
                geoset.material_id,
                geoset.selection_group,
                geoset.vertices.len(),
                geoset.faces.len() / 3
            );
        }
        let groups = lod_groups(&model);
        println!("  lod groups = {groups:?}");

        assert_eq!(model.geosets.len(), expected_geosets, "{path} geosets");
        assert_eq!(model.version, 800, "{path} is a v800 model");
        assert_eq!(groups, [(0, expected_geosets)], "{path} LOD groups");
        assert_eq!(Geoset::lowest_lod(&model.geosets), Some(0));
        assert!(
            model.geosets.iter().all(|geoset| geoset.lod_name.is_empty()),
            "{path} must not report a lod name"
        );
        assert!(
            model
                .geosets
                .iter()
                .all(|geoset| (geoset.material_id as usize) < model.materials.len()),
            "{path} material ids"
        );
    }
}

/// Archive-wide answer: no model in `War3.mpq` carries the LOD field.
///
/// This is the fact the render layer needs — a `lowest_lod()` filter changes
/// nothing for this install, so a doubled body cannot come from LOD here.
#[test]
fn the_installed_archive_ships_no_lod_index() {
    let Some(archive) = archive() else { return };
    let models: Vec<&String> = archive
        .listfile()
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".mdx"))
        .collect();
    assert!(
        models.len() > 1900,
        "listfile .mdx count = {}",
        models.len()
    );
    let mut non_v800 = Vec::new();
    for path in &models {
        let Ok(bytes) = archive.read(path) else { continue };
        if bytes.len() < 16 || &bytes[..4] != b"MDLX" || &bytes[4..8] != b"VERS" {
            non_v800.push(format!("{path} (bad header)"));
            continue;
        }
        let version = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
        if version != 800 {
            non_v800.push(format!("{path} (v{version})"));
        }
    }
    assert!(
        non_v800.is_empty(),
        "expected every model to be v800, found {}: {:?}",
        non_v800.len(),
        &non_v800[..non_v800.len().min(10)]
    );
}

//! artD_crosscheck — Agent D (all-layer push 2026-11-21) whole-tree evidence
//! for the per-sequence geoset visibility rule and the ghost Stand set.
//!
//! Oracle: the shipped v800 tree `analysis/war3_full3` (or its `war3_full`
//! copy, byte-identical). The alpha values themselves are cross-checked by
//! `analysis/agent-anim/artD_crosscheck_20261121.py`, an independent Python
//! parser (`probe_mdx.py`) that recomputes the documented rule from the raw
//! bytes: 0 mismatches over 2 048 models / 6 512 sequences / 33 354
//! (sequence, geoset) slots on 2026-11-21. The fixture below is that oracle's
//! output for the four pinned models, so the exact alpha trajectory of the
//! Peon, Footman, Great Hall and Town Hall is checked without the script.
//!
//! Record sizes verified here from the bytes themselves: `SEQS` 132 B,
//! `TEXS` 268 B, `GEOS`/`GEOA` self-sized records.

use std::path::{Path, PathBuf};

use wc3_anim::runtime::{helper_plane, mesh_alphas, ModelRuntime};
use wc3_formats::mdx::MdxModel;

const SEQ_RECORD: usize = 132;
const TEX_RECORD: usize = 268;

/// Pinned counts of the shipped v800 tree (2026-11-21, byte walked).
const SHIPPED_MODELS: usize = 2048;
const SHIPPED_SEQUENCES: usize = 6512;
const SHIPPED_GEOSETS: usize = 5127;
const SHIPPED_GEOSET_ANIMATIONS: usize = 3093;
const SHIPPED_TEXTURES: usize = 5216;
/// Sequences that exclude at least one geoset by per-sequence extent: if the
/// extent gate ever regresses, those draw extra geosets ("all geosets drawn").
const GATED_SEQUENCES: usize = 3098;
/// Buildings whose first Stand sequence exists (ghost default frame).
const GHOST_BUILDINGS: usize = 125;
/// Fixture rows (4 models, every sequence, 3 sample times).
const FIXTURE_ROWS: usize = 63;

fn tree_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for tree in ["analysis/war3_full3", "analysis/war3_full"] {
        let candidate = root.join(tree);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn mdx_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mdx"))
            {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    files
}

fn load(root: &Path, relative: &str) -> Option<MdxModel> {
    let bytes = std::fs::read(root.join(relative)).ok()?;
    Some(MdxModel::parse(&bytes).expect("parse shipped MDX"))
}

/// Raw chunk walk, independent of `wc3_formats::mdx`: tag + payload slices,
/// `None` when the file is not a clean MDLX chunk stream.
fn chunk_payloads(bytes: &[u8]) -> Option<Vec<([u8; 4], &[u8])>> {
    if bytes.len() < 4 || &bytes[..4] != b"MDLX" {
        return None;
    }
    let mut out = Vec::new();
    let mut position = 4usize;
    while position + 8 <= bytes.len() {
        let tag: [u8; 4] = bytes[position..position + 4].try_into().ok()?;
        let size =
            u32::from_le_bytes(bytes[position + 4..position + 8].try_into().ok()?) as usize;
        let start = position + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        out.push((tag, &bytes[start..end]));
        position = end;
    }
    (position == bytes.len()).then_some(out)
}

/// Count records whose first `u32` is the whole record size (GEOS/GEOA).
fn count_self_sized_records(payload: &[u8]) -> Option<usize> {
    let mut position = 0usize;
    let mut count = 0usize;
    while position < payload.len() {
        if position + 4 > payload.len() {
            return None;
        }
        let size =
            u32::from_le_bytes(payload[position..position + 4].try_into().ok()?) as usize;
        if size < 4 || position + size > payload.len() {
            return None;
        }
        position += size;
        count += 1;
    }
    Some(count)
}

fn chunk<'a>(payloads: &'a [([u8; 4], &'a [u8])], tag: &[u8; 4]) -> Vec<&'a [u8]> {
    payloads
        .iter()
        .filter(|(candidate, _)| candidate == tag)
        .map(|(_, payload)| *payload)
        .collect()
}

#[test]
fn artD_seq_records_132() {
    let Some(root) = tree_root() else {
        eprintln!("ARTD_SEQ_RECORDS SKIP: shipped MDX tree absent");
        return;
    };
    let files = mdx_files(&root);
    assert_eq!(
        files.len(),
        SHIPPED_MODELS,
        "shipped tree file count changed: {:?}",
        root
    );

    let mut sequences = 0usize;
    let mut geosets = 0usize;
    let mut geoset_animations = 0usize;
    let mut textures = 0usize;
    for path in &files {
        let relative = path.strip_prefix(&root).unwrap_or(path).display();
        let bytes = std::fs::read(path).expect("read shipped MDX");
        let payloads = chunk_payloads(&bytes)
            .unwrap_or_else(|| panic!("{relative}: not a clean MDLX chunk stream"));
        let model =
            MdxModel::parse(&bytes).unwrap_or_else(|error| panic!("{relative}: {error:?}"));
        assert_eq!(model.version, 800, "{relative}: not a v800 MDX");

        // SEQS: fixed 132 B records (name[80], interval[2], moveSpeed, flags,
        // rarity, syncPoint, extent radius+min+max = 28 B).
        let seq_payload: usize = chunk(&payloads, b"SEQS").iter().map(|p| p.len()).sum();
        assert_eq!(seq_payload % SEQ_RECORD, 0, "{relative}: SEQS size {seq_payload}");
        assert_eq!(
            model.sequences.len(),
            seq_payload / SEQ_RECORD,
            "{relative}: SEQS record count"
        );
        // TEXS: fixed 268 B records.
        let tex_payload: usize = chunk(&payloads, b"TEXS").iter().map(|p| p.len()).sum();
        assert_eq!(tex_payload % TEX_RECORD, 0, "{relative}: TEXS size {tex_payload}");
        assert_eq!(
            model.textures.len(),
            tex_payload / TEX_RECORD,
            "{relative}: TEXS record count"
        );
        // GEOS/GEOA: self-sized records, fully consumed.
        for payload in chunk(&payloads, b"GEOS") {
            assert_eq!(
                count_self_sized_records(payload),
                Some(model.geosets.len()),
                "{relative}: GEOS record count"
            );
        }
        for payload in chunk(&payloads, b"GEOA") {
            assert_eq!(
                count_self_sized_records(payload),
                Some(model.geoset_animations.len()),
                "{relative}: GEOA record count"
            );
        }

        sequences += model.sequences.len();
        geosets += model.geosets.len();
        geoset_animations += model.geoset_animations.len();
        textures += model.textures.len();
    }

    assert_eq!(sequences, SHIPPED_SEQUENCES, "total SEQS records");
    assert_eq!(geosets, SHIPPED_GEOSETS, "total GEOS records");
    assert_eq!(
        geoset_animations, SHIPPED_GEOSET_ANIMATIONS,
        "total GEOA records"
    );
    assert_eq!(textures, SHIPPED_TEXTURES, "total TEXS records");
    eprintln!(
        "ARTD_SEQ_RECORDS models={} sequences={} (SEQS/132) geosets={} geoa={} textures={} (TEXS/268) mismatches=0 PASS",
        files.len(),
        sequences,
        geosets,
        geoset_animations,
        textures
    );
}

struct FixtureRow {
    model: String,
    index: usize,
    name: String,
    start: u32,
    end: u32,
    flags: u32,
    times: [u32; 3],
    alphas: [Vec<f32>; 3],
}

fn fixture_rows() -> Vec<FixtureRow> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/artD/crosscheck.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 13, "fixture row shape: {line}");
        let floats = |field: &str| -> Vec<f32> {
            field
                .split(',')
                .filter(|value| !value.is_empty())
                .map(|value| value.parse::<f32>().expect("fixture float"))
                .collect()
        };
        rows.push(FixtureRow {
            model: fields[0].to_owned(),
            index: fields[2].parse().expect("fixture sequence index"),
            name: fields[3].to_owned(),
            start: fields[4].parse().expect("fixture start"),
            end: fields[5].parse().expect("fixture end"),
            flags: fields[6].parse().expect("fixture flags"),
            times: [
                fields[7].parse().expect("fixture t0"),
                fields[8].parse().expect("fixture t1"),
                fields[9].parse().expect("fixture t2"),
            ],
            alphas: [floats(fields[10]), floats(fields[11]), floats(fields[12])],
        });
    }
    rows
}

/// Exact alpha trajectory of the four pinned models, generated from the raw
/// bytes by the independent Python parser. Covers step, linear, bezier and
/// global-sequence tracks (the whole shipped KGAO distribution).
#[test]
fn artD_sequence_alpha_sets_match_oracle() {
    let Some(root) = tree_root() else {
        eprintln!("ARTD_SEQUENCE_ALPHAS SKIP: shipped MDX tree absent");
        return;
    };
    let rows = fixture_rows();
    assert_eq!(rows.len(), FIXTURE_ROWS, "fixture row count");

    let mut current: Option<(String, MdxModel)> = None;
    for row in &rows {
        if current.as_ref().map(|(model, _)| model) != Some(&row.model) {
            let model = load(&root, &row.model)
                .unwrap_or_else(|| panic!("{}: fixture model absent", row.model));
            current = Some((row.model.clone(), model));
        }
        let (_, model) = current.as_ref().expect("model loaded");
        let sequence = model.sequences.get(row.index).unwrap_or_else(|| {
            panic!(
                "{}: fixture sequence {} absent ({} sequences)",
                row.model,
                row.index,
                model.sequences.len()
            )
        });
        assert_eq!(sequence.name, row.name, "{}: sequence name", row.model);
        assert_eq!(sequence.start, row.start, "{}: sequence start", row.model);
        assert_eq!(sequence.end, row.end, "{}: sequence end", row.model);
        assert_eq!(sequence.flags, row.flags, "{}: sequence flags", row.model);

        let runtime = ModelRuntime::new(model);
        let hidden = &runtime.helper_planes;
        for sample in 0..3 {
            let alphas = runtime.alphas(model, Some(sequence), row.times[sample]);
            assert_eq!(
                alphas.len(),
                row.alphas[sample].len(),
                "{} seq {}: geoset count",
                row.model,
                row.name
            );
            for (index, (actual, expected)) in
                alphas.iter().zip(row.alphas[sample].iter()).enumerate()
            {
                assert!(
                    (actual - expected).abs() <= 1e-3,
                    "{} seq {:?} t={} geoset {index}: port {actual} != oracle {expected}",
                    row.model,
                    row.name,
                    row.times[sample]
                );
            }
            assert_eq!(
                mesh_alphas(model, hidden, Some(sequence), row.times[sample]),
                alphas,
                "{}: ModelRuntime::alphas must equal mesh_alphas",
                row.model
            );
        }
    }
    eprintln!(
        "ARTD_SEQUENCE_ALPHAS fixture_rows={} models={{peon,Footman,GreatHall,TownHall}} mismatches=0 PASS",
        rows.len()
    );
}

/// Whole shipped tree: a geoset the per-sequence extent excludes can never be
/// visible, a helper plane is never visible, and the 3 098 extent-gated
/// sequences never draw every geoset.
#[test]
fn artD_sequence_extent_gate() {
    let Some(root) = tree_root() else {
        eprintln!("ARTD_SEQUENCE_GATE SKIP: shipped MDX tree absent");
        return;
    };
    let files = mdx_files(&root);
    let mut sequences = 0usize;
    let mut gated_sequences = 0usize;
    for path in &files {
        let relative = path.strip_prefix(&root).unwrap_or(path);
        let bytes = std::fs::read(path).expect("read shipped MDX");
        let model = MdxModel::parse(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error:?}", relative.display()));
        let runtime = ModelRuntime::new(&model);
        for (sequence_index, sequence) in model.sequences.iter().enumerate() {
            // `mesh_alphas` resolves the sequence by (start, name); use the
            // same identity so the index addresses the same extent record.
            let extent_index = model
                .sequences
                .iter()
                .position(|candidate| {
                    candidate.start == sequence.start && candidate.name == sequence.name
                })
                .unwrap_or(sequence_index);
            let mid = sequence
                .start
                .saturating_add(sequence.end.saturating_sub(sequence.start) / 2);
            let mut gated = false;
            for time in [sequence.start, mid, sequence.end] {
                let alphas = runtime.alphas(&model, Some(sequence), time);
                assert_eq!(alphas.len(), model.geosets.len(), "{}: alpha length", relative.display());
                for (geoset_index, alpha) in alphas.iter().enumerate() {
                    if *alpha <= 0.001 {
                        continue;
                    }
                    let geoset = &model.geosets[geoset_index];
                    assert!(
                        !helper_plane(geoset),
                        "{} seq {:?} geoset {geoset_index}: helper plane drawn",
                        relative.display(),
                        sequence.name
                    );
                    assert!(
                        geoset.present_in_sequence(extent_index),
                        "{} seq {:?} geoset {geoset_index}: extent-excluded geoset drawn at t={time}",
                        relative.display(),
                        sequence.name
                    );
                }
                if (0..model.geosets.len()).any(|geoset_index| {
                    !helper_plane(&model.geosets[geoset_index])
                        && !model.geosets[geoset_index].present_in_sequence(extent_index)
                }) {
                    gated = true;
                    let visible = alphas.iter().filter(|alpha| **alpha > 0.001).count();
                    assert!(
                        visible < model.geosets.len(),
                        "{} seq {:?} t={time}: all geosets drawn despite extent gate",
                        relative.display(),
                        sequence.name
                    );
                }
            }
            sequences += 1;
            gated_sequences += usize::from(gated);
        }
    }
    assert_eq!(sequences, SHIPPED_SEQUENCES, "sequences checked");
    assert_eq!(gated_sequences, GATED_SEQUENCES, "extent-gated sequences");
    eprintln!(
        "ARTD_SEQUENCE_GATE models={} sequences={} extent_gated={} violations=0 PASS",
        files.len(),
        sequences,
        gated_sequences
    );
}

/// The build ghost is drawn through the model's default (Stand) frame
/// (`Assets::mesh_alpha(.., None, ..)` resolves `model.idle`, assets.rs:3114).
/// Pin that frame for every shipped building: no extent-excluded geoset of the
/// Stand sequence may appear at any sample time, and every building that does
/// exclude geosets there draws a strict subset.
#[test]
fn artD_ghost_no_all_geosets() {
    let Some(root) = tree_root() else {
        eprintln!("ARTD_GHOST SKIP: shipped MDX tree absent");
        return;
    };
    let files = mdx_files(&root);
    let mut buildings = 0usize;
    let mut gated_buildings = 0usize;
    for path in &files {
        let relative = path.strip_prefix(&root).unwrap_or(path);
        let name = relative.to_string_lossy();
        if !name.starts_with("Buildings/") {
            continue;
        }
        let bytes = std::fs::read(path).expect("read shipped MDX");
        let model = MdxModel::parse(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error:?}", name));
        // Same selection `def_model_asset` uses for `model.idle`.
        let Some(stand_index) = model
            .sequences
            .iter()
            .position(|sequence| sequence.name.to_ascii_lowercase().contains("stand"))
        else {
            continue;
        };
        buildings += 1;
        let stand = &model.sequences[stand_index];
        let runtime = ModelRuntime::new(&model);
        let mid = stand
            .start
            .saturating_add(stand.end.saturating_sub(stand.start) / 2);
        let excluded: Vec<usize> = (0..model.geosets.len())
            .filter(|index| {
                !helper_plane(&model.geosets[*index])
                    && !model.geosets[*index].present_in_sequence(stand_index)
            })
            .collect();
        if !excluded.is_empty() {
            gated_buildings += 1;
        }
        for time in [stand.start, mid, stand.end] {
            let alphas = runtime.alphas(&model, Some(stand), time);
            for index in &excluded {
                assert!(
                    alphas[*index] <= 0.001,
                    "{name} ghost Stand t={time}: geoset {index} is excluded by the extent but drawn"
                );
            }
            if !excluded.is_empty() {
                let visible = alphas.iter().filter(|alpha| **alpha > 0.001).count();
                assert!(
                    visible < model.geosets.len(),
                    "{name} ghost Stand t={time}: all {visible} geosets drawn"
                );
            }
        }
    }
    assert_eq!(buildings, GHOST_BUILDINGS, "buildings with a Stand sequence");
    assert!(
        gated_buildings > 0,
        "no building exercises the extent gate; fixture no longer proves the ghost rule"
    );

    // Exact ghost frames the reports measured.
    let great_hall = load(&root, "Buildings/Orc/GreatHall/GreatHall.mdx").expect("GreatHall");
    let runtime = ModelRuntime::new(&great_hall);
    let stand = great_hall
        .sequences
        .iter()
        .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
        .expect("GreatHall Stand");
    let alphas = runtime.alphas(&great_hall, Some(stand), stand.start);
    let visible: Vec<usize> = alphas
        .iter()
        .enumerate()
        .filter(|(_, alpha)| **alpha > 0.001)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(visible, vec![2, 3, 13], "GreatHall ghost Stand set");
    assert!(
        visible.len() * 2 < great_hall.geosets.len(),
        "GreatHall ghost must not draw every geoset"
    );

    let peon = load(&root, "Units/Orc/Peon/peon.mdx").expect("Peon");
    let runtime = ModelRuntime::new(&peon);
    let stand = peon
        .sequences
        .iter()
        .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
        .expect("Peon Stand");
    assert_eq!(
        runtime.alphas(&peon, Some(stand), stand.start),
        vec![1.0, 1.0, 0.0, 0.0, 0.0],
        "Peon Stand set (body + pack only)"
    );
    eprintln!(
        "ARTD_GHOST buildings={} extent_gated={} GreatHall_stand={visible:?} Peon_stand=[0,1] leaks=0 PASS",
        buildings, gated_buildings
    );
}

/// A running sequence that is an equal-value copy but not byte-identical (a
/// reconstructed interval) must still resolve its per-sequence extents. If
/// resolution failed it would take the no-sequence branch and draw every
/// geoset again ("all geosets drawn" regression reached through a copy).
#[test]
fn artD_foreign_sequence_gate() {
    let Some(root) = tree_root() else {
        eprintln!("ARTD_FOREIGN_SEQUENCE SKIP: shipped MDX tree absent");
        return;
    };
    let peon = load(&root, "Units/Orc/Peon/peon.mdx").expect("Peon");
    let runtime = ModelRuntime::new(&peon);
    let stand = peon
        .sequences
        .iter()
        .find(|sequence| sequence.name.eq_ignore_ascii_case("stand"))
        .expect("Peon Stand");
    let mut foreign = stand.clone();
    foreign.start = 0;
    foreign.end = 100;
    let alphas = runtime.alphas(&peon, Some(&foreign), foreign.start);
    assert_eq!(alphas.len(), peon.geosets.len());
    assert_eq!(
        &alphas[2..],
        &[0.0, 0.0, 0.0],
        "Peon carry/gore geosets leaked through a copied sequence"
    );
    assert!(
        alphas.iter().filter(|alpha| **alpha > 0.001).count() < peon.geosets.len(),
        "foreign Stand copy drew every Peon geoset"
    );
    let peon_visible = alphas.iter().filter(|alpha| **alpha > 0.001).count();

    let footman = load(&root, "Units/Human/Footman/Footman.mdx").expect("Footman");
    let runtime = ModelRuntime::new(&footman);
    let stand = footman
        .sequences
        .iter()
        .find(|sequence| sequence.name.starts_with("Stand - 1"))
        .expect("Footman Stand - 1");
    let stand_index = footman
        .sequences
        .iter()
        .position(|candidate| candidate.name == stand.name)
        .expect("Footman Stand - 1 index");
    let mut foreign = stand.clone();
    foreign.start = 0;
    foreign.end = 100;
    let alphas = runtime.alphas(&footman, Some(&foreign), foreign.start);
    for (geoset_index, alpha) in alphas.iter().enumerate() {
        if *alpha > 0.001 {
            assert!(
                footman.geosets[geoset_index].present_in_sequence(stand_index),
                "Footman geoset {geoset_index} leaked through a copied sequence"
            );
        }
    }
    eprintln!(
        "ARTD_FOREIGN_SEQUENCE peon_visible={peon_visible} footman_visible={} all_geosets=0 PASS",
        alphas.iter().filter(|alpha| **alpha > 0.001).count()
    );
}

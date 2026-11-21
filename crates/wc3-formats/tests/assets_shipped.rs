//! Shipped-corpus inventory and value checks for the formats layer.
//!
//! Oracle: the operator's extracted retail tree (`analysis/war3_full3`) and the
//! SLK tables it carries. The inventory pass is always on (header reads only);
//! the full decode pass runs with `H2_ASSET_CORPUS=1` and asserts the exact
//! corpus totals measured by the asset catalog
//! (`analysis/agent-assets/catalog-20261121/asset-catalog.json`).
//!
//! Mechanics: `assets.models` (MDX parse/geosets/sequences, BLP mips),
//! `abilities.data` and `items.data` (SLK cell values).
//! Tests skip with an explicit line when the tree is absent.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use wc3_formats::blp;
use wc3_formats::mdx::MdxModel;
use wc3_formats::slk::SlkTable;

fn tree() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
    root.is_dir().then_some(root)
}

fn corpus_enabled() -> bool {
    std::env::var_os("H2_ASSET_CORPUS").is_some_and(|value| value != "0")
}

/// Every file under `root` whose extension matches case-insensitively.
fn corpus(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
                    {
                        out.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// First `len` bytes of a file (headers only; the corpus pass never needs the
/// payload until it decodes it).
fn read_head(path: &Path, len: usize) -> Vec<u8> {
    let mut file = std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut head = vec![0u8; len];
    let mut filled = 0usize;
    while filled < len {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) => panic!("{}: {error}", path.display()),
        }
    }
    head.truncate(filled);
    head
}

/// `2 048` shipped models: every one is `MDLX` version 800. With
/// `H2_ASSET_CORPUS=1` every model is fully parsed and the totals are asserted.
#[test]
fn assets_models_mdx_inventory_shipped_tree() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let models = corpus(&root, "mdx");
    assert_eq!(models.len(), 2048, "shipped MDX count");
    let mut versions = std::collections::BTreeMap::new();
    for path in &models {
        let bytes = read_head(path, 16);
        let header = wc3_formats::parse_mdx_header(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        *versions.entry(header.version).or_insert(0usize) += 1;
    }
    assert_eq!(
        versions,
        std::collections::BTreeMap::from([(800u32, 2048usize)]),
        "every shipped model is classic MDX v800"
    );

    if !corpus_enabled() {
        eprintln!("inventory only (set H2_ASSET_CORPUS=1 for the full parse)");
        return;
    }
    let mut geosets = 0usize;
    let mut sequences = 0usize;
    let mut nodes = 0usize;
    for path in &models {
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let model = MdxModel::parse(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        geosets += model.geosets.len();
        sequences += model.sequences.len();
        nodes += model.pivots.len();
    }
    eprintln!("mdx={} geosets={geosets} sequences={sequences} nodes={nodes}", models.len());
    assert_eq!(geosets, 5127, "shipped geoset count (all models)");
    assert_eq!(sequences, 6512, "shipped sequence count (all models)");
    assert_eq!(nodes, 36023, "shipped pivot/node count (all models)");
}

/// `3 935` shipped textures: every one is `BLP1`, the JPEG/paletted split is
/// `2 920 / 1 015`, and with `H2_ASSET_CORPUS=1` every mip of every file
/// decodes to `width*height*4` bytes.
#[test]
fn assets_models_blp_inventory_shipped_tree() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let textures = corpus(&root, "blp");
    assert_eq!(textures.len(), 3935, "shipped BLP count");
    let mut jpeg = 0usize;
    let mut paletted = 0usize;
    let mut dimensions = std::collections::BTreeSet::new();
    for path in &textures {
        let bytes = read_head(path, 156);
        assert!(
            bytes.len() >= 156,
            "{}: shorter than a BLP1 header",
            path.display()
        );
        assert_eq!(
            bytes.get(..4),
            Some(&b"BLP1"[..]),
            "{}: classic BLP1",
            path.display()
        );
        let compression = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        match compression {
            0 => jpeg += 1,
            1 => paletted += 1,
            other => panic!("{}: compression {other}", path.display()),
        }
        let width = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let height = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        dimensions.insert((width, height));
    }
    eprintln!(
        "blp={} jpeg={jpeg} paletted={paletted} distinct_dimensions={}",
        textures.len(),
        dimensions.len()
    );
    assert_eq!(jpeg, 2920, "JPEG BLP1 count");
    assert_eq!(paletted, 1015, "paletted BLP1 count");
    assert_eq!(dimensions.len(), 39, "distinct texture dimensions");

    if !corpus_enabled() {
        eprintln!("inventory only (set H2_ASSET_CORPUS=1 for the full decode)");
        return;
    }
    let mut decoded_pixels = 0u64;
    let mut decoded_mips = 0u64;
    let mut declared_mips = 0u64;
    let mut min_side = u32::MAX;
    let mut max_side = 0u32;
    for path in &textures {
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let image =
            blp::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(image.width > 0 && image.height > 0, "{}", path.display());
        assert_eq!(
            image.rgba.len(),
            image.width as usize * image.height as usize * 4,
            "{}: mip 0 is RGBA8",
            path.display()
        );
        let offsets = bytes
            .get(28..92)
            .expect("BLP1 mip offset table")
            .chunks_exact(4)
            .filter(|offset| offset.iter().any(|byte| *byte != 0))
            .count() as u64;
        assert_eq!(
            image.mipmaps.len() as u64,
            offsets,
            "{}: decoded every declared mip",
            path.display()
        );
        for (level, mip) in image.mipmaps.iter().enumerate() {
            let shift = level as u32;
            assert_eq!(mip.width, (image.width >> shift).max(1), "{}: mip width", path.display());
            assert_eq!(
                mip.height,
                (image.height >> shift).max(1),
                "{}: mip height",
                path.display()
            );
            assert_eq!(
                mip.rgba.len(),
                mip.width as usize * mip.height as usize * 4,
                "{}: mip {level} is RGBA8",
                path.display()
            );
        }
        decoded_pixels += image.width as u64 * image.height as u64;
        decoded_mips += image.mipmaps.len() as u64;
        declared_mips += offsets;
        min_side = min_side.min(image.width.min(image.height));
        max_side = max_side.max(image.width.max(image.height));
    }
    eprintln!(
        "decoded_pixels={decoded_pixels} mips={decoded_mips} declared={declared_mips} \
         side={min_side}..{max_side}"
    );
    assert_eq!(decoded_pixels, 98_722_207, "mip-0 pixel total");
    assert_eq!(decoded_mips, 28_748, "decoded mip total");
    assert_eq!(declared_mips, 28_748, "every declared mip decoded");
    assert_eq!((min_side, max_side), (4, 512), "shipped texture size range");
}

/// `Units/AbilityData.slk`: the sheet shape and the Paladin's Holy Light cells
/// (`AHhb`) exactly as the shipped table carries them.
#[test]
fn abilities_data_slk_cells_shipped() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let path = root.join("Units/AbilityData.slk");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let table = SlkTable::parse(&bytes).expect("AbilityData.slk parses");
    assert_eq!(table.columns.len(), 56, "AbilityData columns");
    assert_eq!(table.rows.len(), 399, "AbilityData data rows");
    let row = table
        .rows
        .iter()
        .position(|cells| cells.first().is_some_and(|id| id == "AHhb"))
        .expect("AHhb (Holy Light) row");
    for (column, expected) in [
        ("code", "AHhb"),
        ("comments", "Paladin - Holy Light"),
        ("Cool1", "5"),
        ("Cool2", "5"),
        ("Cool3", "5"),
        ("Cost1", "75"),
        ("Cost2", "75"),
        ("Cost3", "75"),
        ("Data11", "200"),
        ("Data21", "400"),
        ("Data31", "600"),
        ("Dur1", "0"),
        ("Rng1", "800"),
        ("levels", "3"),
        ("hero", "1"),
        ("reqLevel", "1"),
        ("targs", "air,ground,organic,notself,invu,vuln,nonancient"),
    ] {
        assert_eq!(
            table.cell(row, column),
            Some(expected),
            "AHhb.{column}"
        );
    }
}

/// `Units/ItemData.slk`: the sheet shape and the Claws of Attack +12 (`ratc`)
/// cells exactly as the shipped table carries them.
#[test]
fn items_data_slk_cells_shipped() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let path = root.join("Units/ItemData.slk");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let table = SlkTable::parse(&bytes).expect("ItemData.slk parses");
    assert_eq!(table.columns.len(), 21, "ItemData columns");
    assert_eq!(table.rows.len(), 132, "ItemData data rows");
    let row = table
        .rows
        .iter()
        .position(|cells| cells.first().is_some_and(|id| id == "ratc"))
        .expect("ratc (Claws of Attack +12) row");
    for (column, expected) in [
        ("comment", "Claws of Attack +12"),
        ("Level", "7"),
        ("goldcost", "500"),
        ("HP", "100"),
        ("abilList", "AItc"),
        ("armor", "Wood"),
        ("droppable", "1"),
        ("prio", "63"),
        ("stockMax", "1"),
        ("stockRegen", "120"),
        ("stockStart", "0"),
        ("file", "Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl"),
    ] {
        assert_eq!(table.cell(row, column), Some(expected), "ratc.{column}");
    }
}

/// The shipped particle tracks that start before frame 0 (`-100`, `-133`,
/// `-167`, `-33`) keep their pre-start key and sample to their last in-sequence
/// value at the tail of the sequence (WC3-I-009). With `H2_ASSET_CORPUS=1` the
/// whole corpus is scanned: exactly three models carry such keys.
#[test]
fn assets_models_mdx_particle_prestart_keys_shipped() {
    use wc3_formats::mdx::{MdxModel, Node, Track};

    fn has_negative<T>(track: &Track<T>) -> bool {
        track.keys.iter().any(|key| (key.time as i32) < 0)
    }

    fn node_negative(node: &Node) -> usize {
        usize::from(has_negative(&node.translation))
            + usize::from(has_negative(&node.rotation))
            + usize::from(has_negative(&node.scaling))
    }

    fn negative_tracks(model: &MdxModel) -> usize {
        let mut count = 0usize;
        for bone in &model.bones {
            count += node_negative(&bone.node);
        }
        for helper in &model.helpers {
            count += node_negative(helper);
        }
        for attachment in &model.attachments {
            count += node_negative(&attachment.node);
        }
        for event in &model.events {
            count += node_negative(event);
        }
        for material in &model.materials {
            for layer in &material.layers {
                count += usize::from(layer.alpha_track.as_ref().is_some_and(has_negative));
                count += usize::from(layer.texture_id_track.as_ref().is_some_and(has_negative));
            }
        }
        for animation in &model.geoset_animations {
            count += usize::from(animation.alpha_track.as_ref().is_some_and(has_negative));
            count += usize::from(animation.color_track.as_ref().is_some_and(has_negative));
        }
        for emitter in &model.particle_emitters {
            count += node_negative(&emitter.node);
            for track in [
                &emitter.emission_rate_track,
                &emitter.gravity_track,
                &emitter.longitude_track,
                &emitter.latitude_track,
                &emitter.life_span_track,
                &emitter.speed_track,
                &emitter.visibility_track,
            ] {
                count += usize::from(track.as_ref().is_some_and(has_negative));
            }
        }
        for emitter in &model.particle_emitters2 {
            count += node_negative(&emitter.node);
            for track in [
                &emitter.speed_track,
                &emitter.variation_track,
                &emitter.latitude_track,
                &emitter.gravity_track,
                &emitter.visibility_track,
                &emitter.emission_rate_track,
                &emitter.width_track,
                &emitter.length_track,
            ] {
                count += usize::from(track.as_ref().is_some_and(has_negative));
            }
        }
        for ribbon in &model.ribbon_emitters {
            count += node_negative(&ribbon.node);
            count += usize::from(ribbon.height_above_track.as_ref().is_some_and(has_negative));
            count += usize::from(ribbon.height_below_track.as_ref().is_some_and(has_negative));
            count += usize::from(ribbon.alpha_track.as_ref().is_some_and(has_negative));
            count += usize::from(ribbon.color_track.as_ref().is_some_and(has_negative));
            count += usize::from(ribbon.texture_slot_track.as_ref().is_some_and(has_negative));
            count += usize::from(ribbon.visibility_track.as_ref().is_some_and(has_negative));
        }
        count
    }

    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };

    // Three named models: the pre-start key is first in signed order and the
    // tail of the track holds the last in-sequence value.
    let cases: [(&str, &str, i32); 3] = [
        ("Units/Critters/Hydralisk/Hydralisk.mdx", "BlizParticlePuff", -100),
        ("Units/Orc/HeroFarseer/HeroFarSeer.mdx", "BlizParticle03", -167),
        (
            "UI/Glues/MainMenu/MainMenu3d/MainMenu3d.mdx",
            "BlizParticle04dandilion01",
            -133,
        ),
    ];
    for (relative, emitter_name, first_signed) in cases {
        let bytes = std::fs::read(root.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
        let model = MdxModel::parse(&bytes).unwrap_or_else(|error| panic!("{relative}: {error}"));
        let emitter = model
            .particle_emitters2
            .iter()
            .find(|emitter| emitter.node.name == emitter_name)
            .unwrap_or_else(|| panic!("{relative}: emitter {emitter_name}"));
        let track = emitter
            .emission_rate_track
            .as_ref()
            .or(emitter.visibility_track.as_ref())
            .unwrap_or_else(|| panic!("{relative}: no scalar track on {emitter_name}"));
        assert_eq!(
            track.keys.first().map(|key| key.time as i32),
            Some(first_signed),
            "{relative}: the pre-start key stays first in signed order"
        );
        let in_sequence = track
            .keys
            .iter()
            .filter(|key| (key.time as i32) >= 0)
            .count();
        assert!(in_sequence >= 8, "{relative}: in-sequence keys");
        let last_time = track
            .keys
            .iter()
            .filter(|key| (key.time as i32) >= 0)
            .map(|key| key.time)
            .max()
            .expect("in-sequence keys");
        let tail = track.sample(last_time + 60_000).expect("tail sample");
        let at_last = track.sample(last_time).expect("last-key sample");
        assert_eq!(
            tail, at_last,
            "{relative}: past the last key the value is held (pre-start key is not sampled)"
        );
    }

    if !corpus_enabled() {
        eprintln!("named models only (set H2_ASSET_CORPUS=1 for the whole corpus)");
        return;
    }
    let models = corpus(&root, "mdx");
    let mut with_pre_start = Vec::new();
    let mut tracks = 0usize;
    for path in &models {
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let model = MdxModel::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let count = negative_tracks(&model);
        if count > 0 {
            tracks += count;
            with_pre_start.push(path.display().to_string());
        }
    }
    eprintln!(
        "models_with_prestart_keys={} tracks={tracks} files={with_pre_start:?}",
        with_pre_start.len()
    );
    assert_eq!(with_pre_start.len(), 3, "three shipped models carry pre-start keys");
    assert_eq!(tracks, 9, "nine shipped tracks carry pre-start keys");
}

/// `Units/UnitBalance.slk` + `Units/UnitData.slk`: the Footman (`hfoo`) combat
/// and building cells the simulation reads.
#[test]
fn combat_damage_table_stats_slk_cells_shipped() {
    let Some(root) = tree() else {
        eprintln!("skip: no extracted tree (analysis/war3_full3)");
        return;
    };
    let balance_bytes = std::fs::read(root.join("Units/UnitBalance.slk")).expect("UnitBalance.slk");
    let balance = SlkTable::parse(&balance_bytes).expect("UnitBalance.slk parses");
    assert_eq!(balance.columns.len(), 44, "UnitBalance columns");
    assert_eq!(balance.rows.len(), 466, "UnitBalance data rows");
    let row = balance
        .rows
        .iter()
        .position(|cells| cells.first().is_some_and(|id| id == "hfoo"))
        .expect("hfoo (Footman) row");
    for (column, expected) in [
        ("comment(s)", "Footman"),
        ("HP", "420"),
        ("realHP", "420"),
        ("def", "2"),
        ("realdef", "2"),
        ("defUp", "2"),
        ("defType", "medium"),
        ("level", "2"),
        ("goldcost", "160"),
        ("lumbercost", "0"),
        ("bldtm", "20"),
        ("sight", "1400"),
        ("nsight", "800"),
        ("spd", "270"),
        ("regenHP", "0.25"),
        ("regenType", "always"),
        ("upgrades", "Rhar,Rhme,Rhde"),
        ("sortBalance", "a2"),
    ] {
        assert_eq!(balance.cell(row, column), Some(expected), "hfoo.{column}");
    }

    let data_bytes = std::fs::read(root.join("Units/UnitData.slk")).expect("UnitData.slk");
    let data = SlkTable::parse(&data_bytes).expect("UnitData.slk parses");
    assert_eq!(data.columns.len(), 34, "UnitData columns");
    assert_eq!(data.rows.len(), 466, "UnitData data rows");
    let row = data
        .rows
        .iter()
        .position(|cells| cells.first().is_some_and(|id| id == "hfoo"))
        .expect("hfoo row in UnitData");
    for (column, expected) in [
        ("comment(s)", "Footman"),
        ("movetp", "foot"),
        ("collision", "31"),
        ("points", "100"),
        ("castpt", "0.3"),
        ("castbsw", "0.51"),
        ("death", "3.04"),
        ("impactZ", "60"),
        ("launchZ", "60"),
        ("prio", "6"),
        ("propWin", "60"),
        ("targType", "ground"),
        ("race", "human"),
        ("valid", "1"),
    ] {
        assert_eq!(data.cell(row, column), Some(expected), "hfoo.UnitData.{column}");
    }
}

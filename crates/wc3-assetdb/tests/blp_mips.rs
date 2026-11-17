//! BLP mip-chain consumption: the decoder must return every declared level, the
//! asset layer must preserve them, and mip 0 must stay the surface consumers
//! upload.

use std::path::PathBuf;

use wc3_assetdb::blp_report::probe;
use wc3_assetdb::AssetDb;
use wc3_formats::blp::decode;

fn extracted(relative: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/war3_full3")
        .join(relative.replace('\\', "/"));
    path.is_file().then_some(path)
}

fn assert_full_chain(relative: &str) {
    let Some(path) = extracted(relative) else {
        eprintln!("skipping {relative}: extracted file missing");
        return;
    };
    let bytes = std::fs::read(&path).expect("read blp");
    let header = probe(&bytes).expect("probe header");
    let image = decode(&bytes).expect("decode blp");
    assert_eq!(
        image.mipmaps.len(),
        header.mip_count as usize,
        "{relative}: decoded levels must match the header ({} declared)",
        header.mip_count
    );
    assert!(image.mipmaps.len() > 1, "{relative}: expected a mip chain");
    let mut width = image.width;
    let mut height = image.height;
    for (level, mip) in image.mipmaps.iter().enumerate() {
        assert_eq!((mip.width, mip.height), (width, height), "{relative} level {level} dims");
        assert_eq!(
            mip.rgba.len(),
            (mip.width as usize) * (mip.height as usize) * 4,
            "{relative} level {level} bytes"
        );
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    assert_eq!(
        image.rgba.as_slice(),
        image.mipmaps[0].rgba.as_slice(),
        "{relative}: `rgba` must be mip 0"
    );
}

#[test]
fn blp1_jpeg_decodes_every_declared_mip() {
    // 512x256, 10 declared levels in the retail file.
    assert_full_chain("TerrainArt\\Village\\Village_Dirt.blp");
}

#[test]
fn blp1_paletted_decodes_every_declared_mip() {
    // 64x64 icon, paletted BLP1.
    assert_full_chain("ReplaceableTextures\\CommandButtons\\BTNFootman.blp");
}

#[test]
fn assetdb_texture_preserves_all_levels_and_mip0() {
    let mpq = std::env::var_os("WC3_MPQ")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq")
        });
    if !mpq.is_file() {
        eprintln!("skipping: War3.mpq not available");
        return;
    }
    let index = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../analysis/agent-assets/war3.index");
    let mut db = match index.is_file() {
        true => match AssetDb::open_mpq_best(&mpq, &index) {
            Ok((db, _)) => db,
            Err(err) => panic!("open indexed: {err}"),
        },
        false => AssetDb::open_mpq(&mpq).expect("open mpq"),
    };
    let texture = db
        .texture_shared("TerrainArt/Village/Village_Dirt.blp")
        .expect("texture");
    assert_eq!(texture.dimensions(), (512, 256));
    assert_eq!(texture.mip_levels(), 10, "all declared levels preserved");
    assert_eq!(texture.mip0().len(), 512 * 256 * 4);
    assert_eq!(texture.mip0(), texture.image.rgba.as_slice());
    assert!(texture.mip_bytes() > texture.mip0().len(), "mips beyond mip 0 are held");
    let last = texture.mip(texture.mip_levels() - 1).expect("last mip");
    assert!(last.width >= 1 && last.height >= 1);
    assert!(texture.mip(texture.mip_levels()).is_none());
}

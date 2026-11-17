//! QA probe: command-card icon indices used by the HUD.
use std::path::{Path, PathBuf};

use wc3_game::{Assets, UiAction};
use wc3_sim::UnitKind;

fn data() -> Option<(PathBuf, PathBuf)> {
    let mpq = [
        PathBuf::from("/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq"),
        PathBuf::from("/data/data/com.termux/files/home/src/wc3rust/analysis/war3_iso/War3.mpq"),
    ]
    .into_iter()
    .find(|p| p.exists())?;
    let map = Path::new(
        "/data/data/com.termux/files/home/src/wc3rust/analysis/war3_map/Maps/Campaign/Human04.w3m",
    );
    if !map.exists() {
        return None;
    }
    Some((mpq, map.to_path_buf()))
}

#[test]
fn command_icons_resolve() {
    let Some((mpq, map)) = data() else {
        eprintln!("SKIP: no assets");
        return;
    };
    let assets = Assets::load(Some(&mpq), Some(&map));
    let names = [
        ("BTNFootman", assets.ui.texture("BTNFootman")),
        ("BTNArcher", assets.ui.texture("BTNArcher")),
        ("BTNPeasant", assets.ui.texture("BTNPeasant")),
        ("BTNHumanBuild", assets.ui.texture("BTNHumanBuild")),
        ("BTNStop", assets.ui.texture("BTNStop")),
        ("BTNMove", assets.ui.texture("BTNMove")),
        ("BTNAttack", assets.ui.texture("BTNAttack")),
        ("BTNHoldPosition", assets.ui.texture("BTNHoldPosition")),
        ("BTNGatherGold", assets.ui.texture("BTNGatherGold")),
        ("GoldIcon", assets.ui.texture("GoldIcon")),
        ("LumberIcon", assets.ui.texture("LumberIcon")),
        ("SupplyIcon", assets.ui.texture("SupplyIcon")),
    ];
    for (name, index) in names {
        println!("ICON {name} exists={} index={index:?}", index.is_some());
    }
    let actions = [
        ("TrainFootman", UiAction::Train(UnitKind::Footman)),
        ("Stop", UiAction::Stop),
        ("AttackMove", UiAction::AttackMove),
        ("Gather", UiAction::Gather),
        ("Hold", UiAction::Hold),
        ("SelectArmy", UiAction::SelectArmy),
        ("CentreBase", UiAction::CentreBase),
        ("ZoomIn", UiAction::ZoomIn),
        ("ZoomOut", UiAction::ZoomOut),
    ];
    let mut ok = 0;
    for (name, action) in actions {
        let index = assets.ui.command_icon(&action);
        println!("CMD {name} icon={index:?}");
        if index.is_some() {
            ok += 1;
        }
    }
    for index in 46..=61 {
        if let Some(texture) = assets.textures.get(index) {
            let (mut sum, mut alpha, mut black) = (0u64, 0u64, 0u64);
            for px in texture.rgba.chunks_exact(4) {
                sum += px[0] as u64 + px[1] as u64 + px[2] as u64;
                alpha += px[3] as u64;
                if px[0] < 8 && px[1] < 8 && px[2] < 8 { black += 1; }
            }
            let n = (texture.width * texture.height).max(1) as u64;
            println!(
                "TEX {index} {}x{} mean={:.1} alpha={:.1} black={:.2}",
                texture.width,
                texture.height,
                sum as f64 / (3.0 * n as f64),
                alpha as f64 / n as f64,
                black as f64 / n as f64
            );
        }
    }
    println!("ICON_RESOLVED {ok}/{} textures={}", 9, assets.textures.len());
}

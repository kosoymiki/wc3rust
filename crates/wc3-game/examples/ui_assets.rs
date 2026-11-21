//! Decode the shipped WC3 UI BLPs to RGBA PNGs for the design lane.
//!
//! `hud_mock` needs to *see* the retail art with its alpha channel: the older
//! `blp_info` PPM dump drops alpha and paints the transparent key magenta, so
//! every panel corner would build from a magenta source. This example keeps the
//! RGBA buffer and writes it through the same `soft_raster::write_png` the HUD
//! proof uses.
//!
//! It also prints one `UI_ASSET` line per file (dimensions, average of the
//! opaque pixels) and a `UI_PIXEL` line for a named sample point, so the
//! palette in `analysis/design/HUD-DIRECTION.md` is measured from the bytes on
//! disk instead of guessed.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example ui_assets --offline
//! ```
//!
//! Output: `analysis/design/src_rgba/<stem>.png` (mode 0600 not required; the
//! repo tree is the operator's private workspace).

use std::path::{Path, PathBuf};

use wc3_formats::blp;
use wc3_game::hudskin::soft_raster;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// (relative path under the extracted archive, sample points in source pixels
/// for the palette table).
const ASSETS: &[(&str, &[(u32, u32, &str)])] = &[
    (
        "UI/Console/Human/HumanUITile01.blp",
        &[(96, 300, "stone"), (160, 330, "portrait_well")],
    ),
    (
        "UI/Console/Human/HumanUITile02.blp",
        &[(64, 260, "minimap_arch"), (300, 300, "selection_well")],
    ),
    (
        "UI/Console/Human/HumanUITile03.blp",
        &[(250, 260, "card_frame"), (260, 300, "card_well")],
    ),
    ("UI/Console/Human/HumanUITile04.blp", &[(8, 10, "right_edge")]),
    (
        "UI/Widgets/EscMenu/Human/human-options-menu-background.blp",
        &[(128, 128, "menu_fill")],
    ),
    (
        "UI/Widgets/EscMenu/Human/human-options-menu-border.blp",
        &[(0, 32, "border_left"), (416, 32, "border_corner")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground.blp",
        &[(16, 16, "button_fill")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Down.blp",
        &[(16, 16, "button_down_fill")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Disabled.blp",
        &[(16, 16, "button_disabled_fill")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp",
        &[(0, 16, "button_border"), (224, 16, "button_border_corner")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder-Down.blp",
        &[(0, 16, "button_border_down")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder-Disabled.blp",
        &[(0, 16, "button_border_disabled")],
    ),
    (
        "UI/Widgets/Glues/GlueScreen-Button-KeyboardHighlight.blp",
        &[(64, 16, "button_highlight")],
    ),
    (
        "UI/Widgets/Console/Human/human-console-button-up.blp",
        &[(8, 8, "tile_border"), (96, 8, "tile_corner")],
    ),
    (
        "UI/Widgets/Console/Human/human-console-button-down.blp",
        &[(8, 8, "tile_border_down")],
    ),
    (
        "UI/Widgets/Console/Human/human-console-button-highlight.blp",
        &[(64, 16, "tile_highlight")],
    ),
    (
        "UI/Widgets/ToolTips/Human/human-tooltip-background.blp",
        &[(32, 32, "tooltip_fill")],
    ),
    (
        "UI/Widgets/ToolTips/Human/human-tooltip-border.blp",
        &[(0, 8, "tooltip_border"), (112, 8, "tooltip_border_corner")],
    ),
    (
        "UI/Feedback/Resources/ResourceGold.blp",
        &[(16, 16, "gold")],
    ),
    (
        "UI/Feedback/Resources/ResourceLumber.blp",
        &[(16, 16, "lumber")],
    ),
    (
        "UI/Feedback/Resources/ResourceSupply.blp",
        &[(16, 16, "supply")],
    ),
    (
        "UI/Feedback/Resources/ResourceUpkeep.blp",
        &[(16, 16, "upkeep")],
    ),
    (
        "UI/Feedback/Resources/ResourceManaStone.blp",
        &[(16, 16, "mana_stone")],
    ),
    (
        "UI/Feedback/HPBarConsole/human-healthbar-fill.blp",
        &[(16, 4, "hp_fill")],
    ),
    (
        "UI/Feedback/ManaBarConsole/human-manabar-fill.blp",
        &[(16, 4, "mana_fill")],
    ),
    (
        "UI/Widgets/Console/Human/infocard-gold.blp",
        &[(16, 16, "info_gold")],
    ),
    (
        "UI/Widgets/Console/Human/infocard-supply.blp",
        &[(16, 16, "info_supply")],
    ),
];

fn main() {
    let root = repo_root();
    let src = root.join("analysis/war3_full3");
    let out_dir = root.join("analysis/design/src_rgba");
    std::fs::create_dir_all(&out_dir).expect("analysis/design/src_rgba");

    for (relative, samples) in ASSETS {
        let path = src.join(relative);
        let Ok(bytes) = std::fs::read(&path) else {
            println!("UI_ASSET missing={relative}");
            continue;
        };
        let Ok(image) = blp::decode(&bytes) else {
            println!("UI_ASSET decode_failed={relative}");
            continue;
        };
        let stem = Path::new(relative)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("asset");
        let out = out_dir.join(format!("{stem}.png"));
        if let Err(error) = soft_raster::write_png(&out, image.width, image.height, &image.rgba) {
            println!("UI_ASSET write_failed={relative} error={error}");
            continue;
        }
        let mut sum = [0u64; 3];
        let mut opaque = 0u64;
        for px in image.rgba.chunks_exact(4) {
            if px[3] > 16 {
                sum[0] += px[0] as u64;
                sum[1] += px[1] as u64;
                sum[2] += px[2] as u64;
                opaque += 1;
            }
        }
        let n = opaque.max(1);
        println!(
            "UI_ASSET {} {}x{} bytes={} opaque={} avg=#{:02x}{:02x}{:02x}",
            relative,
            image.width,
            image.height,
            bytes.len(),
            opaque,
            (sum[0] / n) as u8,
            (sum[1] / n) as u8,
            (sum[2] / n) as u8
        );
        for (x, y, role) in *samples {
            if *x >= image.width || *y >= image.height {
                continue;
            }
            let at = ((y * image.width + x) * 4) as usize;
            let px = &image.rgba[at..at + 4];
            println!(
                "UI_PIXEL {} {} {} #(r={} g={} b={} a={})",
                stem, role, format!("{x},{y}"), px[0], px[1], px[2], px[3]
            );
        }
    }
}

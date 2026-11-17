//! HUD acceptance dump for the operator's FINAL asset pack (ledger 457).
//!
//! Draws the real `MobileHud::draw` path at the two phone orientations, writes
//! the headless PNGs and a machine-readable element table, and prints the
//! layout gate:
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example hud_layout --offline
//! ```
//!
//! Artifacts:
//! * `analysis/hudskin/pack-2414x1080.png`
//! * `analysis/hudskin/pack-1080x2414.png`
//! * `analysis/hudskin/pack-layout-2414x1080.json`
//! * `analysis/hudskin/pack-layout-1080x2414.json`
//!
//! The JSON is consumed by `tools/hud_pack_check.py`, which independently
//! re-derives the manifest, section, inset, leaf, text and safe-area rules.

use std::path::{Path, PathBuf};

use glam::Vec2;
use serde_json::json;

use wc3_game::assets::TextureImage;
use wc3_game::hud::{self, Font, Rect};
use wc3_game::hudskin::pack::{self, HudPack};
use wc3_game::hudskin::soft_raster;
use wc3_game::hudskin::Skin;
use wc3_game::hud::mobile_hud::{
    ArmyChip, CommandButton, GroupView, HudView, MinimapDot, MinimapView, MobileHud,
    ProductionView, ResourcePill, SelectionView, ToastTone, ToastView,
};
use wc3_game::style::{Chrome, Tokens};

const MIN_LEAF_PX: f32 = 90.0;

fn main() {
    let root = pack::find_installed().unwrap_or_else(|| {
        eprintln!("HUD_PACK missing=manifest.json (searched {:?})", pack::pack_search_dirs());
        std::process::exit(2);
    });
    let mut textures: Vec<TextureImage> = Vec::new();
    let pack = HudPack::load(&root, &mut textures).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let skin = Skin::from_pack(&pack).unwrap_or_else(|| {
        eprintln!("HUD_PACK role_mapping=failed");
        std::process::exit(2);
    });
    let summary = pack.slice_summary();
    println!("HUD_PACK {}", summary);

    let chrome = Chrome::new(&mut textures);
    let font = Font::load();
    let minimap = minimap_texture();
    let minimap_index = textures.len();
    textures.push(minimap.clone());

    let mut failures = 0usize;
    for viewport in [
        Vec2::new(2414.0, 1080.0),
        Vec2::new(1080.0, 2414.0),
    ] {
        let hud = MobileHud::new();
        let tokens = Tokens::default();
        let view = proof_view(&pack, &skin, viewport);
        let mut batch = hud::UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let name = format!("pack-{}x{}.png", viewport.x as u32, viewport.y as u32);
        let out_dir = repo_root().join("analysis/hudskin");
        std::fs::create_dir_all(&out_dir).expect("analysis/hudskin");
        let png = out_dir.join(&name);
        let rgba = soft_raster::rasterize(
            &batch,
            &textures,
            Some(&font),
            Some(&minimap),
            viewport.x as u32,
            viewport.y as u32,
        );
        soft_raster::write_png(&png, viewport.x as u32, viewport.y as u32, &rgba)
            .expect("write proof png");
        println!(
            "HUD_PACK frame={} bytes={} scale={:.5}",
            png.display(),
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0),
            layout.scale
        );

        let table = element_table(&pack, &skin, &layout, &view, &batch, &textures, viewport);
        let json_path = out_dir.join(format!(
            "pack-layout-{}x{}.json",
            viewport.x as u32,
            viewport.y as u32
        ));
        std::fs::write(
            &json_path,
            serde_json::to_vec_pretty(&table).expect("serialise layout"),
        )
        .expect("write layout json");

        // Gate: the same numeric rules the Python checker re-derives.
        let mut local = 0usize;
        println!(
            "\n== viewport {:.0}x{:.0} sections={:?}",
            viewport.x, viewport.y, layout.section_assets
        );
        for (index, section) in layout.sections.iter().enumerate() {
            println!(
                "section {} {:<32} x={:>7.1} w={:>7.1} y={:>7.1} h={:>7.1}",
                index,
                layout.section_assets[index],
                section.x,
                section.w,
                section.y,
                section.h
            );
        }
        println!(
            "{:<22} {:>8} {:>8} {:>8} {:>8} {:>8}  verdict",
            "element", "x", "y", "w", "h", "min px"
        );
        let mut rows: Vec<(&str, Rect, bool)> = vec![
            ("minimap", layout.minimap, true),
            ("minimap image", layout.minimap_image, false),
            ("selection", layout.selection, true),
            ("portrait", layout.portrait, false),
            ("card", layout.card, false),
            ("groups", layout.groups_area, false),
            ("inventory", layout.inventory, false),
            ("army", layout.army, false),
            ("topbar", layout.top_meta, false),
            ("tooltip", layout.tooltip, false),
        ];
        for slot in &layout.card_slots {
            rows.push(("card cell", *slot, true));
        }
        for slot in &layout.group_slots {
            rows.push(("group hit", *slot, true));
        }
        for slot in &layout.army_slots {
            rows.push(("army chip", *slot, true));
        }
        for slot in &layout.inventory_parts.touches {
            rows.push(("inventory slot", *slot, true));
        }
        for (label, rect, leaf) in rows {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            let min_side = rect.w.min(rect.h);
            let ok = !leaf || min_side + 0.01 >= MIN_LEAF_PX;
            if !ok {
                local += 1;
            }
            println!(
                "{:<22} {:>8.1} {:>8.1} {:>8.1} {:>8.1} {:>8.1}  {}",
                label,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                min_side,
                if ok { "ok" } else { "FAIL" }
            );
        }
        // Text floor at the device scale.
        for (label, scale) in [
            ("map_name", 0.90 * layout.scale),
            ("resources", 0.85 * layout.scale),
            ("upkeep", 0.80 * layout.scale),
            ("hotkey", 0.80 * layout.scale),
            ("prose", 0.80 * layout.scale),
        ] {
            let px = hud::HudFonts::text_px(scale);
            if px + 0.01 < hud::MIN_TEXT_PX {
                println!("text {label} px={px:.1} FAIL (< {})", hud::MIN_TEXT_PX);
                local += 1;
            }
        }
        // Card right margin inside the safe area.
        let card_right = viewport.x - layout.safe.right;
        if layout.card.right() > card_right + 0.5 {
            println!(
                "card right {:.1} > safe edge {:.1} FAIL",
                layout.card.right(),
                card_right
            );
            local += 1;
        }
        // Sections tile the band.
        if layout.orientation == wc3_game::hud::mobile_hud::Orientation::Landscape {
            // Sections tile the console band, which sits inside the horizontal
            // safe gutters (the platform insets are authoritative), not the raw
            // viewport width.
            let total: f32 = layout.sections.iter().map(|section| section.w).sum();
            if (total - layout.console.w).abs() > 0.5 {
                println!(
                    "sections width {total:.1} != console {:.1} FAIL",
                    layout.console.w
                );
                local += 1;
            }
        } else {
            let total: f32 = layout.sections.iter().map(|section| section.h).sum();
            if (total - layout.console.h).abs() > 0.5 {
                println!(
                    "sections height {total:.1} != console {:.1} FAIL",
                    layout.console.h
                );
                local += 1;
            }
        }
        let issues = layout.issues(&tokens);
        if !issues.is_empty() {
            println!("layout.issues = {} {:?}", issues.len(), issues);
            local += issues.len();
        }
        println!("viewport {name}: {} issue(s)", local);
        failures += local;
    }
    println!(
        "\nHUD_LAYOUT_GATE {}",
        if failures == 0 { "PASS" } else { "FAIL" }
    );
    if failures != 0 {
        std::process::exit(1);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn proof_view<'a>(pack: &HudPack, skin: &'a Skin, viewport: Vec2) -> HudView<'a> {
    let icon = |name: &str| pack.texture(name);
    let resources: &'static [ResourcePill<'static>] = Box::leak(
        vec![
            ResourcePill::new("1200", "GOLD", [0.95, 0.76, 0.30, 1.0])
                .icon(icon("icon_gold_64")),
            ResourcePill::new("340", "LUMBER", [0.48, 0.85, 0.56, 1.0])
                .icon(icon("icon_lumber_64")),
            ResourcePill::new("7/12", "SUPPLY", [0.44, 0.71, 1.0, 1.0])
                .icon(icon("icon_food_64")),
        ]
        .into_boxed_slice(),
    );
    let hotkeys = ["S", "A", "B", "G", "M", "H", "F", "R", "", "", "", ""];
    let labels = [
        "STOP", "ATTACK", "BUILD", "GATHER", "MOVE", "HOLD", "TRAIN", "RESEARCH", "", "", "", "",
    ];
    let commands: &'static [CommandButton<'static>] = Box::leak(
        labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                if label.is_empty() {
                    CommandButton::empty()
                } else {
                    CommandButton::new(
                        label,
                        icon(&format!("command_icon_{:02}_64", index + 1)),
                    )
                    .hotkey(&hotkeys[index])
                    .enabled(true)
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let dots: &'static [MinimapDot] = Box::leak(
        vec![
            MinimapDot::new(0.24, 0.32, 6.0, [0.44, 0.71, 1.0, 1.0]),
            MinimapDot::new(0.72, 0.66, 7.0, [1.0, 0.35, 0.37, 1.0]),
            MinimapDot::new(0.18, 0.22, 7.0, [0.95, 0.76, 0.30, 1.0]),
        ]
        .into_boxed_slice(),
    );
    let groups: &'static [GroupView] = Box::leak(
        (0..10)
            .map(|index| GroupView {
                rect: Rect::new(0.0, 0.0, 0.0, 0.0),
                count: (index % 4) as u32,
                active: index == 0,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let army: &'static [ArmyChip] = Box::leak(
        vec![
            ArmyChip {
                icon: icon("command_icon_02_64"),
                hp: 0.86,
                selected: true,
            },
            ArmyChip {
                icon: icon("command_icon_03_64"),
                hp: 0.62,
                selected: false,
            },
            ArmyChip {
                icon: icon("command_icon_04_64"),
                hp: 0.35,
                selected: false,
            },
        ]
        .into_boxed_slice(),
    );
    let items: &'static [Option<usize>] = Box::leak(
        vec![
            icon("command_icon_05_64"),
            icon("command_icon_06_64"),
            None,
            None,
            None,
            None,
        ]
        .into_boxed_slice(),
    );
    HudView::new(viewport)
        .resources(resources)
        .minimap(MinimapView {
            dots,
            camera: Some([0.22, 0.26, 0.30, 0.34]),
            title: "MINIMAP",
            ..MinimapView::new("MINIMAP")
        })
        .selection(Some(
            SelectionView::new("ARCHER", 300, 300)
                .class("WARRIOR")
                .kind("RANGED")
                .portrait(icon("command_icon_02_64"))
                .stats(3, 14)
                .production(Some(ProductionView {
                    name: "TRAINING",
                    progress: 0.42,
                    remaining: 9,
                })),
        ))
        .commands(commands)
        .map_name("Альянс04", 61.0)
        .mode("MOVE")
        .upkeep("NO UPKEEP")
        .groups(groups)
        .army(army)
        .item_icons(items)
        .toast(Some(ToastView::new(
            "THE ENEMY TOWN HALL IS UNDER ATTACK",
            ToastTone::Danger,
        )))
        .tooltip(Some(
            "TRAIN FOOTMAN\nCost: 60 gold, 20 lumber\nRequires: Barracks".to_string(),
        ))
         .hud_skin(Some(skin))
}

fn minimap_texture() -> TextureImage {
    let side = 256u32;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let index = ((y * side + x) * 4) as usize;
            let nx = x as f32 / side as f32;
            let ny = y as f32 / side as f32;
            let grass = ((nx * 17.0).sin() * (ny * 13.0).cos() * 0.5 + 0.5) * 0.18;
            let water = nx > 0.62 && ny > 0.55;
            let (r, g, b) = if water {
                (0.10, 0.24 + grass * 0.2, 0.42)
            } else {
                (0.16 + grass * 0.4, 0.34 + grass * 0.5, 0.12 + grass * 0.2)
            };
            rgba[index] = (r * 255.0) as u8;
            rgba[index + 1] = (g * 255.0) as u8;
            rgba[index + 2] = (b * 255.0) as u8;
            rgba[index + 3] = 255;
        }
    }
    TextureImage {
        width: side,
        height: side,
        rgba,
    }
}

fn section_of(layout: &wc3_game::hud::mobile_hud::MobileHudLayout, rect: Rect) -> usize {
    let point = rect.centre();
    for (index, section) in layout.sections.iter().enumerate() {
        if point.x >= section.x
            && point.x <= section.right()
            && point.y >= section.y
            && point.y <= section.bottom()
        {
            return index;
        }
    }
    usize::MAX
}

#[allow(clippy::too_many_arguments)]
fn element_table(
    pack: &HudPack,
    skin: &Skin,
    layout: &wc3_game::hud::mobile_hud::MobileHudLayout,
    view: &HudView<'_>,
    batch: &hud::UiBatch,
    textures: &[TextureImage],
    viewport: Vec2,
) -> serde_json::Value {
    let s = layout.scale;
    let mut elements = Vec::new();
    let mut texts = Vec::new();
    let mut push = |id: &str,
                    rect: Rect,
                    leaf: bool,
                    art: Option<&str>,
                    overlap: Option<Rect>| {
        let section = section_of(layout, rect);
        let mut object = json!({
            "id": id,
            "rect": [rect.x, rect.y, rect.w, rect.h],
            "section": section,
            "leaf": leaf,
            "min_px": rect.w.min(rect.h),
            "crosses_section": overlap.map(|check| {
                let a = section_of(layout, check);
                a != section
            }).unwrap_or(false),
        });
        if let Some(name) = art
            && let Some(asset) = pack.asset(name)
        {
            let authored = [asset.w as f32, asset.h as f32];
            object["art"] = json!(name);
            object["authored"] = json!(authored);
            object["insets"] = json!(asset.insets);
            object["sliced"] = json!(asset.sliced());
            object["scale"] = json!(s);
            object["stretch"] = json!([
                rect.w / (authored[0] * s).max(0.01),
                rect.h / (authored[1] * s).max(0.01),
            ]);
            if asset.sliced()
                && let Some(corners) = pack.corner_quads(name, rect, s)
            {
                let rows: Vec<_> = corners
                    .iter()
                    .map(|(dst, src)| {
                        json!({
                            "dst": [dst.x, dst.y, dst.w, dst.h],
                            "src": [src.x, src.y, src.w, src.h],
                        })
                    })
                    .collect();
                object["corners"] = json!(rows);
            }
        }
        elements.push(object);
    };

    for (index, section) in layout.sections.iter().enumerate() {
        push(
            &format!("section_{index}"),
            *section,
            false,
            Some(layout.section_assets[index]),
            None,
        );
    }
    push("minimap", layout.minimap, true, Some("minimap_frame_normal_216"), None);
    push("minimap_image", layout.minimap_image, false, None, None);
    push(
        "minimap_header",
        layout.minimap_header,
        false,
        Some("minimap_header_normal_216x20"),
        None,
    );
    push("selection", layout.selection, true, None, None);
    push(
        "selection_panel",
        layout.panel,
        false,
        Some("selection_panel_normal_240x180"),
        None,
    );
    push("portrait", layout.portrait, false, Some("portrait_frame_normal_128"), Some(layout.selection));
    push("card", layout.card, false, None, None);
    if layout.inventory.w > 0.0 {
        push("inventory", layout.inventory, false, None, None);
    }
    for (index, slot) in layout.card_slots.iter().enumerate() {
        push(
            &format!("card_slot_{index}"),
            *slot,
            true,
            Some("button_border_normal_64"),
            Some(layout.card),
        );
    }
    for (index, slot) in layout.group_slots.iter().enumerate() {
        push(
            &format!("group_hit_{index}"),
            *slot,
            true,
            None,
            Some(layout.groups_area),
        );
        let art_side = (pack.group_cell * s).max(44.0);
        let art = Rect::new(
            slot.centre().x - art_side * 0.5,
            slot.centre().y - art_side * 0.5,
            art_side,
            art_side,
        );
        push(
            &format!("group_art_{index}"),
            art,
            false,
            Some("control_group_button_inactive_44"),
            Some(layout.groups_area),
        );
    }
    for (index, slot) in layout.army_slots.iter().enumerate() {
        push(
            &format!("army_{index}"),
            *slot,
            true,
            Some("button_border_empty_64"),
            Some(layout.army),
        );
    }
    for (index, slot) in layout.inventory_parts.slots.iter().enumerate() {
        // The drawn plate fits the band; the *touch* rect carries the 90 px
        // finger floor, so the interactive leaf is the touch rect (the art is
        // reported separately and is deliberately smaller).
        push(
            &format!("inventory_slot_{index}"),
            layout.inventory_parts.touches[index],
            true,
            Some("button_border_empty_64"),
            Some(layout.inventory),
        );
        push(
            &format!("inventory_art_{index}"),
            *slot,
            false,
            Some("button_border_empty_64"),
            Some(layout.inventory),
        );
    }
    // The topbar is a measured composition (flat column + authored plates at
    // both ends), not a plain 9-slice: its element row carries no stretch
    // claim; the draw trace and the PNG prove the treatment.
    push("topbar", layout.top_meta, false, None, None);
    for (index, pill) in layout.resource_pills.iter().enumerate() {
        push(
            &format!("resource_pill_{index}"),
            *pill,
            false,
            Some("resource_pill_normal_236x32"),
            None,
        );
    }
    if layout.tooltip.w > 0.0 {
        push(
            "tooltip",
            layout.tooltip,
            false,
            Some("tooltip_panel_normal_384x256"),
            None,
        );
    }
    push("hp_bar", layout.hp_bar, false, None, None);
    push(
        "production_bar",
        layout.production_bar,
        false,
        Some("build_progress_normal_64x8"),
        None,
    );
    if layout.toast.w > 0.0 {
        push(
            "toast",
            layout.toast,
            false,
            Some("tooltip_panel_normal_384x256"),
            None,
        );
    }

    for (id, rect, scale) in [
        ("map_name", layout.map_name, 0.90 * s),
        ("upkeep", layout.upkeep, 0.80 * s),
        ("fps", layout.fps, 0.80 * s),
        ("selection_name", layout.name, 0.90 * s),
        ("selection_class", layout.class, 0.80 * s),
        ("hp_text", layout.hp_bar, 0.80 * s),
        ("stats", layout.stats, 0.80 * s),
        ("hotkey_gold", layout.card_slots.first().copied().unwrap_or_default(), 0.80 * s),
    ] {
        texts.push(json!({
            "id": id,
            "rect": [rect.x, rect.y, rect.w, rect.h],
            "px": hud::HudFonts::text_px(scale),
        }));
    }

    // Texture index -> pack asset name (font atlases and the flat/minimap
    // textures are not art and are simply named).
    let mut names: Vec<Option<String>> = vec![None; textures.len()];
    for asset in &pack.assets {
        if asset.texture < names.len() {
            names[asset.texture] = Some(asset.name.clone());
        }
    }
    for (slot, label) in [
        "font_primary",
        "font_serif",
        "font_sans",
        "font_numeric",
    ]
    .iter()
    .enumerate()
    {
        let index = pack.font_textures[slot];
        if index < names.len() {
            names[index] = Some((*label).to_string());
        }
    }
    let mut draws = Vec::new();
    for draw in &batch.textured {
        let name = names
            .get(draw.texture)
            .cloned()
            .flatten()
            .unwrap_or_else(|| format!("texture_{}", draw.texture));
        for chunk in draw.vertices.chunks_exact(6) {
            let xs: Vec<f32> = chunk.iter().map(|vertex| vertex.pos[0]).collect();
            let ys: Vec<f32> = chunk.iter().map(|vertex| vertex.pos[1]).collect();
            let u0 = chunk.iter().map(|v| v.uv[0]).fold(f32::MAX, f32::min);
            let u1 = chunk.iter().map(|v| v.uv[0]).fold(f32::MIN, f32::max);
            let v0 = chunk.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min);
            let v1 = chunk.iter().map(|v| v.uv[1]).fold(f32::MIN, f32::max);
            let x0 = xs.iter().cloned().fold(f32::MAX, f32::min);
            let x1 = xs.iter().cloned().fold(f32::MIN, f32::max);
            let y0 = ys.iter().cloned().fold(f32::MAX, f32::min);
            let y1 = ys.iter().cloned().fold(f32::MIN, f32::max);
            draws.push(json!({
                "texture": draw.texture,
                "name": name,
                "dst": [x0, y0, x1 - x0, y1 - y0],
                "uv": [u0, v0, u1, v1],
            }));
        }
    }

    json!({
        "viewport": [viewport.x, viewport.y],
        "orientation": format!("{:?}", layout.orientation),
        "scale": s,
        "safe": [layout.safe.left, layout.safe.top, layout.safe.right, layout.safe.bottom],
        "console": [layout.console.x, layout.console.y, layout.console.w, layout.console.h],
        "sections": layout.sections.iter().enumerate().map(|(index, rect)| json!({
            "index": index,
            "asset": layout.section_assets[index],
            "rect": [rect.x, rect.y, rect.w, rect.h],
            "insets": pack.insets(layout.section_assets[index]),
            "authored": pack.asset(layout.section_assets[index]).map(|asset| [asset.w, asset.h]),
        })).collect::<Vec<_>>(),
        "summary": pack.slice_summary().to_string(),
        "samples": {
            "map_name": view.map_name,
            "upkeep": view.upkeep,
            "unit_name": view.selection.as_ref().map(|selection| selection.name.clone()).unwrap_or_default(),
            "hotkeys": view.commands.iter().map(|command| command.hotkey).filter(|hotkey| !hotkey.is_empty()).collect::<Vec<_>>(),
        },
        "elements": elements,
        "texts": texts,
        "draws": draws,
        "skin_roles": skin.role_names,
    })
}

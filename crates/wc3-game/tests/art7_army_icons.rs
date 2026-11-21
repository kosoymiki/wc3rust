//! art7_army_icons — bug #7 evidence (Agent B, ledger 586 / resumed under 598).
//!
//! Operator report #7 (REPORT.md:17): "5 units selected, six icon slots visible
//! but EMPTY (no unit icons)". Root cause is not the atlas draw path —
//! `mobile_hud::draw_army_chips` already draws `chip.icon` when it is `Some`.
//! The empty slots come from the icon resolver in `Game::army_chips`: every one
//! of its four steps ends in `UiTheme::texture(name)`, so if the command-icon
//! BLPs were never loaded into the theme the chip carries `None` and the slot
//! is painted as a bare plate.
//!
//! Two proofs, one mechanism:
//!
//! 1. `art7_army_icons` seeds the `UiTheme` icon cache through the same public
//!    `load_command_icon` path the real asset loader uses (a resolver that
//!    returns a decodable white BLP), then asserts that five selected units
//!    produce five chips, each with a resolved texture index, one distinct icon
//!    key per unit def, a live HP ratio and the gold active-subgroup frame.
//! 2. `art7_army_icons_shipped_blp` mounts the extracted shipped install
//!    (`analysis/war3_full3`) as a loose asset source, so the real BLP icon
//!    pipeline runs end to end: object-data `Art` field -> icon alias ->
//!    `ReplaceableTextures\CommandButtons\*.blp` decode -> `UiTheme` texture ->
//!    `Game::army_chips`. It also pins the labels to the shipped names
//!    (`*UnitStrings.txt`/`UnitUI`), not to hardcoded card codes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use wc3_game::assets::TextureImage;
use wc3_game::ui::UiResourceResolver;
use wc3_game::{Assets, Game};

/// Synthetic BLP resolver: every command-icon path decodes to a small opaque
/// sheet. This is a test fixture, not shipped art — the icon *keys* and the
/// resolver wiring are what this test proves, not the pixel content.
struct SyntheticIcons;

impl UiResourceResolver for SyntheticIcons {
    fn read_ui_text(&mut self, _path: &str) -> Option<String> {
        None
    }
    fn read_ui_texture(&mut self, _path: &str) -> Option<TextureImage> {
        Some(TextureImage::white())
    }
}

#[test]
fn art7_army_icons() {
    let mut game = Game::new(Assets::load(None, None), 0xA27);
    game.enter_battle();
    game.resize(2414.0, 1080.0);

    // Select up to five *distinct-def* friendly units: five chips, five icon
    // keys. The shipped base has a hall + workers (+ guards).
    let mut chosen = Vec::new();
    let mut seen_defs = HashSet::new();
    for unit in game.sim.alive_units() {
        if unit.owner != game.player || !seen_defs.insert(unit.def().id) {
            continue;
        }
        chosen.push((unit.id, unit.def().id));
        if chosen.len() == 5 {
            break;
        }
    }
    assert!(chosen.len() >= 2, "base must expose >=2 distinct unit defs");

    // Seed one icon per *actual* selected def id through the real alias +
    // load_command_icon API (same calls as assets.rs:2229/2247). Using the
    // selected defs (not a hard-coded roster) keeps the test honest across the
    // fallback and retail-map scenarios.
    let mut resolver = SyntheticIcons;
    for (_id, def) in &chosen {
        let key = format!("ART7_{}", def.to_ascii_uppercase());
        game.assets.ui.set_icon_alias(def, &key);
        assert!(
            game.assets
                .ui
                .load_command_icon(&mut resolver, &mut game.assets.textures, &key)
                .is_some(),
            "synthetic icon must load for {def}"
        );
    }

    game.selection = chosen.iter().map(|(id, _)| *id).collect();
    let chips = game.army_chips();
    assert_eq!(
        chips.len(),
        chosen.len(),
        "one army chip per selected unit (5 selected -> 5 chips)"
    );

    let mut icon_indices = HashSet::new();
    for (index, chip) in chips.iter().enumerate() {
        let icon = chip.icon.unwrap_or_else(|| {
            panic!("chip {index} has no resolved icon texture (empty army slot)")
        });
        assert!(
            game.assets.textures.get(icon).is_some(),
            "chip {index} icon index {icon} must address a real texture"
        );
        icon_indices.insert(icon);
        assert!((0.0..=1.0).contains(&chip.hp), "chip {index} HP in 0..=1");
        assert_eq!(
            chip.selected,
            index == 0,
            "only the first chip is the active subgroup"
        );
    }
    assert_eq!(
        icon_indices.len(),
        chosen.len(),
        "distinct unit defs must resolve to distinct icon keys"
    );
    for (_id, def) in &chosen {
        assert!(
            game.assets.ui.type_icon(def).is_some(),
            "the drawn chips resolve through type_icon for {def}"
        );
    }

    eprintln!(
        "ART7_ARMY_ICONS units={} chips={} distinct_icons={} PASS defs={:?}",
        chosen.len(),
        chips.len(),
        icon_indices.len(),
        chosen.iter().map(|(_, d)| *d).collect::<Vec<_>>()
    );
}

/// Repository root, resolved from this test crate (`crates/wc3-game/tests`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// The extracted shipped install used as the asset oracle by the rest of the
/// suite (`analysis/war3_full3`, StormLib extract). Not bundled game data: it
/// only exists on the operator's machine and the test skips without it.
fn shipped_install() -> Option<PathBuf> {
    let root = repo_root().join("analysis/war3_full3");
    (root.join("Units/UnitData.slk").is_file()).then_some(root)
}

/// Real command-button art is opaque, multi-colour and at least 32 px. A flat
/// white fixture or an unloaded 1x1 texture fails this: it is the difference
/// between "the slot has an index" and "the slot shows shipped art".
fn is_shipped_command_button(image: &TextureImage) -> bool {
    if image.width < 32 || image.height < 32 {
        return false;
    }
    let pixels = (image.rgba.len() / 4) as u64;
    let mut opaque = 0u64;
    let mut colours = HashSet::new();
    for pixel in image.rgba.chunks_exact(4) {
        if pixel[3] > 200 {
            opaque += 1;
        }
        colours.insert((pixel[0] / 24, pixel[1] / 24, pixel[2] / 24));
    }
    opaque * 100 >= pixels * 50 && colours.len() >= 8
}

/// Two tests in this binary each mount a full shipped install; serialize the
/// mounts so audio/object-data globals are initialized one at a time.
static SHIPPED_LOAD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// #7 shipped-path proof: the real BLP icons, resolved from the extracted
/// install, reach the army chips with the shipped labels.
#[test]
fn art7_army_icons_shipped_blp() {
    let _guard = SHIPPED_LOAD.lock().unwrap_or_else(|error| error.into_inner());
    let Some(root) = shipped_install() else {
        eprintln!("ART7_SHIPPED_SKIP analysis/war3_full3 missing (no shipped oracle)");
        return;
    };
    let assets = Assets::load(Some(&root), None);
    assert!(
        assets
            .debug
            .iter()
            .any(|line| line.starts_with("loose_install=")),
        "the loose shipped install must mount as an asset source: {:?}",
        assets.debug.iter().take(6).collect::<Vec<_>>()
    );
    assert!(
        assets.models.iter().any(|model| !model.fallback),
        "real MDX models must come from the shipped tree"
    );

    let mut game = Game::new(assets, 0xA27);
    game.enter_battle();
    game.resize(2414.0, 1080.0);

    let mut chosen = Vec::new();
    let mut seen_defs = HashSet::new();
    for unit in game.sim.alive_units() {
        if unit.owner != game.player || !seen_defs.insert(unit.def().id) {
            continue;
        }
        chosen.push((unit.id, unit.def().id));
        if chosen.len() == 5 {
            break;
        }
    }
    assert!(chosen.len() >= 2, "base must expose >=2 distinct unit defs");

    let icon_dir = root.join("ReplaceableTextures/CommandButtons");
    let mut icon_names = Vec::new();
    for (_id, def) in &chosen {
        let name = game
            .assets
            .ui
            .type_icon_name(def)
            .unwrap_or_else(|| panic!("{def} must carry a shipped command-icon alias"))
            .to_string();
        assert!(
            icon_dir.join(format!("{name}.blp")).is_file(),
            "{def} -> {name}.blp must exist in the shipped install"
        );
        let texture = game
            .assets
            .ui
            .type_icon(def)
            .unwrap_or_else(|| panic!("{def} icon {name} must be decoded into the theme"));
        let image = game
            .assets
            .textures
            .get(texture)
            .unwrap_or_else(|| panic!("texture {texture} for {def} must exist"));
        assert!(
            is_shipped_command_button(image),
            "{def} icon {name} must be real command-button art, got {}x{}",
            image.width,
            image.height
        );

        // Labels come from the shipped object data, not a hardcoded 3-char code
        // (`KA3`) and not a truncated placeholder (`DP..`).
        let label = game
            .assets
            .name_for_def(def)
            .unwrap_or_else(|| panic!("{def} must carry a shipped unit name"));
        assert!(
            label.chars().filter(|c| c.is_alphabetic()).count() >= 3,
            "{def} label {label:?} must be a real shipped name"
        );
        assert!(
            !label.contains("...") && label != *def,
            "{def} label {label:?} must not be a truncated code"
        );
        icon_names.push(name);
    }

    game.selection = chosen.iter().map(|(id, _)| *id).collect();
    game.resize(2414.0, 1080.0);
    let chips = game.army_chips();
    assert_eq!(chips.len(), chosen.len(), "one chip per selected unit");
    let mut indices = HashSet::new();
    for (index, chip) in chips.iter().enumerate() {
        let icon = chip
            .icon
            .unwrap_or_else(|| panic!("chip {index} has no shipped BLP icon (empty slot)"));
        indices.insert(icon);
        assert!(
            game.assets.textures.get(icon).is_some(),
            "chip {index} icon {icon} addresses a real texture"
        );
    }
    assert_eq!(
        indices.len(),
        chosen.len(),
        "distinct defs use distinct shipped icons: {icon_names:?}"
    );

    // The command card's build/train labels are the same shipped names the
    // object data carries — the `art7` label contract, not a private table.
    // The worker build card is drawn only when the worker is the selection
    // (`has_worker && !has_hall`, hud.rs), so select just the worker here.
    let worker = chosen
        .iter()
        .find(|(_, def)| game.assets.build_menu(def).iter().any(|entry| !entry.name.is_empty()));
    if let Some((worker_id, def)) = worker {
        let menu = game.assets.build_menu(def);
        assert!(!menu.is_empty(), "the worker has a shipped build menu");
        let army_selection = std::mem::replace(&mut game.selection, vec![*worker_id]);
        // `self.menu` is recomputed by `refresh_buttons` (via resize) for the
        // new selection; without it the card would still show the hall's list.
        game.resize(2414.0, 1080.0);
        let buttons = wc3_game::hud::build_buttons(&game);
        game.selection = army_selection;
        let build_labels: Vec<&str> = buttons
            .iter()
            .filter(|button| matches!(button.action, wc3_game::UiAction::BuildDef(_)))
            .map(|button| button.label.as_str())
            .collect();
        assert!(
            !build_labels.is_empty(),
            "the selected worker draws its build card: {:?}",
            buttons.iter().map(|b| b.label.as_str()).collect::<Vec<_>>()
        );
        for label in build_labels {
            assert!(
                !label.contains("..."),
                "card label {label:?} must not be truncated"
            );
            assert!(
                menu.iter().any(|entry| entry.name.to_uppercase() == label),
                "card label {label:?} must come from the shipped menu: {:?}",
                menu.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>()
            );
        }
    }

    eprintln!(
        "ART7_SHIPPED_BLP defs={:?} icons={icon_names:?} chips={} distinct={} PASS",
        chosen.iter().map(|(_, def)| *def).collect::<Vec<_>>(),
        chips.len(),
        indices.len()
    );
}

/// The Human04 campaign map the visual regression and the GPU evidence use.
fn shipped_map() -> Option<PathBuf> {
    let map = repo_root().join("analysis/war3_map/Maps/Campaign/Human04.w3m");
    map.is_file().then_some(map)
}

/// Same source order as the visual regression: the operator's MPQ first, the
/// extracted tree second. `None` means no shipped data on this machine.
fn shipped_assets(map: &Path) -> Option<Assets> {
    for mpq in [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.wc3rust.p0/files/War3.mpq",
    ] {
        let mpq = Path::new(mpq);
        if mpq.is_file() {
            return Some(Assets::load(Some(mpq), Some(map)));
        }
    }
    let loose = repo_root().join("analysis/war3_full3");
    loose.is_dir().then(|| Assets::load(Some(&loose), Some(map)))
}

/// Optional headless-Vulkan frame for the #7 evidence bundle. Off by default:
/// the shared phone's GPU path takes ~1-2 min per frame and the device slot is
/// agent G's, so the mandatory gate stays CPU-only. `H2_GPU_EVIDENCE=1` runs it
/// and writes a real 2414x1080 frame (headless, NOT a device capture).
#[test]
fn art7_army_icons_gpu_frame() {
    if std::env::var("H2_GPU_EVIDENCE").ok().as_deref() != Some("1") {
        eprintln!("ART7_GPU_SKIP set H2_GPU_EVIDENCE=1 to render a headless frame");
        return;
    }
    let _guard = SHIPPED_LOAD.lock().unwrap_or_else(|error| error.into_inner());
    let Some(map) = shipped_map() else {
        eprintln!("ART7_GPU_SKIP Human04.w3m missing");
        return;
    };
    let Some(assets) = shipped_assets(&map) else {
        eprintln!("ART7_GPU_SKIP no shipped install");
        return;
    };
    let mut game = Game::new(assets, 0xA27);
    game.enter_battle();
    game.resize(2414.0, 1080.0);
    for _ in 0..8 {
        game.update(1.0 / 30.0);
    }
    let mut seen = HashSet::new();
    let mut chosen = Vec::new();
    for unit in game.sim.alive_units() {
        if unit.owner != game.player || !seen.insert(unit.def().id) {
            continue;
        }
        chosen.push(unit.id);
        if chosen.len() == 5 {
            break;
        }
    }
    game.selection = chosen;
    game.resize(2414.0, 1080.0);
    let chips = game.army_chips();
    assert!(
        chips.iter().all(|chip| chip.icon.is_some()),
        "every chip must resolve a shipped icon: {chips:?}"
    );
    let scene = game.scene();
    // Drawn chip windows from the batch, not the stored hit layout: the two can
    // diverge and the screenshot must be the pixels the player sees.
    let chip_icons: Vec<usize> = chips.iter().filter_map(|chip| chip.icon).collect();
    let mut windows = Vec::new();
    for texture in &chip_icons {
        let mut best: Option<(f32, f32, f32, f32, f32)> = None;
        for draw in &scene.ui.textured {
            if draw.texture != *texture || draw.vertices.is_empty() {
                continue;
            }
            let x0 = draw.vertices.iter().map(|v| v.pos[0]).fold(f32::INFINITY, f32::min);
            let y0 = draw.vertices.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
            let x1 = draw.vertices.iter().map(|v| v.pos[0]).fold(f32::NEG_INFINITY, f32::max);
            let y1 = draw.vertices.iter().map(|v| v.pos[1]).fold(f32::NEG_INFINITY, f32::max);
            let area = (x1 - x0) * (y1 - y0);
            // The same icon also appears as the selection portrait and a card
            // icon; the army chip is the *bottom-band* copy (y > 0.8 * 1080).
            if y0 < 864.0 {
                continue;
            }
            if best.map_or(true, |(a, ..)| area > a) {
                best = Some((area, x0, y0, x1, y1));
            }
        }
        let Some((_, x0, y0, x1, y1)) = best else {
            panic!("chip icon texture {texture} is not drawn in the UI batch");
        };
        windows.push((x0, y0, x1 - x0, y1 - y0));
    }
    assert_eq!(windows.len(), chips.len(), "one drawn window per chip");
    let frame = wc3_game::render::render_scene_to_rgba(
        &game.assets,
        &game.font.atlas,
        game.assets.minimap.as_ref(),
        &scene,
        2414,
        1080,
    )
    .expect("headless frame");
    let out_dir = repo_root().join("analysis/agent-hud/all-layers-20261121");
    std::fs::create_dir_all(&out_dir).expect("evidence dir");
    wc3_game::render::write_ppm(
        out_dir
            .join("headless-NOT-DEVICE-army-panel-2414x1080.ppm")
            .to_str()
            .expect("utf8 path"),
        2414,
        1080,
        &frame,
    );
    let mut report = Vec::new();
    for (index, (sx, sy, sw, sh)) in windows.iter().copied().enumerate() {
        let inset = 6.0f32.min(sw.min(sh) * 0.2);
        let x0 = (sx + inset).max(0.0) as u32;
        let y0 = (sy + inset).max(0.0) as u32;
        let x1 = ((sx + sw - inset).max(x0 as f32) as u32).min(2414);
        let y1 = ((sy + sh - inset).max(y0 as f32) as u32).min(1080);
        let mut colours = HashSet::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let at = ((y * 2414 + x) * 4) as usize;
                colours.insert((frame[at] / 24, frame[at + 1] / 24, frame[at + 2] / 24));
            }
        }
        assert!(
            colours.len() >= 16,
            "army window {index} is a bare plate in the headless frame ({} colours) at ({sx:.0},{sy:.0})",
            colours.len()
        );
        report.push(format!("window{index}={}x{} colours={}", sw as i32, sh as i32, colours.len()));
    }
    eprintln!(
        "ART7_GPU_VISIBLE png={} chips={} PASS {}",
        out_dir.join("headless-NOT-DEVICE-army-panel-2414x1080.ppm").display(),
        chips.len(),
        report.join(" | ")
    );
}

//! FDF/UI framework integration tests against the extracted retail corpus.
//!
//! The per-file counts below come from an independent Python reference
//! implementation (`analysis/agent-ui-framework/tools/fdf_reference.py`), so
//! this test cross-checks two parsers rather than only asserting self
//! consistency. Corpus: 70 files, 1936 declarations, 6462 properties,
//! 59 IncludeFile statements, 1294 INHERITS (388 WITHCHILDREN).
//!
//! Run: `cargo test -p wc3-game --test ui_fdf`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use glam::Vec2;
use wc3_formats::fdf::{FdfDocument, FdfNode};
use wc3_game::hud::{Font, Rect, UiBatch};
use wc3_game::ui::text::{Localization, TextShaper, TextStyle};
use wc3_game::ui::{AlphaMode, LayoutConfig, PaintKind, SkinTable, TextBindings, UiState, UiTheme};

const EXPECTED: &[(&str, usize, usize, usize)] = &[
    ("UI/FrameDef/DateStrings.fdf", 1, 19, 0),
    ("UI/FrameDef/GlobalStrings.fdf", 1, 728, 0),
    ("UI/FrameDef/Glue/AdvancedOptionsDisplay.fdf", 17, 41, 1),
    ("UI/FrameDef/Glue/AdvancedOptionsPane.fdf", 23, 62, 1),
    ("UI/FrameDef/Glue/BattleNetChatActionMenu.fdf", 4, 15, 1),
    ("UI/FrameDef/Glue/BattleNetChatPanel.fdf", 75, 258, 1),
    ("UI/FrameDef/Glue/BattleNetChatroom.fdf", 1, 1, 1),
    ("UI/FrameDef/Glue/BattleNetCustomCreatePanel.fdf", 34, 87, 1),
    ("UI/FrameDef/Glue/BattleNetCustomJoinPanel.fdf", 30, 82, 1),
    ("UI/FrameDef/Glue/BattleNetCustomLoadPanel.fdf", 15, 36, 1),
    ("UI/FrameDef/Glue/BattleNetFriendsListBox.fdf", 2, 2, 1),
    ("UI/FrameDef/Glue/BattleNetFriendsPane.fdf", 6, 10, 1),
    ("UI/FrameDef/Glue/BattleNetMain.fdf", 64, 148, 1),
    ("UI/FrameDef/Glue/BattleNetNewsBox.fdf", 2, 1, 1),
    ("UI/FrameDef/Glue/BattleNetProfilePanel.fdf", 79, 231, 1),
    ("UI/FrameDef/Glue/BattleNetStandardPanel.fdf", 35, 79, 1),
    ("UI/FrameDef/Glue/BattleNetTeamInvitation.fdf", 19, 48, 1),
    ("UI/FrameDef/Glue/BattleNetTeamPanel.fdf", 81, 229, 1),
    ("UI/FrameDef/Glue/BattleNetTemplates.fdf", 83, 317, 0),
    ("UI/FrameDef/Glue/BattleNetUserListBox.fdf", 2, 2, 1),
    ("UI/FrameDef/Glue/CampaignMenu.fdf", 104, 169, 1),
    ("UI/FrameDef/Glue/CheckListBox.fdf", 3, 2, 1),
    ("UI/FrameDef/Glue/DecoratedMapListBox.fdf", 3, 2, 1),
    ("UI/FrameDef/Glue/DialogWar3.fdf", 13, 41, 1),
    ("UI/FrameDef/Glue/GameChatroom.fdf", 16, 46, 1),
    ("UI/FrameDef/Glue/ListBoxWar3.fdf", 3, 4, 1),
    ("UI/FrameDef/Glue/LoadSavedGameScreen.fdf", 11, 27, 1),
    ("UI/FrameDef/Glue/LocalMultiplayerCreate.fdf", 28, 70, 1),
    ("UI/FrameDef/Glue/LocalMultiplayerJoin.fdf", 28, 70, 1),
    ("UI/FrameDef/Glue/LocalMultiplayerLoad.fdf", 11, 26, 1),
    ("UI/FrameDef/Glue/MainMenu.fdf", 84, 248, 1),
    ("UI/FrameDef/Glue/MapInfoPane.fdf", 15, 44, 1),
    ("UI/FrameDef/Glue/MapListBox.fdf", 3, 2, 1),
    ("UI/FrameDef/Glue/MapPreferenceBox.fdf", 3, 2, 1),
    ("UI/FrameDef/Glue/MultiplayerMenu.fdf", 0, 0, 0),
    ("UI/FrameDef/Glue/OptionsMenu.fdf", 127, 276, 1),
    ("UI/FrameDef/Glue/PlayerSlot.fdf", 25, 120, 1),
    ("UI/FrameDef/Glue/ScoreScreen.fdf", 74, 207, 2),
    ("UI/FrameDef/Glue/SinglePlayerMenu.fdf", 41, 120, 1),
    ("UI/FrameDef/Glue/Skirmish.fdf", 26, 57, 1),
    ("UI/FrameDef/Glue/StandardTemplates.fdf", 147, 568, 0),
    ("UI/FrameDef/Glue/TeamSetup.fdf", 1, 0, 1),
    ("UI/FrameDef/Glue/ViewReplayScreen.fdf", 13, 37, 1),
    ("UI/FrameDef/NetworkStrings.fdf", 1, 127, 0),
    ("UI/FrameDef/UI/AllianceDialog.fdf", 16, 53, 1),
    ("UI/FrameDef/UI/AllianceSlot.fdf", 11, 54, 1),
    ("UI/FrameDef/UI/ChatDialog.fdf", 24, 74, 1),
    ("UI/FrameDef/UI/CinematicPanel.fdf", 9, 49, 1),
    ("UI/FrameDef/UI/ConsoleUI.fdf", 10, 55, 0),
    ("UI/FrameDef/UI/EscMenuMainPanel.fdf", 57, 160, 1),
    ("UI/FrameDef/UI/EscMenuOptionsPanel.fdf", 107, 247, 1),
    ("UI/FrameDef/UI/EscMenuSaveGamePanel.fdf", 37, 101, 1),
    ("UI/FrameDef/UI/EscMenuTemplates.fdf", 52, 241, 0),
    ("UI/FrameDef/UI/GameResultDialog.fdf", 9, 22, 1),
    ("UI/FrameDef/UI/GameSaveSplashDialog.fdf", 3, 14, 1),
    ("UI/FrameDef/UI/InfoPanelBuildingDetail.fdf", 11, 18, 1),
    ("UI/FrameDef/UI/InfoPanelItemDetail.fdf", 3, 4, 1),
    ("UI/FrameDef/UI/InfoPanelTemplates.fdf", 4, 40, 0),
    ("UI/FrameDef/UI/InfoPanelUnitDetail.fdf", 26, 59, 1),
    ("UI/FrameDef/UI/LeaderBoard.fdf", 4, 19, 1),
    ("UI/FrameDef/UI/LogDialog.fdf", 8, 41, 1),
    ("UI/FrameDef/UI/QuestDialog.fdf", 45, 182, 1),
    ("UI/FrameDef/UI/ReplayPanel.fdf", 23, 68, 1),
    ("UI/FrameDef/UI/ResourceBar.fdf", 10, 25, 0),
    ("UI/FrameDef/UI/ScriptDialog.fdf", 5, 18, 1),
    ("UI/FrameDef/UI/SimpleInfoPanel.fdf", 76, 146, 0),
    ("UI/FrameDef/UI/SuspendDialog.fdf", 10, 33, 1),
    ("UI/FrameDef/UI/TimerDialog.fdf", 4, 22, 1),
    ("UI/FrameDef/UI/UnresponsiveDialog.fdf", 5, 17, 1),
    ("UI/FrameDef/UI/UpperButtonBar.fdf", 13, 39, 0),
];

fn corpus_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../analysis/war3_data");
    root.canonicalize().unwrap_or(root)
}

fn walk_fdf(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read corpus dir {}: {error}", dir.display()))
    {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk_fdf(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("fdf"))
        {
            out.push(path);
        }
    }
}

fn load_corpus() -> (
    Vec<(String, FdfDocument)>,
    Vec<(String, usize, usize, usize)>,
) {
    let root = corpus_root();
    let mut files = Vec::new();
    walk_fdf(&root, &mut files);
    files.sort();
    let mut documents = Vec::new();
    let mut counts = Vec::new();
    for path in &files {
        let relative = path
            .strip_prefix(&root)
            .expect("relative")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document = FdfDocument::parse_named(&text, relative.replace('/', "\\"))
            .unwrap_or_else(|error| panic!("parse {relative}: {error}"));
        let (nodes, props) = document.count();
        counts.push((relative.clone(), nodes, props, document.includes.len()));
        documents.push((relative, document));
    }
    (documents, counts)
}

fn count_nodes(document: &FdfDocument) -> (usize, usize, usize) {
    let mut inherits = 0usize;
    let mut with_children = 0usize;
    let mut empty_names = 0usize;
    document.walk(&mut |node: &FdfNode| {
        if node.inherits.is_some() {
            inherits += 1;
        }
        if node.inherit_children {
            with_children += 1;
        }
        if node.name.as_deref().is_none_or(str::is_empty) {
            empty_names += 1;
        }
    });
    (inherits, with_children, empty_names)
}

fn load_skin() -> SkinTable {
    let path = corpus_root().join("UI/war3skins.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    SkinTable::parse(&text)
}

fn load_theme() -> UiTheme {
    let (documents, _) = load_corpus();
    let mut theme = UiTheme::from_documents(
        documents
            .into_iter()
            .map(|(_, document)| document)
            .collect(),
        load_skin(),
    );
    for name in [
        "UI/FrameDef/GlobalStrings.fdf",
        "UI/FrameDef/DateStrings.fdf",
        "UI/FrameDef/NetworkStrings.fdf",
    ] {
        let path = corpus_root().join(name);
        let text = std::fs::read_to_string(&path).expect("strings file");
        theme
            .localization_mut()
            .parse_string_list_text(&text)
            .expect("string list");
    }
    theme
}

fn config(width: f32, height: f32) -> LayoutConfig {
    LayoutConfig::new(Vec2::new(width, height))
}

fn approx(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.02,
        "expected {right}, got {left} (delta {})",
        (left - right).abs()
    );
}

#[test]
fn corpus_all_70_files_match_reference() {
    let (documents, counts) = load_corpus();
    assert_eq!(documents.len(), 70, "corpus file count");
    assert_eq!(counts.len(), EXPECTED.len(), "reference row count");

    let mut total_decls = 0usize;
    let mut total_props = 0usize;
    let mut total_includes = 0usize;
    let mut inherits = 0usize;
    let mut with_children = 0usize;
    let mut empty_names = 0usize;
    for ((relative, _, _, _), (expected_name, expected_decls, expected_props, expected_includes)) in
        counts.iter().zip(EXPECTED)
    {
        assert_eq!(relative, expected_name, "file order");
    }
    for ((relative, document), (_, expected_decls, expected_props, expected_includes)) in
        documents.iter().zip(EXPECTED)
    {
        let (decls, props) = document.count();
        assert_eq!(decls, *expected_decls, "{relative}: declarations");
        assert_eq!(props, *expected_props, "{relative}: properties");
        assert_eq!(
            document.includes.len(),
            *expected_includes,
            "{relative}: includes"
        );
        total_decls += decls;
        total_props += props;
        total_includes += document.includes.len();
        let (document_inherits, document_with_children, document_empty_names) =
            count_nodes(document);
        inherits += document_inherits;
        with_children += document_with_children;
        empty_names += document_empty_names;
    }
    assert_eq!(total_decls, 1936, "total declarations");
    assert_eq!(total_props, 6462, "total properties");
    assert_eq!(total_includes, 59, "total IncludeFile statements");
    assert_eq!(inherits, 1294, "total INHERITS statements");
    assert_eq!(with_children, 388, "total INHERITS WITHCHILDREN");
    // 3 StringList blocks + 9 ConsoleUI textures + 1 ReplayPanel texture +
    // 3 unnamed `Texture INHERITS` in ResourceBar. (The Python reference
    // reports 13 because it takes the INHERITS string as a name there.)
    assert_eq!(empty_names, 16, "unnamed declarations");

    // Every include target must exist in the corpus.
    let root = corpus_root();
    let mut file_set = HashSet::new();
    for (relative, _) in &documents {
        file_set.insert(relative.clone());
    }
    let mut distinct_includes = HashSet::new();
    for (relative, document) in &documents {
        for include in &document.includes {
            let normalized = include.replace('\\', "/");
            assert!(
                file_set.contains(&normalized),
                "{relative}: include target missing: {normalized}"
            );
            distinct_includes.insert(normalized);
        }
    }
    assert_eq!(distinct_includes.len(), 4, "distinct include targets");

    // Retail main-frame names are unique.
    let mut names = HashSet::new();
    for (_, document) in &documents {
        for node in &document.nodes {
            if let Some(name) = node.name.as_deref().filter(|name| !name.is_empty()) {
                assert!(
                    names.insert(name.to_string()),
                    "duplicate main frame {name}"
                );
            }
        }
    }
    assert_eq!(names.len(), 310, "unique top-level frame names");

    // Tree depth stays below the documented limit (root + nested layers).
    let max_depth = documents
        .iter()
        .map(|(_, document)| document.nodes.iter().map(depth).max().unwrap_or(0))
        .max()
        .unwrap_or(0);
    assert_eq!(max_depth, 6, "deepest declaration chain");
    let _ = root;
}

fn depth(node: &FdfNode) -> usize {
    1 + node.children.iter().map(depth).max().unwrap_or(0)
}

#[test]
fn template_inheritance_is_complete() {
    let (documents, _) = load_corpus();
    let theme = UiTheme::from_documents(
        documents
            .into_iter()
            .map(|(_, document)| document)
            .collect(),
        load_skin(),
    );
    assert_eq!(theme.template_count(), 310, "top-level templates");

    // Every INHERITS target must resolve, including cross-kind inheritance
    // (120 retail statements inherit a CHECKBOX/TEXTBUTTON template from a
    // different concrete kind).
    let mut checked = 0usize;
    for document in theme.documents() {
        document.walk(&mut |node: &FdfNode| {
            if let Some(base) = &node.inherits {
                assert!(
                    theme.template(base).is_some(),
                    "unresolved template {base} used by {:?}",
                    node.name
                );
                checked += 1;
            }
        });
    }
    assert_eq!(checked, 1294, "resolved INHERITS statements");

    // ResourceBarGoldText pulls Font/Height/TextLength from a `String`
    // template while being a `String` child itself.
    let tree = theme
        .resolve_tree("ResourceBarFrame")
        .expect("ResourceBarFrame");
    assert!(tree.warnings.is_empty(), "{:?}", tree.warnings);
    let gold = tree.find("ResourceBarGoldText").expect("gold text");
    let gold = tree.node(gold);
    assert_eq!(gold.template.as_deref(), Some("ResourceBarTextTemplate"));
    approx(gold.height().unwrap(), 0.01640625);
    let font = gold.font().expect("inherited font");
    assert_eq!(font.name, "MasterFont");
    approx(font.size, 0.01);
    assert_eq!(gold.number("TextLength"), Some(8.0));
    assert_eq!(gold.font_justification_h(), Some("JUSTIFYRIGHT"));
    assert_eq!(gold.anchor().unwrap().0, "TOPRIGHT");

    // WITHCHILDREN merges a local child over the template child with the same
    // name instead of duplicating it (MainMenu's RealmSelectOKButton).
    let tree = theme.resolve_tree("MainMenuFrame").expect("MainMenuFrame");
    assert!(tree.warnings.is_empty(), "{:?}", tree.warnings);
    let button_id = tree.find("RealmSelectOKButton").expect("button");
    let button = tree.node(button_id);
    let mut matching = 0usize;
    for child in &button.children {
        let child = tree.node(*child);
        if child.name.as_deref() == Some("RealmSelectOKText") {
            matching += 1;
            assert_eq!(child.text(), Some("OK"), "local text wins");
            let font = child.font().expect("style inherited from text template");
            assert_eq!(font.name, "MasterFont");
            approx(font.size, 0.013);
        }
    }
    assert_eq!(matching, 1, "WITHCHILDREN child merged exactly once");
    assert_eq!(button.button_text(), Some("RealmSelectOKText"));
    assert!(button.children.len() > 2, "template children cloned");
}

#[test]
fn skin_and_localization_tables() {
    let skin = load_skin();
    let sections: Vec<&str> = skin.sections().collect();
    for expected in ["Default", "Human", "Orc", "NightElf", "Undead", "Main"] {
        assert!(
            sections.contains(&expected),
            "missing skin section {expected}"
        );
    }
    assert!(skin.len() > 400, "skin entries: {}", skin.len());
    assert_eq!(
        skin.get_in("Default", "ConsoleTexture01"),
        Some("UI\\Console\\Human\\HumanUITile01")
    );
    assert_eq!(
        skin.get_in("Orc", "ConsoleTexture01"),
        Some("UI\\Console\\Orc\\OrcUITile01")
    );
    assert_eq!(
        skin.resolve_texture("ConsoleTexture01").as_deref(),
        Some("UI\\Console\\Human\\HumanUITile01.blp")
    );
    assert_eq!(
        skin.get("GoldIcon"),
        Some("UI\\Feedback\\Resources\\ResourceGold.blp")
    );
    let mut skin = skin;
    assert!(skin.set_race("Orc"));
    assert_eq!(
        skin.resolve_texture("ConsoleTexture01").as_deref(),
        Some("UI\\Console\\Orc\\OrcUITile01.blp")
    );
    assert!(skin.set_race("Human"));
    // Human does not override ConsoleTexture01; Default must be used.
    assert_eq!(
        skin.resolve_texture("ConsoleTexture01").as_deref(),
        Some("UI\\Console\\Human\\HumanUITile01.blp")
    );
    let fonts = skin.font_entries();
    assert!(
        fonts.contains(&("MasterFont".to_string(), "Fonts\\FRIZQT__.TTF".to_string())),
        "font entries: {fonts:?}"
    );

    let theme = load_theme();
    let localization = theme.localization();
    assert_eq!(localization.len(), 1748, "StringList entries (canonical parser count)");
    let account = localization.text("ACCOUNT_CREATION");
    assert_ne!(account, "ACCOUNT_CREATION");
    assert!(!account.is_empty());
    let colon = localization.text("COLON_ACCOUNT_NAME");
    assert!(colon.ends_with(':'), "colon key: {colon:?}");
    let key = localization.text("KEY_MENU");
    assert_ne!(key, "KEY_MENU");
    assert_eq!(localization.text("RESTART_MISSION"), "RESTART_MISSION");
    assert_eq!(localization.text("00:00:00"), "00:00:00");

    let mut wts = Localization::new();
    assert_eq!(wts.parse_wts("STRING 7\n{\nHello |nworld\n}\n"), 1);
    assert_eq!(wts.text("TRIGSTR_7"), "Hello |nworld");
}

#[test]
fn layout_console_ui_anchors() {
    let theme = load_theme();
    let font = Font::load();
    let bindings = TextBindings::new();
    let layout = theme
        .build_layout("ConsoleUI", &config(1920.0, 1080.0), &bindings, Some(&font))
        .expect("ConsoleUI layout");
    let root = layout.named("ConsoleUI").expect("root");
    assert_eq!(root.rect.x, 0.0);
    assert_eq!(root.rect.y, 0.0);
    assert_eq!(root.rect.w, 1920.0);
    assert_eq!(root.rect.h, 1080.0);
    assert_eq!(root.children.len(), 9, "ConsoleUI textures");
    assert!(
        !root
            .paint
            .iter()
            .any(|paint| paint.kind == PaintKind::Texture)
    );

    let first = layout.node(root.children[0]);
    approx(first.rect.x, 0.0);
    approx(first.rect.y, 0.0);
    approx(first.rect.w, 0.256 * 1920.0);
    approx(first.rect.h, 0.032 * 1080.0);
    let paint = first.paint.first().expect("texture paint");
    assert_eq!(paint.kind, PaintKind::Texture);
    assert_eq!(paint.key.as_deref(), Some("ConsoleTexture01"));
    assert_eq!(
        paint.file.as_deref(),
        Some("UI\\Console\\Human\\HumanUITile01.blp"),
        "skin-decorated path"
    );
    assert_eq!(
        paint.uv,
        [0.0, 0.0, 1.0, 0.125],
        "TexCoord left,right,top,bottom"
    );
    assert_eq!(paint.blend, AlphaMode::AlphaKey);

    // Anchor TOPRIGHT,-0.288,0 on a 0.053-wide tile.
    let third = layout.node(root.children[2]);
    approx(third.rect.x, 1920.0 - 0.288 * 1920.0 - 0.053 * 1920.0);
    approx(third.rect.y, 0.0);

    // Hit-testing: the first tile owns the top-left pixel; nothing owns a
    // point outside the screen.
    let hit = layout.hit_test(Vec2::new(8.0, 8.0)).expect("texture hit");
    assert_eq!(
        layout.node(hit).paint[0].key.as_deref(),
        Some("ConsoleTexture01")
    );
    assert_eq!(layout.hit_test(Vec2::new(-4.0, 8.0)), None);
}

#[test]
fn layout_resource_bar() {
    let theme = load_theme();
    let font = Font::load();
    let bindings = TextBindings::new()
        .with("ResourceBarGoldText", "1234")
        .with("ResourceBarLumberText", "350")
        .with("ResourceBarSupplyText", "12/20");
    let layout = theme
        .build_layout(
            "ResourceBarFrame",
            &config(1920.0, 1080.0),
            &bindings,
            Some(&font),
        )
        .expect("resource bar layout");
    let frame = layout.named("ResourceBarFrame").expect("frame");
    approx(frame.rect.x, 0.0);
    approx(frame.rect.y, 0.0);
    approx(frame.rect.w, 0.33828125 * 1920.0);
    approx(frame.rect.h, 0.0218 * 1080.0);

    let gold = layout.named("ResourceBarGoldText").expect("gold");
    approx(gold.rect.y, 0.003125 * 1080.0);
    // Anchor TOPRIGHT,-0.26484375,0 relative to the bar's right edge.
    approx(
        gold.rect.x + gold.rect.w,
        frame.rect.x + frame.rect.w - 0.26484375 * 1920.0,
    );
    assert!(gold.rect.w > 0.0, "dynamic text measured: {:?}", gold.rect);
    assert_eq!(gold.text.as_ref().unwrap().text, "1234");
    approx(gold.rect.h, 0.01640625 * 1080.0);

    let mut icon = None;
    for (_, paint) in layout.paints() {
        if paint.key.as_deref() == Some("GoldIcon") {
            icon = Some(paint);
            break;
        }
    }
    let icon = icon.expect("GoldIcon paint");
    assert_eq!(
        icon.file.as_deref(),
        Some("UI\\Feedback\\Resources\\ResourceGold.blp")
    );

    let lumber = layout.named("ResourceBarLumberText").expect("lumber");
    approx(
        lumber.rect.x + lumber.rect.w,
        frame.rect.x + frame.rect.w - 0.178125 * 1920.0,
    );
    let supply = layout.named("ResourceBarSupplyText").expect("supply");
    approx(
        supply.rect.x + supply.rect.w,
        frame.rect.x + frame.rect.w - 0.09140625 * 1920.0,
    );

    // Hit-testing the bar area returns a descendant, not the root.
    let hit = layout.hit_test(Vec2::new(10.0, 10.0)).expect("bar hit");
    assert_ne!(hit, layout.find("ResourceBarFrame").unwrap());
}

#[test]
fn layout_simple_info_panel() {
    let theme = load_theme();
    let font = Font::load();
    let bindings = TextBindings::new()
        .with("SimpleNameValue", "Footman")
        .with("SimpleClassValue", "Human");
    let layout = theme
        .build_layout(
            "SimpleInfoPanelUnitDetail",
            &config(1920.0, 1080.0),
            &bindings,
            Some(&font),
        )
        .expect("unit detail layout");
    let root = layout
        .named("SimpleInfoPanelUnitDetail")
        .expect("unit detail root");
    assert_eq!(root.rect.w, 1920.0);
    assert_eq!(root.rect.h, 1080.0);
    let name = layout.named("SimpleNameValue").expect("name value");
    approx(name.rect.x + name.rect.w * 0.5, 960.0);
    approx(name.rect.y, 0.0);
    assert!(name.rect.w > 0.0);
    assert!(name.rect.h > 0.0);
    assert_eq!(name.text.as_ref().unwrap().text, "Footman");
    let bar = layout.named("SimpleHeroLevelBar").expect("hero bar");
    approx(bar.rect.y, name.rect.y + name.rect.h + 0.0015 * 1080.0);
    approx(bar.rect.x, 960.0);
    approx(bar.rect.h, 0.015625 * 1080.0);

    // Every rect in the tree is finite and non-negative.
    for node in &layout.nodes {
        assert!(node.rect.x.is_finite() && node.rect.y.is_finite());
        assert!(node.rect.w.is_finite() && node.rect.h.is_finite());
        assert!(node.rect.w >= 0.0 && node.rect.h >= 0.0);
    }

    // `Layer "ARTWORK"` groups the build-queue texture; the layer name is
    // propagated to the paint so a renderer can order layers.
    let building = theme
        .build_layout(
            "SimpleInfoPanelBuildingDetail",
            &config(1920.0, 1080.0),
            &TextBindings::new(),
            Some(&font),
        )
        .expect("building detail layout");
    let mut layered = 0usize;
    for (_, paint) in building.paints() {
        if paint.layer.as_deref() == Some("ARTWORK") {
            layered += 1;
        }
    }
    assert!(layered > 0, "ARTWORK layer paint");
    assert_eq!(
        building
            .named("SimpleInfoPanelBuildingDetail")
            .unwrap()
            .rect
            .w,
        1920.0
    );
}

#[test]
fn hit_testing_main_menu_button() {
    let theme = load_theme();
    let font = Font::load();
    let layout = theme
        .build_layout(
            "MainMenuFrame",
            &config(1920.0, 1080.0),
            &TextBindings::new(),
            Some(&font),
        )
        .expect("main menu layout");
    let button = layout.find("RealmSelectOKButton").expect("button");
    let centre = layout.node(button).centre();
    let hits = layout.hit_test_all(centre);
    assert!(
        hits.iter()
            .any(|index| *index == button || layout.node(button).children.contains(index)),
        "button subtree not in hit list: {hits:?}"
    );
    assert_eq!(
        layout.hit_test_interactive(centre),
        Some(button),
        "interactive hit returns the button, not a decorative container"
    );
    assert!(!hits.is_empty());
}

#[test]
fn text_shaping_uses_real_font_and_fdf_style() {
    let root = corpus_root();
    let text = std::fs::read_to_string(root.join("UI/FrameDef/Glue/StandardTemplates.fdf"))
        .expect("standard templates");
    let document = FdfDocument::parse(&text).expect("parse");
    let template = document
        .find("StandardButtonTextTemplate")
        .expect("template");
    let style = TextStyle::from_fdf(template);
    assert_eq!(style.font, "MasterFont");
    approx(style.size, 0.013);
    assert_eq!(style.halign, wc3_game::ui::text::HAlign::Center);
    assert_eq!(style.valign, wc3_game::ui::text::VAlign::Middle);
    let shadow = style.shadow.expect("shadow");
    approx(shadow.offset[0], 0.001);
    approx(shadow.offset[1], -0.001);
    assert!(style.flags.contains(&"FIXEDSIZE".to_string()));

    let registry = wc3_game::ui::FontRegistry::with_builtin_fonts();
    assert_eq!(registry.resolve("MasterFont"), "Fonts\\FRIZQT__.TTF");

    let font = Font::load();
    let shaper = TextShaper::new(&font);
    let screen = Vec2::new(1920.0, 1080.0);
    let shaped = shaper.shape("|cffffcc00GOLD|r 100", &style, screen, None);
    assert_eq!(shaped.lines.len(), 1);
    assert_eq!(shaped.lines[0].runs.len(), 2);
    assert_eq!(shaped.lines[0].runs[0].text, "GOLD");
    assert!(shaped.width > 0.0);
    let wrapped = shaper.shape(
        "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG",
        &style,
        screen,
        Some(shaped.width.max(20.0)),
    );
    assert!(wrapped.lines.len() > 1, "word wrap");
    let mut batch = UiBatch::default();
    shaper.draw(
        &mut batch,
        &shaped,
        Rect::new(0.0, 0.0, 300.0, 40.0),
        &style,
    );
    assert!(!batch.font.is_empty(), "glyph quads emitted");
}

#[test]
fn frame_interaction_shortcuts_focus_and_popups() {
    let theme = load_theme();
    let font = Font::load();

    let main = theme
        .build_layout(
            "MainMenuFrame",
            &config(1920.0, 1080.0),
            &TextBindings::new(),
            Some(&font),
        )
        .expect("main menu layout");
    let order = main.focus_order();
    assert!(order.len() >= 3, "focus ring: {order:?}");
    let first = &main.nodes[order[0]];
    assert!(
        first.interaction.tab_default,
        "focus starts at TabFocusDefault"
    );
    // Retail MainMenu has a dangling TabFocusNext ("MultiPlayerButton" is
    // created by script); the ring must still follow every resolvable link.
    let linked = main
        .nodes
        .iter()
        .enumerate()
        .find_map(|(index, node)| {
            let name = node.interaction.tab_next.as_deref()?;
            main.find(name)
                .map(|target| (index, target, name.to_string()))
        })
        .expect("at least one resolvable TabFocusNext");
    assert_eq!(main.tab_next(linked.0), Some(linked.1));
    assert!(order.contains(&linked.0));
    assert_eq!(main.node(linked.1).name.as_deref(), Some(linked.2.as_str()));

    let options = theme
        .build_layout(
            "OptionsMenu",
            &config(1920.0, 1080.0),
            &TextBindings::new(),
            Some(&font),
        )
        .expect("options layout");
    let shortcut = options
        .nodes
        .iter()
        .find_map(|node| node.interaction.shortcut.clone())
        .expect("a ControlShortcutKey");
    let target = options
        .shortcut_target(&shortcut)
        .expect("shortcut resolves");
    assert_eq!(
        options.node(target).interaction.shortcut.as_deref(),
        Some(shortcut.as_str())
    );
    let popup_button = options.nodes.iter().enumerate().find(|(index, node)| {
        node.interaction.popup_menu.is_some() && options.popup_target(*index).is_some()
    });
    let (index, node) = popup_button.expect("PopupMenuFrame resolves");
    let target = options.popup_target(index).unwrap();
    assert_eq!(
        options.node(target).name.as_deref(),
        node.interaction.popup_menu.as_deref()
    );
    // POPUPMENU templates are inherited; interaction metadata survives.
    assert!(
        options
            .nodes
            .iter()
            .any(|node| { node.kind.eq_ignore_ascii_case("POPUPMENU") && node.children.len() > 0 })
    );
}

#[test]
fn ui_state_bindings_cover_resource_and_info_frames() {
    let state = UiState {
        gold: 500,
        lumber: 250,
        supply: 12,
        supply_max: 20,
        army: 4,
        enemy: 3,
        fps: 60.0,
        map_name: "Test Map",
        selected_name: "Footman",
        selected_class: "Human",
    };
    let bindings = state.bindings();
    assert_eq!(bindings.get("ResourceBarGoldText"), Some("500"));
    assert_eq!(bindings.get("ResourceBarLumberText"), Some("250"));
    assert_eq!(bindings.get("ResourceBarSupplyText"), Some("12/20"));
    assert_eq!(bindings.get("SimpleNameValue"), Some("Footman"));
    assert_eq!(bindings.get("SimpleClassValue"), Some("Human"));
    assert_eq!(bindings.get("MapNameValue"), Some("Test Map"));
}

// ---------------------------------------------------------------------------
// FULL GOAL mechanic ids: ui.fdf.frames / ui.fonts / ui.text / ui.layout
// ---------------------------------------------------------------------------

fn shipped_font_bytes() -> Vec<u8> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../analysis/war3_full3/Fonts/FRIZQT__.TTF");
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn ui_fdf_frames_six_screens_from_shipped_definitions() {
    let theme = load_theme();
    let font = Font::load();
    let bindings = TextBindings::new();
    let mut total_frames = 0usize;
    let mut total_required = 0usize;
    for screen in wc3_game::fdf_ui::UiScreen::ALL {
        let coverage = wc3_game::fdf_ui::coverage(&theme, screen);
        assert!(
            coverage.complete(),
            "{} incomplete: docs {}/{}, root {} found={}, missing frames {:?}, missing skin {:?}",
            coverage.screen,
            coverage.documents_loaded,
            coverage.documents_expected,
            coverage.root,
            coverage.root_found,
            coverage.required_missing,
            coverage.skin_missing
        );
        assert!(
            coverage.skin_present + coverage.required_present > 0,
            "{} consumed nothing",
            coverage.screen
        );
        total_frames += coverage.frames;
        total_required += coverage.required_present;
        if !coverage.root.is_empty() {
            let layout = wc3_game::fdf_ui::render(
                &theme,
                screen,
                &config(1920.0, 1080.0),
                &bindings,
                Some(&font),
            )
            .unwrap_or_else(|| panic!("{} did not render", coverage.screen));
            assert_eq!(layout.len(), coverage.frames, "{} frame count", coverage.screen);
        }
    }
    assert!(total_frames >= 200, "resolved frames {total_frames}");
    eprintln!(
        "ui.fdf.frames screens={} frames={} required_controls={total_required}",
        wc3_game::fdf_ui::SCREEN_COUNT,
        total_frames
    );
    assert!(total_required >= 12, "required controls {total_required}");
}

#[test]
fn ui_fonts_shipped_frizqt_covers_latin_and_cyrillic() {
    let bytes = shipped_font_bytes();
    let theme = load_theme();
    let strings = wc3_game::fdf_ui::ui_strings(&theme);
    let coverage = wc3_game::fdf_ui::glyph_coverage(&bytes, &strings).expect("shipped font parses");
    assert!(
        coverage.strings_checked >= 100,
        "strings checked {}",
        coverage.strings_checked
    );
    assert!(
        coverage.cyrillic_present >= 90,
        "cyrillic glyphs {} / {}",
        coverage.cyrillic_present,
        coverage.cyrillic_total
    );
    assert!(
        coverage.latin_present >= 84,
        "latin glyphs {} / {}",
        coverage.latin_present,
        coverage.latin_total
    );
    assert!(
        coverage.missing.is_empty(),
        "UI strings use glyphs missing from the shipped font: {:?}",
        coverage.missing
    );
    eprintln!(
        "ui.fonts strings={} chars={} latin={}/{} cyrillic={}/{} missing=0",
        coverage.strings_checked,
        coverage.chars_checked,
        coverage.latin_present,
        coverage.latin_total,
        coverage.cyrillic_present,
        coverage.cyrillic_total
    );
}

#[test]
fn ui_text_labels_resolve_from_shipped_stringlist() {
    let theme = load_theme();
    let mut labels = 0usize;
    let mut resolved = 0usize;
    let mut literals = 0usize;
    let mut unresolved = Vec::new();
    for screen in wc3_game::fdf_ui::UiScreen::ALL {
        let spec = screen.spec();
        if spec.root.is_empty() {
            continue;
        }
        let coverage = wc3_game::fdf_ui::string_coverage(&theme, spec.root);
        labels += coverage.labels;
        resolved += coverage.resolved_keys;
        literals += coverage.literals;
        unresolved.extend(coverage.unresolved);
    }
    assert!(labels >= 10, "labels {labels}");
    assert_eq!(
        resolved + literals + unresolved.len(),
        labels,
        "every label is resolved, literal, or unresolved"
    );
    assert!(
        unresolved.is_empty(),
        "labels fell back to raw keys (no shipped string): {:?}",
        unresolved
    );
    let localization_entries = theme.localization().entries().count();
    eprintln!(
        "ui.text labels={labels} resolved_keys={resolved} literals={literals} unresolved={} stringlist_entries={localization_entries}",
        unresolved.len()
    );
    assert!(
        localization_entries >= 500,
        "shipped StringList entries {localization_entries}"
    );
}

#[test]
fn ui_layout_anchors_three_resolutions_without_overflow() {
    let theme = load_theme();
    let font = Font::load();
    let bindings = TextBindings::new();
    for (width, height) in [(1280.0_f32, 720.0_f32), (1920.0, 1080.0), (2560.0, 1440.0)] {
        let cfg = config(width, height);
        for screen in wc3_game::fdf_ui::UiScreen::ALL {
            let spec = screen.spec();
            if spec.root.is_empty() {
                continue;
            }
            let layout = wc3_game::fdf_ui::render(&theme, screen, &cfg, &bindings, Some(&font))
                .unwrap_or_else(|| panic!("{} at {width}x{height}", spec.root));
            let overflow = wc3_game::fdf_ui::overflow(&layout);
            assert!(
                overflow.is_empty(),
                "{} overflows at {width}x{height}: {:?}",
                spec.root,
                overflow
            );
        }
        // Numeric anchor scaling: the first ConsoleUI tile is 0.256 of width
        // and 0.032 of height at every resolution.
        let layout = wc3_game::fdf_ui::render(
            &theme,
            wc3_game::fdf_ui::UiScreen::Console,
            &cfg,
            &bindings,
            Some(&font),
        )
        .expect("ConsoleUI");
        let root = layout.named("ConsoleUI").expect("root");
        assert_eq!(root.rect.x, 0.0);
        assert_eq!(root.rect.y, 0.0);
        assert_eq!(root.rect.w, width);
        assert_eq!(root.rect.h, height);
        let first = layout.node(root.children[0]);
        assert!(
            (first.rect.w - 0.256 * width).abs() < 0.5,
            "tile width {} at {width}",
            first.rect.w
        );
        assert!(
            (first.rect.h - 0.032 * height).abs() < 0.5,
            "tile height {} at {height}",
            first.rect.h
        );
        // Nine-slice segments tile the rect exactly (no gaps/overlaps).
        let rect = Rect::new(0.0, 0.0, width, height);
        let segments = wc3_game::fdf_ui::nine_slice(rect, 24.0);
        let area: f32 = segments.iter().map(|segment| segment.w * segment.h).sum();
        assert!(
            (area - width * height).abs() < 0.5,
            "nine-slice area {area} vs {}",
            width * height
        );
        assert_eq!(segments.len(), 9);
        eprintln!("ui.layout {width}x{height} overflow=0 nine_slice_area_ok=true");
    }
}

//! Runtime HUD render matrix (ledger 553, polish addendum §1–§4).
//!
//! Everything below runs the **real** `MobileHud::draw` path: the composition
//! comes from `ui::widgets::hud::solve_hud`, the art is decoded from the
//! operator's extracted archive, and the `UiBatch` is rasterised by
//! `hudskin::soft_raster`. There is no PIL composite anywhere.
//!
//! Outputs (`analysis/agent-hud/553/`):
//!
//! * `sel-<state>-<w>x<h>.png` — the five selection states (§3.5) at the device
//!   viewport and in portrait;
//! * `sel-tooltip-2414x1080.png` — a hovered command with its tooltip;
//! * `res-<w>x<h>.png` — the resolution matrix (1280x720 … 1080x2400);
//! * `spec-<name>.json` — the layout dump `layout_audit.py` reads;
//! * `tokens.json` — the type table and the contrast table of the same tokens.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example hud_mock --offline
//! python3 ~/.config/opencode/skills/design-qa/scripts/layout_audit.py spec-*.json
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::Vec2;

use wc3_game::assets::TextureImage;
use wc3_game::hud::mobile_hud::{
    CommandButton, HudView, MenuButton, MenuPart, MenuScreen, MenuSkin, MenuView, MinimapDot,
    MinimapView, MobileHud, ResourcePill, SafeArea, SelectionView, ToastTone, ToastView,
};
use wc3_game::hud::{Font, UiBatch};
use wc3_game::hudskin::soft_raster;
use wc3_game::style::Chrome;
use wc3_game::ui::layout::text_fit;
use wc3_game::ui::theme::{TypeRole, Theme};
use wc3_game::ui::widgets::portrait;

/// Device viewport measured by the tech-lead (2414x1080 px at density 3).
const DEVICE: (f32, f32, f32) = (2414.0, 1080.0, 3.0);
/// Portrait phone (1080x2400 px at density 3).
const PORTRAIT: (f32, f32, f32) = (1080.0, 2400.0, 3.0);
/// Insets the device reports, in **dp** (the API takes dp: 210/120/90/96 px at
/// density 3). Passing pixels here would inset the HUD twice.
const DEVICE_INSETS: (f32, f32, f32, f32) = (70.0, 40.0, 30.0, 32.0);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn art_root() -> PathBuf {
    repo_root().join("analysis/war3_full3")
}

/// Decoded art, cached by archive-relative path.
struct Art {
    textures: Vec<TextureImage>,
    cache: HashMap<String, usize>,
}

impl Art {
    fn new() -> Self {
        // `Chrome::new` pushes the 1x1 white texel, so the list starts empty.
        Art {
            textures: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// Texture index for an archive path, or `None` when the install has no
    /// such file (never a stand-in asset).
    fn texture(&mut self, relative: &str) -> Option<usize> {
        let key = relative.to_ascii_lowercase();
        if let Some(index) = self.cache.get(&key) {
            return Some(*index);
        }
        let path = art_root().join(relative);
        let bytes = std::fs::read(&path).ok()?;
        let image = wc3_formats::blp::decode(&bytes).ok()?;
        let index = self.textures.len();
        // `blp::decode` returns its own image type; the mock uploads the same
        // tight RGBA8 layout as the game's `TextureImage`.
        self.textures.push(TextureImage {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
        self.cache.insert(key, index);
        Some(index)
    }

    /// First existing command-button icon from a candidate list.
    fn icon(&mut self, stems: &[&str]) -> Option<(usize, String)> {
        for stem in stems {
            let path = format!("ReplaceableTextures/CommandButtons/{stem}.blp");
            if let Some(index) = self.texture(&path) {
                return Some((index, (*stem).to_string()));
            }
        }
        None
    }
}

/// Which of the five selection states a mock shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sel {
    None,
    Unit,
    Hero,
    Building,
    Multi,
}

impl Sel {
    fn key(self) -> &'static str {
        match self {
            Sel::None => "none",
            Sel::Unit => "unit",
            Sel::Hero => "hero",
            Sel::Building => "building",
            Sel::Multi => "multi",
        }
    }
}

/// One selected object's data, resolved from real art and real labels.
struct Subject {
    def: &'static str,
    name: &'static str,
    class: &'static str,
    kind: &'static str,
    portrait_model: Option<String>,
    icon: Option<usize>,
    icon_name: Option<String>,
    portrait: Option<usize>,
    source: portrait::PortraitSource,
    hp: i32,
    max_hp: i32,
    mana: Option<(i32, i32)>,
    level: Option<u32>,
    armor: i32,
    damage: i32,
    production: Option<(&'static str, f32)>,
}

/// A retail roster row: label, def id, portrait model directory, icon stems.
struct Roster {
    def: &'static str,
    name: &'static str,
    class: &'static str,
    kind: &'static str,
    portrait_dir: &'static str,
    portrait_stem: &'static str,
    icons: &'static [&'static str],
}

const ROSTER: [Roster; 3] = [
    Roster {
        def: "opeo",
        name: "Пеон",
        class: "РАБОЧИЙ",
        kind: "ОРК",
        portrait_dir: "Units/Orc/Peon",
        portrait_stem: "peon_portrait.mdx",
        icons: &["BTNPeon"],
    },
    Roster {
        def: "Obla",
        name: "Вождь клана",
        class: "ГЕРОЙ",
        kind: "ОРК",
        portrait_dir: "Units/Orc/HeroBladeMaster",
        portrait_stem: "HeroBladeMaster_portrait.mdx",
        icons: &["BTNHeroBlademaster", "BTNChaosBlademaster"],
    },
    Roster {
        def: "ogre",
        name: "Великий чертог",
        class: "ЗДАНИЕ",
        kind: "ОРК",
        portrait_dir: "Buildings/Orc/GreatHall",
        portrait_stem: "GreatHall_portrait.mdx",
        icons: &["BTNGreatHall"],
    },
];

fn subject(art: &mut Art, row: &Roster) -> Subject {
    // Model portrait first (retail convention `<Model>_portrait.mdx`), then the
    // 2D icon, then a letter plate with a logged reason.
    let model_path = art_root().join(row.portrait_dir).join(row.portrait_stem);
    let model = if model_path.is_file() {
        Some(format!("{}/{}", row.portrait_dir, row.portrait_stem))
    } else {
        None
    };
    let labelled_icon = |art: &mut Art, stems: &[&'static str]| -> Option<(usize, String)> {
        art.icon(stems)
    };
    let found = labelled_icon(art, row.icons);
    let (icon, icon_name) = match found {
        Some((index, name)) => (Some(index), Some(name)),
        None => (None, None),
    };
    let source = portrait::resolve(row.def, model.as_deref(), icon_name.as_deref());
    Subject {
        def: row.def,
        name: row.name,
        class: row.class,
        kind: row.kind,
        portrait_model: model,
        icon,
        portrait: icon,
        icon_name,
        source,
        hp: 420,
        max_hp: 420,
        mana: None,
        level: None,
        armor: 2,
        damage: 12,
        production: None,
    }
}

/// A command slot of the mock card: label, hotkey, candidate icon stems.
struct Cmd {
    label: &'static str,
    hotkey: &'static str,
    icons: &'static [&'static str],
    enabled: bool,
    cooldown: Option<f32>,
    cost: Option<u32>,
    progress: f32,
}

const fn cmd(
    label: &'static str,
    hotkey: &'static str,
    icons: &'static [&'static str],
) -> Cmd {
    Cmd {
        label,
        hotkey,
        icons,
        enabled: true,
        cooldown: None,
        cost: None,
        progress: 0.0,
    }
}

const MOVE: Cmd = cmd("ДВИЖЕНИЕ", "M", &["BTNMove"]);
const STOP: Cmd = cmd("СТОП", "S", &["BTNStop"]);
const HOLD: Cmd = cmd("УДЕРЖАТЬ", "H", &["BTNHoldPosition"]);
const ATTACK: Cmd = cmd("АТАКА", "A", &["BTNAttack"]);
const PATROL: Cmd = cmd("ПАТРУЛЬ", "P", &["BTNPatrol"]);
const BUILD: Cmd = cmd("СТРОИТЬ", "B", &["BTNBasicStruct"]);
const PILLAGE: Cmd = cmd("ГРАБЁЖ", "G", &["BTNPillage"]);
const CRITICAL: Cmd = cmd("КРИТ. УДАР", "C", &["BTNCriticalStrike"]);
const MIRROR: Cmd = cmd("ЗЕРКАЛО", "R", &["BTNMirrorImage"]);
const MELEE_UP: Cmd = cmd("УЛУЧШ. ОРУЖИЕ", "W", &["BTNOrcMeleeUpOne"]);
const TRAIN_PEON: Cmd = Cmd {
    label: "ПЕОН",
    hotkey: "P",
    icons: &["BTNPeon"],
    enabled: true,
    cooldown: None,
    cost: Some(75),
    progress: 0.0,
};
const TRAIN_GRUNT: Cmd = Cmd {
    label: "ВОРГ",
    hotkey: "G",
    icons: &["BTNGrunt", "BTNPeon"],
    enabled: false,
    cooldown: None,
    cost: Some(200),
    progress: 0.0,
};
const QUEUE: Cmd = Cmd {
    label: "В ОЧЕРЕДИ",
    hotkey: "",
    icons: &["BTNPeon"],
    enabled: true,
    cooldown: None,
    cost: None,
    progress: 0.35,
};

struct Card {
    buttons: Vec<CommandButton<'static>>,
    icons: usize,
    slots: usize,
    missing: usize,
}

fn card(art: &mut Art, slots: &[Option<&Cmd>]) -> Card {
    let mut buttons = Vec::with_capacity(slots.len());
    let mut icons = 0usize;
    let mut present = 0usize;
    let mut missing = 0usize;
    for slot in slots {
        match slot {
            Some(command) => {
                present += 1;
                let art = art.icon(command.icons);
                let icon = art.as_ref().map(|(index, _)| *index);
                if icon.is_some() {
                    icons += 1;
                } else {
                    missing += 1;
                }
                let mut button = CommandButton::new(command.label, icon)
                    .short(command.label)
                    .hotkey(command.hotkey)
                    .sub(command.label)
                    .enabled(command.enabled)
                    .progress(command.progress);
                if let Some(cooldown) = command.cooldown {
                    button = button.cooldown(Some(cooldown));
                }
                if let Some(cost) = command.cost {
                    button = button.cost(Some(cost));
                }
                buttons.push(button);
            }
            None => {
                buttons.push(CommandButton::empty());
            }
        }
    }
    Card {
        buttons,
        icons,
        slots: present,
        missing,
    }
}

/// The three resource pills, alive for the whole run (the view borrows them).
fn resources() -> &'static [ResourcePill<'static>] {
    static PILLS: std::sync::OnceLock<[ResourcePill<'static>; 3]> = std::sync::OnceLock::new();
    PILLS.get_or_init(|| {
        [
            ResourcePill::new("1240", "GOLD", [0.95, 0.76, 0.30, 1.0]),
            ResourcePill::new("385", "LUMBER", [0.48, 0.85, 0.56, 1.0]),
            ResourcePill::new("17/40", "SUPPLY", [0.44, 0.71, 1.0, 1.0]),
        ]
    })
}

fn view<'a>(
    size: Vec2,
    density: f32,
    insets: (f32, f32, f32, f32),
    subject: Option<&'a Subject>,
    selection_count: usize,
    commands: &'a [CommandButton<'a>],
    minimap: Option<MinimapView<'a>>,
    tooltip: Option<String>,
    tooltip_costs: Vec<String>,
) -> HudView<'a> {
    let mut view = HudView::new(size)
        .density(density)
        .safe(SafeArea::new(insets.0, insets.1, insets.2, insets.3))
        .resources(resources())
        .commands(commands)
        .map_name("Орда 01", 61.0)
        .mode("ДВИЖЕНИЕ")
        .upkeep("БЕЗ СОДЕРЖАНИЯ")
        .selection_count(selection_count)
        .toast(Some(ToastView::new(
            "ГОРОДСКОЙ ЗАЛ ПОД АТАКОЙ",
            ToastTone::Danger,
        )))
        .tooltip(tooltip)
        .tooltip_costs(tooltip_costs);
    if let Some(minimap) = minimap {
        view = view.minimap(minimap);
    }
    if let Some(subject) = subject {
        let mut selection = SelectionView::new(subject.name, subject.hp, subject.max_hp)
            .class(subject.class)
            .kind(subject.kind)
            .stats(subject.armor, subject.damage)
            .def(subject.def)
            .portrait_source(subject.source.clone())
            .portrait(subject.portrait);
        if let Some(mana) = subject.mana {
            selection = selection.mana(mana.0, mana.1);
        }
        if let Some(level) = subject.level {
            selection = selection.level(level);
        }
        if let Some((name, progress)) = subject.production {
            selection = selection.production(Some(
                wc3_game::hud::mobile_hud::ProductionView {
                    name,
                    progress,
                    remaining: 12,
                },
            ));
        }
        view = view.selection(Some(selection));
    }
    view
}

fn minimap_dots() -> [MinimapDot; 4] {
    [
        MinimapDot::new(0.24, 0.32, 6.0, [0.44, 0.71, 1.0, 1.0]),
        MinimapDot::new(0.72, 0.66, 7.0, [1.0, 0.35, 0.37, 1.0]),
        MinimapDot::new(0.18, 0.22, 7.0, [0.95, 0.76, 0.30, 1.0]),
        MinimapDot::new(0.55, 0.44, 5.0, [0.36, 0.89, 0.54, 1.0]),
    ]
}

fn minimap_image() -> TextureImage {
    let (w, h) = (64u32, 64u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let ridge = ((x as f32 * 0.31).sin() * (y as f32 * 0.17).cos() * 0.5 + 0.5) * 0.35;
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[
                channel(0.18 + ridge),
                channel(0.22 + ridge * 0.7),
                channel(0.16 + ridge * 0.4),
                255,
            ]);
        }
    }
    TextureImage {
        width: w,
        height: h,
        rgba,
    }
}

/// One element of the audit dump.
struct Dump {
    id: String,
    rect: [f32; 4],
    kind: &'static str,
    parent: Option<String>,
    text: Option<(String, f32)>,
    min_target_dp: Option<f32>,
    full_bleed: bool,
    allow_overlap: bool,
}

#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Vec<Dump>,
    id: &str,
    rect: wc3_game::hud::Rect,
    kind: &'static str,
    parent: Option<&str>,
    min_target_dp: Option<f32>,
    full_bleed: bool,
    allow_overlap: bool,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    out.push(Dump {
        id: id.to_string(),
        rect: [rect.x, rect.y, rect.w, rect.h],
        kind,
        parent: parent.map(str::to_string),
        text: None,
        min_target_dp,
        full_bleed,
        allow_overlap,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_text(
    out: &mut Vec<Dump>,
    id: &str,
    rect: wc3_game::hud::Rect,
    parent: Option<&str>,
    text: String,
    font_px: f32,
) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    out.push(Dump {
        id: id.to_string(),
        rect: [rect.x, rect.y, rect.w, rect.h],
        kind: "label",
        parent: parent.map(str::to_string),
        text: Some((text, font_px)),
        min_target_dp: None,
        full_bleed: false,
        allow_overlap: false,
    });
}

fn write_spec(path: &Path, screen: Vec2, density: f32, insets_dp: (f32, f32, f32, f32), dumps: &[Dump]) {
    let mut json = String::with_capacity(8192);
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"screen\": {{\"w\": {:.0}, \"h\": {:.0}, \"density\": {:.2}}},\n",
        screen.x, screen.y, density
    ));
    json.push_str(&format!(
        "  \"safe_area\": {{\"left\": {:.2}, \"top\": {:.2}, \"right\": {:.2}, \"bottom\": {:.2}}},\n",
        insets_dp.0, insets_dp.1, insets_dp.2, insets_dp.3
    ));
    json.push_str("  \"elements\": [\n");
    for (index, dump) in dumps.iter().enumerate() {
        json.push_str("    {");
        json.push_str(&format!("\"id\": \"{}\", ", dump.id));
        json.push_str(&format!(
            "\"rect\": [{:.1}, {:.1}, {:.1}, {:.1}], ",
            dump.rect[0], dump.rect[1], dump.rect[2], dump.rect[3]
        ));
        json.push_str(&format!("\"kind\": \"{}\", ", dump.kind));
        match &dump.parent {
            Some(parent) => json.push_str(&format!("\"parent\": \"{parent}\", ")),
            None => json.push_str("\"parent\": null, "),
        }
        if let Some((text, font_px)) = &dump.text {
            json.push_str(&format!(
                "\"text\": {}, \"font_px\": {:.1}, ",
                json_string(text),
                font_px
            ));
        }
        if let Some(min) = dump.min_target_dp {
            json.push_str(&format!("\"min_target_dp\": {min:.1}, "));
        }
        json.push_str(&format!("\"full_bleed\": {}, ", dump.full_bleed));
        json.push_str(&format!("\"allow_overlap\": {}", dump.allow_overlap));
        json.push('}');
        if index + 1 < dumps.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n}\n");
    std::fs::write(path, json).expect("spec json");
    // Machine JSON must parse: a silent syntax error would invalidate the audit.
    let text = std::fs::read_to_string(path).expect("read spec");
    let mut depth = 0i32;
    for ch in text.chars() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            _ => {}
        }
    }
    assert_eq!(depth, 0, "spec {path:?} is not balanced");
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ellipsized(text: &str, size_dp: f32, width_dp: f32, density: f32) -> String {
    text_fit::ellipsize(text, size_dp * density, width_dp * density)
}

fn main() {
    let out_dir = repo_root().join("analysis/agent-hud/553");
    std::fs::create_dir_all(&out_dir).expect("analysis/agent-hud/553");

    let font = Font::load();
    let mut art = Art::new();
    let chrome = Chrome::new(&mut art.textures);
    let hud = MobileHud::new();
    let theme: Theme = hud.theme;
    let minimap_tex = minimap_image();
    let dots = minimap_dots();

    // Resolution matrix from the ledger + the addendum's two device sizes.
    let matrix: [(f32, f32, f32, &str); 9] = [
        (DEVICE.0, DEVICE.1, DEVICE.2, "device"),
        (PORTRAIT.0, PORTRAIT.1, PORTRAIT.2, "portrait"),
        (1280.0, 720.0, 1.0, "1280x720"),
        (1920.0, 1080.0, 1.0, "1920x1080"),
        (2560.0, 1080.0, 1.0, "2560x1080"),
        (1024.0, 768.0, 1.0, "1024x768"),
        (800.0, 600.0, 1.0, "800x600"),
        (2400.0, 1080.0, 3.0, "2400x1080"),
        (1080.0, 2400.0, 3.0, "1080x2400"),
    ];

    // ------------------------------------------------------------- subjects
    let mut subjects = Vec::new();
    for row in ROSTER.iter() {
        let mut subject = subject(&mut art, row);
        match row.def {
            "opeo" => {
                subject.hp = 240;
                subject.max_hp = 240;
                subject.armor = 0;
                subject.damage = 7;
            }
            "Obla" => {
                subject.hp = 620;
                subject.max_hp = 620;
                subject.mana = Some((180, 285));
                subject.level = Some(4);
                subject.armor = 4;
                subject.damage = 34;
            }
            _ => {
                subject.hp = 1500;
                subject.max_hp = 1500;
                subject.armor = 6;
                subject.damage = 0;
                subject.production = Some(("ПЕОН", 0.35));
            }
        }
        // The portrait diagnostic, once per def (addendum §1).
        if let Some(line) = portrait::log_once(subject.def, &subject.source) {
            println!("{line}");
        }
        subjects.push(subject);
    }

    // ------------------------------------------------------------- cards
    let mut cards: Vec<Card> = Vec::new();
    // Worker: build menu.
    cards.push(card(&mut art, &[
        Some(&BUILD), Some(&PILLAGE), Some(&MOVE), Some(&STOP),
        Some(&HOLD), Some(&ATTACK), Some(&PATROL), None,
        None, None, None, None,
    ]));
    // Hero: abilities + orders.
    cards.push(card(&mut art, &[
        Some(&MOVE), Some(&STOP), Some(&HOLD), Some(&ATTACK),
        Some(&PATROL), Some(&CRITICAL), Some(&MIRROR), None,
        None, None, None, None,
    ]));
    // Building: train + research queue.
    cards.push(card(&mut art, &[
        Some(&TRAIN_PEON), Some(&TRAIN_GRUNT), Some(&QUEUE), Some(&MELEE_UP),
        None, None, None, None, None, None, None, None,
    ]));
    for (index, line) in cards
        .iter()
        .map(|card| {
            format!(
                "HUD_CARD slots={} icons={} missing={} page=1/1",
                card.slots, card.icons, card.missing
            )
        })
        .enumerate()
    {
        println!("HUD_CARD def={} {line}", ROSTER[index].def);
    }

    let empty_commands: [CommandButton<'static>; 0] = [];
    let hedge_empty: [CommandButton<'static>; 12] = std::array::from_fn(|_| CommandButton::empty());
    let mut rendered = Vec::new();

    // ------------------------------------------- five selection states (device)
    for (index, state) in [Sel::None, Sel::Unit, Sel::Hero, Sel::Building, Sel::Multi]
        .into_iter()
        .enumerate()
    {
        let subject = match state {
            Sel::None => None,
            Sel::Unit => subjects.first(),
            Sel::Hero => subjects.get(1),
            Sel::Building => subjects.get(2),
            _ => subjects.first(),
        };
        let commands: &[CommandButton<'static>] = match state {
            Sel::None => &hedge_empty,
            Sel::Unit => &cards[0].buttons,
            Sel::Hero => &cards[1].buttons,
            Sel::Building => &cards[2].buttons,
            Sel::Multi => &cards[0].buttons,
        };
        let count = if state == Sel::Multi { 6 } else { usize::from(subject.is_some()) };
        let size = Vec2::new(DEVICE.0, DEVICE.1);
        let minimap = MinimapView {
            dots: &dots,
            camera: Some([0.22, 0.26, 0.30, 0.34]),
            title: "МИНИ-КАРТА",
            ..MinimapView::new("МИНИ-КАРТА")
        };
        let view = view(
            size,
            DEVICE.2,
            DEVICE_INSETS,
            subject,
            count,
            commands,
            Some(minimap),
            None,
            Vec::new(),
        );
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let rgba = soft_raster::rasterize(
            &batch,
            &art.textures,
            Some(&font),
            Some(&minimap_tex),
            size.x as u32,
            size.y as u32,
        );
        let name = format!("sel-{}-{}x{}", state.key(), size.x as u32, size.y as u32);
        let png = out_dir.join(format!("{name}.png"));
        soft_raster::write_png(&png, size.x as u32, size.y as u32, &rgba).expect("png");
        let spec = spec_for(
            &theme,
            &layout,
            size,
            DEVICE.2,
            DEVICE_INSETS,
            subject,
            count,
            state == Sel::Building,
        );
        let spec_path = out_dir.join(format!("spec-{name}.json"));
        write_spec(
            &spec_path,
            size,
            DEVICE.2,
            safe_dp(DEVICE_INSETS, DEVICE.2),
            &spec,
        );
        println!(
            "HUD_MOCK {} plan={} mode={} elements={} bytes={}",
            png.display(),
            layout.layout_mode.key(),
            layout.layout_mode.key(),
            spec.len(),
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0)
        );
        rendered.push((name, index));
    }

    // ------------------------------------------------------------- tooltip
    {
        let subject = subjects.get(1);
        let size = Vec2::new(DEVICE.0, DEVICE.1);
        let minimap = MinimapView {
            dots: &dots,
            camera: Some([0.22, 0.26, 0.30, 0.34]),
            title: "МИНИ-КАРТА",
            ..MinimapView::new("МИНИ-КАРТА")
        };
        let view = view(
            size,
            DEVICE.2,
            DEVICE_INSETS,
            subject,
            1,
            &cards[1].buttons,
            Some(minimap),
            Some("КРИТИЧЕСКИЙ УДАР".to_string()),
            vec!["Урон: 34".to_string(), "Перезарядка: 6 с".to_string()],
        );
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let rgba = soft_raster::rasterize(
            &batch,
            &art.textures,
            Some(&font),
            Some(&minimap_tex),
            size.x as u32,
            size.y as u32,
        );
        let png = out_dir.join("sel-tooltip-2414x1080.png");
        soft_raster::write_png(&png, size.x as u32, size.y as u32, &rgba).expect("png");
        let mut spec = spec_for(&theme, &layout, size, DEVICE.2, DEVICE_INSETS, subject, 1, false);
        // The tooltip is modal-ish: it may cover the console it describes.
        if !layout.tooltip.is_empty() {
            push(&mut spec, "tooltip", layout.tooltip, "panel", None, None, false, true);
            push_text(
                &mut spec,
                "tooltip_title",
                layout.tooltip.inset(8.0 * DEVICE.2, 8.0 * DEVICE.2),
                Some("tooltip"),
                ellipsized(
                    "КРИТИЧЕСКИЙ УДАР",
                    theme.px(TypeRole::Heading, 1.0) / DEVICE.2,
                    layout.tooltip.w - 16.0 * DEVICE.2 / DEVICE.2,
                    DEVICE.2,
                ),
                theme.px(TypeRole::Heading, DEVICE.2),
            );
        }
        let spec_path = out_dir.join("spec-sel-tooltip-2414x1080.json");
        write_spec(
            &spec_path,
            size,
            DEVICE.2,
            safe_dp(DEVICE_INSETS, DEVICE.2),
            &spec,
        );
        println!(
            "HUD_MOCK {} tooltip={:?} bytes={}",
            png.display(),
            layout.tooltip,
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0)
        );
    }

    // -------------------------------------------------- portrait device render
    {
        let subject = subjects.get(1);
        let size = Vec2::new(PORTRAIT.0, PORTRAIT.1);
        let minimap = MinimapView {
            dots: &dots,
            camera: Some([0.30, 0.30, 0.40, 0.24]),
            title: "МИНИ-КАРТА",
            ..MinimapView::new("МИНИ-КАРТА")
        };
        let view = view(
            size,
            PORTRAIT.2,
            (0.0, 0.0, 0.0, 0.0),
            subject,
            1,
            &cards[1].buttons,
            Some(minimap),
            None,
            Vec::new(),
        );
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let rgba = soft_raster::rasterize(
            &batch,
            &art.textures,
            Some(&font),
            Some(&minimap_tex),
            size.x as u32,
            size.y as u32,
        );
        let png = out_dir.join("sel-hero-1080x2400.png");
        soft_raster::write_png(&png, size.x as u32, size.y as u32, &rgba).expect("png");
        let spec = spec_for(&theme, &layout, size, PORTRAIT.2, (0.0, 0.0, 0.0, 0.0), subject, 1, false);
        let spec_path = out_dir.join("spec-sel-hero-1080x2400.json");
        write_spec(
            &spec_path,
            size,
            PORTRAIT.2,
            (0.0, 0.0, 0.0, 0.0),
            &spec,
        );
        println!(
            "HUD_MOCK {} mode={} bytes={}",
            png.display(),
            layout.layout_mode.key(),
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0)
        );
    }

    // --------------------------------------------------------- resolution sweep
    for (w, h, density, label) in matrix {
        let size = Vec2::new(w, h);
        // The device insets are dp already; a desktop window has none.
        let insets = if density >= 3.0 {
            DEVICE_INSETS
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        let subject = subjects.get(0);
        let minimap = MinimapView {
            dots: &dots,
            camera: Some([0.22, 0.26, 0.30, 0.34]),
            title: "МИНИ-КАРТА",
            ..MinimapView::new("МИНИ-КАРТА")
        };
        let view = view(size, density, insets, subject, 1, &cards[0].buttons, Some(minimap), None, Vec::new());
        let mut batch = UiBatch::default();
        let layout = hud.draw(&view, &mut batch, &font, &chrome);
        let rgba = soft_raster::rasterize(
            &batch,
            &art.textures,
            Some(&font),
            Some(&minimap_tex),
            size.x as u32,
            size.y as u32,
        );
        let name = format!("res-{label}");
        let png = out_dir.join(format!("{name}.png"));
        soft_raster::write_png(&png, size.x as u32, size.y as u32, &rgba).expect("png");
        let spec = spec_for(&theme, &layout, size, density, insets, subject, 1, false);
        let spec_path = out_dir.join(format!("spec-{name}.json"));
        write_spec(&spec_path, size, density, safe_dp(insets, density), &spec);
        println!(
            "HUD_MOCK {} plan={} mode={} bytes={}",
            png.display(),
            layout.layout_mode.key(),
            layout.layout_mode.key(),
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0)
        );
    }

    // ------------------------------------------------------------- tokens
    let mut tokens = String::from("{\n  \"density\": 3.0,\n  \"type\": [\n");
    for (element, role, dp, px) in theme.type_table(3.0) {
        tokens.push_str(&format!(
            "    {{\"element\": \"{element}\", \"token\": \"{}\", \"dp\": {dp:.1}, \"px\": {px:.1}}},\n",
            role.key()
        ));
    }
    tokens.push_str("  ],\n  \"contrast\": [\n");
    for row in theme.contrast_audit() {
        tokens.push_str(&format!(
            "    {{\"label\": \"{}\", \"ratio\": {:.2}, \"min\": {:.1}, \"pass\": {}}},\n",
            row.label, row.ratio, row.minimum, row.passes
        ));
    }
    tokens.push_str("  ]\n}\n");
    let tokens_path = out_dir.join("tokens.json");
    std::fs::write(&tokens_path, &tokens).expect("tokens.json");
    println!("TOKENS {}", tokens_path.display());
    for (element, role, dp, px) in theme.type_table(3.0) {
        println!("TYPE {element:<18} {:<8} {dp:>5.1} dp {px:>6.1} px", role.key());
    }
    for row in theme.contrast_audit() {
        println!(
            "CONTRAST {:<28} {:>5.2}:1 min {:.1} {}",
            row.label,
            row.ratio,
            row.minimum,
            if row.passes { "PASS" } else { "FAIL" }
        );
    }

    // ------------------------------------------------------- campaign page (521)
    if std::env::var("HUD_MOCK_MENU").is_ok() {
        let skin = menu_skin(&mut art);
        let mut menu =
            MenuView::for_screen(MenuScreen::Campaign, Vec2::new(DEVICE.0, DEVICE.1), "Орда 01")
                .tabs(MENU_TABS)
                .rows(MENU_ROWS)
                .nav(MENU_NAV)
                .page("HUMAN 1/2", 0.5)
                .redraw(Some(skin));
        menu.safe = Some(SafeArea::new(210.0, 120.0, 90.0, 96.0));
        menu.active_tab = 0;
        let mut batch = UiBatch::default();
        let layout = hud.draw_menu(&menu, &mut batch, &font, &chrome);
        let rgba = soft_raster::rasterize(
            &batch,
            &art.textures,
            Some(&font),
            None,
            DEVICE.0 as u32,
            DEVICE.1 as u32,
        );
        let png = out_dir.join("menu-campaign-2414x1080.png");
        soft_raster::write_png(&png, DEVICE.0 as u32, DEVICE.1 as u32, &rgba).expect("png");
        println!(
            "MENU_MOCK {} tabs={} rows={} bytes={}",
            png.display(),
            layout.tabs.len(),
            layout.visible_rows,
            std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0)
        );
    }

    let _ = empty_commands;
    let _ = chrome;
    println!("HUD_MOCK_DONE {} files", rendered.len());
}

fn safe_dp(insets: (f32, f32, f32, f32), _density: f32) -> (f32, f32, f32, f32) {
    insets
}

/// Build the audit dump from the solved dp layout (all rects converted to px).
fn spec_for(
    theme: &Theme,
    layout: &wc3_game::hud::mobile_hud::MobileHudLayout,
    screen: Vec2,
    density: f32,
    insets_px: (f32, f32, f32, f32),
    subject: Option<&Subject>,
    count: usize,
    building: bool,
) -> Vec<Dump> {
    // `layout` is already a `MobileHudLayout`: every rect is in framebuffer
    // pixels, converted exactly once by `to_px`. Scaling again here was the
    // 3x-toast bug the audit caught.
    let d = density;
    let _ = insets_px;
    let px = |r: wc3_game::hud::Rect| r;
    let mut out: Vec<Dump> = Vec::new();
    push(&mut out, "top_bar", px(layout.top_meta), "panel", None, None, true, false);
    push(&mut out, "resources", px(layout.resources), "panel", Some("top_bar"), None, false, false);
    for (index, pill) in layout.resource_pills.iter().enumerate() {
        push(
            &mut out,
            &format!("resource_pill{index}"),
            px(*pill),
            "panel",
            Some("resources"),
            None,
            false,
            false,
        );
    }
    if !layout.map_name.is_empty() {
        push_text(
            &mut out,
            "map_name",
            px(layout.map_name),
            Some("top_bar"),
            ellipsized("Орда 01", theme.px(TypeRole::Body, 1.0), layout.map_name.w, d),
            theme.px(TypeRole::Body, d),
        );
    }
    push(&mut out, "console", px(layout.console), "panel", None, None, true, false);
    push(&mut out, "viewport", px(layout.safe_frame), "panel", None, None, true, true);
    if !layout.minimap_frame.is_empty() {
        push(
            &mut out,
            "minimap",
            px(layout.minimap_frame),
            "target",
            Some("console"),
            Some(48.0),
            false,
            false,
        );
    }
    if !layout.minimap_rail.is_empty() {
        push(
            &mut out,
            "minimap_rail",
            px(layout.minimap_rail),
            "target",
            Some("console"),
            Some(48.0),
            false,
            false,
        );
    }
    if !layout.selection.is_empty() {
        push(
            &mut out,
            "selection",
            px(layout.selection),
            "panel",
            Some("console"),
            None,
            false,
            false,
        );
        push(
            &mut out,
            "portrait",
            px(layout.portrait),
            "icon",
            Some("selection"),
            None,
            false,
            false,
        );
    }
    if let Some(subject) = subject {
        let name_font = theme.px(TypeRole::Heading, d);
        let label_font = theme.px(TypeRole::Label, d);
        push_text(
            &mut out,
            "unit_name",
            px(layout.name),
            Some("selection"),
            ellipsized(subject.name, theme.px(TypeRole::Heading, 1.0), layout.name.w, d),
            name_font,
        );
        push_text(
            &mut out,
            "unit_class",
            px(layout.class),
            Some("selection"),
            ellipsized(subject.class, theme.px(TypeRole::Label, 1.0), layout.class.w, d),
            label_font,
        );
        if !layout.hp_bar.is_empty() {
            push(&mut out, "hp_bar", px(layout.hp_bar), "panel", Some("selection"), None, false, false);
        }
        if !layout.mana_bar.is_empty() {
            push(&mut out, "mana_bar", px(layout.mana_bar), "panel", Some("selection"), None, false, false);
        }
        if !layout.stats.is_empty() {
            push_text(
                &mut out,
                "unit_stats",
                px(layout.stats),
                Some("selection"),
                ellipsized(
                    &format!("БРОНЯ {}  УРОН {}", subject.armor, subject.damage),
                    theme.px(TypeRole::Label, 1.0),
                    layout.stats.w,
                    d,
                ),
                label_font,
            );
        }
        if let Some((prod, _)) = subject.production {
            if !layout.production_label.is_empty() {
                push_text(
                    &mut out,
                    "production",
                    px(layout.production_label),
                    Some("selection"),
                    ellipsized(prod, theme.px(TypeRole::Label, 1.0), layout.production_label.w, d),
                    label_font,
                );
            }
        }
    }
    for (index, slot) in layout.army_slots.iter().enumerate() {
        push(
            &mut out,
            &format!("army_slot{index}"),
            px(*slot),
            "button",
            Some("selection"),
            Some(48.0),
            false,
            false,
        );
    }
    if !layout.army_badge.is_empty() {
        push(&mut out, "army_badge", px(layout.army_badge), "label", Some("selection"), None, false, false);
    }
    if building {
        push(&mut out, "groups", px(layout.groups_area), "panel", Some("console"), None, false, false);
    }
    for (index, slot) in layout.group_slots.iter().enumerate() {
        push(
            &mut out,
            &format!("group_slot{index}"),
            px(*slot),
            "button",
            if building { Some("groups") } else { Some("console") },
            Some(48.0),
            false,
            false,
        );
    }
    for (name, rect) in [
        ("groups_button", layout.groups_button),
        ("commands_button", layout.commands_button),
    ] {
        if !rect.is_empty() {
            push(&mut out, name, px(rect), "button", Some("console"), Some(48.0), false, false);
        }
    }
    if !layout.card.is_empty() {
        push(&mut out, "card", px(layout.card), "panel", Some("console"), None, false, false);
    }
    for (index, slot) in layout.card_slots.iter().enumerate() {
        push(
            &mut out,
            &format!("card_slot{index}"),
            px(*slot),
            "button",
            Some("card"),
            Some(48.0),
            false,
            false,
        );
    }
    if !layout.toast.is_empty() {
        push(&mut out, "toast", px(layout.toast), "panel", None, None, false, true);
        push_text(
            &mut out,
            "toast_text",
            px(layout.toast.inset(8.0 * d, 0.0)),
            Some("toast"),
            ellipsized(
                "ГОРОДСКОЙ ЗАЛ ПОД АТАКОЙ",
                theme.px(TypeRole::Label, 1.0),
                layout.toast.w - 16.0,
                d,
            ),
            theme.px(TypeRole::Label, d),
        );
    }
    let _ = count;
    let _ = screen;
    out
}

const MENU_TABS: &[MenuButton<'static>] = &[
    MenuButton { label: "PROLOGUE", sub: "1", action: 30, enabled: true },
    MenuButton { label: "HUMAN", sub: "2", action: 31, enabled: true },
    MenuButton { label: "UNDEAD", sub: "3", action: 32, enabled: true },
    MenuButton { label: "ORC", sub: "4", action: 33, enabled: true },
    MenuButton { label: "NIGHT ELF", sub: "5", action: 34, enabled: true },
    MenuButton { label: "BONUS", sub: "6", action: 35, enabled: true },
];

const MENU_ROWS: &[MenuButton<'static>] = &[
    MenuButton { label: "EXODUS OF THE HORDE", sub: "CHAPTER 1", action: 40, enabled: true },
    MenuButton { label: "THE LONG MARCH", sub: "LOCKED", action: 41, enabled: false },
    MenuButton { label: "CRY OF THE WARS", sub: "CHAPTER 3", action: 42, enabled: true },
    MenuButton { label: "THE PRODIGAL", sub: "CHAPTER 4", action: 43, enabled: true },
    MenuButton { label: "LOST IN THE SHADOWS", sub: "LOCKED", action: 44, enabled: false },
    MenuButton { label: "THE FINAL STAND", sub: "LOCKED", action: 45, enabled: false },
];

const MENU_NAV: &[MenuButton<'static>] = &[
    MenuButton { label: "PREV", sub: "", action: 50, enabled: true },
    MenuButton { label: "NEXT", sub: "", action: 51, enabled: true },
    MenuButton { label: "START", sub: "", action: 0, enabled: true },
    MenuButton { label: "BACK", sub: "", action: 3, enabled: true },
];

fn menu_skin(art: &mut Art) -> MenuSkin {
    let part = |art: &mut Art, relative: &str| -> MenuPart {
        let index = art.texture(relative).unwrap_or(0);
        let (w, h) = art
            .textures
            .get(index)
            .map(|image| (image.width as f32, image.height as f32))
            .unwrap_or((1.0, 1.0));
        MenuPart::new(index, w, h, [12.0, 12.0, 12.0, 12.0])
    };
    let mut some = |art: &mut Art, relative: &str| Some(part(art, relative));
    MenuSkin {
        panel: some(art, "UI/Widgets/EscMenu/Human/human-options-menu-background.blp"),
        panel_border: some(art, "UI/Widgets/EscMenu/Human/human-options-menu-border.blp"),
        title_band: some(art, "UI/Widgets/Glues/GlueScreen-Button1-BackdropBorder.blp"),
        button_normal: some(art, "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground.blp"),
        button_hover: some(art, "UI/Widgets/Glues/GlueScreen-Button-KeyboardHighlight.blp"),
        button_pressed: some(art, "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Down.blp"),
        button_disabled: some(
            art,
            "UI/Widgets/Glues/GlueScreen-Button1-BackdropBackground-Disabled.blp",
        ),
        loading_frame: None,
        banner: None,
        fonts: None,
    }
}

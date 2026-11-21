# FDF/UI framework — integration notes for the game layer

Owner: `agent-ui-framework`. Scope: FDF parser (`crates/wc3-formats/src/fdf.rs`),
FDF/UI framework (`crates/wc3-game/src/ui.rs`), text/localization/fonts
(`crates/wc3-game/src/text.rs`), tests (`crates/wc3-game/tests/ui_fdf.rs`).

The modern phone HUD remains `style.rs` / `mobile_hud.rs`; this layer is the
data-driven FDF/legacy UI and never draws stone chrome on its own.

## 1. What exists now

```rust
// wc3-formats
pub struct FdfDocument { pub nodes: Vec<FdfNode>, pub includes: Vec<String>, pub includer: String }
impl FdfDocument {
    pub fn parse(text: &str) -> Result<Self, FormatError>;
    pub fn parse_named(text: &str, includer: impl Into<String>) -> Result<Self, FormatError>;
    pub fn find(&self, name: &str) -> Option<&FdfNode>;
    pub fn count(&self) -> (usize, usize);              // declarations, properties
    pub fn collect_strings(&self, out: &mut Vec<(String, String)>); // StringList
}
pub struct FdfNode {
    pub kind: String, pub name: Option<String>, pub inherits: Option<String>,
    pub inherit_children: bool, pub props: Vec<FdfProp>, pub children: Vec<FdfNode>,
}
impl FdfNode {
    pub fn prop(&self, key: &str) -> Option<&FdfProp>;
    pub fn props_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a FdfProp> + 'a;
    pub fn set_points(&self) -> Vec<FdfSetPoint>;       // repeated SetPoint preserved
    pub fn anchor(&self) -> Option<(String, f32, f32)>;
    pub fn has_set_all_points(&self) -> bool;
    pub fn font(&self) -> Option<FdfFontRef>;           // FrameFont, then Font
    pub fn font_color(&self) -> Option<[f32; 4]>;
    pub fn tex_coord(&self) -> Option<[f32; 4]>;        // left,right,top,bottom
    pub fn texture_uv(&self) -> Option<[f32; 4]>;       // u0,v0,u1,v1
    pub fn text(&self) -> Option<&str>;
    pub fn button_text(&self) -> Option<&str>;
    pub fn normal_text(&self) -> Option<(&str, &str)>;  // template, key
    pub fn walk(&self, visit: &mut impl FnMut(&FdfNode));
}
```

```rust
// wc3-game::ui
pub struct UiTheme { /* docs, templates, skin, strings, fonts, textures */ }
impl UiTheme {
    pub fn load(db: &mut AssetDb, textures: &mut Vec<TextureImage>) -> Self; // unchanged signature
    pub fn load_with<R: UiResourceResolver>(r: &mut R, textures: &mut Vec<TextureImage>) -> Self;
    pub fn from_documents(docs: Vec<FdfDocument>, skin: SkinTable) -> Self;
    pub fn load_document<R: UiResourceResolver>(&mut self, r: &mut R, path: &str, depth: usize) -> bool;
    pub fn set_race(&mut self, race: &str) -> bool;
    pub fn skin(&self) -> &SkinTable;
    pub fn localization(&self) -> &Localization;
    pub fn fonts(&self) -> &FontRegistry;
    pub fn template(&self, name: &str) -> Option<&FdfNode>;
    pub fn template_count(&self) -> usize;              // 310 retail
    pub fn resolve_tree(&self, root: &str) -> Option<ResolvedTree>;
    pub fn build_layout(&self, root: &str, cfg: &LayoutConfig,
                        bindings: &TextBindings, font: Option<&Font>) -> Option<UiLayout>;
    pub fn layout_tree(&self, tree: &ResolvedTree, root: FrameId,
                       cfg: &LayoutConfig, bindings: &TextBindings,
                       font: Option<&Font>) -> UiLayout;
    pub fn texture(&self, name: &str) -> Option<usize>; // Assets::textures index
    pub fn unit_icon(&self, kind: UnitKind) -> Option<usize>;      // used by game.rs
    pub fn command_icon(&self, action: &UiAction) -> Option<usize>; // used by game.rs
}

pub struct LayoutConfig { pub screen: Vec2, pub min_font_px: f32, pub measure_text: bool }
pub struct TextBindings; // frame-name -> dynamic string
pub struct UiLayout { pub screen: Vec2, pub nodes: Vec<LayoutNode>, pub roots: Vec<usize> }
impl UiLayout {
    pub fn find(&self, name: &str) -> Option<usize>;
    pub fn named(&self, name: &str) -> Option<&LayoutNode>;
    pub fn hit_test(&self, p: Vec2) -> Option<usize>;
    pub fn hit_test_interactive(&self, p: Vec2) -> Option<usize>;
    pub fn hit_test_all(&self, p: Vec2) -> Vec<usize>;
    pub fn texts(&self) -> impl Iterator<Item = (usize, &LayoutText)>;
    pub fn paints(&self) -> impl Iterator<Item = (usize, &LayoutPaint)>;
}
pub struct LayoutNode {
    pub frame: FrameId, pub parent: Option<usize>, pub children: Vec<usize>,
    pub kind: String, pub name: Option<String>, pub source: String,
    pub rect: Rect, pub norm: [f32; 4], pub visible: bool, pub interactive: bool,
    pub text: Option<LayoutText>, pub button: Option<ButtonLabels>,
    pub paint: Vec<LayoutPaint>, pub control_backdrop: Option<String>,
    pub layer: Option<String>, pub decorate: bool,
}
pub struct LayoutText { pub text: String, pub style: TextStyle }
pub struct LayoutPaint {
    pub kind: PaintKind, pub key: Option<String>, pub file: Option<String>,
    pub texture: Option<usize>, pub uv: [f32; 4], pub color: [f32; 4],
    pub blend: AlphaMode, pub layer: Option<String>, pub backdrop: Option<BackdropPaint>,
}
```

`UiState::bindings() -> TextBindings` maps gold/lumber/supply/selected/map/FPS
to `ResourceBarGoldText`, `ResourceBarLumberText`, `ResourceBarSupplyText`,
`ResourceBarUpkeepText`, `SimpleNameValue`, `NameValue`, `SimpleClassValue`,
`ClassValue`, `SimpleHoldNameValue`, `MapNameValue`, `ArmyValue`,
`EnemyValue`, `FpsValue`.

## 2. Screen integration (no runtime edit required)

```rust
// once per screen change (cache this, not per frame)
let theme: &UiTheme = &assets.ui;
let cfg = LayoutConfig::new(Vec2::new(width, height));
let bindings = ui_state.bindings();
let layout = theme.build_layout("ResourceBarFrame", &cfg, &bindings, Some(&font));

// touch: topmost interactive frame
if let Some(index) = layout.hit_test_interactive(touch) {
    let node = layout.node(index);
    // node.name / node.kind / node.control_backdrop drive the command action
}

// dynamic text placement
if let Some(gold) = layout.named("ResourceBarGoldText") {
    let shaped = TextShaper::new(&font).shape(
        &gold.text.as_ref().unwrap().text,
        &gold.text.as_ref().unwrap().style,
        cfg.screen,
        Some(gold.rect.w),
    );
    shaper.draw(&mut batch, &shaped, gold.rect, &gold.text.as_ref().unwrap().style);
}

// FDF paints (optional; modern chrome owner may skip)
for (_, paint) in layout.paints() {
    if let (Some(texture), true) = (paint.texture, paint.kind == PaintKind::Texture) {
        // node rect is layout.node(frame_index).rect
    }
}
```

Retail roots available from the default documents: `ConsoleUI`,
`ResourceBarFrame`, `UpperButtonBarFrame`, `SimpleInfoPanel`,
`SimpleInfoPanelUnitDetail`, `SimpleInfoPanelBuildingDetail`, `EscMenuMainPanel`,
`QuestDialog`, and the Glue screens once loaded with `load_document`.

## 3. Canonical `wc3-re` resource API contract

`wc3-re` is being produced by the dedicated RE agent. This layer does not add a
competing texture resolver: it depends on the two-method trait below and the
`AssetDb` adapter that exists today. The canonical API should provide (or be
adaptable with a 20-line shim to):

```rust
pub trait UiResourceResolver {
    /// Read a game-data text file by archive path (`UI\war3skins.txt`,
    /// `UI\FrameDef\...\*.fdf`). Must accept backslash paths, return UTF-8
    /// (retail FDF/StringLists are UTF-8; BOM tolerated).
    fn read_ui_text(&mut self, path: &str) -> Option<String>;

    /// Decode a BLP texture by archive path to RGBA8, top-down rows.
    /// Returns width/height/rgba exactly like `assets::TextureImage`.
    fn read_ui_texture(&mut self, path: &str) -> Option<TextureImage>;
}
```

Needed from `wc3-re` in the final manual:

1. **Path normalization**: `UI\war3skins.txt`, `UI\FrameDef\GlobalStrings.fdf`,
   `Fonts\FRIZQT__.TTF` (font files are not decoded by this layer; the skin
   table only stores their paths for a future localised font atlas).
2. **Text read**: FDF, war3skins, WTS (map strings) as UTF-8 (BOM allowed,
   CRLF allowed). `wc3-re` may own a cached `war3skins.txt`/`GlobalStrings.fdf`
   reader; if so, expose the parsed key/value pairs and the adapter calls
   `SkinTable::parse` / `Localization::insert` (no resolution rules live in
   `wc3-re`).
3. **Texture decode**: BLP1/BLP2 → RGBA8. Skin keys have **no extension**
   (`ConsoleTexture01 = UI\Console\Human\HumanUITile01`); `SkinTable` appends
   `.blp` and `wc3-re` receives the final path.
4. **Optional**: model paths (`BackgroundArt`, `MainMenu`) are exposed as
   strings only; this layer never decodes MDX.
5. **Team colour / replaceable textures**: not needed for FDF chrome; if the
   canonical API applies team tint, it must do so when uploading, not by
   rewriting the path returned to `UiTheme::texture`.

Do not duplicate `war3skins.txt` parsing or `StringList` resolution in
`wc3-re`; those semantics are covered by `SkinTable` and `Localization`
(126 `COLON_*`, 82 `KEY_*`, 874 StringList entries observed).

## 4. Layout conventions (must match this implementation)

* `Width`/`Height`, `SetPoint` offsets and anchor offsets are **fractions of
  the screen**, not of the parent. `Anchor TOPLEFT,x,y` is a `SetPoint` of the
  same point on the parent; FDF `+y` is **up**, so the pixel formula is
  `y = target_point_y - y_fraction * screen.h`.
* `SetPoint` is applied in property order into per-axis slots
  (`min`/`mid`/`max`); opposing min+max anchors stretch the frame and override
  `Width`/`Height`; `mid` centres; a later anchor on the same slot replaces it.
* `SetAllPoints` is two opposing points against the parent (or the named
  frame). A top-level frame with no geometry is the screen canvas.
* Templates: `INHERITS` copies properties (kind equality is not enforced —
  120 retail statements inherit across kinds). `INHERITS WITHCHILDREN` clones
  children; a local child with the same non-empty name merges over the clone.
* Dynamic text: bindings by frame name take precedence over the FDF `Text`
  value. `ButtonText` names a child text frame; `NormalText`/`DisabledText`
  carry (text-template, localization-key).

## 5. Evidence

* `analysis/agent-ui-framework/evidence/fdf-reference.json` — independent
  Python reference counts per file.
* `cargo test -p wc3-game --test ui_fdf` — 9 integration tests over all 70
  files plus ConsoleUI/ResourceBar/SimpleInfoPanel layout, MainMenu hit-test,
  skin/localization/font and text shaping.
* `cargo test -p wc3-game --lib ui` / `--lib ui::text` — 21 unit tests.

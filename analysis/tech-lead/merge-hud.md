# HUD/UX merge manifest — one base (v2)

State: **PARTIAL (HUD parity merged; device menu shot scheduled by dispatcher)** — 2026-11-13. `cargo check -p wc3-game --lib` rc 0; HUD tests 7/7; full lib 91 passed; mock screenshots `analysis/agent-hud/console-v5.png` (2414x1080 headless mock): exactly 9 ConsoleUI quads with FDF aspect/UV (top 5 + bottom 4, no invented centre quad), real `HumanUIPortraitMask` frame, 24 px name/class/HP/stats, hotkey+tooltip card, 288 console/216 minimap/128 portrait/4x3x64 card. Device window capture after `wm size` debugging returned black (`has_android_window=false` / no GAME_FIRST_FRAME); v4 remains the last device capture.

Authority: `analysis/tech-lead/BASE.md` v1 (published 2026-11-12), `analysis/tech-lead/MERGE_SOURCE_OF_TRUTH.md`. `WC3RE_MERGE_MATRIX.md` is still absent; BASE.md wins.

## 1. Canonical target per concern

| Concern | Canonical target | HUD consumption |
|---|---|---|
| MPQ/Storm read, archive priority | `wc3_re::storm::StormFs` (`open_wc3_install`, `read`, `info`, `search`) | via `wc3_game::re::ReLayer` / `Assets`; HUD never opens MPQ |
| Skin table + FDF decoration | `wc3_re::skin::SkinTable` (`parse`, `resolve`, `decorate`, `texture_entries`) | `ui::SkinTable` is a thin active-race wrapper; `parse` delegates bytes to canonical; no local `[Section]` parser |
| Command-button inventory | `wc3_re::icons::{audit, resolve_icon}` + `wc3_re::objectdata::ObjectDb::icon_paths` | `UiTheme` resolves BTN names through the retail `ReplaceableTextures\CommandButtons\<name>.blp` default and canonical BLP decode; object-data overrides are injected by `Assets` later |
| BLP1/BLP2 decode | `wc3-formats::blp::decode` (canonical primitive); `wc3_re::blp::probe` for encoding/alpha | `UiResourceResolver::read_ui_texture` / `AssetDb::texture` decode via canonical; HUD receives texture indices only |
| FDF documents/layout | `wc3-formats::fdf::{FdfDocument, FdfNode, FdfProp}` | `UiTheme` parses/keeps templates and skin keys; console rects are the explicit WC3 4×3 grid, not a second FDF renderer |
| Object/unit icon paths | `wc3_re::objectdata::ObjectDb` + `icons` | BTN name mapping only in HUD (`UiAction`→`BTNStop` etc.); path resolution is canonical |
| Multi-archive map UI | `wc3_re::mapfiles::MapBundle` | map name/minimap data provided by `Assets`; no map parsing in HUD |
| Text/localization | `wc3-formats::gamedata::{Wts,TextTable}` / `wc3_re::{wts,txt}` | FDF strings through `UiTheme::Localization`; no second text parser |
| Safe area | `hud::mobile_hud::SafeArea` | one type for HUD + input (`game::SafeInsets` is now `pub type SafeInsets = SafeArea`) |
| Renderer UI ABI | `hud::{Rect, UiBatch, UiVertex, Font, TextAlign}` | presentation primitives, no asset parsing |

## 2. OWNER files changed (this layer)

| File | Change | Key lines (current) |
|---|---|---|
| `crates/wc3-game/src/ui.rs` | local skin parser deleted; canonical `wc3_re::skin::SkinTable` wrapper; canonical console/resource texture preload; command icon path probing reduced to one retail path; FDF/skin/texture cache kept | `SkinTable` 139-260; `UiTheme::load_with` 312-370; `load_texture_key` 609-650; `command_icon_paths` 677-680; `load_command_icon` 682-722; `unit_icon` 724; `command_icon` 734 |
| `crates/wc3-game/src/style.rs` | procedural glass/rounded chrome deleted; `Tokens` is the WC3 dark-iron/gold palette; `Chrome` owns only a 1×1 flat texel + `flat_rect` | `Tokens` 18-102; `Chrome` 104-122 |
| `crates/wc3-game/src/hud.rs` | old floating `HudLayout` ring/chips/context geometry deleted; compatibility struct now carries only `console/resource_row/minimap/selection/command/button`; `build_buttons` builds action metadata only (rects assigned from card slots) | `HudLayout` 588-608; `Button` 610; `build_buttons` 618-690; `Rect` 13; `UiBatch` 129; `Font` 238 |
| `crates/wc3-game/src/mobile_hud.rs` | WC3 console grid v2: 288 px bar at viewport bottom, real `HumanUITile01..04` 9-slice, 216 minimap, 128 portrait + HP/mana/stats/production, resources inside console, 4×3 card at 64 px cells, 8 used + 4 empty, compact toast/hint, top strip map/FPS; hit test; no joystick/ring/chips | `HudView` 318; `HudHit` 430; `MobileHudLayout` 439; `layout` 582; `draw` 766; `draw_console_tiles` 979; `draw_button_chrome` 1028; `draw_selection` 1060; `draw_card_slot` 1191; card plate at 874 |
| `crates/wc3-game/src/game.rs` (layer-owned HUD/input portions) | `SafeInsets` aliased to `SafeArea`; `rebuild_layout` copies only `MobileHudLayout`; card-slot action mapping; selection/placement/marquee routing preserved; old `HudLayout` field copies/`offset_rect` deleted | `SafeInsets` 76; `rebuild_layout` 435; `card_button_indices` 466; `apply_mobile_button_rects` 476; `button_at` 923; `tap_action` 1107; `pick_unit` 1158; `box_select` 1202; `place_building` 1228; `build_hud` 1500 |

No changes to READ-ONLY `assets.rs`, `wc3-formats/src/fdf.rs`, `wc3-re/src/skin.rs`, `wc3-re/src/icons.rs`.

## 3. READ-ONLY patches

**None required for the HUD layer as written.** `Assets::load` already calls `UiTheme::load(db, &mut textures)`; `UiTheme::load_with` now loads the console art through canonical skin keys (`ConsoleTexture01..04`) and the retail fallback path, and preloads `CommandButtonBorder` + resource BLPs. `Assets` only needs to keep passing the resulting `textures`/`ui`/`chrome` handles and map name/minimap; no frozen file edit.

If the merge owner prefers `Assets` to inject canonical `IconAudit` paths instead of the retail default, the only read-only change is in `assets.rs` after `let ui = UiTheme::load(...)`: copy `ReLayer::icon_audit()` paths into `UiTheme` (patch to be authored by merge owner; HUD consumes indices either way).

## 4. Symbols ported (source → target)

| Symbol | Source (old/duplicate) | Target (canonical consuming) |
|---|---|---|
| war3skins grammar | `ui.rs` old `SkinTable::parse` (deleted; was lines 133-293) | `wc3_re::skin::SkinTable::parse`; wrapper `ui.rs:172-175` |
| skin key lookup / race fallback | old `get_section/get` | `wc3_re::skin::SkinTable::resolve`; wrapper `ui.rs:206-214` |
| FDF decorate resolution | old `resolve_texture_key` + local path append | `skin.resolve` + wrapper `resolve_texture` `ui.rs:219-226`; `UiTheme::resolve_texture_key` `ui.rs:588-607` |
| console textures | `UiTheme` explicit `HumanUITile01..04` probing | canonical skin keys `ConsoleTexture01..04` → `load_texture_key(..., true)` `ui.rs:330-351`; retail path fallback only |
| command icon paths | old 4-path search (`command_icon_paths`, deleted) | one canonical retail path `ui.rs:677-680`; canonical BLP decode through resolver |
| resource icons | old `GoldIcon/LumberIcon/SupplyIcon` keys | `ResourceGold/Lumber/Supply` preload `ui.rs:352-368`; `game.rs:1460-1463` uses those keys |
| UI raster/geometry | procedural glass `Chrome` (deleted) | real BLP indices + `Chrome::flat_rect` `style.rs:118-121` |
| console bar | old floating widgets | `MobileHudLayout.console` + `draw_console_tiles` `mobile_hud.rs:582,979` |
| command card geometry | old ring/context/chips `HudLayout` | `MobileHudLayout.card/card_slots` `mobile_hud.rs:439`; `game.rs:466-484` |
| safe area | `game::SafeInsets` local struct | `pub type SafeInsets = SafeArea` `game.rs:76`; `SafeArea` `mobile_hud.rs:23` |
| selection/placement routing | existing `tap_action/pick_unit/place_building` | kept; blockers are console/card/selection/minimap only `game.rs:923-972,1107,1228` |

## 5. Duplicates removed / thinned (delegation proof)

- **Skin parser:** no `[Section]`/`split_once('=')` code remains in `ui.rs`; `SkinTable::parse` is a one-line call to `wc3_re::skin::SkinTable::parse` (`ui.rs:174-176`). Wrapper only stores `active`, synthesises the canonical `[Main]` section (`main_skins` from `SkinTable::skins`) and delegates `get/get_in/resolve_texture/resolve_model/font_entries/len`.
- **Style chrome:** no `rounded_rect`, `rounded_border`, `circle_texture`, `ring_texture`, `terrain_detail_texture` left in `style.rs` (grep clean); only `Chrome { flat }` and `flat_rect`.
- **Layout engine:** no `HudLayout::button_rect/context_slot/primary/chips/context_row` left in `hud.rs` or `game.rs`; one geometry source is `MobileHudLayout`.
- **Command icon resolver:** single retail path; no four-path probe loop.
- **UiBatch painter order:** `textured_quad` now only extends the immediately previous draw (`hud.rs:145-175`) instead of searching backwards; this keeps real console tiles under the card plate/icons in both the GPU path and the headless raster.
- **Safe-area type:** one alias, no duplicated clamp logic outside `SafeArea::sanitised`.

## 6. Public API consumed by game/base (exact signatures)

```rust
// mobile_hud.rs
pub struct MobileHud { pub tokens: Tokens }
impl MobileHud {
    pub fn new() -> Self;
    pub fn with_tokens(tokens: Tokens) -> Self;
    pub fn layout(&self, view: &HudView<'_>) -> MobileHudLayout;
    pub fn draw(&self, view: &HudView<'_>, ui: &mut UiBatch, font: &Font, chrome: &Chrome) -> MobileHudLayout;
}
pub struct SafeArea { pub left: f32, pub top: f32, pub right: f32, pub bottom: f32 }
impl SafeArea { pub const fn new(...); pub fn scaled(self,f32)->Self; pub fn is_zero(self)->bool; pub fn sanitised(self,Vec2)->Self; }
pub struct HudView<'a> {
  pub viewport: Vec2, pub safe: Option<SafeArea>,
  pub resources: &'a [ResourcePill<'a>], pub selection: Option<SelectionView<'a>>,
  pub minimap: MinimapView<'a>, pub commands: &'a [CommandButton<'a>],
  pub phase: Option<PhaseView<'a>>, pub toast: Option<ToastView>, pub hint: Option<HintView>,
  pub map_name: &'a str, pub fps: f32, pub mode: &'a str, pub upkeep: &'a str,
  pub console_tiles: [Option<usize>;4], pub button_border: Option<usize>,
}
impl<'a> HudView<'a> { pub fn new(Vec2)->Self; pub fn safe/resources/selection/minimap/commands/phase/toast/hint/map_name/mode/upkeep/console_tiles/button_border(...)->Self; }
pub struct MobileHudLayout { pub console, minimap, minimap_header, selection, portrait, name, class, hp_bar, hp_text, mana_bar, mana_text, stats, production_label, production_bar, resources, upkeep, card, mode, top_meta, map_name, fps, toast, hint, phase: Rect; pub resource_pills: Vec<Rect>; pub card_slots: Vec<Rect>; pub cell: f32; pub scale: f32; pub safe: SafeArea; pub safe_frame: Rect; }
impl MobileHudLayout { pub fn zone_rects(&self)->Vec<(&'static str,Rect)>; pub fn interactive(&self)->Vec<(&'static str,Rect)>; pub fn hit(&self,Vec2)->Option<HudHit>; pub fn issues(&self,&Tokens)->Vec<String>; }
pub enum HudHit { Minimap, Card(usize), Selection, Console, Phase }

// hud.rs
pub struct Rect; impl Rect { pub fn new/contains/centre/right/bottom/inset/inset_sides/expanded/with_position/with_size/intersects/intersection_area/contains_rect/is_inside/clamped_inside }
pub struct UiBatch { pub white: Vec<UiVertex>, pub font: Vec<UiVertex>, pub minimap: Vec<UiVertex>, pub textured: Vec<UiDraw> }
pub struct Font; impl Font { pub fn load(); pub fn measure(&self,&str,f32)->f32; pub fn truncate(&self,&str,f32,f32)->String; pub fn text_box(...)->Rect; pub fn text/text_centred(...) }
pub struct HudLayout { pub scale:f32, pub top_bar,resource_row,minimap,selection,command:Rect, pub button:f32 }

// style.rs
pub struct Tokens; pub struct Chrome { pub flat: usize } impl Chrome { pub fn new(&mut Vec<TextureImage>)->Self; pub fn flat_rect(&self,&mut UiBatch,Rect,[f32;4]); }
```

## 7. Tests to add after freeze (names + fixtures; not run yet)

- `hud::mobile_hud::tests::console_layout_is_safe_and_clear_at_reference_resolutions` — fixtures `2414x1080`, `1080x2414`, `1280x720`, `SafeArea::{landscape_1080,portrait_1080}`; asserts no zone overlaps, console bottom edge, no safe-frame breach.
- `hud::mobile_hud::tests::labels_fit_inside_the_console` — real `Font::load()`, card labels/name measurements.
- `hud::mobile_hud::tests::hit_test_covers_minimap_card_and_selection`.
- `hud::mobile_hud::tests::headless_console_mock_writes_png` — outputs `analysis/agent-hud/mock-console-2414x1080.png` using real `analysis/war3_full3/UI/Console/Human/HumanUITile01..04.blp` + `human-activebutton.blp`.
- `hud::mobile_hud::tests::commandbuttons_catalog_validates_all_691_icons` — 691 BLPs from `ReplaceableTextures\CommandButtons`, 64×64, alpha range, atlas `analysis/agent-hud/icons/`.
- `game::tests::console_hud_rects_do_not_swallow_world_taps` — prints `HUD_RECTS`, asserts card/selection compact and `button_at` does not claim a world point 100 px above console.
- `game::tests::tap_selects_moves_and_places_a_building` — select Footman via `world_to_screen`, tap ground → `Command::Move`, select Peasant → `UiAction::Build(TownHall)` → tap ground → `Command::Build` and `placing=None`.
- `game::tests::command_card_tiles_have_a_minimum_finger_sized_hit_target` — card padding nearest-centre.
- `game::tests::safe_insets_keep_the_hud_inside_the_system_bars` — console/resource/minimap/card inside safe area.

Fixtures: real extracted install at `analysis/war3_full3`; no synthetic UI geometry except mock view structs.

## 7b. Exact rect table — 2414x1080, SafeArea(left=48, top=80, right=48, bottom=0)

| Element | x | y | w | h |
|---|---:|---:|---:|---:|
| console | 48 | 792 | 2318 | 288 |
| top strip | 48 | 80 | 2318 | 32 |
| top quad 01 | 48 | 80 | 593.4 | 34.56 |
| top quad 02-L | 641.4 | 80 | 201.7 | 34.56 |
| top quad 02-R | 1575.0 | 80 | 122.9 | 34.56 |
| top quad 03 | 1698.2 | 80 | 593.4 | 34.56 |
| top quad 04 | 2292.0 | 80 | 74.2 | 34.56 |
| bottom quad 01 | 48 | 889.9 | 593.4 | 190.1 |
| bottom quad 02 | 641.4 | 918 | 593.4 | 162 |
| bottom quad 03 | 1698.2 | 889.9 | 593.4 | 190.1 |
| bottom quad 04 | 2292.0 | 889.9 | 74.2 | 190.1 |
| minimap | 72 | 840 | 216 | 216 |
| minimap header | 72 | 796 | 216 | 20 |
| portrait | 312 | 832 | 128 | 128 |
| selection info panel | 308 | 794 | 392 | 176 |
| name (24 px) | 312 | 794 | 380 | 30 |
| class (24 px) | 312 | 824 | 380 | 30 |
| HP bar/text | 456 | 836 | 240 | 24 |
| mana bar/text | 456 | 864 | 240 | 16 |
| stats (24 px) | 456 | 886 | 240 | 24 |
| production label/bar | 456 | 912 / 940 | 240 | 24 / 6 |
| resources row | 1622 | 794 | 720 | 44 |
| resource pills (3) | 1622, 1866.7, 2111.3 | 794 | 236 | 44 |
| upkeep | 1622 | 839 | 720 | 12 |
| card | 2074 | 856 | 268 | 200 |
| centre info gap | 1234.8 | 889.9 | 463.6 | 190 (no quad; FDF gap) |
| empty hint (above console) | 72 | 762 | 2270 | 24 |
| card slots row1/2/3 | 2074,2142,2210,2278 | 856 / 924 / 992 | 64 | 64 |
| toast/hint | centered above console | 792-48 | ≤360 | 40 |
| top meta text | 60 / 2006 | 80 | 1274 / 360 | 32 |

All values are the live `MobileHudLayout` rects printed by `HUD_RECTS`/mock; hit-testing uses `console`, `minimap`, `selection`, `card_slots[12]` only.

## 8. Execution evidence (this cycle)

| Check | Command | Result |
|---|---|---|
| Scope compile | `cargo check -p wc3-game --lib` | **rc 0, 0 errors** (final 2026-11-13) |
| HUD layer tests | `cargo test -p wc3-game --lib -- --test-threads=1 hud::mobile_hud` | **rc 0, 6 passed** (layout, labels, hit test, contrast, headless mock, 691-icon catalog) |
| Routing tests | `cargo test -p wc3-game --lib -- --nocapture --test-threads=1 game::tests::console_hud_rects game::tests::tap_selects_moves game::tests::command_card_tiles game::tests::safe_insets` | **rc 0, 4 passed**; printed `HUD_RECTS console=(48,780,2318,300) card=(2074,808,268,200) selection=(312,786,384,168) minimap=(72,828,216,216)` |
| UI/theme tests | `cargo test -p wc3-game --lib -- ui::tests` | **rc 0, 6 passed** |
| FDF/HUD integration gate | `cargo test -p wc3-game --test ui_fdf -- --test-threads=1` | **rc 0, 10 passed** (Main section restored; StringList fixture updated to canonical 1748) |
| APK build | `bash scripts/build-apk.sh` (release lib first) | **rc 0**, `build/android-p0/herofolends-p0.apk` |
| Device screenshot v4 | `pm install -r` + `am start -W` + `screencap` | `analysis/agent-hud/console-v4.png` (2414x1080); game force-stopped after capture |
| Full lib suite | `cargo test -p wc3-game --lib -- --test-threads=1` | **rc 0, 91 passed, 18 ignored** (`analysis/agent-hud/full-lib-lumber.log`) |

HUD artifact: `analysis/agent-hud/mock-console-2414x1080.png` regenerated with real `HumanUITile01..04` + command-button chrome; device screenshot still intentionally pending.

## 8b. Final HUD diff (v3 → v4)

- Empty selection: removed the in-console `NO SELECTION`/two-line card; portrait slot now has a framed dark inset and one 20 px `TAP A UNIT OR DRAG A BOX` line above the console (`hint` at console.y-30).
- Command card: long/truncated words removed; 64 px cells now show icon + one large 24 px hotkey (`S/A/B/G/M/H/F/R/-/+`) from `CommandButton::hotkey`, fed by `UiAction`.
- Console centre: added a 10th quad using `HumanUITile02` UV `[0.25,0.75,0.3125,1.0]` in the FDF gap `0.512..0.712`, so minimap→card reads as one console.
- Resources: 24 px values retained; labels shortened to `GOLD/LUM/SUP`; band widened to 720 px above the card.
- Console height fixed at 288 px; minimap 216, portrait 128, card 4x3×64; no joystick/floating chips.
- Painter order remains call order (`hud.rs`), so console tiles stay under card/icon quads.

## 8c. QA `input.build` fix (device-verified)

- Card mapping worker: `game.rs:497` `card_button_indices` = `[0,1,2,3,4,5,8,9]`; slot 7 → button 9 BUILD. `apply_mobile_button_rects` (`game.rs:536`) assigns the same `MobileHudLayout.card_slots[7]` that is drawn.
- Selection now refreshes the card/touch snapshot immediately in `tap_action`/`box_select` (`game.rs:1113,1189`), not on the next simulation tick.
- `button_at`/`touch` log: `BUTTON BUILD` at `game.rs:963`; BUILD enable is `hud.rs:904` (`has_worker && can_afford(TownHall)`).
- Device values: safe area `210/120/90/96`; `buttons[9]=(BUILD, Build(TownHall), enabled=true, rect=2236,924,64,64)`; `card_slots[7]=(2236,924,64,64)`; hit centre `(2268,956)` → button 9. Log: `BUTTON BUILD`; then `PLACE_TAP 1200,400` → `PLACE_WORLD 4917,4085`; units 19→20, gold 1037→611, supply 17/36.
- Selection class: `0.50*30=15 px`, rect h20 (`mobile_hud.rs:1358-1362`); stats use `tokens.text` (`mobile_hud.rs:1405-1409`), contrast on dark panel >4.5.
- Screenshot: `analysis/agent-hud/build-fixed.png` (new TownHall visible); intermediate worker selection `analysis/agent-hud/build-fixed-army.png`. Observed logcat excerpt: `06:28:52.102 BUTTON BUILD`; `06:28:53.197 PLACE_TAP 1200,400`; `PLACE_WORLD 4917,4085`; next state `units=20 gold=611 supply=17/36`.

## 8d. Round 2 fidelity (v5)

- Background: exactly 9 `ConsoleUI.fdf` quads; bottom heights use `0.176H` (q1/3/4) and `0.150H` (q2), top `0.032H`; no synthetic 10th quad. FDF centre gap remains by design.
- Portrait: real `HumanUIPortraitMask.blp` frame (`PortraitFrame` alias in `ui.rs`), dark inner inset; empty state has only the 20 px hint above the console.
- Text: name/class/stats 24 px (`0.80*30`), HP 21.6 px, production 21 px, resource values 24 px, card hotkeys 24 px.
- Tooltips: pressed card command shows full `label · sub` in a 30 px tooltip above the console (`HudView.tooltip` → `MobileHudLayout.tooltip`).
- Artifact: `analysis/agent-hud/console-v5.png` is the **headless UiBatch mock** (not device); device WM produced a black window after size/rotation debugging. Last real device visual remains `console-v4.png`.

## 8e. PARITY UI (round 2, partial due game.rs handoff)

`PARITY_MATRIX.md` is still absent, so rows are not alignment-certified. HUD APIs are implemented in owner paths:

- Per-selection card: `hud::build_buttons` already builds by selection (worker build menu, hall train menu, army orders, empty navigation); ability slots 10/11 and tooltips are wired; `card_button_indices` preserves QA slot7 BUILD.
- Tooltips: pressed command → `HudView.tooltip` → 30 px tooltip above console; world long-press unit tooltip already in game.
- Portrait API: `HudView.portrait_frame` consumes real `HumanUIPortraitMask`; `portrait` rect remains the MDL camera target. Renderer camera-to-texture patch still needed for a live 3D portrait (render.rs read-only; see §9).
- Resources/upkeep: 24 px values + `GOLD/LUM/SUP` + `upkeep_label` in console.
- Main menu / map select / loading: `MenuScreen`, `MenuView`, `MenuLayout`, `MobileHud::{layout_menu,draw_menu}` with PLAY/MAP SELECT/QUIT, START/BACK, progress bar; headless mocks `analysis/agent-hud/mock-menu-{main,map,loading}.png`.
- `game.rs` integration is prepared in `analysis/tech-lead/patches/hud-parity-game.rs.patch` (Screen enum/fields/update/pointer/build_hud + `open_main_menu`); **not applied** because game.rs is temporarily owned by the touch agent. No device.

RC: `cargo check -p wc3-game --lib` 0 errors; HUD tests 7/7 rc 0 (adds menu mock test).

## 8f. PARITY_MATRIX §4 alignment (2026-11-13)

| Row | Current | HUD action |
|---|---|---|
| P1 Main menu | PARTIAL | `Screen::MainMenu` + `MenuView` PLAY/MAP SELECT/QUIT; host patch `patches/hud-parity-app.md` calls `game.open_main_menu()`; options/credits still missing |
| P2 Map select | PARTIAL | `MenuScreen::MapSelect` START/BACK, current map title; no 44-map list/preview (needs canonical `discover_maps` + selection UI) |
| P3 Loading | PARTIAL | `Screen::Loading` + 1.1 s progress animation; not real asset progress |
| P4 Console | FULL | 288 px, 9 FDF quads, 4×3×64 card; 64 px vs 48 dp advisory remains |
| P5 Minimap | PARTIAL | dots/camera/frame; no terrain-derived map/fog |
| P6 Portrait | PARTIAL | real `HumanUIPortraitMask` frame + icon; 3-D MDL camera request §9 |
| P7 Stats | PARTIAL+ | armor/damage/range now shown; attack/armor type + hero attrs still missing |
| P8 Card | PARTIAL+ | SLK-derived worker build menu (`sim_bridge::build_menu`) + typed `Trains=/Builds=` producer menu; BUILD hall pinned at slot 9; order follows profile list, not `button_pos` |
| P9 Tooltips | PARTIAL+ | world long-press + pressed-command tooltip now uses real `human-tooltip-background/border`; cost/icon layout incomplete |
| P10 Resources | FULL (HUD) | 24 px gold + non-zero `sim.lumber` + `used/cap` supply + upkeep label; lumber economy source is sim |
| P11 Upkeep | PARTIAL | label wired; RoC thresholds in sim |
| P12 Groups | PARTIAL | Ctrl+digit in touch; no on-screen group row |
| P13 Hotkeys | FULL | labels/hotkeys generated; `CommandStrings.txt` source not yet used |

## 8g. Sim TOP-3/5 HUD patches applied (lumber + SLK build menu)

- `analysis/agent-sim/patches/hud-lumber.patch` — applied: resources use `sim.lumber[player]` (`game.rs` resource block) instead of hardcoded `0`; supply remains `used/cap`.
- `analysis/agent-sim/patches/hud-build-menu.patch` — applied/rebased: `hud::build_buttons` now takes `sim_bridge::build_menu(&sim, player)` (farm/tower/producer/hall order) for worker slots 4/5/8; after the typed `menu_actions` rewrite it re-asserts SLK build entries and preserves `BUILD` TownHall at button 9/card slot 7 for the phone build flow. No hardcoded building id/price beyond the TownHall fallback.
- `analysis/agent-sim/patches/hud-farm-button.patch` — superseded by the SLK build menu above.
- Touch changes kept: `TouchEnd{index,double,hold}`, Hotkey/ControlGroups, long-press tooltip, static map titles.
- Gate: `cargo check -p wc3-game --lib` rc 0; HUD 8/8 (incl. BUILD slot7) rc 0; full lib log `analysis/agent-hud/full-lib-lumber.log`.

## 8h. PARITY #4/#6 game runtime applied (2026-11-13)

- Patch: `analysis/tech-lead/patch-game-parity46.diff` applied `-p0` to `crates/wc3-game/src/game.rs` (backup `/data/data/com.termux/files/usr/tmp/opencode/game.rs.pre-parity46`).
- `Engine`/`JassBridge` fields attached; `attach_map_runtime` called after scenario; `Game::update` delegates to `JassBridge::step_engine` when a map runtime exists.
- Gates: `cargo check -p wc3-game --lib` rc 0 (`analysis/agent-re/round4-check.log`); `map_units_spawn` 1 passed — 90 placements, `main_ok`, triggers > 0, `world.game_time` advances (`analysis/agent-re/round4-map-units-spawn.log`); full lib **95 passed / 18 ignored rc 0** (`analysis/agent-re/round4-game-lib.log`).
- Touch/HUD menu/lumber/build-menu changes preserved.

## 9. Renderer patch request — MDL portrait camera (world-owned; do not edit render.rs)

HUD already consumes a real frame + icon; 3-D portrait requires the GPU layer:

1. `wc3-formats::mdx`: expose `MdxModel.cameras: Vec<MdxCamera>` from the `CAMS` chunk (position, target, roll, FOV, near/far) — currently the parser only skips `CAMS` (`mdx.rs:596-603`).
2. `wc3-game::assets::ModelAsset`: add `portrait_camera: Option<usize>` resolved from `PortraitAnims.slk` (58 rows) or `CAMS[0]`, and `portrait_sequence: Option<usize>`.
3. `game.rs::Scene`: add `portrait: Option<PortraitDraw { model: usize, sequence: usize, world: Mat4, team_color: [f32;4] }>` for the first selected unit; keep `SelectionView` as fallback.
4. `render.rs::Renderer`: allocate one 256×256 `Rgba8UnormSrgb` portrait texture + depth; in `render_with` before the UI pass render that single model with the portrait camera/ortho projection; expose its bind group under a new `UiTexture::Portrait` batch.
5. `hud.rs`: add `UiTexture::Portrait` to `UiBatch`/`UiVertex` dispatch; `mobile_hud.rs` draws `layout.portrait` with it when `view.portrait_model.is_some()`, otherwise `HumanUIPortraitMask` + icon.
6. Acceptance: `qa.hud.portrait3d` screenshot shows the selected MDL head/torso animated in the 128 px slot; no UI layout change.

## 9. Conflicts / blockers

1. **`analysis/tech-lead/WC3RE_MERGE_MATRIX.md` missing** — manifest uses BASE.md only. Needs dispatcher publication before freeze.
2. **No HUD blocker remains.** The earlier `wc3-re` `decode_windows_1252` break is gone; current `cargo check`/tests are green. Remaining are dispatcher-owned full-suite/device gates.
3. **Device freeze** — no `am start/input/screencap`; final console-grid + selection/placement screenshots wait for dispatcher freeze.
4. **FDF card geometry**: card is the explicit WC3 4×3 grid from the operator spec; FDF is consumed for skin/texture/strings, not as a second layout renderer. If BASE requires FDF-derived cell positions, the merge owner must expose `ConsoleUI` resolved rects from `wc3-formats::fdf`; HUD will consume them unchanged.
5. **Icon override paths**: `UiTheme` currently uses the retail default path; object-data icon overrides need the `Assets`/`ReLayer` `IconAudit` injection noted in §3.

## 10. Next step

Wait for merge owner assignment after `BASE.md` v2/freeze. Then: run the listed tests, fix compile conflicts in merge-owned files via dispatcher, one device cycle (install → console screenshot `analysis/agent-hud/console-grid.png` → select Footman → move → Peasant → BUILD → place → `analysis/agent-hud/select-place/`), update this manifest to `DONE`.

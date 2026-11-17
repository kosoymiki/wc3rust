# HUD REDRAW v0.1 — INTEGRATION (ledger 347)

Status: **steps 1–2 implemented and gated; step 3 (device) not run — launch slot not granted.**
No `am start`/`monkey`/APK install was executed for this integration.

## 1. BLP1 override as the highest-priority asset source

`crates/wc3-game/src/assets.rs`:

| Item | Location |
|---|---|
| `hud_redraw_dir(root)` — candidate order: `H2_HUD_REDRAW_DIR`, `<install>/redraw-v0.1/blp`, `<install>/../redraw-v0.1/blp`, repo `analysis/agent-hud/redraw-v0.1/blp` | `assets.rs:358` |
| `hud_redraw_source(root, debug)` — `DirSource` layer + `hud_redraw=<dir>` debug marker | `assets.rs:381` |
| Storm branch layers redraw **first**: `sources = [redraw, StormSource]`, `AssetDb::open_sources(sources, "wc3-install+hud-redraw")` | `assets.rs:414-418` |
| MPQ branch layers redraw **first**: `sources = [redraw, MpqSource]`, `AssetDb::open_sources(sources, "mpq+hud-redraw")` | `assets.rs:433-439` |
| MPQ fallback retained when the pack is absent | `assets.rs:437` |

Pack BLP1 files (verified by test): `UI/Console/Human/HumanUITile01..04.blp` (256×288),
`UI/Widgets/Console/Human/CommandButton/human-multipleselection-border.blp` (64×64),
`UI/Console/Human/HumanUIPortraitMask.blp` (128×128), plus the added state/minimap/topbar/pill/
selection/tooltip/group BLPs (29 new files, `analysis/agent-hud/redraw-v0.1/blp/**`).

BLP1 exporter: `analysis/agent-hud/redraw-v0.1/source/make_blp1.py` (mirrors
`crates/wc3-formats/src/blp.rs::decode_blp1`: 156-byte header, BGRA palette, 8-bit alpha plane).
It does not overwrite the six pre-verified exports.

Tests (`assets.rs`):
- `assets::hud_redraw_tests::blp1_override_wins_and_decodes` — pack tile is 256×288 (wins over retail),
  border 64×64, portrait 128×128, all six button states 64×64, minimap/topbar/pill/selection/group
  BLPs decode.
- `assets::hud_redraw_tests::redraw_dir_discovery_prefers_install_root`.

## 2. Console 9-slice adaptation to the pack's 4 tiles (256×288, insets 32/48/32/48)

`crates/wc3-game/src/mobile_hud.rs`:

| Item | Location |
|---|---|
| `RedrawSkin` (console caps/mids, topbar, button[6], portrait[3], minimap frame/header, pills, selection panel, group buttons, tooltip) | `mobile_hud.rs:543` |
| `RedrawQuad` / `RedrawParts` | `mobile_hud.rs:578` / `:585` |
| `PortraitFrame::{Normal, Hero, Enemy}` | `mobile_hud.rs:591` |
| `build_redraw_parts(skin, console, top_bar, scale)` — caps at `256*scale`, mid span repeats the 192 px centre (`32/256` inset bands) round-robin between the two mid variants, full console height | `mobile_hud.rs:606` |
| `MobileHudLayout.redraw: Option<RedrawParts>` | `mobile_hud.rs:706` |
| Layout computes redraw parts only when the skin is present | `mobile_hud.rs:1026` |
| Draw: redraw console+topbar quads replace the retail 9 quads; retail path untouched when absent | `mobile_hud.rs:1128` |
| Minimap frame/header, selection panel, pill background, portrait-by-kind, card state textures | `mobile_hud.rs:1242`, `:1375`, `:1400`, `:1607`, `:1798` |
| `command_visual_state` (normal/pressed/disabled/cooldown/empty; autocast index available) | `mobile_hud.rs:1797` |

Wiring in `crates/wc3-game/src/game.rs`:
- `hud_redraw_skin()` requires `HumanUITile01` dimensions == 256×288 (pack active) and maps the
  `HudRedraw*` theme aliases; `game.rs:1186`.
- `.redraw(self.hud_redraw_skin())` on both HudView builds (`game.rs:762`, `game.rs:2503`).
- Selection portrait kind: enemy owner → `Enemy`, `unit.def().hero` → `Hero`, else `Normal`
  (`game.rs:2368`).
- Control-group row uses the pack active/inactive buttons when loaded (`game.rs:1198`).

## 3. Gates (exact commands, run per instruction — no full suite, no device)

| Command | rc | Result |
|---|---:|---|
| `cargo check -p wc3-game --lib --offline` | **0** | 0 errors, 46 warnings |
| `cargo test -p wc3-game --lib --offline hud::` | **0** | **19 passed, 0 failed** (`analysis/agent-hud/redraw-hud-tests.log`) |
| `cargo test -p wc3-game --lib --offline assets::hud_redraw_tests` | **0** | **2 passed** (`analysis/agent-hud/redraw-override-tests.log`) |

New tests: `redraw_console_parts_cover_the_console_seamlessly` (coverage/no-gap/UV bounds at
0.62–2.0×, cap width `256*scale`, right cap texture), `redraw_round_robin_mids_use_both_variants`,
`redraw_button_states_map_from_command_flags`.

## Not yet wired / next (not claimed)
- Device screenshot vs `previews/mock_hud_landscape.png`: **not run** (launch slot not granted).
- Pack deployment path when the slot is granted: copy `analysis/agent-hud/redraw-v0.1/blp` to
  `/data/data/org.herofolends.p0/files/redraw-v0.1/blp` (the discovery root), rebuild the APK.
- Autocast/cooldown *driving*: state textures exist and the index is available; the game currently
  only maps pressed/disabled/empty/progress.
- Tooltip pack panel alias is loaded but the tooltip draw still uses the retail background/border
  pair (phase 2 item).

## Increment 2 (phase 2 completion, no device)

- **Pack resource-pill icons** wired in `game.rs:2244`: `HudRedrawGold/Lumber/Food` first, retail
  `ResourceGold/Lumber/Supply` fallback (closure `resource_icon`).
- **Pack build/production progress strip**: `build/build_progress.png` → BLP1
  `UI/Console/Human/HumanBuildProgress.blp` (converter `make_blp1.py`), theme alias
  `HudRedrawProgress` (`ui.rs:508-590` alias loop), `RedrawSkin.progress`
  (`mobile_hud.rs:590`), drawn under the production fill (`mobile_hud.rs:1840`), skin wiring
  `game.rs:1210`.
- **All pack aliases** now load in a silent loop in `UiTheme::load_with` (`ui.rs`): topbar, pill/warn,
  gold/lumber/food(+warn), button normal/pressed/disabled/cooldown/autocast/empty, glow, cooldown
  radial, portrait normal/hero/enemy, minimap frame/alert/header, selection panel, group
  inactive/active, tooltip panel, progress. Missing files stay `None` → retail fallback.
- **Headless redraw mock**: `hud::mobile_hud::tests::headless_redraw_mock_writes_png` draws the full
  HUD with the pack skin and writes `analysis/agent-hud/mock-redraw-2414x1080.png` (2414×1080,
  9.9 MB) — the device-free comparison artifact against `previews/mock_hud_landscape.png`.

### Final scoped gates (increment 2)
| Command | rc | Result |
|---|---:|---|
| `cargo check -p wc3-game --lib --offline` | **0** | 0 errors, 46 warnings |
| `cargo test -p wc3-game --lib --offline hud::` | **0** | **20 passed, 0 failed** (`redraw-hud-tests2.log`) |
| `cargo test -p wc3-game --lib --offline assets::hud_redraw_tests` | **0** | **2 passed** |

### Remaining (device slot only)
- Device screenshot vs the pack preview: not run (launch forbidden until the tech-lead grants a slot).
  Deploy step when granted: copy `analysis/agent-hud/redraw-v0.1/blp` →
  `/data/data/org.herofolends.p0/files/redraw-v0.1/blp`, rebuild APK, capture `console-v6.png`.
- Tooltip pack panel is loaded but the tooltip draw still uses the retail background/border
  (the pack panel is a 384×256 9-slice while the current tooltip is a 30 px strip; phase 2 needs a
  tooltip layout pass).
- Autocast/cooldown *driving*: textures/index exist; game maps pressed/disabled/empty/progress only.

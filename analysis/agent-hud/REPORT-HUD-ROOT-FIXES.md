# HUD ROOT FIXES — card labels, 4 hero abilities, menu restyle, army icons (ledger 347)

Gate: `cargo test -p wc3-game --lib hud::` → **58 passed, 0 failed, rc 0**
(`/data/data/com.termux/files/usr/tmp/hud-root6.log`); `cargo check -p wc3-game --lib` → **rc 0**.
Device screenshot: **not produced** — `H2_DEVICE_SLOT` is unset.

## 1. Card labels — no 3-char codes, no mid-word cuts

**Root:** `draw_card_slot` drew `command.short` through one `text_box`; `short` came from the
game's abbreviation table (`GATH`, `ATK`) or a hotkey string (`KA3`), and `text_box` truncates with
`...` at the 24 px floor.

**Fix:**
- `crates/wc3-game/src/hud.rs:927` `card_label(short, label)` — accepts the shipped short name only
  when it is a real name: has letters, no digits/dots, and is not a truncated prefix of the label
  (`GATH`/`KA3`/`DP..` rejected, `BURROW` kept); otherwise the full label, uppercase (Cyrillic-safe).
- `crates/wc3-game/src/mobile_hud.rs:2365` `draw_card_label` — two lines, greedy word wrap measured
  with `Font::measure`; a word wider than the cell is drawn whole (slight overflow instead of a cut);
  shadow pass included. Call site `mobile_hud.rs:2562`.
- Test: `ui_command_card_labels_never_three_char_codes` — `GATH→GATHER`, `KA3→КАЗАРМЫ`,
  `DP..→DEEP WATER`, `BURROW→BURROW`, `FOOT→FOOTMAN`, `""→SPIRIT LODGE`.
- Headless: `analysis/agent-hud/card-fix/mock-peon-card.png` (FARM, GUARD TOWER, ARCANE SANCTUM,
  MORE 1/5, GATHER, BUILD…).

## 2. Hero card — 4 ability slots at 6/7/10/11

**Root:** the card only asked `ability_button(0..2)`, and the unconditional BASE slot overwrote the
third ability.

**Fix:** `hud.rs:1166` — for a hero, take all four shipped `def().ability_ids` (3 basic + ultimate),
place at slots 6/7/10/11, label from the object DB name, hotkey from `assets.ability_hotkey(id)`
(first-letter fallback), implemented spells become `Cast`, shipped-but-unsimulated ids become a
disabled data-only row (`DATA`); BASE/ZOOM only fill the row when no ability exists.
- Test: `ui_command_card_hero_has_four_ability_slots_with_hotkeys` (Hpal: 4 non-empty labels +
  hotkeys, ≥2 `Cast`, no BASE overwrite).
- Headless: `analysis/agent-hud/card-fix/mock-hero-card.png` (ATTACK/MOVE/STOP/HOLD/PATROL/ARMY +
  AHHB/AHDS/AHRE/AHAD in the four ability slots; device shows shipped names/hotkeys).

## 3. Menu/dialog restyle — pack 9-slice, title band, 44–56 px buttons

**Root:** the menu was a full-screen flat rectangle with tiny text.

**Fix:** `mobile_hud.rs:911` `MenuSkin` (panel, panel border, title band, button normal/hover/pressed/
disabled, loading frame) consumed by `draw_menu`:
- `mobile_hud.rs:1788` title band rect; `:1808` button height `(52*s).clamp(44,56)`.
- `mobile_hud.rs:1819` `draw_menu`: 9-slice panel + border, 9-slice title band, state buttons with
  9-slice art and gold borders, readable label/sub (0.80/0.46 scale, FDF font), hover sheen.
- `mobile_hud.rs:1937` loading chrome: shipped band art behind the map banner and a bordered
  progress bar (no flat strip).
- Pack art: `analysis/agent-hud/redraw-v0.1/blp/UI/Console/Human/HumanMenuPanel.blp`,
  `HumanMenuButton{Normal,Hover,Pressed,Disabled}.blp`, `HumanLoadingFrame.blp`,
  `HumanScorePanel.blp` (procedural reference v1 pack, converted with
  `analysis/agent-hud/redraw-v0.1/source/make_blp1.py`).
- Tests/artifacts: `ui_menu_restyled_headless_mock_writes_png` →
  `analysis/agent-hud/card-fix/mock-menu-restyled.png`; the merged FDF menu suite
  `ui_menus_shipped_art_mock_pngs` (8 menu screens + 3 dialogs) now passes and writes
  `analysis/agent-fdf/mocks/{menu_*,dialog_*}.png`.

## 4. Army row — never an empty icon frame

**Root:** the selection icon used only `unit_icon_name(kind)`; a miss drew a flat rect (no identity).

**Fix:** single-selection portrait fallback in `mobile_hud.rs:2217` — coloured square + the unit's
first letter when the texture is missing. Multi-unit row is `game.rs` (read-only): exact patch in
`analysis/agent-hud/patches/card-fix-game.md` §5 (`assets.icon_name_for_def(def_id)` first, then the
kind icon, then a coloured tile + first letter; never an empty frame).

## game.rs / ui.rs patches (read-only files)

`analysis/agent-hud/patches/card-fix-game.md`: identity `card_button_indices[0..11]`, `command()`
empty-slot + icon fallback, test slot 7→9, `LearnAbility`/`SetRally` actions, army-row icon fallback,
`MenuView` `MenuSkin` wiring, and the companion `ui.rs` alias list for the pack menu BLPs.

## Honest notes
- Redraw/menu pack = **procedural reference v1**, not final hand-drawn art.
- Device shot requires `H2_DEVICE_SLOT=1`; no launch attempted.
- Offline card mocks show ability ids as labels because object-data names/hotkeys load from the
  operator's install at runtime; the device card uses the shipped names/hotkeys.

# FULL GOAL — data-driven WC3 UI (ledger 347, HUD)

Status: **implemented + mechanic-tested (device-free).** `game.rs` callers are delivered as the exact
patch request `analysis/agent-hud/patches/ui-data-driven-game.md` (game.rs is READ_ONLY in this round).

Art honesty: `analysis/agent-hud/redraw-v0.1` remains **procedural reference v1**, not final hand-drawn art.

## Modules (new, declared in `hud.rs`, used by `hud::*` wrappers)

| Module | Mechanic | File |
|---|---|---|
| card | `ui.command-card` | `crates/wc3-game/src/ui/card.rs` |
| tooltips | `ui.tooltips` | `crates/wc3-game/src/ui/tooltips.rs` |
| minimap | `ui.minimap`, `fog.visibility` | `crates/wc3-game/src/ui/minimap.rs` |
| menu | `ui.menu`, `persistence.save` | `crates/wc3-game/src/ui/menu.rs` |
| resources | `economy.upkeep`, `economy.supply`, `ui.resources` | `crates/wc3-game/src/ui/resources.rs` |

Declared from `hud.rs` with `#[path = "ui/*.rs"]`; runtime wrappers in `hud.rs`
(`card_model`, `resource_view`, `minimap_blips`, `minimap_fog`, `unit_tooltip`, `menu_state`).

## Mechanic → test → rc

| Mechanic | Tests (all `ok`) |
|---|---|
| `ui.command-card` | `ui_command_card_always_twelve_slots_in_wc3_order`, `ui_command_card_button_pos_maps_to_row_major_slots`, `ui_command_card_menu_entries_use_shipped_hotkey_and_cost_labels`, `ui_command_card_abilities_use_buttonpos_and_next_free_slot`, `ui_command_card_worker_build_and_hero_ability_submenus` |
| `ui.tooltips` | `ui_tooltips_shipped_units_carry_name_hotkey_and_extended_text`, `ui_tooltips_shipped_abilities_and_items_have_data`, `ui_tooltips_sim_roster_covers_every_def`, `ui_tooltips_sim_spells_and_items_cover_the_tables`, `ui_tooltips_menu_entry_uses_shipped_cost_hotkey_and_text`, `ui_tooltips_shipped_text_is_not_replaced_by_a_generic_fallback` |
| `ui.minimap` | `ui_minimap_owner_colors_are_distinct`, `ui_minimap_blips_owner_colors_fog_and_creeps`, `ui_minimap_click_to_move_camera_maps_pixels_to_world`, `ui_minimap_camera_rect_is_normalized_and_clamped` |
| `fog.visibility` | `fog_visibility_mask_matches_sim_grid` |
| `economy.upkeep` | `economy_upkeep_ui_tiers_boundaries` |
| `economy.supply` | `economy_supply_ui_caps_by_ruleset` |
| `ui.resources` (no gap row yet) | `ui_resources_view_reads_sim_values`, `ui_resources_idle_worker_button_follows_orders`, `ui_resources_under_attack_alert_from_damage_events` |
| `ui.menu` (no gap row yet) | `ui_menu_single_player_flow_transitions`, `ui_menu_back_navigation_chain`, `ui_menu_pause_resume_options_flow`, `ui_menu_map_choice_uses_shipped_map_list` |
| `persistence.save` | `persistence_save_placeholder_is_explicit_without_sim_api` |

**Gate:** `cargo test -p wc3-game --lib --offline hud::` → **50 passed, 0 failed, rc 0**
(`/data/data/com.termux/files/usr/tmp/hud-full3.log`).
`cargo check -p wc3-game --lib --offline` → **rc 0**.

## Evidence per UI element

### 1. `ui.tooltips` (shipped data, no generic fallback)
- Shipped units with names **≥300**; sampled 64: every title non-empty, no `?`/`UNIT` generic title;
  hotkeys ≥10; shipped extended text (`Tip`/`Ubertip`) present for ≥8.
- Shipped abilities with names **≥100**; extended texts for ≥16 sampled.
- Shipped items with names **132** (RoC ItemData in `analysis/war3_full3`); titles data-driven.
- `wc3_data::MenuEntry` path: hotkey `F,F,F` → `F`; `160G 2F`; shipped tooltip text kept with
  colour escapes stripped; `REQUIRES` names preserved.
- Sim fallback coverage: all **99** roster defs (gold cost + HP stat), **11** spell defs
  (cooldown/mana), first **64** item defs (level).

### 2. `ui.minimap` + `fog.visibility`
- Owner colours: own = green, enemy = red, neutral creeps = yellow, ally = blue (exposed).
- Blips from `Sim`: own always; enemies/creeps only when `Sim::is_visible`; far enemy (7000,7000)
  is absent; creep blip carries `neutral=true`; all positions in `[0,1]`.
- Fog mask dimensions equal `sim.grid.width×height`; sampled cells agree with `Sim::is_visible`;
  visible ⇒ explored; `visible_fraction > 0` after one step.
- Click-to-move: minimap centre → world centre; min/max corners → `(0,0)`/`(world)`; outside clamps.
- Camera box normalised and clamped to `[0,1]`.

### 3. `ui.menu` + `persistence.save`
- Full single-player flow: Main → SinglePlayer → Race (Night Elf) → Opponent (Undead) →
  Map (`Ночь эльфов 05`, 4 players) → Difficulty (Hard) → Loading → InGame.
- Back chain: Difficulty→MapSelect→OpponentSelect→RaceSelect→SinglePlayer→Main.
- Pause→Options→Back→Paused→Resume→InGame; Pause→QuitToMenu.
- Map list is host-provided (3 shipped-style entries); empty list blocks Start with
  `"no map selected"`.
- Save/load: `SaveStatus::unavailable("Sim exposes no save/load API")`; `SaveSlot`/`LoadSlot` keep
  the page and set `"save unavailable: …"` / `"load unavailable: …"` — no fake success. A
  `SaveStatus::available(3)` state records `"saved slot 2"` once Sim exposes save.

### 4. `economy.upkeep` / `economy.supply` / `ui.resources`
- Upkeep tiers at 0/49/50/51/79/80/81/100 food → None/None/None/Low/Low/Low/High/High; labels
  `NO UPKEEP`/`LOW UPKEEP`/`HIGH UPKEEP`.
- Supply ceiling per ruleset: RoC 90, TFT 100 (pure function; `Sim::supply_cap` ≤ ceiling).
- `resource_view` reads real `Sim` banks/supply/upkeep; idle-worker count 1 for an idle peasant,
  0 after a move order; `idle_worker_action` returns `SELECT IDLE WORKER` only when >0.
- Damage events produce an `under_attack` position at the damaged unit; the attacker's owner never
  receives the defender position as its own alert.

### 5. Card integrity (`ui.command-card`)
- Exactly **12** slots always (`CARD_SLOTS`), row-major WC3 order.
- `Buttonpos` mapping: (0,0)=0, (3,0)=3, (0,1)=4, (2,2)=10, (3,2)=11; out-of-grid `(4,0)/(0,3)` = None.
- Menu entries use `hotkey_label()` (first level of `F,F,F`) and `cost_label()` (`160G 2F`, `80G`).
- Abilities use shipped `Buttonpos`; entries without a position take the first free slot.
- Worker build and hero ability pages are explicit submenus; root survives a submenu visit.

## What is not done (honest)
- `game.rs` wiring: READ_ONLY this round; the five exact caller edits are in
  `analysis/agent-hud/patches/ui-data-driven-game.md`.
- `ui.menu` and `ui.resources` have no `MECHANICS_GAP_MAP.md` rows yet; tests are named with those
  ids for the tech-lead to register (TEST_POLICY: row first). Nearest existing rows:
  `economy.upkeep`, `economy.supply`, `persistence.save` (covered).
- Save/load real round-trip is `persistence.save` MISSING in the gap map; the menu reports that
  explicitly instead of pretending.
- Fog **rendering** overlay in `mobile_hud` still draws dots/camera only; the mask model is ready
  (`hud::minimap_fog`) for the render patch.

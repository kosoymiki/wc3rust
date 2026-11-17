# Patch request — wire the data-driven UI models into `game.rs` (read-only for HUD)

`game.rs` is READ_ONLY in the full-goal round. The UI models live in
`crates/wc3-game/src/ui/{tooltips,minimap,menu,resources,card}.rs`, are declared from
`hud.rs`, and have HUD-layer wrappers in `hud.rs`:

```rust
hud::card_model(game) -> hud::card::CardModel              // ui.command-card
hud::resource_view(game) -> hud::resources::ResourceView   // economy.upkeep / supply / idle / alert
hud::minimap_blips(game) -> Vec<hud::minimap::Blip>        // ui.minimap
hud::minimap_fog(game) -> hud::minimap::Fog                // fog.visibility
hud::unit_tooltip(game, unit_id) -> Option<hud::tooltips::TooltipData> // ui.tooltips
hud::menu_state(maps) -> hud::menu::MenuState              // ui.menu / persistence.save
```

Apply these five callers (each is a small, independent edit).

## 1. `ui.tooltips` — extended per-object text on long-press

Anchor: `Game::draw`, where `tooltip_text`/`pressed_tooltip` is composed
(`let tooltip = self.tooltip_text.clone().or(pressed_tooltip);`).

```rust
let tooltip = self.tooltip_text.clone().or(pressed_tooltip).map(|text| {
    // Prefer the shipped per-object tooltip when the selection has data.
    if let Some(id) = self.selection.first() {
        if let Some(tip) = hud::unit_tooltip(self, *id) {
            let lines = tip.lines();
            if lines.len() > 1 {
                return lines.join("\n");
            }
        }
    }
    text
});
```

For a card entry, append `hud::tooltips::menu(entry).lines()` in
`hud::build_buttons`'s `sub` (the HUD model already exposes it; `game.rs` can also call it
via `self.assets.menu_entry(...)` if that API exists).

## 2. `ui.minimap` — blips from Sim (owner colours + fog + creeps) and click camera

Anchor 1: `Game::draw`, the dots loop (`for unit in self.sim.alive_units() { ... dots.push(MinimapDot::new(...)) }`).

```rust
let dots: Vec<MinimapDot> = hud::minimap_blips(self)
    .into_iter()
    .map(|blip| MinimapDot::new(blip.pos.x, blip.pos.y, blip.size, blip.color))
    .collect();
```

Anchor 2: minimap pointer handling (where a tap inside `self.layout.minimap` centres the camera).

```rust
if self.layout.minimap.contains(point) {
    let target = hud::minimap::camera_from_click(
        self.layout.minimap,
        point,
        self.assets.world_size,
    );
    self.camera.centre_on(target);
    // optional fog overlay: let hud::minimap_fog(self) feed MinimapView::fog
}
```

## 3. `ui.menu` — single-player flow + in-game menu

Anchor: `Game::open_main_menu` / `Game::activate_menu`.

```rust
// Discover maps from the shipped map list once (host already knows the paths).
let maps: Vec<hud::menu::MapChoice> = self
    .assets
    .shipped_maps()
    .into_iter()
    .map(|(name, path, players)| hud::menu::MapChoice { name, path, players })
    .collect();
self.menu_state = Some(hud::menu_state(maps));
```

`activate_menu(action)` should translate the menu action into
`hud::menu::MenuAction` and call `state.apply(action)`; on `MenuPage::InGame` resume the sim.
Save/load slots must call `state.apply(MenuAction::SaveSlot(n))` — the model reports
`save unavailable: Sim exposes no save/load API` until `Sim` exposes save/load
(`persistence.save` remains MISSING in the gap map).

## 4. `ui.resources` — sim-driven shelf, idle worker, alerts

Anchor: `Game::upkeep_label` and the resource pill builder.

```rust
pub fn upkeep_label(&self) -> &'static str {
    hud::resource_view(self).upkeep.label()
}
```

In the alert/toast path (`Game::update`, after `self.sim.step`):

```rust
let resources = hud::resource_view(self);
if !resources.under_attack.is_empty() {
    self.toast = Some(("YOUR BASE IS UNDER ATTACK".to_string(), 3.0));
}
```

Idle-worker button: when `resources.idle_workers > 0`, add a card entry
`hud::resources::idle_worker_action(count)` that selects `self.sim.idle_workers(self.player)`.

## 5. `ui.card` — 12 slots, WC3 order, hotkeys, submenus

Anchor: `Game::draw`, the `card_indices`/commands loop.

```rust
let model = hud::card_model(self);
let commands: Vec<CommandButton<'_>> = (0..12)
    .map(|slot| match model.entry(slot) {
        Some(entry) => {
            // map entry -> Button via the existing self.buttons lookup by label
            // (labels are unique per card page), then CommandButton::new(...)
            // with entry.hotkey / entry.cost / entry.enabled.
            command_for_entry(self, entry)
        }
        None => CommandButton::empty(),
    })
    .collect();
```

Worker build page: root entry with `submenu = Some(CardPage::WorkerBuild)` opens
`hud::card::from_menu(&build_entries, CardPage::WorkerBuild)`. Hero abilities page:
`hud::card::from_abilities(&ability_rows, CardPage::HeroAbilities)` using the shipped
`Buttonpos` (already in `wc3_data::abilities::AbilityView::button_pos`).

## Acceptance

`cargo test -p wc3-game --lib hud::` (mechanic-named tests):
- `hud::card::tests::ui_command_card_*`
- `hud::tooltips::tests::ui_tooltips_*`
- `hud::minimap::tests::ui_minimap_*`, `fog_visibility_mask_matches_sim_grid`
- `hud::menu::tests::ui_menu_*`, `persistence_save_placeholder_is_explicit_without_sim_api`
- `hud::resources::tests::ui_resources_*`, `economy_upkeep_ui_tiers_boundaries`,
  `economy_supply_ui_caps_by_ruleset`

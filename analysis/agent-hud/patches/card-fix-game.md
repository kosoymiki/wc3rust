# Patch — card fix callers in `game.rs` (read-only for HUD)

`hud.rs::build_buttons` now returns exactly 12 slot-indexed buttons (empty slots are
`Button::empty()`), with the worker page builds at 4/5/8, MORE at 7, ARMY at 6, BUILD at 9,
and the producer/hall training rows at 0..5. Apply the four edits below.

## 1. `card_button_indices` → identity (all 12 slots)

`crates/wc3-game/src/game.rs:1062`

```rust
    fn card_button_indices(&self) -> [usize; 12] {
        // build_buttons authors every slot exactly (row-major 4x3); no reordering.
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    }
```

`apply_mobile_button_rects` then assigns `mobile.card_slots[i]` to button `i`; the 8 legacy
slots keep their positions, MORE/ARMY/queue slots become visible.

## 2. `command()` — empty slot + icon fallback

`crates/wc3-game/src/game.rs:3092`

```rust
        let command = |index: usize| -> CommandButton<'_> {
            let Some(button) = self.buttons.get(index) else {
                return CommandButton::empty();
            };
            if button.label.is_empty() {
                return CommandButton::empty();
            }
            let pressed = self.touch.pressed_button() == Some(index);
            // Every real button gets an icon: object-data alias first, then the
            // action map, then the generic build banner for build/train rows.
            let icon = self.assets.ui.command_icon(&button.action).or_else(|| {
                matches!(
                    button.action,
                    UiAction::BuildDef(_) | UiAction::TrainDef(_) | UiAction::Build(_)
                )
                .then(|| self.assets.ui.texture("BTNHumanBuild"))
                .flatten()
            });
            CommandButton::new(&button.label, icon)
                // ... existing short/hotkey/sub/enabled/pressed chain
```

## 3. Update the layout test for identity slots

`crates/wc3-game/src/game.rs:4251` (`build_button_slot7_enters_placement`):

```rust
-        let slot = game.mobile_layout.as_ref().expect("layout").card_slots[7];
+        let slot = game.mobile_layout.as_ref().expect("layout").card_slots[9];
```

BUILD stays at button 9; with identity indices it is card slot 9 (row 3, col 2).

## 4. Hero LEARN + building RALLY (data exists, actions absent)

`UiAction` (`game.rs:49`) gains:

```rust
    LearnAbility(&'static str),
    SetRally,
```

Handler in `Game::act`:

```rust
            UiAction::LearnAbility(ability_id) => {
                if let Some(hero) = self.selection.iter().copied().find(|id| {
                    self.sim.unit(*id).map(|unit| unit.def().hero).unwrap_or(false)
                }) {
                    match self.sim.learn_ability(hero, ability_id) {
                        Ok(()) => self.toast(format!("LEARNED {ability_id}"), 1.2),
                        Err(reason) => self.toast(reason, 1.2),
                    }
                }
            }
            UiAction::SetRally => {
                self.rally_placing = true; // mirror of self.placing
            }
```

Ground tap in `Game::pointer_up` (next to the `placing` branch):

```rust
            if self.rally_placing {
                if let Some(building) = self.selected_hall().or_else(|| {
                    self.selection.iter().copied().find(|id| {
                        self.sim.unit(*id).map(|unit| unit.is_building()).unwrap_or(false)
                    })
                }) {
                    self.sim.set_rally(building, Some(world_point));
                    self.rally_placing = false;
                    return;
                }
            }
```

Then `hud.rs::build_buttons` can fill the hero LEARN slot (8) and the producer RALLY slot (6)
with these actions; until the patch lands those slots stay empty instead of pretending.

## Acceptance after patch

```bash
bash analysis/tech-lead/run_mechanic_tests.sh --check   # hud:: 53 tests rc0
cargo test -p wc3-game --lib hud::                      # 53 passed
```

Device (only with `H2_DEVICE_SLOT=1`): select a peasant → card shows GATHER/MOVE/STOP/HOLD +
3 buildings + MORE + ARMY + BUILD; capture to `analysis/agent-hud/card-fix/shot.png`.

## 5. Army selection row — icon_name_for_def + letter fallback (game.rs ~3210)

```rust
                let icon = self
                    .assets
                    .icon_name_for_def(unit.def().id)
                    .and_then(|name| self.assets.ui.texture(&name))
                    .or_else(|| {
                        self.assets
                            .unit_icon_name(unit.kind)
                            .and_then(|name| self.assets.ui.texture(&name))
                    });
                match icon {
                    Some(texture) => {
                        ui.textured_quad(texture, rect, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
                    }
                    None => {
                        // Never an empty frame: coloured tile + unit initial.
                        ui.rect(rect, [0.16, 0.22, 0.34, 0.95]);
                        let letter = unit
                            .def()
                            .name
                            .chars()
                            .find(|character| character.is_alphabetic())
                            .map(|character| character.to_string())
                            .unwrap_or_default();
                        self.font.text_box(
                            ui,
                            &letter,
                            rect,
                            0.9 * s,
                            [1.0; 4],
                            TextAlign::Centre,
                            None,
                        );
                    }
                }
```

## 6. MenuView pack skin (game.rs, both `MenuView::for_screen` call sites)

```rust
        let menu_skin = crate::hud::mobile_hud::MenuSkin {
            panel: self.assets.ui.texture("HudMenuPanel"),
            button_normal: self.assets.ui.texture("HudMenuButtonNormal"),
            button_pressed: self.assets.ui.texture("HudMenuButtonPressed"),
            button_disabled: self.assets.ui.texture("HudMenuButtonDisabled"),
            loading_frame: self.assets.ui.texture("HudLoadingFrame"),
        };
        let mut view = MenuView::for_screen(screen, self.viewport, &self.menu_map_name)
            .redraw(Some(menu_skin));
```

## Companion ui.rs aliases (menu pack, add to the HudRedraw loop)

```rust
            ("HudMenuPanel", "UI\\Console\\Human\\HumanMenuPanel.blp"),
            ("HudMenuButtonNormal", "UI\\Console\\Human\\HumanMenuButtonNormal.blp"),
            ("HudMenuButtonHover", "UI\\Console\\Human\\HumanMenuButtonHover.blp"),
            ("HudMenuButtonPressed", "UI\\Console\\Human\\HumanMenuButtonPressed.blp"),
            ("HudMenuButtonDisabled", "UI\\Console\\Human\\HumanMenuButtonDisabled.blp"),
            ("HudLoadingFrame", "UI\\Console\\Human\\HumanLoadingFrame.blp"),
            ("HudScorePanel", "UI\\Console\\Human\\HumanScorePanel.blp"),
```

//! The typed command card (hud.md §3.6).
//!
//! A slot is not a label and an icon: it is a real command with an identity, a
//! target type, a cost, a cooldown and a *reason* for being unavailable. The
//! interface may only draw a button when the game logic can execute it, and the
//! reason travels with the data instead of being guessed from a number.

/// Stable identity of a game command.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CommandId(pub u32);

/// What the command needs the player to pick after pressing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TargetType {
    /// Executes immediately.
    #[default]
    None,
    /// A world point (move, attack-ground, rally).
    Point,
    /// A unit or building.
    Unit,
    /// An area (blizzard, flame strike).
    Area,
    /// Only the caster itself (divine shield, stop).
    SelfOnly,
}

impl TargetType {
    pub fn key(self) -> &'static str {
        match self {
            TargetType::None => "none",
            TargetType::Point => "point",
            TargetType::Unit => "unit",
            TargetType::Area => "area",
            TargetType::SelfOnly => "self",
        }
    }
}

/// Why a command cannot run right now.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum DisabledReason {
    #[default]
    None,
    /// Not enough gold (the amount still missing).
    Gold(u32),
    Lumber(u32),
    /// Supply would exceed the cap.
    Food,
    /// Cooldown running (seconds left).
    Cooldown(f32),
    /// The prerequisite was never researched.
    NotResearched,
    /// Needs a target the current selection cannot provide.
    WrongTarget,
    /// Out of charges.
    NoCharges,
    /// The producer is busy.
    Busy,
    /// No room at the rally point / build site blocked.
    Blocked,
}

impl DisabledReason {
    pub fn key(self) -> &'static str {
        match self {
            DisabledReason::None => "none",
            DisabledReason::Gold(_) => "gold",
            DisabledReason::Lumber(_) => "lumber",
            DisabledReason::Food => "food",
            DisabledReason::Cooldown(_) => "cooldown",
            DisabledReason::NotResearched => "not-researched",
            DisabledReason::WrongTarget => "wrong-target",
            DisabledReason::NoCharges => "no-charges",
            DisabledReason::Busy => "busy",
            DisabledReason::Blocked => "blocked",
        }
    }

    /// Tooltip line for the reason.
    pub fn label(self) -> Option<String> {
        Some(match self {
            DisabledReason::None => return None,
            DisabledReason::Gold(missing) => format!("Требуется ещё {missing} золота"),
            DisabledReason::Lumber(missing) => format!("Требуется ещё {missing} дерева"),
            DisabledReason::Food => "Недостаточно пищи".to_string(),
            DisabledReason::Cooldown(left) => format!("Восстановление: {left:.1} с"),
            DisabledReason::NotResearched => "Требуется исследование".to_string(),
            DisabledReason::WrongTarget => "Неподходящая цель".to_string(),
            DisabledReason::NoCharges => "Заряды закончились".to_string(),
            DisabledReason::Busy => "Занят".to_string(),
            DisabledReason::Blocked => "Нет места".to_string(),
        })
    }
}

/// Cooldown progress (`remaining / total`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cooldown {
    pub remaining: f32,
    pub total: f32,
}

impl Cooldown {
    pub fn new(remaining: f32, total: f32) -> Self {
        Cooldown {
            remaining: remaining.max(0.0),
            total: total.max(0.001),
        }
    }

    /// Share of the slot still veiled (`1.0` = just started).
    pub fn fraction(&self) -> f32 {
        (self.remaining / self.total).clamp(0.0, 1.0)
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }
}

/// What one command costs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ResourceCost {
    pub gold: u32,
    pub lumber: u32,
    pub food: u32,
    pub mana: u32,
}

impl ResourceCost {
    pub const NONE: ResourceCost = ResourceCost {
        gold: 0,
        lumber: 0,
        food: 0,
        mana: 0,
    };

    pub const fn new(gold: u32, lumber: u32) -> Self {
        ResourceCost {
            gold,
            lumber,
            food: 0,
            mana: 0,
        }
    }

    pub fn is_free(&self) -> bool {
        self.gold == 0 && self.lumber == 0 && self.food == 0 && self.mana == 0
    }
}

/// The action a command maps to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandAction {
    Train(u16),
    Build(u16),
    Research(u16),
    Ability(u16),
    Move,
    Stop,
    Hold,
    Attack,
    AttackMove,
    Patrol,
    Cancel,
    Rally,
    UseItem(u8),
    Custom(u16),
}

impl CommandAction {
    pub fn key(self) -> &'static str {
        match self {
            CommandAction::Train(_) => "train",
            CommandAction::Build(_) => "build",
            CommandAction::Research(_) => "research",
            CommandAction::Ability(_) => "ability",
            CommandAction::Move => "move",
            CommandAction::Stop => "stop",
            CommandAction::Hold => "hold",
            CommandAction::Attack => "attack",
            CommandAction::AttackMove => "attack-move",
            CommandAction::Patrol => "patrol",
            CommandAction::Cancel => "cancel",
            CommandAction::Rally => "rally",
            CommandAction::UseItem(_) => "use-item",
            CommandAction::Custom(_) => "custom",
        }
    }
}

/// One command slot.
#[derive(Clone, PartialEq, Debug)]
pub struct CommandSlot {
    pub id: CommandId,
    /// Texture index of the command icon (`None` = no art in the install).
    pub icon: Option<usize>,
    pub label: &'static str,
    /// Hotkey letter shown on the badge.
    pub hotkey: char,
    pub enabled: bool,
    pub reason: DisabledReason,
    pub cooldown: Option<Cooldown>,
    pub cost: ResourceCost,
    pub target: TargetType,
    pub action: CommandAction,
    /// Hotkey badge visible (only when a real binding exists).
    pub show_hotkey: bool,
}

impl CommandSlot {
    pub fn new(id: CommandId, label: &'static str, action: CommandAction) -> Self {
        CommandSlot {
            id,
            icon: None,
            label,
            hotkey: '\0',
            enabled: true,
            reason: DisabledReason::None,
            cooldown: None,
            cost: ResourceCost::NONE,
            target: TargetType::None,
            action,
            show_hotkey: false,
        }
    }

    pub const fn with_icon(mut self, icon: Option<usize>) -> Self {
        self.icon = icon;
        self
    }

    pub const fn with_hotkey(mut self, hotkey: char) -> Self {
        self.hotkey = hotkey;
        self.show_hotkey = hotkey != '\0';
        self
    }

    pub const fn with_cost(mut self, cost: ResourceCost) -> Self {
        self.cost = cost;
        self
    }

    pub fn with_target(mut self, target: TargetType) -> Self {
        self.target = target;
        self
    }

    /// Mark the slot unavailable for a concrete game reason.
    pub fn disabled(mut self, reason: DisabledReason) -> Self {
        self.enabled = false;
        self.reason = reason;
        self
    }

    pub fn with_cooldown(mut self, cooldown: Cooldown) -> Self {
        self.cooldown = Some(cooldown);
        self
    }

    /// Is the command executable right now?
    pub fn is_live(&self) -> bool {
        self.enabled
            && self.reason == DisabledReason::None
            && self.cooldown.map(|c| !c.is_active()).unwrap_or(true)
    }

    /// Slot state for the painter (§3.6 normal/hover/pressed/selected/
    /// disabled/cooldown/unavailable/targeting).
    pub fn state(&self, hovered: bool, pressed: bool, targeting: bool) -> crate::ui::core::semantics::WidgetState {
        use crate::ui::core::semantics::WidgetState;
        if targeting {
            return WidgetState::Selected;
        }
        if pressed {
            return WidgetState::Pressed;
        }
        if !self.enabled || self.reason != DisabledReason::None {
            return WidgetState::Disabled;
        }
        if self.cooldown.map(|c| c.is_active()).unwrap_or(false) {
            return WidgetState::Loading;
        }
        if hovered {
            return WidgetState::Hover;
        }
        WidgetState::Normal
    }

    /// Has real art behind a real command.
    pub fn has_art(&self) -> bool {
        self.icon.is_some()
    }
}

/// A card page: the twelve slots the player sees.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CommandCard {
    pub slots: Vec<Option<CommandSlot>>,
    /// Current page and total pages (a hero with more than 12 abilities).
    pub page: u8,
    pub pages: u8,
}

impl CommandCard {
    /// A card with `count` empty slots (12 for the retail grid).
    pub fn with_slots(count: usize) -> Self {
        CommandCard {
            slots: vec![None; count],
            page: 1,
            pages: 1,
        }
    }

    pub fn place(&mut self, index: usize, slot: CommandSlot) {
        if let Some(cell) = self.slots.get_mut(index) {
            *cell = Some(slot);
        }
    }

    pub fn slot(&self, index: usize) -> Option<&CommandSlot> {
        self.slots.get(index).and_then(|slot| slot.as_ref())
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|slot| slot.is_none())
    }

    /// Slots holding a real command.
    pub fn commands(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Slots holding a command *and* its icon.
    pub fn icons(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.as_ref().map(|s| s.has_art()).unwrap_or(false))
            .count()
    }

    /// Slots with a command but no art: these are the plates the addendum
    /// forbids. For a retail roster this must be zero.
    pub fn missing_art(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.as_ref().map(|s| !s.has_art()).unwrap_or(false))
            .count()
    }

    /// The `HUD_CARD` diagnostic line.
    pub fn log_line(&self) -> String {
        format!(
            "HUD_CARD slots={} icons={} missing={} page={}/{}",
            self.commands(),
            self.icons(),
            self.missing_art(),
            self.page,
            self.pages
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> CommandCard {
        let mut card = CommandCard::with_slots(12);
        card.place(
            0,
            CommandSlot::new(CommandId(1), "TRAIN FOOTMAN", CommandAction::Train(1))
                .with_icon(Some(7))
                .with_hotkey('F')
                .with_cost(ResourceCost::new(135, 0)),
        );
        card.place(
            1,
            CommandSlot::new(CommandId(2), "HOLY LIGHT", CommandAction::Ability(2))
                .with_icon(Some(9))
                .disabled(DisabledReason::Cooldown(3.0))
                .with_cooldown(Cooldown::new(3.0, 12.0)),
        );
        card
    }

    #[test]
    fn a_slot_reports_its_real_state() {
        let card = card();
        let train = card.slot(0).unwrap();
        assert!(train.is_live());
        assert_eq!(
            train.state(false, false, false),
            crate::ui::core::semantics::WidgetState::Normal
        );
        let holy = card.slot(1).unwrap();
        assert!(!holy.is_live());
        assert_eq!(
            holy.state(false, false, false),
            crate::ui::core::semantics::WidgetState::Disabled
        );
    }

    #[test]
    fn targeting_and_hover_are_distinct_states() {
        let slot = CommandSlot::new(CommandId(3), "MOVE", CommandAction::Move);
        assert_eq!(
            slot.state(true, false, false),
            crate::ui::core::semantics::WidgetState::Hover
        );
        assert_eq!(
            slot.state(false, true, false),
            crate::ui::core::semantics::WidgetState::Pressed
        );
        assert_eq!(
            slot.state(false, false, true),
            crate::ui::core::semantics::WidgetState::Selected
        );
    }

    #[test]
    fn a_command_without_art_is_counted_as_missing() {
        let mut card = card();
        assert_eq!(card.missing_art(), 0);
        card.place(2, CommandSlot::new(CommandId(4), "STOP", CommandAction::Stop));
        assert_eq!(card.missing_art(), 1);
        assert!(card.log_line().contains("missing=1"));
    }

    #[test]
    fn reasons_carry_a_human_label() {
        assert!(DisabledReason::None.label().is_none());
        assert!(DisabledReason::Gold(40).label().unwrap().contains("40"));
        assert!(DisabledReason::Cooldown(1.5).label().unwrap().contains("1.5"));
        assert_eq!(DisabledReason::Food.key(), "food");
    }

    #[test]
    fn costs_and_cooldowns_are_data() {
        let cost = ResourceCost::new(60, 15);
        assert!(!cost.is_free());
        assert!(ResourceCost::NONE.is_free());
        let cd = Cooldown::new(3.0, 12.0);
        assert!(cd.is_active());
        assert!((cd.fraction() - 0.25).abs() < 0.01);
    }
}

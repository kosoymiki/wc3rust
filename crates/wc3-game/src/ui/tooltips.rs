//! Per-object tooltips (mechanic `ui.tooltips`).
//!
//! Everything a tooltip shows comes from the shipped data when it exists:
//! unit/ability/item names, hotkeys, costs, and the extended `Tip`/`Ubertip`
//! text (`wc3_data` view methods). The sim tables are the offline fallback for
//! numbers, and a builder from a `MenuEntry` covers the command-card tooltip.
//! No generic "UNIT"/"?" line is emitted for an object that has shipped data.

use wc3_data::abilities::AbilityView;
use wc3_data::items::ItemView;
use wc3_data::menu::MenuEntry;
use wc3_data::units::UnitView;
use wc3_sim::abilities::SpellDef;
use wc3_sim::data::{ItemDef, UnitDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooltipSource {
    Unit,
    Ability,
    Item,
    Menu,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TooltipData {
    pub source: TooltipSource,
    pub title: String,
    pub subtitle: String,
    pub hotkey: Option<String>,
    pub stats: Vec<(String, String)>,
    pub costs: Vec<(String, i32)>,
    /// Extended text lines from the shipped `Ubertip`/`Tip` (colour codes removed).
    pub body: Vec<String>,
    pub requires: Vec<String>,
}

impl TooltipData {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
    }

    /// Flat render order: title, subtitle, stats, costs, body, requires.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![self.title.clone(), self.subtitle.clone()];
        for (label, value) in &self.stats {
            out.push(format!("{label}: {value}"));
        }
        if !self.costs.is_empty() {
            let costs: Vec<String> = self
                .costs
                .iter()
                .map(|(label, value)| format!("{value} {label}"))
                .collect();
            out.push(costs.join("  "));
        }
        out.extend(self.body.iter().cloned());
        if !self.requires.is_empty() {
            out.push(format!("REQUIRES: {}", self.requires.join(", ")));
        }
        out.retain(|line| !line.trim().is_empty());
        out
    }
}

fn clean_text(text: &str) -> Option<String> {
    let stripped = wc3_formats::fdf::strip_escape_codes(text);
    let trimmed = stripped.replace(['\r', '\n'], " ");
    let trimmed = trimmed.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed == "_" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn body_lines(text: Option<String>) -> Vec<String> {
    let Some(text) = text.and_then(|text| clean_text(&text)) else {
        return Vec::new();
    };
    text.split("  ")
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn unit_subtitle(def: Option<&UnitDef>) -> String {
    let Some(def) = def else {
        return "UNIT".to_string();
    };
    let kin = if def.hero {
        "HERO"
    } else if def.is_building() {
        "BUILDING"
    } else if def.stats.is_worker {
        "WORKER"
    } else {
        "UNIT"
    };
    format!("{} {}", def.race.name(), kin)
}

fn unit_stats(def: &UnitDef) -> Vec<(String, String)> {
    vec![
        ("HP".to_string(), def.stats.hp.to_string()),
        ("ARMOR".to_string(), def.stats.armor.to_string()),
        ("DAMAGE".to_string(), def.stats.damage.to_string()),
        ("RANGE".to_string(), format!("{:.0}", def.stats.range)),
        ("SPEED".to_string(), format!("{:.0}", def.stats.speed)),
    ]
}

/// Tooltip for a shipped unit row plus the sim numbers (when loaded).
pub fn unit(view: &UnitView<'_>, def: Option<&UnitDef>) -> TooltipData {
    let title = view
        .name()
        .or_else(|| def.map(|def| def.name.to_string()))
        .unwrap_or_default();
    let body = body_lines(view.ubertip().or_else(|| view.tip()));
    let mut costs = Vec::new();
    if let Some(def) = def {
        if def.stats.cost > 0 {
            costs.push(("GOLD".to_string(), def.stats.cost));
        }
        if def.stats.lumber_cost > 0 {
            costs.push(("LUMBER".to_string(), def.stats.lumber_cost));
        }
        if def.stats.supply > 0 && !def.is_building() {
            costs.push(("FOOD".to_string(), def.stats.supply));
        }
    }
    TooltipData {
        source: TooltipSource::Unit,
        title,
        subtitle: unit_subtitle(def),
        hotkey: view.hotkey().and_then(|key| clean_text(&key)),
        stats: def.map(unit_stats).unwrap_or_default(),
        costs,
        body,
        requires: Vec::new(),
    }
}

/// Tooltip for a shipped ability row plus the sim spell numbers.
pub fn ability(view: &AbilityView<'_>, def: Option<&SpellDef>) -> TooltipData {
    let title = view
        .name()
        .or_else(|| def.map(|def| def.name.to_string()))
        .unwrap_or_default();
    let body = body_lines(
        view.resolved_ubertip()
            .or_else(|| view.ubertip())
            .or_else(|| view.tip()),
    );
    let mut stats = Vec::new();
    let mut costs = Vec::new();
    if let Some(def) = def {
        stats.push(("TARGET".to_string(), format!("{:?}", def.target)));
        stats.push(("RANGE".to_string(), format!("{:.0}", def.range)));
        if def.radius > 0.0 {
            stats.push(("RADIUS".to_string(), format!("{:.0}", def.radius)));
        }
        stats.push(("POWER".to_string(), def.power.to_string()));
        stats.push((
            "COOLDOWN".to_string(),
            format!("{:.1}s", def.cooldown_ticks as f32 / wc3_sim::TICKS_PER_SECOND as f32),
        ));
        if def.mana > 0 {
            costs.push(("MANA".to_string(), def.mana));
        }
    }
    TooltipData {
        source: TooltipSource::Ability,
        title,
        subtitle: "ABILITY".to_string(),
        hotkey: view.hotkey().and_then(|key| clean_text(&key)),
        stats,
        costs,
        body,
        requires: Vec::new(),
    }
}

/// Tooltip for a shipped item row plus the sim item numbers.
pub fn item(view: &ItemView<'_>, def: Option<&ItemDef>) -> TooltipData {
    let title = view
        .name()
        .or_else(|| def.map(|def| def.name.to_string()))
        .unwrap_or_default();
    let body = body_lines(
        view.ubertip()
            .or_else(|| view.tip())
            .or_else(|| view.description()),
    );
    let mut stats = Vec::new();
    let mut costs = Vec::new();
    if let Some(def) = def {
        stats.push(("LEVEL".to_string(), def.level.to_string()));
        if !def.class.is_empty() {
            stats.push(("CLASS".to_string(), def.class.to_string()));
        }
        if def.charges > 0 {
            stats.push(("CHARGES".to_string(), def.charges.to_string()));
        }
        if def.cost > 0 {
            costs.push(("GOLD".to_string(), def.cost));
        }
    }
    TooltipData {
        source: TooltipSource::Item,
        title,
        subtitle: "ITEM".to_string(),
        hotkey: view.hotkey().and_then(|key| clean_text(&key)),
        stats,
        costs,
        body,
        requires: Vec::new(),
    }
}

/// Tooltip for a command-card entry (shipped `MenuEntry`).
pub fn menu(entry: &MenuEntry) -> TooltipData {
    let mut costs = Vec::new();
    if let Some(gold) = entry.gold.filter(|gold| *gold > 0) {
        costs.push(("GOLD".to_string(), gold));
    }
    if let Some(lumber) = entry.lumber.filter(|lumber| *lumber > 0) {
        costs.push(("LUMBER".to_string(), lumber));
    }
    if let Some(food) = entry.food.filter(|food| *food > 0) {
        costs.push(("FOOD".to_string(), food));
    }
    let mut stats = Vec::new();
    if let Some(time) = entry.build_time.filter(|time| *time > 0) {
        stats.push(("BUILD".to_string(), format!("{time}s")));
    }
    TooltipData {
        source: TooltipSource::Menu,
        title: entry.name.to_uppercase(),
        subtitle: "COMMAND".to_string(),
        hotkey: Some(entry.hotkey_label()),
        stats,
        costs,
        body: body_lines(entry.tooltip.clone()),
        requires: entry.requires_names.clone(),
    }
}

/// Sim-only builders (offline fallback; never used when a shipped view exists).
pub fn unit_from_def(def: &UnitDef) -> TooltipData {
    TooltipData {
        source: TooltipSource::Unit,
        title: def.name.to_string(),
        subtitle: unit_subtitle(Some(def)),
        hotkey: None,
        stats: unit_stats(def),
        costs: {
            let mut costs = Vec::new();
            if def.stats.cost > 0 {
                costs.push(("GOLD".to_string(), def.stats.cost));
            }
            if def.stats.lumber_cost > 0 {
                costs.push(("LUMBER".to_string(), def.stats.lumber_cost));
            }
            if def.stats.supply > 0 && !def.is_building() {
                costs.push(("FOOD".to_string(), def.stats.supply));
            }
            costs
        },
        body: Vec::new(),
        requires: Vec::new(),
    }
}

pub fn ability_from_def(def: &SpellDef) -> TooltipData {
    TooltipData {
        source: TooltipSource::Ability,
        title: def.name.to_string(),
        subtitle: "ABILITY".to_string(),
        hotkey: None,
        stats: vec![
            ("TARGET".to_string(), format!("{:?}", def.target)),
            ("RANGE".to_string(), format!("{:.0}", def.range)),
            ("POWER".to_string(), def.power.to_string()),
            (
                "COOLDOWN".to_string(),
                format!(
                    "{:.1}s",
                    def.cooldown_ticks as f32 / wc3_sim::TICKS_PER_SECOND as f32
                ),
            ),
        ],
        costs: if def.mana > 0 {
            vec![("MANA".to_string(), def.mana)]
        } else {
            Vec::new()
        },
        body: Vec::new(),
        requires: Vec::new(),
    }
}

pub fn item_from_def(def: &ItemDef) -> TooltipData {
    TooltipData {
        source: TooltipSource::Item,
        title: def.name.to_string(),
        subtitle: "ITEM".to_string(),
        hotkey: None,
        stats: vec![
            ("LEVEL".to_string(), def.level.to_string()),
            ("CLASS".to_string(), def.class.to_string()),
        ],
        costs: if def.cost > 0 {
            vec![("GOLD".to_string(), def.cost)]
        } else {
            Vec::new()
        },
        body: Vec::new(),
        requires: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_data::source::DirSource;
    use wc3_data::GameData;

    fn shipped_data() -> Option<GameData> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3");
        if !root.is_dir() {
            return None;
        }
        GameData::load(&DirSource::new(root)).ok()
    }

    #[test]
    fn ui_tooltips_shipped_units_carry_name_hotkey_and_extended_text() {
        let Some(data) = shipped_data() else {
            eprintln!("skip: analysis/war3_full3 missing");
            return;
        };
        let mut with_name = 0;
        let mut with_hotkey = 0;
        let mut with_body = 0;
        let mut sampled = 0;
        for view in data.units() {
            let Some(name) = view.name() else { continue };
            if name.trim().is_empty() {
                continue;
            }
            with_name += 1;
            if sampled < 64 {
                let def = wc3_sim::def_of(&view.id_str());
                let tip = unit(&view, def);
                assert!(!tip.title.is_empty(), "title for {}", view.id_str());
                assert!(
                    !tip.title.contains('?') && tip.title != "UNIT",
                    "no generic title for {}: {}",
                    view.id_str(),
                    tip.title
                );
                if tip.hotkey.is_some() {
                    with_hotkey += 1;
                }
                if !tip.body.is_empty() {
                    with_body += 1;
                }
                sampled += 1;
            }
        }
        assert!(with_name >= 300, "shipped units with names: {with_name}");
        assert!(with_hotkey >= 10, "shipped unit hotkeys: {with_hotkey}");
        assert!(with_body >= 8, "shipped unit extended texts: {with_body}");
    }

    #[test]
    fn ui_tooltips_shipped_abilities_and_items_have_data() {
        let Some(data) = shipped_data() else {
            eprintln!("skip: analysis/war3_full3 missing");
            return;
        };
        let mut abilities = 0;
        let mut ability_body = 0;
        for view in data.abilities() {
            let Some(name) = view.name() else { continue };
            if name.trim().is_empty() {
                continue;
            }
            abilities += 1;
            if abilities <= 64 {
                let def = wc3_sim::SPELL_DEFS
                    .iter()
                    .find(|def| def.id.eq_ignore_ascii_case(&view.id_str()));
                let tip = ability(&view, def);
                assert!(!tip.title.is_empty());
                assert!(!tip.title.contains('?'), "ability title {}", tip.title);
                if !tip.body.is_empty() {
                    ability_body += 1;
                }
            }
        }
        assert!(abilities >= 100, "shipped abilities with names: {abilities}");
        assert!(ability_body >= 16, "ability extended texts: {ability_body}");

        let mut items = 0;
        for view in data.items() {
            let Some(name) = view.name() else { continue };
            if name.trim().is_empty() {
                continue;
            }
            items += 1;
            if items <= 64 {
                let def = wc3_sim::data::ITEMS
                    .iter()
                    .find(|def| def.id.eq_ignore_ascii_case(&view.id_str()));
                let tip = item(&view, def);
                assert!(!tip.title.is_empty());
                assert!(!tip.title.contains('?'));
            }
        }
        assert!(items >= 100, "shipped items with names: {items}");
    }

    #[test]
    fn ui_tooltips_sim_roster_covers_every_def() {
        let mut total = 0;
        for def in wc3_sim::ROSTER {
            let tip = unit_from_def(def);
            assert!(!tip.title.is_empty(), "title for {}", def.id);
            if def.stats.cost > 0 {
                assert!(
                    tip.costs.iter().any(|(label, _)| label == "GOLD"),
                    "{} gold cost",
                    def.id
                );
            }
            assert!(
                tip.stats.iter().any(|(label, value)| label == "HP" && !value.is_empty()),
                "{} hp stat",
                def.id
            );
            total += 1;
        }
        assert!(total >= 99, "roster tooltip coverage: {total}");
    }

    #[test]
    fn ui_tooltips_sim_spells_and_items_cover_the_tables() {
        assert_eq!(wc3_sim::SPELL_DEFS.len(), 11);
        for def in wc3_sim::SPELL_DEFS.iter() {
            let tip = ability_from_def(def);
            assert!(!tip.title.is_empty());
            assert!(
                tip.stats.iter().any(|(label, _)| label == "COOLDOWN"),
                "{} cooldown",
                def.id
            );
            if def.mana > 0 {
                assert!(tip.costs.iter().any(|(label, _)| label == "MANA"));
            }
        }
        let mut items = 0;
        for def in wc3_sim::data::ITEMS.iter().take(64) {
            let tip = item_from_def(def);
            assert!(!tip.title.is_empty(), "item title {}", def.id);
            assert!(
                tip.stats.iter().any(|(label, _)| label == "LEVEL"),
                "item level {}",
                def.id
            );
            items += 1;
        }
        assert_eq!(items, 64);
    }

    #[test]
    fn ui_tooltips_menu_entry_uses_shipped_cost_hotkey_and_text() {
        let entry = MenuEntry {
            id: "hfoo".to_string(),
            name: "Footman".to_string(),
            icon: None,
            hotkey: Some("F,F,F".to_string()),
            gold: Some(160),
            lumber: Some(0),
            food: Some(2),
            build_time: Some(20),
            tooltip: Some("|cffffcc00Footman|r — sturdy melee unit.".to_string()),
            requires: vec!["hbar".to_string()],
            requires_names: vec!["Barracks".to_string()],
            kind: wc3_formats::gamedata::ObjectKind::Unit,
        };
        let tip = menu(&entry);
        assert_eq!(tip.title, "FOOTMAN");
        assert_eq!(tip.hotkey.as_deref(), Some("F"));
        assert!(tip.costs.iter().any(|(label, value)| label == "GOLD" && *value == 160));
        assert!(tip.costs.iter().any(|(label, value)| label == "FOOD" && *value == 2));
        assert!(
            tip.body.iter().any(|line| line.contains("sturdy melee")),
            "shipped tooltip text: {:?}",
            tip.body
        );
        assert_eq!(tip.requires, vec!["Barracks".to_string()]);
        assert!(
            !tip.body.iter().any(|line| line.contains("|c")),
            "colour escapes stripped: {:?}",
            tip.body
        );
    }

    #[test]
    fn ui_tooltips_shipped_text_is_not_replaced_by_a_generic_fallback() {
        let Some(data) = shipped_data() else {
            eprintln!("skip: analysis/war3_full3 missing");
            return;
        };
        let mut checked = 0;
        for view in data.abilities() {
            let Some(ubertip) = view.ubertip() else { continue };
            let expected = clean_text(&ubertip);
            if expected.is_none() {
                continue;
            }
            let def = wc3_sim::SPELL_DEFS
                .iter()
                .find(|def| def.id.eq_ignore_ascii_case(&view.id_str()));
            let tip = ability(&view, def);
            assert!(
                !tip.body.is_empty(),
                "shipped ubertip kept for {}",
                view.id_str()
            );
            checked += 1;
            if checked >= 16 {
                break;
            }
        }
        assert!(checked >= 8, "abilities with shipped ubertip: {checked}");
    }
}

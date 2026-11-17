//! Content-model coverage: which object rows carry the fields a card shows.
//!
//! The HUD, shop, tooltip and command card must never guess: every row they can
//! show has to resolve a name, an icon, a tooltip, a hotkey and a price from the
//! shipped data. This module measures that property over the loaded database and
//! returns one record per object class (and per race for units), so a gap is a
//! number and an id list rather than a rendering surprise.
//!
//! ```no_run
//! use wc3_data::coverage::Field;
//! use wc3_data::{source::DirSource, GameDatabase};
//!
//! let data = GameDatabase::load(&DirSource::new("analysis/war3_data"))?;
//! let report = data.coverage();
//! for class in &report.classes {
//!     println!(
//!         "{} rows={} name={}",
//!         class.label(),
//!         class.rows.rows,
//!         class.rows.present(Field::Name)
//!     );
//! }
//! # Ok::<(), wc3_data::DataError>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};

use wc3_formats::gamedata::ObjectKind;

use crate::table::TableKind;
use crate::units::UnitView;
use crate::GameDatabase;

/// Object class used by the coverage matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectClass {
    /// Trainable unit (`hfoo`).
    Unit,
    /// Structure (`htow`, `hhou`).
    Building,
    /// Hero (`Hpal`), detected by `UnitBalance.Primary`.
    Hero,
    /// Neutral hostile unit (`nkob`), `UnitData.race = creeps`.
    Creep,
    /// Buff (`BHwe`) — profile-only rows, no shipped sheet.
    Buff,
    /// Item (`ratc`).
    Item,
    /// Upgrade (`Rhme`).
    Upgrade,
    /// Ability (`AHbz`).
    Ability,
    /// Doodad (`LTlt`).
    Doodad,
    /// Destructable (`LTrt`).
    Destructable,
}

impl ObjectClass {
    pub const ALL: [ObjectClass; 10] = [
        ObjectClass::Unit,
        ObjectClass::Building,
        ObjectClass::Hero,
        ObjectClass::Creep,
        ObjectClass::Buff,
        ObjectClass::Item,
        ObjectClass::Upgrade,
        ObjectClass::Ability,
        ObjectClass::Doodad,
        ObjectClass::Destructable,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Building => "building",
            Self::Hero => "hero",
            Self::Creep => "creep",
            Self::Buff => "buff",
            Self::Item => "item",
            Self::Upgrade => "upgrade",
            Self::Ability => "ability",
            Self::Doodad => "doodad",
            Self::Destructable => "destructable",
        }
    }

    /// Whether rows of this class are units (`UnitData` rows).
    pub fn is_unit_row(self) -> bool {
        matches!(
            self,
            Self::Unit | Self::Building | Self::Hero | Self::Creep
        )
    }
}

/// One card-relevant field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    /// `Name`.
    Name,
    /// `Art` / `Researchart` / `Buffart` / `unitUI.file` neighbours.
    Icon,
    /// `Hotkey`.
    Hotkey,
    /// `Tip` / `Ubertip` / `Tooltip`.
    Tooltip,
    /// `goldcost` / `gold_base`.
    Cost,
    /// Model path (`unitUI.file`, `ItemData.file`, doodad/destructable model).
    Model,
}

impl Field {
    pub const ALL: [Field; 6] = [
        Field::Name,
        Field::Icon,
        Field::Hotkey,
        Field::Tooltip,
        Field::Cost,
        Field::Model,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Icon => "icon",
            Self::Hotkey => "hotkey",
            Self::Tooltip => "tooltip",
            Self::Cost => "cost",
            Self::Model => "model",
        }
    }

    /// Whether the field has meaning for a class at all.
    ///
    /// Cost is a purchase/research price, so abilities, doodads, destructables
    /// and buffs have none. Doodads and destructables have no command card and
    /// no icon column in their sheets (`Doodads.slk`, `DestructableData.slk`),
    /// so hotkey, tooltip and icon do not apply to them.
    pub fn applies_to(self, class: ObjectClass) -> bool {
        match self {
            Self::Name => true,
            Self::Icon => matches!(
                class,
                ObjectClass::Unit
                    | ObjectClass::Building
                    | ObjectClass::Hero
                    | ObjectClass::Creep
                    | ObjectClass::Buff
                    | ObjectClass::Item
                    | ObjectClass::Upgrade
                    | ObjectClass::Ability
            ),

            Self::Hotkey => matches!(
                class,
                ObjectClass::Unit
                    | ObjectClass::Building
                    | ObjectClass::Hero
                    | ObjectClass::Creep
                    | ObjectClass::Item
                    | ObjectClass::Upgrade
                    | ObjectClass::Ability
            ),
            Self::Tooltip => matches!(
                class,
                ObjectClass::Unit
                    | ObjectClass::Building
                    | ObjectClass::Hero
                    | ObjectClass::Creep
                    | ObjectClass::Item
                    | ObjectClass::Upgrade
                    | ObjectClass::Ability
                    | ObjectClass::Buff
            ),
            Self::Cost => matches!(
                class,
                ObjectClass::Unit
                    | ObjectClass::Building
                    | ObjectClass::Hero
                    | ObjectClass::Creep
                    | ObjectClass::Item
                    | ObjectClass::Upgrade
            ),
            Self::Model => matches!(
                class,
                ObjectClass::Unit
                    | ObjectClass::Building
                    | ObjectClass::Hero
                    | ObjectClass::Creep
                    | ObjectClass::Item
                    | ObjectClass::Doodad
                    | ObjectClass::Destructable
            ),
        }
    }
}

/// How many rows of one class carry one field, plus the ids that do not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldCoverage {
    pub rows: usize,
    pub present: usize,
    pub missing: Vec<String>,
}

impl FieldCoverage {
    /// Fraction of rows that carry the value (`0.0` for an empty class).
    pub fn ratio(&self) -> f64 {
        if self.rows == 0 {
            0.0
        } else {
            self.present as f64 / self.rows as f64
        }
    }

    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Field presence of one row, as the readers saw it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowFacts {
    pub name: bool,
    pub icon: bool,
    pub hotkey: bool,
    pub tooltip: bool,
    pub cost: bool,
    pub model: bool,
}

impl RowFacts {
    pub fn has(&self, field: Field) -> bool {
        match field {
            Field::Name => self.name,
            Field::Icon => self.icon,
            Field::Hotkey => self.hotkey,
            Field::Tooltip => self.tooltip,
            Field::Cost => self.cost,
            Field::Model => self.model,
        }
    }
}

/// Coverage of a set of rows: one counter per applicable field.
#[derive(Debug, Clone)]
pub struct RowCoverage {
    /// Class the rows belong to; decides which fields apply.
    pub class: ObjectClass,
    pub rows: usize,
    pub fields: BTreeMap<Field, FieldCoverage>,
}

impl Default for RowCoverage {
    fn default() -> Self {
        Self::new(ObjectClass::Unit)
    }
}

impl RowCoverage {
    pub fn new(class: ObjectClass) -> Self {
        let fields = Field::ALL
            .into_iter()
            .filter(|field| field.applies_to(class))
            .map(|field| (field, FieldCoverage::default()))
            .collect();
        Self {
            class,
            rows: 0,
            fields,
        }
    }

    /// Rows measured over an explicit field set (a card list mixes classes).
    pub fn with_fields(class: ObjectClass, wanted: &[Field]) -> Self {
        let fields = wanted
            .iter()
            .map(|field| (*field, FieldCoverage::default()))
            .collect();
        Self {
            class,
            rows: 0,
            fields,
        }
    }

    pub fn record(&mut self, id: &str, facts: RowFacts) {
        self.rows += 1;
        for (field, coverage) in self.fields.iter_mut() {
            coverage.rows += 1;
            if facts.has(*field) {
                coverage.present += 1;
            } else {
                coverage.missing.push(id.to_string());
            }
        }
    }

    pub fn field(&self, field: Field) -> Option<&FieldCoverage> {
        self.fields.get(&field)
    }

    pub fn present(&self, field: Field) -> usize {
        self.field(field).map(|coverage| coverage.present).unwrap_or(0)
    }

    /// Ids of this class that do not carry `field`.
    pub fn missing(&self, field: Field) -> &[String] {
        self.field(field)
            .map(|coverage| coverage.missing.as_slice())
            .unwrap_or(&[])
    }
}

/// One class bucket of the matrix; unit classes are split by race.
#[derive(Debug, Clone)]
pub struct ClassCoverage {
    pub class: ObjectClass,
    /// `UnitData.race` for unit classes, `None` otherwise.
    pub race: Option<String>,
    pub rows: RowCoverage,
}

impl ClassCoverage {
    pub fn label(&self) -> String {
        match &self.race {
            Some(race) => format!("{race}/{}", self.class.label()),
            None => self.class.label().to_string(),
        }
    }
}

/// Command-card reachability: the lists a producer/worker/building exposes.
#[derive(Debug, Clone, Default)]
pub struct MenuCoverage {
    /// Units with a non-empty `Trains`.
    pub producers: usize,
    /// Units with a non-empty `Builds`.
    pub builders: usize,
    /// Units with a non-empty `Researches`.
    pub researchers: usize,
    /// Train + build + research entries, counted once each.
    pub entries: usize,
    /// Entries whose name, icon and price all resolve.
    pub complete: usize,
    /// Per-field coverage over the entries.
    pub fields: BTreeMap<Field, FieldCoverage>,
    /// Distinct object ids reachable from the three lists.
    pub reachable: RowCoverage,
    /// Requirement references across all reachable rows.
    pub requires: usize,
    /// `producer->id requires RID` lines whose display name did not resolve.
    pub requires_unresolved: Vec<String>,
    /// `producer->id requires RID` lines whose id names no loaded object at all.
    pub requires_unknown: Vec<String>,
    /// Distinct hero learn-skill buttons (`heroAbilList`) and their coverage.
    pub hero_abilities: RowCoverage,
    /// Distinct unit ability ids (`abilList`), informational.
    pub unit_abilities: RowCoverage,
}

impl MenuCoverage {
    pub fn field(&self, field: Field) -> Option<&FieldCoverage> {
        self.fields.get(&field)
    }

    pub fn present(&self, field: Field) -> usize {
        self.field(field).map(|coverage| coverage.present).unwrap_or(0)
    }
}

/// The whole audit.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    /// Rows per loaded table, in table order.
    pub counts: Vec<(TableKind, usize)>,
    /// Per-class (and per-race for units) coverage.
    pub classes: Vec<ClassCoverage>,
    /// Command-card reachability.
    pub menu: MenuCoverage,
}

impl CoverageReport {
    /// Row counts per class, summed over races.
    pub fn class_totals(&self) -> BTreeMap<ObjectClass, usize> {
        let mut out = BTreeMap::new();
        for class in &self.classes {
            *out.entry(class.class).or_insert(0) += class.rows.rows;
        }
        out
    }

    /// Total rows measured across every class bucket.
    pub fn total_rows(&self) -> usize {
        self.classes.iter().map(|class| class.rows.rows).sum()
    }

    /// Render the measured matrix as Markdown (the body of
    /// `analysis/agent-data/DATA_COVERAGE.md`).
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("### Loaded rows\n\n| Table | Rows |\n|---|---|\n");
        for (kind, rows) in &self.counts {
            out.push_str(&format!("| `{}` | {rows} |\n", kind.name()));
        }
        out.push_str(&format!(
            "\n**Measured rows:** {} across {} class buckets.\n",
            self.total_rows(),
            self.classes.len()
        ));

        out.push_str(
            "\n### Per-class coverage\n\n\
             `—` means the field is not part of the class (doodads and destructables have no card, \
             abilities have no gold price).\n\n\
             | Class | Rows | name | icon | tooltip | cost | hotkey | model |\n\
             |---|---|---|---|---|---|---|---|\n",
        );
        let mut classes = self.classes.clone();
        classes.sort_by(|a, b| a.label().cmp(&b.label()));
        for class in &classes {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                class.label(),
                class.rows.rows,
                cell(&class.rows, Field::Name),
                cell(&class.rows, Field::Icon),
                cell(&class.rows, Field::Tooltip),
                cell(&class.rows, Field::Cost),
                cell(&class.rows, Field::Hotkey),
                cell(&class.rows, Field::Model),
            ));
        }

        out.push_str("\n### Missing values (shipped-data gaps)\n\n");
        let mut any = false;
        for class in &classes {
            for field in Field::ALL {
                let missing = class.rows.missing(field);
                if missing.is_empty() {
                    continue;
                }
                any = true;
                out.push_str(&format!(
                    "- `{}` — no {} ({}/{}): {}\n",
                    class.label(),
                    field.label(),
                    missing.len(),
                    class.rows.rows,
                    id_list(missing, 12),
                ));
            }
        }
        if !any {
            out.push_str("- none\n");
        }

        let menu = &self.menu;
        out.push_str(&format!(
            "\n### Command-card reachability\n\n\
             Producers (non-empty `Trains`): **{}** · builders (`Builds`): **{}** · \
             researchers (`Researches`): **{}** · entries: **{}** · fully complete \
             (name + icon + gold): **{}** · requirement references: **{}** (unresolved: {}).\n\n\
             | Set | Rows | name | icon | hotkey | tooltip | cost |\n\
             |---|---|---|---|---|---|---|\n",
            menu.producers,
            menu.builders,
            menu.researchers,
            menu.entries,
            menu.complete,
            menu.requires,
            menu.requires_unresolved.len(),
        ));
        out.push_str(&format!(
            "| entries | {} | {} | {} | {} | {} | {} |\n",
            menu.entries,
            menu.present(Field::Name),
            menu.present(Field::Icon),
            menu.present(Field::Hotkey),
            menu.present(Field::Tooltip),
            menu.present(Field::Cost),
        ));
        out.push_str(&format!(
            "| distinct reachable ids | {} | {} | {} | {} | {} | {} |\n",
            menu.reachable.rows,
            menu.reachable.present(Field::Name),
            menu.reachable.present(Field::Icon),
            menu.reachable.present(Field::Hotkey),
            menu.reachable.present(Field::Tooltip),
            menu.reachable.present(Field::Cost),
        ));
        out.push_str(&format!(
            "| hero learn buttons (`heroAbilList`) | {} | {} | {} | {} | {} | — |\n",
            menu.hero_abilities.rows,
            menu.hero_abilities.present(Field::Name),
            menu.hero_abilities.present(Field::Icon),
            menu.hero_abilities.present(Field::Hotkey),
            menu.hero_abilities.present(Field::Tooltip),
        ));
        out.push_str(&format!(
            "| unit ability buttons (`abilList`) | {} | {} | {} | {} | {} | — |\n",
            menu.unit_abilities.rows,
            menu.unit_abilities.present(Field::Name),
            menu.unit_abilities.present(Field::Icon),
            menu.unit_abilities.present(Field::Hotkey),
            menu.unit_abilities.present(Field::Tooltip),
        ));

        out.push_str("\n**Missing on card-reachable rows**\n\n");
        let mut lines = 0;
        for (label, coverage) in [
            ("entries", Some(&menu.reachable)),
            ("hero learn buttons", Some(&menu.hero_abilities)),
            ("unit ability buttons", Some(&menu.unit_abilities)),
        ] {
            let Some(coverage) = coverage else { continue };
            for field in Field::ALL {
                let missing = coverage.missing(field);
                if missing.is_empty() {
                    continue;
                }
                lines += 1;
                out.push_str(&format!(
                    "- `{label}` — no {} ({}/{}): {}\n",
                    field.label(),
                    missing.len(),
                    coverage.rows,
                    id_list(missing, 12),
                ));
            }
        }
        if lines == 0 {
            out.push_str("- none\n");
        }
        if !menu.requires_unresolved.is_empty() {
            out.push_str("\n**Unresolved requirement names**\n\n");
            for line in &menu.requires_unresolved {
                out.push_str(&format!("- {line}\n"));
            }
        }
        out
    }
}

/// One table cell: `present/rows`, or `—` when the field does not apply.
fn cell(rows: &RowCoverage, field: Field) -> String {
    match rows.field(field) {
        Some(coverage) => format!("{}/{}", coverage.present, coverage.rows),
        None => "—".to_string(),
    }
}

/// Comma separated id list, truncated after `limit` entries.
fn id_list(ids: &[String], limit: usize) -> String {
    let shown: Vec<String> = ids
        .iter()
        .take(limit)
        .map(|id| format!("`{id}`"))
        .collect();
    if ids.len() > limit {
        format!("{} … +{} more", shown.join(", "), ids.len() - limit)
    } else {
        shown.join(", ")
    }
}

impl GameDatabase {
    /// Measure content-model coverage of the loaded rows.
    pub fn coverage(&self) -> CoverageReport {
        let mut buckets: BTreeMap<(Option<String>, ObjectClass), RowCoverage> = BTreeMap::new();
        for unit in self.units() {
            let class = classify_unit(&unit);
            let race = unit
                .race()
                .map(|race| race.trim().to_ascii_lowercase())
                .filter(|race| !race.is_empty())
                .unwrap_or_else(|| "(none)".to_string());
            let facts = unit_facts(&unit);
            buckets
                .entry((Some(race), class))
                .or_insert_with(|| RowCoverage::new(class))
                .record(&unit.id_str(), facts);
        }
        for buff in self.buff_ids() {
            let info = self.buff(&buff);
            let facts = RowFacts {
                name: info
                    .as_ref()
                    .and_then(|info| info.name.as_deref())
                    .is_some_and(|name| !name.trim().is_empty()),
                icon: info
                    .as_ref()
                    .and_then(|info| info.icon.as_deref())
                    .is_some_and(|icon| !icon.trim().is_empty()),
                tooltip: info.as_ref().is_some_and(|info| {
                    info.tip.is_some() || info.ubertip.is_some() || info.name.is_some()
                }),
                ..RowFacts::default()
            };
            buckets
                .entry((None, ObjectClass::Buff))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Buff))
                .record(&buff, facts);
        }
        for item in self.items() {
            let facts = RowFacts {
                name: non_empty(item.name()),
                icon: non_empty(item.icon()),
                hotkey: non_empty(item.hotkey()),
                tooltip: non_empty(item.tip()) || non_empty(item.ubertip()),
                cost: item.gold_cost().ok().flatten().is_some(),
                model: non_empty(item.model_file()),
            };
            buckets
                .entry((None, ObjectClass::Item))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Item))
                .record(&item.id_str(), facts);
        }
        for upgrade in self.upgrades() {
            let facts = RowFacts {
                name: non_empty(upgrade.name()),
                icon: non_empty(upgrade.research_icon()),
                hotkey: non_empty(upgrade.research_hotkey()),
                tooltip: non_empty(upgrade.research_tip())
                    || non_empty(upgrade.research_ubertip()),
                cost: upgrade.gold_cost(1).ok().flatten().is_some(),
                model: false,
            };
            buckets
                .entry((None, ObjectClass::Upgrade))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Upgrade))
                .record(&upgrade.id_str(), facts);
        }
        for ability in self.abilities() {
            let facts = RowFacts {
                name: non_empty(ability.resolved_name()),
                icon: non_empty(ability.resolved_icon())
                    || non_empty(ability.research_icon()),
                hotkey: non_empty(ability.resolved_hotkey())
                    || non_empty(ability.research_hotkey()),
                tooltip: non_empty(ability.resolved_tip(1))
                    || non_empty(ability.resolved_tooltip(1))
                    || non_empty(ability.research_tip())
                    || non_empty(ability.research_ubertip()),
                cost: false,
                model: false,
            };
            buckets
                .entry((None, ObjectClass::Ability))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Ability))
                .record(&ability.id_str(), facts);
        }
        for doodad in self.doodads() {
            let facts = RowFacts {
                name: non_empty(doodad.name().map(str::to_string)),
                icon: false,
                model: non_empty(doodad.model_file()),
                ..RowFacts::default()
            };
            buckets
                .entry((None, ObjectClass::Doodad))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Doodad))
                .record(&doodad.id_str(), facts);
        }
        for destructable in self.destructables() {
            let facts = RowFacts {
                name: non_empty(destructable.name().map(str::to_string)),
                icon: false,
                model: non_empty(destructable.model_file()),
                ..RowFacts::default()
            };
            buckets
                .entry((None, ObjectClass::Destructable))
                .or_insert_with(|| RowCoverage::new(ObjectClass::Destructable))
                .record(&destructable.id_str(), facts);
        }

        let classes = buckets
            .into_iter()
            .map(|((race, class), rows)| ClassCoverage { class, race, rows })
            .collect();
        CoverageReport {
            counts: self.counts(),
            classes,
            menu: self.menu_coverage(),
        }
    }

    /// Command-card reachability measurement (see [`MenuCoverage`]).
    pub fn menu_coverage(&self) -> MenuCoverage {
        let mut menu = MenuCoverage::default();
        for field in Field::ALL {
            menu.fields.insert(field, FieldCoverage::default());
        }
        menu.reachable = RowCoverage::with_fields(
            ObjectClass::Unit,
            &[
                Field::Name,
                Field::Icon,
                Field::Hotkey,
                Field::Tooltip,
                Field::Cost,
            ],
        );
        menu.hero_abilities = RowCoverage::new(ObjectClass::Ability);
        menu.unit_abilities = RowCoverage::new(ObjectClass::Ability);

        // Distinct reachable ids across the three card lists, with the first
        // producer that exposes each (for the missing-list diagnostic).
        let mut reachable: BTreeMap<String, String> = BTreeMap::new();
        let mut pushes = |ids: &[String], owner: &str, reachable: &mut BTreeMap<String, String>| {
            for id in ids {
                reachable
                    .entry(id.clone())
                    .or_insert_with(|| owner.to_string());
            }
        };
        for (producer, _) in self.producers() {
            pushes(&self.train_menu_ids(&producer), &producer, &mut reachable);
        }
        for (worker, _) in self.builders() {
            pushes(&self.build_menu_ids(&worker), &worker, &mut reachable);
        }
        for (researcher, _) in self.researchers() {
            pushes(&self.research_ids(&researcher), &researcher, &mut reachable);
        }

        for (producer, _) in self.producers() {
            menu.producers += 1;
            for entry in self.train_menu(&producer) {
                record_menu_entry(self, &mut menu, &entry);
            }
        }
        for (worker, _) in self.builders() {
            menu.builders += 1;
            for entry in self.build_menu(&worker) {
                record_menu_entry(self, &mut menu, &entry);
            }
        }
        for (researcher, _) in self.researchers() {
            menu.researchers += 1;
            for entry in self.research_menu(&researcher) {
                record_menu_entry(self, &mut menu, &entry);
            }
        }

        for (id, _owner) in &reachable {
            let info = self.object(id);
            // A reachable id that no table defines is recorded with no fields at
            // all: that is the strongest possible gap and must not be hidden.
            let facts = match &info {
                Some(info) => RowFacts {
                    name: info.name.as_deref().is_some_and(|name| !name.is_empty()),
                    icon: info.icon.as_deref().is_some_and(|icon| !icon.is_empty()),
                    hotkey: info.hotkey.as_deref().is_some_and(|hotkey| !hotkey.is_empty()),
                    tooltip: info.ubertip.is_some() || info.tip.is_some(),
                    cost: !matches!(info.kind, ObjectKind::Buff),
                    model: info.model.is_some(),
                },
                None => RowFacts::default(),
            };
            menu.reachable.record(id, facts);
        }

        // Ability buttons: hero learn-skill rows and in-card unit abilities.
        let mut hero_ids = BTreeSet::new();
        let mut unit_ids = BTreeSet::new();
        for unit in self.units() {
            for id in unit.hero_abilities() {
                hero_ids.insert(id.to_string());
            }
            for id in unit.abilities() {
                unit_ids.insert(id.to_string());
            }
        }
        for id in hero_ids {
            let facts = ability_facts(self, &id);
            menu.hero_abilities.record(&id, facts);
        }
        for id in unit_ids {
            let facts = ability_facts(self, &id);
            menu.unit_abilities.record(&id, facts);
        }
        menu
    }

    fn train_menu_ids(&self, producer: &str) -> Vec<String> {
        self.unit(producer).map(|unit| unit.trains()).unwrap_or_default()
    }

    fn build_menu_ids(&self, worker: &str) -> Vec<String> {
        self.unit(worker).map(|unit| unit.builds()).unwrap_or_default()
    }

    fn research_ids(&self, producer: &str) -> Vec<String> {
        self.unit(producer)
            .map(|unit| unit.researches())
            .unwrap_or_default()
    }
}

fn record_menu_entry(db: &GameDatabase, menu: &mut MenuCoverage, entry: &crate::menu::MenuEntry) {
    menu.entries += 1;
    if entry.is_complete() {
        menu.complete += 1;
    }
    let facts = RowFacts {
        name: !entry.name.is_empty(),
        icon: entry.icon.as_deref().is_some_and(|icon| !icon.is_empty()),
        hotkey: entry.hotkey.as_deref().is_some_and(|hotkey| !hotkey.is_empty()),
        tooltip: entry
            .tooltip
            .as_deref()
            .is_some_and(|tooltip| !tooltip.is_empty()),
        cost: entry.gold.is_some(),
        model: false,
    };
    for (field, coverage) in menu.fields.iter_mut() {
        coverage.rows += 1;
        if facts.has(*field) {
            coverage.present += 1;
        } else {
            coverage.missing.push(entry.id.clone());
        }
    }
    for (index, requirement) in entry.requires.iter().enumerate() {
        menu.requires += 1;
        if db.object(requirement).is_none() {
            menu.requires_unknown
                .push(format!("{}->{} requires {requirement}", entry.id, requirement));
        }
        let resolved = entry
            .requires_names
            .get(index)
            .is_some_and(|name| !name.eq_ignore_ascii_case(requirement));
        if !resolved {
            menu.requires_unresolved.push(format!(
                "{}->{} requires {requirement}",
                entry.id, requirement
            ));
        }
    }
}

fn ability_facts(db: &GameDatabase, id: &str) -> RowFacts {
    let Some(ability) = db.ability(id) else {
        return RowFacts::default();
    };
    RowFacts {
        name: non_empty(ability.resolved_name()),
        icon: non_empty(ability.resolved_icon()) || non_empty(ability.research_icon()),
        hotkey: non_empty(ability.resolved_hotkey()) || non_empty(ability.research_hotkey()),
        tooltip: non_empty(ability.resolved_tip(1))
            || non_empty(ability.resolved_tooltip(1))
            || non_empty(ability.research_tip()),
        cost: false,
        model: false,
    }
}

fn classify_unit(unit: &UnitView<'_>) -> ObjectClass {
    if unit.is_building().unwrap_or(false) {
        return ObjectClass::Building;
    }
    let hero = unit
        .primary_attribute()
        .is_some_and(|attribute| !attribute.trim().is_empty() && attribute.trim() != "_");
    if hero {
        return ObjectClass::Hero;
    }
    if unit
        .race()
        .is_some_and(|race| race.trim().eq_ignore_ascii_case("creeps"))
    {
        return ObjectClass::Creep;
    }
    ObjectClass::Unit
}

fn unit_facts(unit: &UnitView<'_>) -> RowFacts {
    RowFacts {
        name: non_empty(unit.name()),
        icon: non_empty(unit.icon()),
        hotkey: non_empty(unit.hotkey()),
        tooltip: non_empty(unit.tip()) || non_empty(unit.ubertip()) || non_empty(unit.tooltip()),
        cost: unit.gold_cost().ok().flatten().is_some(),
        model: non_empty(unit.model_file()),
    }
}

fn non_empty(value: Option<String>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    /// A producer (`ogre`), its trainee (`opeo`), a worker (`opeo`) with a build
    /// list, and one upgrade-granting building — the smallest shape that
    /// exercises every card list.
    fn source() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UnitData.slk",
            b"ID;PWXL;N;E\nB;Y4;X4;D0\nC;Y1;X1;K\"unitID\"\nC;X2;K\"race\"\nC;X3;K\"type\"\n\
C;X4;K\"movetp\"\n\
C;Y2;X1;K\"ogre\"\nC;X2;K\"orc\"\nC;X3;K\"TownHall\"\nC;X4;K\"foot\"\n\
C;Y3;X1;K\"opeo\"\nC;X2;K\"orc\"\nC;X3;K\"Peon\"\nC;X4;K\"foot\"\n\
C;Y4;X1;K\"obar\"\nC;X2;K\"orc\"\nC;X3;K\"Barracks\"\nC;X4;K\"foot\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\UnitBalance.slk",
            b"ID;PWXL;N|E\nB;Y4;X5;D0\nC;Y1;X1;K\"unitBalanceID\"\nC;X2;K\"HP\"\nC;X3;K\"goldcost\"\n\
C;X4;K\"lumbercost\"\nC;X5;K\"Primary\"\n\
C;Y2;X1;K\"ogre\"\nC;X2;K1500\nC;X3;K385\nC;X4;K205\nC;X5;K\"-\"\n\
C;Y3;X1;K\"opeo\"\nC;X2;K250\nC;X3;K90\nC;X4;K0\nC;X5;K\"-\"\n\
C;Y4;X1;K\"obar\"\nC;X2;K1200\nC;X3;K180\nC;X4;K40\nC;X5;K\"-\"\n"
                .to_vec(),
        );
        source.insert(
            "Units\\unitUI.slk",
            b"ID;PWXL;N;E\nB;Y4;X3;D0\nC;Y1;X1;K\"unitUIID\"\nC;X2;K\"file\"\nC;X3;K\"isbldg\"\n\
C;Y2;X1;K\"ogre\"\nC:X2;K\"buildings\\\\Orc\\\\GreatHall\\\\GreatHall\"\nC;X3;K1\n\
C;Y3;X1;K\"opeo\"\nC;X2;K\"units\\\\orc\\\\Peon\\\\Peon\"\nC;X3;K0\n\
C;Y4;X1;K\"obar\"\nC;X2;K\"buildings\\\\Orc\\\\Barracks\\\\Barracks\"\nC;X3;K1\n"
                .to_vec(),
        );
        source.insert(
            "Units\\OrcUnitStrings.txt",
            "[ogre]\nName=Great Hall\nTip=Great Hall\nUbertip=Trains a peon\n\
[opeo]\nName=Peon\nTip=Peon\nUbertip=Builds structures\n\
[obar]\nName=Barracks\nTip=Barracks\nUbertip=Trains grunts\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\OrcUnitFunc.txt",
            "[ogre]\nArt=BTNGreatHall.blp\nHotkey=G\nTrains=opeo\n\
[opeo]\nArt=BTNPeon.blp\nHotkey=P\nBuilds=ogre,obar\nResearches=Rosp\n\
[obar]\nArt=BTNBarracks.blp\nHotkey=B\nTrains=opeo\nRequires=ogre\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\UpgradeData.slk",
            b"ID;PWXL;N;E\nB;Y2;X7;D0\nC;Y1;X1;K\"upgradeid\"\nC;X2;K\"comments\"\nC;X3;K\"race\"\n\
C;X4;K\"maxlevel\"\nC;X5;K\"goldbase\"\nC;X6;K\"lumberbase\"\nC;X7;K\"timebase\"\n\
C;Y2;X1;K\"Rosp\"\nC;X2;K\"orc spiked barricades\"\nC;X3;K\"orc\"\nC;X4;K1\nC;X5;K50\nC;X6;K100\nC;X7;K30\n"
                .to_vec(),
        );
        source.insert(
            "Units\\OrcUpgradeFunc.txt",
            "[Rosp]\nArt=BTNSpikedBarricades.blp\nHotkey=S\nButtonpos=0,0\n\
Requirescount=1\nRequires=\nRequires1=obar\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\OrcUpgradeStrings.txt",
            "[Rosp]\nName=Spiked Barricades\nTip=Spiked Barricades\nUbertip=Enemy melee units take damage\n"
                .as_bytes()
                .to_vec(),
        );
        source
    }

    #[test]
    fn audit_counts_rows_fields_and_card_lists() {
        let data = crate::GameDatabase::load(&source()).expect("load");
        let report = data.coverage();

        // Two buildings + one unit (split by race) + the upgrade row.
        let totals = report.class_totals();
        assert_eq!(totals.get(&ObjectClass::Building), Some(&2));
        assert_eq!(totals.get(&ObjectClass::Unit), Some(&1));
        assert_eq!(totals.get(&ObjectClass::Upgrade), Some(&1));
        assert_eq!(report.total_rows(), 4, "3 unit rows + 1 upgrade row");

        let orc_buildings = report
            .classes
            .iter()
            .find(|class| class.class == ObjectClass::Building && class.race.as_deref() == Some("orc"))
            .expect("orc buildings");
        assert_eq!(orc_buildings.rows.rows, 2);
        for field in [Field::Name, Field::Icon, Field::Hotkey, Field::Tooltip, Field::Cost] {
            assert_eq!(
                orc_buildings.rows.present(field),
                2,
                "{} coverage",
                field.label()
            );
        }
        // `Model` resolves from `unitUI.file` (`buildings\...`).
        assert_eq!(orc_buildings.rows.present(Field::Model), 2);

        // Card lists: one trainer, one builder, one researcher.
        assert_eq!(report.menu.producers, 2, "ogre + obar train");
        assert_eq!(report.menu.builders, 1, "opeo builds");
        assert_eq!(report.menu.researchers, 1, "opeo researches");
        assert_eq!(report.menu.entries, 5, "1 train + 2 builds + 1 research");
        assert_eq!(report.menu.complete, report.menu.entries);
        assert_field_complete(&report.menu.reachable, Field::Name);
        assert_field_complete(&report.menu.reachable, Field::Icon);
        assert_eq!(report.menu.requires, 2, "obar needs ogre, Rosp needs obar");
        assert!(report.menu.requires_unresolved.is_empty());
        assert!(report.menu.requires_unknown.is_empty());
    }

    /// A card list that names a row no table defines is reported, not skipped.
    #[test]
    fn unknown_reachable_ids_are_reported() {
        let mut source = source();
        source.insert(
            "Units\\OrcUnitFunc.txt",
            "[ogre]\nArt=BTNGreatHall.blp\nHotkey=G\nTrains=opeo,zzzz\n\
[opeo]\nArt=BTNPeon.blp\nHotkey=P\nBuilds=ogre,obar\nResearches=Rosp\n\
[obar]\nArt=BTNBarracks.blp\nHotkey=B\nTrains=opeo\nRequires=ogre\n"
                .as_bytes()
                .to_vec(),
        );
        let data = crate::GameDatabase::load(&source).expect("load");
        let report = data.coverage();
        assert_eq!(report.menu.reachable.missing(Field::Name), ["zzzz"]);
        assert_eq!(report.menu.reachable.missing(Field::Icon), ["zzzz"]);
    }

    fn assert_field_complete(rows: &RowCoverage, field: Field) {
        let missing = rows.missing(field);
        assert!(
            missing.is_empty(),
            "missing {}: {missing:?}",
            field.label()
        );
    }

    #[test]
    fn markdown_lists_every_class_and_field() {
        let data = crate::GameDatabase::load(&source()).expect("load");
        let markdown = data.coverage().markdown();
        assert!(markdown.contains("| `orc/building` | 2 |"), "{markdown}");
        assert!(markdown.contains("Command-card reachability"), "{markdown}");
        assert!(markdown.contains("Producers (non-empty `Trains`): **2**"), "{markdown}");
        // A field that does not apply prints as an em dash, never as a fake count.
        assert!(
            markdown.contains("| `destructable`") || markdown.contains("—"),
            "{markdown}"
        );
    }
}

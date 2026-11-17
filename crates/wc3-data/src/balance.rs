//! Balance audit: every unit, ability, item and upgrade row validated against the
//! SLK cells through the typed accessors.
//!
//! The audit is deliberately mechanical: it walks *every* row of the four balance
//! families (`UnitData`/`UnitBalance`/`UnitWeapons`, `AbilityData`, `ItemData`,
//! `UpgradeData`), reads the numbers a simulator/HUD consumes, and reports cells
//! that cannot be interpreted or that contradict the sheet (negative costs,
//! damage minimum above maximum, declared levels without parsed levels).
//!
//! It is not a gameplay balance opinion: an "issue" here means the data cannot be
//! used as typed numbers, which is exactly what the game layer would hit.

use crate::GameDatabase;

/// How severe a balance finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The cell cannot be used as a typed number (or contradicts the sheet).
    Error,
    /// A documented engine sentinel (`Area1 == -1` means "no area").
    Warning,
}

/// One balance finding for one row and field.
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceIssue {
    pub severity: Severity,
    pub id: String,
    pub field: &'static str,
    pub detail: String,
}

/// Audit result for one balance family.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FamilyAudit {
    /// Rows present in the primary sheet.
    pub rows: usize,
    /// Rows whose numbers were read end-to-end.
    pub checked: usize,
    pub issues: Vec<BalanceIssue>,
}

impl FamilyAudit {
    fn note(&mut self, id: &str, field: &'static str, detail: impl Into<String>) {
        self.issues.push(BalanceIssue {
            severity: Severity::Error,
            id: id.to_string(),
            field,
            detail: detail.into(),
        });
    }

    fn warn(&mut self, id: &str, field: &'static str, detail: impl Into<String>) {
        self.issues.push(BalanceIssue {
            severity: Severity::Warning,
            id: id.to_string(),
            field,
            detail: detail.into(),
        });
    }

    /// Findings that make the row unusable.
    pub fn errors(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .count()
    }

    /// Documented sentinels (reported, not gating).
    pub fn warnings(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
            .count()
    }
}

/// The four-family balance audit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BalanceAudit {
    pub units: FamilyAudit,
    pub abilities: FamilyAudit,
    pub items: FamilyAudit,
    pub upgrades: FamilyAudit,
}

impl BalanceAudit {
    /// `(family, audit)` pairs in report order.
    pub fn families(&self) -> [(&'static str, &FamilyAudit); 4] {
        [
            ("units", &self.units),
            ("abilities", &self.abilities),
            ("items", &self.items),
            ("upgrades", &self.upgrades),
        ]
    }

    pub fn rows(&self) -> usize {
        self.families().iter().map(|(_, audit)| audit.rows).sum()
    }

    pub fn checked(&self) -> usize {
        self.families().iter().map(|(_, audit)| audit.checked).sum()
    }

    pub fn issues(&self) -> usize {
        self.families()
            .iter()
            .map(|(_, audit)| audit.issues.len())
            .sum()
    }

    pub fn errors(&self) -> usize {
        self.families().iter().map(|(_, audit)| audit.errors()).sum()
    }

    pub fn warnings(&self) -> usize {
        self.families().iter().map(|(_, audit)| audit.warnings()).sum()
    }

    pub fn ok(&self) -> bool {
        self.errors() == 0
    }
}

fn negative(value: Option<i32>, field: &'static str) -> Option<(&'static str, String)> {
    match value {
        Some(value) if value < 0 => Some((field, format!("negative value {value}"))),
        _ => None,
    }
}

impl GameDatabase {
    /// Validate every balance row of the loaded install.
    pub fn balance_audit(&self) -> BalanceAudit {
        let mut audit = BalanceAudit::default();

        for unit in self.units() {
            audit.units.rows += 1;
            let id = unit.id_str();
            let mut bad = false;
            let mut record = |field: &'static str, detail: String| {
                audit.units.note(&id, field, detail);
                bad = true;
            };
            let checks: [(&'static str, crate::Result<Option<i32>>); 10] = [
                ("UnitBalance.HP", unit.hp()),
                ("UnitBalance.manaN", unit.mana()),
                ("UnitBalance.level", unit.level()),
                ("UnitBalance.goldcost", unit.gold_cost()),
                ("UnitBalance.lumbercost", unit.lumber_cost()),
                ("UnitBalance.buildtime", unit.build_time()),
                ("UnitBalance.fused", unit.food_used()),
                ("UnitBalance.fprovided", unit.food_provided()),
                ("UnitBalance.speed", unit.speed()),
                ("UnitBalance.def", unit.armor_base()),
            ];
            for (field, value) in checks {
                match value {
                    Ok(value) => {
                        if let Some((field, detail)) = negative(value, field) {
                            record(field, detail);
                        }
                    }
                    Err(error) => record(field, format!("unreadable cell: {error}")),
                }
            }
            for weapon in unit.weapons() {
                match (weapon.min_damage(), weapon.max_damage()) {
                    (Ok(min), Ok(max)) => {
                        for value in [min, max] {
                            if let Some((field, detail)) = negative(value, "UnitWeapons.dmgplus") {
                                record(field, detail);
                            }
                        }
                        if let (Some(min), Some(max)) = (min, max) {
                            if min > max {
                                record("UnitWeapons.dmg", format!("min {min} > max {max}"));
                            }
                        }
                    }
                    (min, max) => {
                        if let Err(error) = min {
                            record("UnitWeapons.dmg", format!("unreadable min: {error}"));
                        }
                        if let Err(error) = max {
                            record("UnitWeapons.dmg", format!("unreadable max: {error}"));
                        }
                    }
                }
                match weapon.attacks_per_second() {
                    Ok(value) => {
                        if value.is_some_and(|value| value < 0.0) {
                            record("UnitWeapons.attackbits", "negative attack rate".to_string());
                        }
                    }
                    Err(error) => {
                        record("UnitWeapons.attackbits", format!("unreadable cell: {error}"))
                    }
                }
                match weapon.range() {
                    Ok(value) => {
                        if let Some((field, detail)) = negative(value, "UnitWeapons.rangeN") {
                            record(field, detail);
                        }
                    }
                    Err(error) => {
                        record("UnitWeapons.rangeN", format!("unreadable cell: {error}"))
                    }
                }
            }
            if !bad {
                audit.units.checked += 1;
            }
        }

        for ability in self.abilities() {
            audit.abilities.rows += 1;
            let id = ability.id_str();
            let mut bad = false;
            let mut findings: Vec<BalanceIssue> = Vec::new();
            let mut record = |severity: Severity, field: &'static str, detail: String| {
                findings.push(BalanceIssue {
                    severity,
                    id: id.clone(),
                    field,
                    detail,
                });
                if severity == Severity::Error {
                    bad = true;
                }
            };
            let declared = ability.max_level();
            let levels = match ability.levels() {
                Ok(levels) => levels,
                Err(error) => {
                    record(Severity::Error, "AbilityData.levels", format!("unreadable row: {error}"));
                    Vec::new()
                }
            };
            if declared as usize != levels.len() {
                record(
                    Severity::Error,
                    "AbilityData.levels",
                    format!("declared {declared} levels, parsed {}", levels.len()),
                );
            }
            for level in &levels {
                let suffix = level.level.to_string();
                if let Some((field, detail)) = negative(level.mana_cost, "AbilityData.Cost") {
                    record(Severity::Error, field, format!("level {suffix}: {detail}"));
                }
                if level.cooldown.is_some_and(|value| value < 0.0) {
                    record(Severity::Error, "AbilityData.Cool", format!("level {suffix}: negative cooldown"));
                }
                match level.area {
                    // `Area1 = -1` is the editor's unset sentinel (observed on
                    // the six TFT aura/dummy rows with `targs=_`); anything else
                    // negative is a real contradiction.
                    Some(value) if value == -1.0 => record(
                        Severity::Warning,
                        "AbilityData.Area",
                        format!("level {suffix}: sentinel -1 (no area)"),
                    ),
                    Some(value) if value < 0.0 => {
                        record(Severity::Error, "AbilityData.Area", format!("level {suffix}: negative area"))
                    }
                    _ => {}
                }
                if level.range.is_some_and(|value| value < 0.0) {
                    record(Severity::Error, "AbilityData.Rng", format!("level {suffix}: negative range"));
                }
                if level.duration.is_some_and(|value| value < 0.0) {
                    record(Severity::Error, "AbilityData.Dur", format!("level {suffix}: negative duration"));
                }
                if level.cast.is_some_and(|value| value < 0.0) {
                    record(Severity::Error, "AbilityData.Cast", format!("level {suffix}: negative cast time"));
                }
            }
            audit.abilities.issues.extend(findings);
            if !bad {
                audit.abilities.checked += 1;
            }
        }

        for item in self.items() {
            audit.items.rows += 1;
            let id = item.id_str();
            let mut bad = false;
            let mut record = |field: &'static str, detail: String| {
                audit.items.note(&id, field, detail);
                bad = true;
            };
            let checks: [(&'static str, crate::Result<Option<i32>>); 7] = [
                ("ItemData.goldcost", item.gold_cost()),
                ("ItemData.lumbercost", item.lumber_cost()),
                ("ItemData.Level", item.level()),
                ("ItemData.uses", item.charges()),
                ("ItemData.stockMax", item.stock_max()),
                ("ItemData.stockRegen", item.stock_regen()),
                ("ItemData.stockStart", item.stock_start()),
            ];
            for (field, value) in checks {
                match value {
                    Ok(value) => {
                        if let Some((field, detail)) = negative(value, field) {
                            record(field, detail);
                        }
                    }
                    Err(error) => record(field, format!("unreadable cell: {error}")),
                }
            }
            if !bad {
                audit.items.checked += 1;
            }
        }

        for upgrade in self.upgrades() {
            audit.upgrades.rows += 1;
            let id = upgrade.id_str();
            let mut bad = false;
            let mut record = |field: &'static str, detail: String| {
                audit.upgrades.note(&id, field, detail);
                bad = true;
            };
            let checks: [(&'static str, crate::Result<Option<i32>>); 7] = [
                ("UpgradeData.maxlevel", upgrade.max_level()),
                ("UpgradeData.goldbase", upgrade.gold_base()),
                ("UpgradeData.goldMod", upgrade.gold_per_level()),
                ("UpgradeData.lumberbase", upgrade.lumber_base()),
                ("UpgradeData.lumberMod", upgrade.lumber_per_level()),
                ("UpgradeData.timebase", upgrade.time_base()),
                ("UpgradeData.timeMod", upgrade.time_per_level()),
            ];
            for (field, value) in checks {
                match value {
                    Ok(value) => {
                        if let Some((field, detail)) = negative(value, field) {
                            record(field, detail);
                        }
                    }
                    Err(error) => record(field, format!("unreadable cell: {error}")),
                }
            }
            if !bad {
                audit.upgrades.checked += 1;
            }
        }

        audit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_costs_are_issues() {
        assert_eq!(
            negative(Some(-5), "cost"),
            Some(("cost", "negative value -5".to_string()))
        );
        assert_eq!(negative(Some(0), "cost"), None);
        assert_eq!(negative(None, "cost"), None);
    }

    #[test]
    fn real_installs_have_no_unusable_balance_cells() {
        use std::path::{Path, PathBuf};

        use crate::source::DirSource;
        use crate::GameDatabase;

        let analysis: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis");
        for (root, units, abilities, items, upgrades) in [
            ("war3_data", 466usize, 397usize, 132usize, 58usize),
            ("war3x_data", 812, 763, 273, 89),
        ] {
            let data = GameDatabase::load(&DirSource::new(analysis.join(root)))
                .unwrap_or_else(|error| panic!("load {root}: {error}"));
            let audit = data.balance_audit();
            assert_eq!(audit.units.rows, units, "{root} units");
            assert_eq!(audit.abilities.rows, abilities, "{root} abilities");
            assert_eq!(audit.items.rows, items, "{root} items");
            assert_eq!(audit.upgrades.rows, upgrades, "{root} upgrades");
            assert_eq!(
                audit.rows(),
                units + abilities + items + upgrades,
                "{root} family sum"
            );
            assert_eq!(audit.checked(), audit.rows(), "{root} every row checked");
            assert_eq!(audit.errors(), 0, "{root} unusable cells: {audit:?}");
            assert!(audit.ok(), "{root}");
            if root == "war3x_data" {
                // Six dummy/aura rows carry the editor's `Area1 = -1` sentinel.
                assert_eq!(audit.warnings(), 6, "{root} sentinels");
            } else {
                assert_eq!(audit.warnings(), 0, "{root} sentinels");
            }
        }
    }

    #[test]
    fn audit_shape_adds_up() {
        let mut audit = BalanceAudit::default();
        audit.units.rows = 2;
        audit.units.checked = 2;
        audit.items.rows = 3;
        audit.items.checked = 2;
        audit.items.issues.push(BalanceIssue {
            severity: Severity::Error,
            id: "ratc".to_string(),
            field: "ItemData.goldcost",
            detail: "negative value -1".to_string(),
        });
        audit.items.issues.push(BalanceIssue {
            severity: Severity::Warning,
            id: "phea".to_string(),
            field: "AbilityData.Area",
            detail: "sentinel -1 (no area)".to_string(),
        });
        assert_eq!(audit.rows(), 5);
        assert_eq!(audit.checked(), 4);
        assert_eq!(audit.issues(), 2);
        assert_eq!(audit.errors(), 1);
        assert_eq!(audit.warnings(), 1);
        assert!(!audit.ok());
    }
}
